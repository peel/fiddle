mod support;

use fiddle_runtime::agent::AgentError;
use fiddle_runtime::capability::{
    land, record_fold, undeclared, CapabilityError, ForbiddenShape, GroupMigration, GroupStatus,
    InWorktree, MigrationAttempt, NeedsWork,
};
use fiddle_runtime::cve::dedup::FixedInCommits;
use fiddle_runtime::cve::fold::Landed;
use fiddle_runtime::evaluate::{evaluate, Evaluation, RescanVerdict};
use fiddle_runtime::workspace::{Content, FileEdit, WorkspacePath};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::time::Duration;
use support::cve::{
    advisories_of, ask_git, contract, contract_scanned_by, exit, green_tree, landing_worktree,
    landing_world, migration_world, shown_findings, stdout, tree_rescanned_by, tree_where,
    LandingWorld, MigrationWorld, DEFAULT_LIBRARY_CVES, GO_BUILD, HOST_ROOT, LANDING_CREATED,
    LANDING_UNRELATED, MIGRATION_SOURCE as SOURCE, MIGRATION_TEST_BEFORE,
    MIGRATION_TEST_SOURCE as TEST_SOURCE, SENTINEL_PROSE,
};

fn migrates() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({ "path": SOURCE })),
        MockTurn::tool_call(
            "c2",
            "write_file",
            json!({
                "path": SOURCE,
                "contents": "package main\n\nfunc main() {\n\trenamedName()\n}\n\nfunc renamedName() {}\n",
            }),
        ),
        MockTurn::tool_call("c3", "run_check", json!({})),
        MockTurn::text(
            r#"{"changed_files":["main.go"],"summary":"applied the rename","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump reached this call site"}]}"#,
        ),
    ]
}

const SHOWN: &str = DEFAULT_LIBRARY_CVES[0];

const CLAIMS_DONE: &str = r#"{"changed_files":["main.go","main_test.go"],"summary":"applied the rename","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump, and the call sites it moved"}]}"#;

const ACCOUNTS_FOR_NOTHING: &str = r#"{"changed_files":["main.go","main_test.go"],"summary":"applied the rename","claimed_complete":true}"#;

const RENAMED_SOURCE: &str = "\
package main

func main() {
\trenamedName()
}

func renamedName() {}
";

const RENAMED_TEST: &str = "\
package main

import \"testing\"

func TestRenamedName(t *testing.T) {
\trenamedName()
\tif testing.Short() {
\t\tt.Errorf(\"this test must run even in short mode\")
\t}
}
";

fn migrates_uniformly() -> Vec<MockTurn> {
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (TEST_SOURCE, RENAMED_TEST.to_string()),
    ])
}

fn adds_a_skip() -> Vec<MockTurn> {
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (
            TEST_SOURCE,
            RENAMED_TEST.replace(
                "\trenamedName()\n",
                "\tt.Skip(\"the bump changed this API\")\n\trenamedName()\n",
            ),
        ),
    ])
}

fn weakens_an_assertion() -> Vec<MockTurn> {
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (TEST_SOURCE, RENAMED_TEST.replace("t.Errorf(", "t.Logf(")),
    ])
}

fn adds_control_flow() -> Vec<MockTurn> {
    edits(&[
        (
            SOURCE,
            RENAMED_SOURCE.replace(
                "\trenamedName()\n",
                "\tif len(\"x\") > 0 {\n\t\trenamedName()\n\t}\n",
            ),
        ),
        (TEST_SOURCE, RENAMED_TEST.to_string()),
    ])
}

fn adds_a_replace_directive(world: &MigrationWorld) -> Vec<MockTurn> {
    let module = world.target_module();
    let go_mod = std::fs::read_to_string(world.tree.path().join("go.mod"))
        .expect("the fixture tree has a go.mod");
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (TEST_SOURCE, RENAMED_TEST.to_string()),
        (
            "go.mod",
            format!("{go_mod}\nreplace {module} => ../vendored/{module}\n"),
        ),
    ])
}

fn claims_success_without_editing() -> Vec<MockTurn> {
    vec![MockTurn::text(
        r#"{"changed_files":[],"summary":"nothing needed doing","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump already cleared it"}]}"#,
    )]
}

fn migrates_and_disowns_it() -> Vec<MockTurn> {
    reporting(
        r#"{"changed_files":["main.go","main_test.go"],"summary":"I do not think this is right","claimed_complete":false,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"I made the change and I am not sure of it"}]}"#,
    )
}

fn reporting(report: &str) -> Vec<MockTurn> {
    let mut script = migrates_uniformly();
    script.pop();
    script.push(MockTurn::text(report));
    script
}

fn migrates_and_understates_it() -> Vec<MockTurn> {
    reporting(
        r#"{"changed_files":["main.go"],"summary":"applied the rename","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump, and the call sites it moved"}]}"#,
    )
}

fn accounts_for_it_twice() -> Vec<MockTurn> {
    reporting(
        r#"{"changed_files":["main.go","main_test.go"],"summary":"applied the rename","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"pinned it"},{"cve":"CVE-2026-0001","attempted":false,"note":"actually I left it alone"}]}"#,
    )
}

fn names_an_advisory_nobody_showed_it() -> Vec<MockTurn> {
    reporting(
        r#"{"changed_files":["main.go","main_test.go"],"summary":"applied the rename","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"pinned it"},{"cve":"CVE-2026-9999","attempted":true,"note":"and this one too"}]}"#,
    )
}

fn declines_it() -> Vec<MockTurn> {
    reporting(
        r#"{"changed_files":["main.go","main_test.go"],"summary":"the rename is all I could do","claimed_complete":false,"findings":[{"cve":"CVE-2026-0001","attempted":false,"note":"clearing it needs a major bump I am not confident in"}]}"#,
    )
}

fn edits(files: &[(&str, String)]) -> Vec<MockTurn> {
    let mut script = vec![MockTurn::tool_call(
        "c0",
        "read_file",
        json!({ "path": SOURCE }),
    )];
    for (n, (path, contents)) in files.iter().enumerate() {
        script.push(MockTurn::tool_call(
            format!("w{n}"),
            "write_file",
            json!({ "path": path, "contents": contents }),
        ));
    }
    script.push(MockTurn::tool_call("k", "run_check", json!({})));
    script.push(MockTurn::text(CLAIMS_DONE));
    script
}

struct Sent {
    json: String,
    debug: String,
}

impl Sent {
    fn carries(&self, needle: &str) -> bool {
        self.json.contains(needle) || self.debug.contains(needle)
    }
}

fn sent(model: &MockCompletionModel) -> Sent {
    let requests = model.requests();
    assert!(
        !requests.is_empty(),
        "the model was never called, so there is nothing here to read and every \
         absence below would hold for the emptiest of reasons"
    );
    Sent {
        json: serde_json::to_string(&requests).expect("a CompletionRequest serializes"),
        debug: format!("{requests:#?}"),
    }
}

struct Briefing {
    whole: String,

    fiddles_words: String,

    evidence: Vec<String>,
}

fn briefing(model: &MockCompletionModel, cves: &[String]) -> Briefing {
    let requests = model.requests();
    let first = requests
        .first()
        .expect("the model was called, so there is an opening request to read");
    let rendered = serde_json::to_value(first).expect("a CompletionRequest serializes");
    let history = rendered["chat_history"]
        .as_array()
        .expect("the opening request carries a chat history");

    let mut whole = String::new();
    for message in history {
        match &message["content"] {
            serde_json::Value::String(text) => whole.push_str(text),
            serde_json::Value::Array(parts) => {
                for part in parts {
                    let text = part["text"]
                        .as_str()
                        .expect("every part of the briefing is text");
                    whole.push_str(text);
                }
            }
            other => panic!("the briefing is text, and this is {other}"),
        }
        whole.push('\n');
    }

    let is_evidence =
        |line: &str| line.starts_with("- ") && cves.iter().any(|cve| line.contains(cve.as_str()));
    let evidence: Vec<String> = whole
        .lines()
        .filter(|line| is_evidence(line))
        .map(str::to_string)
        .collect();
    let fiddles_words: String = whole
        .lines()
        .filter(|line| !is_evidence(line))
        .collect::<Vec<&str>>()
        .join("\n");

    Briefing {
        whole,
        fiddles_words,
        evidence,
    }
}

async fn run_migration(
    model: MockCompletionModel,
    world: &MigrationWorld,
) -> Result<MigrationAttempt, fiddle_runtime::capability::CapabilityError> {
    GroupMigration::new(model, world.config())
        .migrate(&world.workspace(), &shown_findings(&world.group))
        .await
}

#[tokio::test]
async fn the_world_holds_everything_the_prompt_must_not() {
    let world = migration_world().await;

    assert!(
        world.report.raw().contains(SENTINEL_PROSE),
        "the document the findings were projected from must carry advisory prose"
    );
    assert!(
        world.resolved.contains("go list -m"),
        "attribution really ran the mechanical rule for this group: {}",
        world.resolved
    );
    assert!(
        world.workspace_root().to_string_lossy().contains(HOST_ROOT),
        "the attempt's worktree must live under a path carrying the host \
         sentinel, or `no host fact` is a claim about a path nothing holds: {}",
        world.workspace_root().display()
    );

    assert!(
        !world.group.findings().is_empty(),
        "a group with no findings would let a prompt carrying no projection pass \
         every assertion in this file"
    );

    assert_eq!(
        world
            .group
            .cves()
            .iter()
            .map(|cve| cve.as_str().to_string())
            .collect::<Vec<String>>(),
        vec![SHOWN.to_string()],
        "the scripted reports below dispose of {SHOWN} and of nothing else"
    );
    for report in [CLAIMS_DONE, ACCOUNTS_FOR_NOTHING] {
        assert_eq!(
            report.contains(SHOWN),
            report != ACCOUNTS_FOR_NOTHING,
            "exactly one of the two reports accounts for {SHOWN}, and it is not \
             the three-field one: {report}"
        );
    }
}

#[tokio::test]
async fn the_prompt_carries_the_projection_and_the_scope_rules_and_nothing_else() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;
    let sent = sent(&model);

    let cve = world.group.cves()[0].as_str().to_string();
    assert!(
        sent.json.contains(&cve),
        "the projection has to reach the model, or every absence below is the \
         absence of an empty prompt"
    );
    for rule in ["refuses the whole attempt", "report it as not attempted"] {
        assert!(
            sent.json.contains(rule),
            "the scope rules reach it, including `{rule}`"
        );
    }

    assert!(!sent.carries(SENTINEL_PROSE), "no advisory prose");

    for mechanical in [
        "go list -m",
        "go mod why",
        "go mod tidy",
        "at_least",
        "dedup",
        "fold",
    ] {
        assert!(
            !sent.carries(mechanical),
            "`{mechanical}` is decided in Rust; no mechanical rule is handed to \
             the model"
        );
    }

    assert!(
        !sent.carries(HOST_ROOT),
        "no host fact, as M1 already requires"
    );
}

#[tokio::test]
async fn the_prompt_names_no_ecosystem_and_no_chosen_version() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;

    let cves: Vec<String> = world
        .group
        .cves()
        .iter()
        .map(|cve| cve.as_str().to_string())
        .collect();
    let briefing = briefing(&model, &cves);

    assert!(
        briefing.whole.contains("read its files"),
        "the system text has to be in the briefing this lane reads: {}",
        briefing.whole
    );
    assert!(
        !briefing.evidence.is_empty(),
        "no line of the briefing looked like a rendered finding, so nothing was \
         subtracted and this lane would be asserting over the scanner's words as \
         well as fiddle's: {}",
        briefing.whole
    );

    for word in [
        "Go",
        "go.mod",
        "go.sum",
        "golang",
        "module",
        "_test.go",
        "t.Skip",
        "Rust",
        "Cargo.toml",
        "requirements.txt",
        "package.json",
    ] {
        assert!(
            !briefing.fiddles_words.contains(word),
            "fiddle's own words name an ecosystem; found {word:?} in:\n{}",
            briefing.fiddles_words
        );
    }

    let finding = world.group.findings()[0].finding();
    let fixed = finding
        .fixed_version
        .as_deref()
        .expect("a fixable finding names a fix");
    for shown in [finding.package.as_str(), finding.current.as_str(), fixed] {
        assert!(
            briefing.whole.contains(shown),
            "the scanner's own evidence is still shown; {shown:?} is missing \
             from:\n{}",
            briefing.whole
        );
        assert!(
            !briefing.fiddles_words.contains(shown),
            "{shown:?} is the scanner's word and appears only where the scanner \
             is quoted; fiddle repeating it in its own words is fiddle deciding \
             it:\n{}",
            briefing.fiddles_words
        );
    }
}

#[tokio::test]
async fn the_six_fields_arrive_and_the_record_they_came_from_does_not() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;
    let sent = sent(&model);

    let finding = world.group.findings()[0].finding();
    for value in [
        finding.cve.as_str(),
        finding.package.as_str(),
        finding.current.as_str(),
        finding
            .fixed_version
            .as_deref()
            .expect("a fixable finding names a fix"),
    ] {
        assert!(
            sent.json.contains(value),
            "the projected `{value}` must reach the model"
        );
    }
    assert!(
        sent.json.contains("Critical") || sent.json.contains("High"),
        "the grade is one of the six fields and must reach it too"
    );

    assert!(
        !sent.carries("hasExploit"),
        "`hasExploit` is a key of the scanner's record, and the record is not \
         what goes to the model"
    );
}

#[tokio::test]
async fn the_bump_target_the_rules_elected_is_not_in_the_prompt() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;
    let sent = sent(&model);

    let target = format!("{:?}", world.group.target());
    assert!(
        target.contains(&world.target_module()),
        "the premise: this group's target names the module, so a prompt that \
         rendered the target would be visible: {target}"
    );
    for rendering in ["Module(", "DockerfileBaseImage", "Rule::", "attribution"] {
        assert!(
            !sent.carries(rendering),
            "`{rendering}` belongs to the answer attribution gave, not to what \
             the model is asked"
        );
    }
}

#[tokio::test]
async fn the_attempt_really_edits_the_tree_through_the_tools() {
    let world = migration_world().await;
    let migration = GroupMigration::new(MockCompletionModel::new(migrates()), world.config());
    let attempt = migration
        .migrate(&world.workspace(), &shown_findings(&world.group))
        .await
        .expect("a scripted migration completes");

    assert!(
        attempt.report.claimed_complete,
        "the model's claim is carried back as evidence"
    );
    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec![SOURCE.to_string()],
        "git saw exactly the file the script wrote"
    );

    let receipts = migration.receipts();
    let called: Vec<&str> = receipts
        .calls
        .iter()
        .map(|call| call.tool.as_str())
        .collect();
    assert_eq!(
        called,
        vec!["read_file", "write_file", "run_check"],
        "the three tools the script calls really ran"
    );
    assert!(
        receipts.calls.iter().all(|call| call.outcome == "ok"),
        "a refusal would make the edit above somebody else's doing: {receipts:?}"
    );

    assert_eq!(
        attempt
            .report
            .findings
            .iter()
            .map(|disposition| disposition.cve.clone())
            .collect::<Vec<String>>(),
        vec![SHOWN.to_string()],
        "the disposition the script sent is carried back: {:?}",
        attempt.report.findings
    );
}

async fn refusal_reason(script: Vec<MockTurn>, world: &MigrationWorld) -> String {
    let failure = run_migration(MockCompletionModel::new(script), world)
        .await
        .expect_err("a report that does not account for what it was shown is refused");

    match failure {
        CapabilityError::Agent(AgentError::Protocol { reason }) => reason,
        other => panic!("a malformed report is a protocol failure, and this is {other:?}"),
    }
}

#[tokio::test]
async fn a_report_that_leaves_an_advisory_out_is_refused() {
    let world = migration_world().await;
    let reason = refusal_reason(reporting(ACCOUNTS_FOR_NOTHING), &world).await;

    assert!(
        reason.contains(SHOWN),
        "the refusal has to name the advisory nothing was said about: {reason}"
    );
}

#[tokio::test]
async fn a_report_naming_an_advisory_nobody_showed_it_is_refused() {
    let world = migration_world().await;
    let reason = refusal_reason(names_an_advisory_nobody_showed_it(), &world).await;

    assert!(
        reason.contains("CVE-2026-9999"),
        "the refusal has to name the advisory the report invented: {reason}"
    );
}

#[tokio::test]
async fn one_advisory_disposed_of_twice_is_refused() {
    let world = migration_world().await;
    let reason = refusal_reason(accounts_for_it_twice(), &world).await;

    assert!(
        reason.contains(SHOWN),
        "the refusal has to name the advisory that arrived twice: {reason}"
    );
}

#[tokio::test]
async fn a_declined_advisory_is_a_protocol_success() {
    let world = migration_world().await;
    let attempt = run_migration(MockCompletionModel::new(declines_it()), &world)
        .await
        .expect("declining what it was shown is an answer, not a broken contract");

    let disposition = match attempt.report.findings.as_slice() {
        [only] => only,
        other => panic!("one advisory was shown, so one disposition comes back: {other:?}"),
    };
    assert_eq!(disposition.cve, SHOWN, "and it is the one that was shown");
    assert!(
        !disposition.attempted,
        "the declining is carried back rather than smoothed over: {disposition:?}"
    );
    assert!(
        !disposition.note.trim().is_empty(),
        "what stopped it is the whole value of a declined disposition: {disposition:?}"
    );
}

#[tokio::test]
async fn no_worktree_survives_the_attempt() {
    let world = migration_world().await;
    for (name, script) in [
        ("migrates", migrates()),
        ("malformed", vec![MockTurn::text("this is not the schema")]),
    ] {
        let _ = run_migration(MockCompletionModel::new(script), &world).await;

        assert!(
            world.workspace_root().exists(),
            "the `{name}` attempt never prepared a workspace, so nothing was proven"
        );
        let leftovers: Vec<String> = std::fs::read_dir(world.workspace_root())
            .expect("the workspace root is readable")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| !name.ends_with(".home"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the `{name}` attempt left a worktree behind: {leftovers:?}"
        );
    }
}

async fn a_proved_tree() -> Evaluation {
    evaluate(&contract_scanned_by("1.2.3"), &tree_rescanned_by("1.2.3"))
        .await
        .expect("an evaluation that was not cancelled")
}

async fn a_tree_that_will_not_build() -> Evaluation {
    evaluate(
        &contract_scanned_by("1.2.3"),
        &tree_where(GO_BUILD, exit(1), stdout("")),
    )
    .await
    .expect("an evaluation that was not cancelled")
}

async fn a_tree_nothing_was_proved_about() -> Evaluation {
    evaluate(&contract(), &green_tree())
        .await
        .expect("an evaluation that was not cancelled")
}

async fn attempted(script: Vec<MockTurn>) -> (MigrationWorld, MigrationAttempt) {
    let world = migration_world().await;
    let attempt = run_migration(MockCompletionModel::new(script), &world)
        .await
        .expect("a scripted migration completes");
    (world, attempt)
}

async fn attempted_with(build: impl Fn(&MigrationWorld) -> Vec<MockTurn>) -> MigrationAttempt {
    let world = migration_world().await;
    let script = build(&world);
    run_migration(MockCompletionModel::new(script), &world)
        .await
        .expect("a scripted migration completes")
}

fn the_one_shape(attempt: &MigrationAttempt) -> &ForbiddenShape {
    assert_eq!(
        attempt.forbidden.len(),
        1,
        "each script here is the uniform migration plus exactly one further \
         thing, so exactly one rule may fire: {:#?}",
        attempt.forbidden
    );
    &attempt.forbidden[0]
}

#[tokio::test]
async fn the_model_cannot_return_a_verdict() {
    let (_world, attempt) = attempted(claims_success_without_editing()).await;

    assert!(
        attempt.report.claimed_complete,
        "the claim is recorded as evidence, which is the premise for the rest \
         of this lane"
    );
    assert!(
        attempt.changed.is_empty(),
        "the model changed nothing, so nothing but the checks can be deciding \
         below: {:?}",
        attempt.changed
    );
    assert!(
        attempt.forbidden.is_empty(),
        "and no shape was found, for the same reason: {:#?}",
        attempt.forbidden
    );

    let refused = GroupStatus::of(
        &a_tree_that_will_not_build().await,
        &attempt.forbidden,
        attempt.undeclared.as_ref(),
    );
    assert!(
        matches!(
            &refused,
            GroupStatus::NeedsWork {
                reason: NeedsWork::CheckFailed { check }
            } if check == GO_BUILD
        ),
        "a model that says it finished does not make a tree that will not \
         build clean, and the refusal names the check that decided: {refused:?}"
    );

    let accepted = GroupStatus::of(
        &a_proved_tree().await,
        &attempt.forbidden,
        attempt.undeclared.as_ref(),
    );
    assert_eq!(
        accepted,
        GroupStatus::Clean,
        "and the same claim over a proved tree is clean — so what changed the \
         answer was the evaluation and not the claim"
    );
}

#[tokio::test]
async fn a_disowned_edit_the_checks_prove_is_still_clean() {
    let (_world, attempt) = attempted(migrates_and_disowns_it()).await;

    assert!(
        !attempt.report.claimed_complete,
        "the premise: the model said it had not finished"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::Clean,
        "the checks decide, and they proved this tree"
    );
}

#[test]
fn nothing_in_this_workspace_decides_on_claimed_complete() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate lives under the workspace's crates directory");

    let mut accesses = 0usize;
    let mut declared = false;
    let mut reads: Vec<String> = Vec::new();
    for file in rust_sources(crates) {
        let text = std::fs::read_to_string(&file).expect("a source file of this workspace");
        for (n, line) in text.lines().enumerate() {
            declared |= line.trim() == "pub claimed_complete: bool,";
            for (at, _) in line.match_indices("claimed_complete") {
                if !line[..at].ends_with('.') {
                    continue;
                }
                accesses += 1;
                if !recorded_rather_than_read(line) {
                    reads.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        declared,
        "no source under {} declares `pub claimed_complete: bool`, so this lane \
         is looking for the wrong name",
        crates.display()
    );
    assert!(
        accesses > 0,
        "nothing under {} reads the field at all, so an allowlist over its \
         readers proved nothing",
        crates.display()
    );
    assert!(
        reads.is_empty(),
        "claimed_complete is evidence and only evidence; these reach it \
         somewhere a plain recording does not explain:\n{}",
        reads.join("\n")
    );
}

fn rust_sources(crates: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending: Vec<std::path::PathBuf> = std::fs::read_dir(crates)
        .expect("the workspace's crates directory is readable")
        .flatten()
        .map(|entry| entry.path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    assert!(
        !pending.is_empty(),
        "no crate under {} has a src directory",
        crates.display()
    );
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|why| panic!("{} is readable: {why}", dir.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

fn recorded_rather_than_read(line: &str) -> bool {
    line.trim()
        .strip_suffix(".claimed_complete,")
        .and_then(|prefix| prefix.split_once(": "))
        .is_some_and(|(field, from)| {
            let identifier =
                |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
            identifier(field) && identifier(from)
        })
}

#[tokio::test]
async fn a_uniform_rename_reaching_the_test_file_is_in_scope() {
    let (_world, attempt) = attempted(migrates_uniformly()).await;

    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec![SOURCE.to_string(), TEST_SOURCE.to_string()],
        "the premise: this attempt really rewrote both files"
    );
    assert!(
        attempt.forbidden.is_empty(),
        "a uniform rename is the one exception the scope rules allow: {:#?}",
        attempt.forbidden
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::Clean
    );
}

#[tokio::test]
async fn an_added_skip_makes_the_group_needs_work() {
    let (_world, attempt) = attempted(adds_a_skip()).await;

    let shape = the_one_shape(&attempt);
    assert!(
        matches!(shape, ForbiddenShape::AddedSkip { path, line }
            if path == TEST_SOURCE && line.contains("t.Skip(")),
        "the skip is named, with the line it was written on: {shape:?}"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        },
        "and every check passing does not rescue it"
    );
}

#[tokio::test]
async fn a_changed_test_assertion_makes_the_group_needs_work() {
    let (_world, attempt) = attempted(weakens_an_assertion()).await;

    let shape = the_one_shape(&attempt);
    assert!(
        matches!(shape, ForbiddenShape::ChangedTestAssertion { path, assertion }
            if path == TEST_SOURCE && assertion.contains("t.Errorf(")),
        "the assertion that left the file is quoted as it read: {shape:?}"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        }
    );
}

#[tokio::test]
async fn a_replace_directive_makes_the_group_needs_work() {
    let attempt = attempted_with(adds_a_replace_directive).await;

    let shape = the_one_shape(&attempt);
    assert!(
        matches!(shape, ForbiddenShape::ReplaceDirective { path, directive }
            if path == "go.mod" && directive.starts_with("replace ")),
        "the directive is named, in the file it was written to: {shape:?}"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        }
    );
}

#[tokio::test]
async fn new_control_flow_makes_the_group_needs_work() {
    let (_world, attempt) = attempted(adds_control_flow()).await;

    let shape = the_one_shape(&attempt);
    assert_eq!(
        shape,
        &ForbiddenShape::NewControlFlow {
            path: SOURCE.to_string(),
            keyword: "if",
            before: 0,
            after: 1,
        },
        "the branch that appeared is named, with what the file had before"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        }
    );
}

#[tokio::test]
async fn a_rename_on_a_branch_line_is_not_new_control_flow() {
    let world = migration_world().await;
    let rewritten = MIGRATION_TEST_BEFORE.replace("if testing.Short()", "if !testing.Verbose()");
    assert_ne!(
        rewritten, MIGRATION_TEST_BEFORE,
        "the premise: the replacement really rewrote the branch line"
    );
    let attempt = run_migration(
        MockCompletionModel::new(edits(&[(TEST_SOURCE, rewritten)])),
        &world,
    )
    .await
    .expect("a scripted migration completes");

    assert!(
        attempt.forbidden.is_empty(),
        "the branch line changed and the number of branches did not: {:#?}",
        attempt.forbidden
    );
}

#[tokio::test]
async fn a_clean_group_is_exactly_an_accepted_one() {
    for (name, evaluation) in [
        ("proved", a_proved_tree().await),
        ("will not build", a_tree_that_will_not_build().await),
        ("nothing proved", a_tree_nothing_was_proved_about().await),
    ] {
        assert_eq!(
            GroupStatus::of(&evaluation, &[], None) == GroupStatus::Clean,
            evaluation.accepted(),
            "`{name}`: clean and accepted must be the same question"
        );
    }

    let unproved = a_tree_nothing_was_proved_about().await;
    assert!(unproved.first_failure().is_none(), "every check passed");
    assert_eq!(
        GroupStatus::of(&unproved, &[], None),
        GroupStatus::NeedsWork {
            reason: NeedsWork::Unproved(RescanVerdict::NotCompared)
        }
    );
}

fn edit(path: &str) -> FileEdit {
    FileEdit {
        path: WorkspacePath::parse(path).expect("test path is relative and clean"),
        before: Content::Text("old".to_string()),
        after: Content::Text("new".to_string()),
    }
}

#[test]
fn an_edit_the_attempt_did_not_declare_is_refused() {
    let declared = vec!["requirements.txt".to_string()];
    let touched = vec![edit("requirements.txt"), edit("setup.py")];
    let refusal = undeclared(&declared, &touched).expect("setup.py was not declared");
    assert!(
        refusal.to_string().contains("setup.py"),
        "the refusal must name the file: {refusal}"
    );
}

#[test]
fn a_declared_file_the_attempt_did_not_touch_is_refused() {
    let declared = vec!["requirements.txt".to_string(), "poetry.lock".to_string()];
    let touched = vec![edit("requirements.txt")];
    let refusal = undeclared(&declared, &touched).expect("poetry.lock was declared and untouched");
    assert!(refusal.to_string().contains("poetry.lock"), "{refusal}");
}

#[test]
fn a_declaration_that_matches_the_diff_is_no_breach() {
    let declared = vec!["requirements.txt".to_string(), "app/main.py".to_string()];
    let touched = vec![edit("app/main.py"), edit("requirements.txt")];
    assert!(
        undeclared(&declared, &touched).is_none(),
        "the same set in a different order is the same set"
    );
}

#[tokio::test]
async fn an_attempt_that_understated_its_diff_is_needs_work_over_green_checks() {
    let (_world, attempt) = attempted(migrates_and_understates_it()).await;

    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec![SOURCE.to_string(), TEST_SOURCE.to_string()],
        "the premise: git saw both files change"
    );
    assert_eq!(
        attempt.report.changed_files,
        vec![SOURCE.to_string()],
        "and the premise's other half: the attempt declared one of them"
    );
    assert!(
        attempt.forbidden.is_empty(),
        "no scope rule fired, so the declaration rule is the only thing that \
         can refuse this: {:#?}",
        attempt.forbidden
    );

    let status = GroupStatus::of(
        &a_proved_tree().await,
        &attempt.forbidden,
        attempt.undeclared.as_ref(),
    );
    let GroupStatus::NeedsWork {
        reason: NeedsWork::Undeclared(breach),
    } = &status
    else {
        panic!("an undeclared edit is not clean, whatever the checks said: {status:?}");
    };
    assert_eq!(
        breach.unannounced,
        vec![TEST_SOURCE.to_string()],
        "the one file it changed and did not mention: {breach:?}"
    );
    assert!(
        breach.unmet.is_empty(),
        "and it did mention the other one: {breach:?}"
    );
    assert!(
        breach.to_string().contains(TEST_SOURCE),
        "the sentence an operator reads names the file: {breach}"
    );
}

#[tokio::test]
async fn an_attempt_whose_declaration_matches_its_diff_has_no_breach() {
    let (_world, attempt) = attempted(migrates_uniformly()).await;

    assert_eq!(
        attempt.undeclared, None,
        "both files declared and both changed: {:?}",
        attempt.undeclared
    );
}

#[tokio::test]
async fn what_the_run_changed_before_briefing_is_excused_and_nothing_beside_it_is() {
    let world = migration_world().await;
    let workspace = world.workspace();

    let manifest = workspace.root().join("go.mod");
    let bumped = std::fs::read_to_string(&manifest).expect("the fixture tree has a go.mod")
        + "\n// moved by the run, before the attempt began\n";
    std::fs::write(&manifest, bumped).expect("the worktree is writable");

    let script = vec![
        MockTurn::tool_call("r", "read_file", json!({ "path": TEST_SOURCE })),
        MockTurn::tool_call(
            "w",
            "write_file",
            json!({ "path": TEST_SOURCE, "contents": RENAMED_TEST }),
        ),
        MockTurn::text(
            r#"{"changed_files":[],"summary":"the bump was enough","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump was enough"}]}"#,
        ),
    ];
    let attempt = GroupMigration::new(MockCompletionModel::new(script), world.config())
        .migrate(&workspace, &shown_findings(&world.group))
        .await
        .expect("a scripted migration completes");

    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec!["go.mod".to_string(), TEST_SOURCE.to_string()],
        "the premise: the diff holds the run's edit and the attempt's together"
    );

    let breach = attempt
        .undeclared
        .as_ref()
        .expect("the attempt declared nothing and edited a file");
    assert_eq!(
        breach.unannounced,
        vec![TEST_SOURCE.to_string()],
        "the bumped manifest is the run's and is excused; the source edit beside \
         it is the attempt's and is not: {breach:?}"
    );
    assert!(
        breach.unmet.is_empty(),
        "the run's own paths are all still in the diff: {breach:?}"
    );
}

const LANDED: [&str; 2] = ["CVE-2026-1", "CVE-2026-2"];

const NOT_LANDED: &str = "CVE-2026-3";

fn refused() -> GroupStatus {
    GroupStatus::NeedsWork {
        reason: NeedsWork::CheckFailed {
            check: GO_BUILD.to_string(),
        },
    }
}

async fn run_group_clean(cves: &[&str]) -> (LandingWorld, Landed) {
    let world = landing_world(cves);
    let landed = land(
        &world.tree,
        &advisories_of(&world.group),
        &GroupStatus::Clean,
        &world.changed,
    )
    .await
    .expect("a clean group lands");
    (world, landed)
}

async fn run_group_needs_work(cves: &[&str]) -> (LandingWorld, Landed) {
    let world = landing_world(cves);
    let landed = land(
        &world.tree,
        &advisories_of(&world.group),
        &refused(),
        &world.changed,
    )
    .await
    .expect("a needs-work group reverts");
    (world, landed)
}

fn nothing_is_staged_by_directory(calls: &[String]) {
    assert!(
        !calls.is_empty(),
        "no git was recorded at all, so every negative below holds for the \
         emptiest of reasons: the seam was never wired in"
    );
    for call in calls {
        for forbidden in ["add -A", "add .", "commit -a"] {
            assert!(
                !call.contains(forbidden),
                "`{forbidden}` stages what nothing classified: {call}"
            );
        }
        let tokens: Vec<&str> = call.split_whitespace().collect();
        let has = |token: &str| tokens.contains(&token);
        assert!(
            !(has("add") && (has("-A") || has("--all") || has("."))),
            "an `add` that names a directory rather than the files the group \
             edited: {call}"
        );
        assert!(
            !(has("commit") && (has("-a") || has("--all"))),
            "a `commit` that stages on the caller's behalf: {call}"
        );
    }
}

fn nothing_rewrites_history(calls: &[String]) {
    assert!(
        !calls.is_empty(),
        "no git was recorded at all, so this proves nothing about what ran"
    );
    for call in calls {
        for forbidden in [
            "push --force",
            "--force-with-lease",
            "reset",
            "rebase",
            "commit --amend",
            "--amend",
            "--no-verify",
        ] {
            assert!(!call.contains(forbidden), "`{forbidden}`: {call}");
        }
    }
}

#[test]
fn a_landing_world_has_something_outside_the_change_set_to_get_wrong() {
    let world = landing_world(&LANDED);

    let changed: Vec<&str> = world.changed.iter().map(|path| path.as_str()).collect();
    assert_eq!(changed, ["go.mod", "go.sum"]);
    assert!(
        !changed.contains(&LANDING_UNRELATED),
        "the discriminating file must be outside the change set"
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "and it must be dirty, or staging by name and by directory agree"
    );
    assert!(
        !world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "and the bump must really have changed the tree, or a commit of nothing \
         would satisfy every lane below"
    );
    assert!(
        world.tree.git_calls().is_empty(),
        "construction must record nothing, or `what the subject ran` is a list \
         holding what this fixture ran: {:?}",
        world.tree.git_calls()
    );
    assert!(
        !world.tree.all_commit_bodies().is_empty(),
        "there has to be a history for an id to be absent from"
    );
}

#[tokio::test]
async fn a_clean_group_commits_only_the_files_it_edited_and_names_every_cve() {
    let (world, landed) = run_group_clean(&LANDED).await;

    assert_eq!(landed, Landed::Committed);
    assert_eq!(
        world.tree.staged_paths(),
        ["go.mod", "go.sum"],
        "the commit carries the group's own files and nothing beside them"
    );
    assert!(
        world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "and they are on the branch rather than still sitting dirty"
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "{LANDING_UNRELATED} was dirty and had nothing to do with this group, so \
         it must still be dirty"
    );

    let fixed = FixedInCommits::read(&world.tree.head_commit_body());
    for cve in LANDED {
        assert!(
            fixed.names(cve),
            "the log is what recovers the fixed set for OS findings next run, \
             and it does not name {cve}: {}",
            world.tree.head_commit_body()
        );
    }

    nothing_is_staged_by_directory(&world.tree.git_calls());
    assert_eq!(
        world.tree.git_calls().first().map(String::as_str),
        Some("add -f -- go.mod go.sum"),
        "staging is the group's paths, by name: {:?}",
        world.tree.git_calls()
    );
}

#[tokio::test]
async fn a_needs_work_group_reverts_and_leaves_no_id_in_any_commit_body() {
    let (world, landed) = run_group_needs_work(&[NOT_LANDED]).await;

    assert_eq!(landed, Landed::Reverted);
    assert!(
        world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "the group's own files are back the way HEAD has them"
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "and a revert by name left the file it was not given alone"
    );

    let bodies = world.tree.all_commit_bodies();
    assert_eq!(
        bodies, world.history_before,
        "a needs-work group makes no commit at all, so the history is what it was"
    );
    let fixed = FixedInCommits::read(&bodies);
    assert!(
        !fixed.names(NOT_LANDED),
        "an id in a body is a claim it was fixed, and the next run's log scan \
         believes it: {bodies}"
    );
    assert!(
        fixed.names("chore"),
        "the reader really reads this history — otherwise the absence above is a \
         fact about the reader: {bodies}"
    );

    nothing_rewrites_history(&world.tree.git_calls());
}

#[tokio::test]
async fn a_forbidden_shape_over_green_checks_reverts_rather_than_committing() {
    let (_migrated, attempt) = attempted(adds_a_skip()).await;
    let evaluation = a_proved_tree().await;
    let status = GroupStatus::of(&evaluation, &attempt.forbidden, attempt.undeclared.as_ref());

    assert!(
        matches!(the_one_shape(&attempt), ForbiddenShape::AddedSkip { .. }),
        "the premise: this attempt switched a test off"
    );
    assert!(
        evaluation.accepted(),
        "the premise: every check passed and the rescan cleared, so a landing \
         that read the evaluation would commit this"
    );
    assert_ne!(
        status,
        GroupStatus::Clean,
        "and the status says otherwise, which is the divergence"
    );

    let world = landing_world(&LANDED);
    let landed = land(
        &world.tree,
        &advisories_of(&world.group),
        &status,
        &world.changed,
    )
    .await
    .expect("a refused group reverts");

    assert_eq!(
        landed,
        Landed::Reverted,
        "GroupStatus is the commit gate, not Evaluation::accepted"
    );
    assert_eq!(
        world.tree.all_commit_bodies(),
        world.history_before,
        "nothing was committed, so nothing on this branch claims a fix"
    );
    let fixed = FixedInCommits::read(&world.tree.all_commit_bodies());
    for cve in LANDED {
        assert!(
            !fixed.names(cve),
            "an out-of-scope group must not claim {cve} was fixed: {}",
            world.tree.all_commit_bodies()
        );
    }
    assert!(
        world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "and the edit is off the tree"
    );
}

#[tokio::test]
async fn a_file_the_attempt_created_does_not_survive_the_revert() {
    let world = landing_world(&[NOT_LANDED]).and_a_created_file();
    assert!(
        !world.tree.is_clean_at(&[LANDING_CREATED]),
        "the premise: the created file is really in the tree"
    );

    let landed = land(
        &world.tree,
        &advisories_of(&world.group),
        &refused(),
        &world.changed,
    )
    .await
    .expect("a needs-work group reverts");

    assert_eq!(landed, Landed::Reverted);
    assert!(
        world
            .tree
            .is_clean_at(&["go.mod", "go.sum", LANDING_CREATED]),
        "every path the group changed is back the way HEAD has it, creations \
         included: {:?}",
        world.tree.git_calls()
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "and still by name — the file the revert was not given is untouched"
    );
    nothing_rewrites_history(&world.tree.git_calls());
}

#[tokio::test]
async fn a_clean_group_that_changed_nothing_commits_nothing_and_says_so() {
    let world = landing_world(&LANDED);

    let refusal = land(
        &world.tree,
        &advisories_of(&world.group),
        &GroupStatus::Clean,
        &[],
    )
    .await
    .expect_err("a clean group with an empty change set is refused");

    assert!(
        matches!(refusal, CapabilityError::NothingProposed),
        "and it is the refusal that says the tree did not change: {refusal:?}"
    );
    assert_eq!(
        world.tree.all_commit_bodies(),
        world.history_before,
        "no commit was made, empty or otherwise"
    );
    let fixed = FixedInCommits::read(&world.tree.all_commit_bodies());
    for cve in LANDED {
        assert!(!fixed.names(cve), "and nothing claims {cve} was fixed");
    }
}

#[tokio::test]
async fn a_fold_is_an_empty_commit_naming_every_id_and_amending_nothing() {
    let world = landing_world(&LANDED);
    let before = world.tree.staged_paths();

    record_fold(&world.tree, &world.group)
        .await
        .expect("a fold is recorded");

    assert!(
        world.tree.staged_paths().is_empty(),
        "a fold changes no file, and this commit carries {:?}",
        world.tree.staged_paths()
    );
    assert_ne!(
        world.tree.staged_paths(),
        before,
        "the premise: the commit before this one was not itself empty, so \
         `empty` above is a fact about the fold"
    );

    let fixed = FixedInCommits::read(&world.tree.head_commit_body());
    for cve in LANDED {
        assert!(
            fixed.names(cve),
            "a fold that named no advisory is invisible to the next run: {}",
            world.tree.head_commit_body()
        );
    }

    let calls = world.tree.git_calls();
    assert!(
        calls.iter().any(|call| call.contains("--allow-empty")),
        "a fold changes nothing, so it needs the flag to become a commit: {calls:?}"
    );
    nothing_rewrites_history(&calls);
    nothing_is_staged_by_directory(&calls);
}

#[tokio::test]
async fn history_is_never_rewritten() {
    let (committed, _) = run_group_clean(&LANDED).await;
    let (reverted, _) = run_group_needs_work(&[NOT_LANDED]).await;
    let folded = landing_world(&LANDED);
    record_fold(&folded.tree, &folded.group)
        .await
        .expect("a fold is recorded");

    for (name, tree) in [
        ("clean", &committed.tree),
        ("needs-work", &reverted.tree),
        ("fold", &folded.tree),
    ] {
        let calls = tree.git_calls();
        assert!(
            !calls.is_empty(),
            "the `{name}` landing recorded nothing, so it proves nothing"
        );
        nothing_rewrites_history(&calls);
        nothing_is_staged_by_directory(&calls);
    }
}

#[tokio::test]
async fn the_production_seam_lands_a_group_in_a_real_worktree() {
    let world = landing_world(&LANDED);
    let attempt = landing_worktree(&world);
    let root = attempt.workspace.root();

    let landed = land(
        &InWorktree::new(&attempt.workspace, Duration::from_secs(60)),
        &advisories_of(&world.group),
        &GroupStatus::Clean,
        &attempt.changed,
    )
    .await
    .expect("a clean group lands in a real worktree");

    assert_eq!(landed, Landed::Committed);
    assert_eq!(
        ask_git(
            root,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        )
        .lines()
        .collect::<Vec<_>>(),
        ["go.mod", "go.sum"],
        "the commit the product made carries the group's own files"
    );

    let body = ask_git(root, &["log", "-1", "--format=%B"]);
    let fixed = FixedInCommits::read(&body);
    for cve in LANDED {
        assert!(
            fixed.names(cve),
            "the product's own body must name {cve}: {body}"
        );
    }
    assert!(
        ask_git(root, &["status", "--porcelain"]).is_empty(),
        "and the worktree is clean, so nothing was left staged or unstaged"
    );
}
