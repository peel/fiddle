//! Black-box acceptance for the `[github]` table as a *deployment*, rather than
//! as a document.
//!
//! `config_check.rs` asserts what the schema accepts and refuses. This file
//! asserts the half that a schema cannot: that the keys reach something. A
//! configuration key that parses, defaults, and is consumed by nothing is the
//! defect `agent.max_capability_attempts` shipped and ADR 013 had to price after
//! the fact, so every scenario here drives the compiled binary and observes
//! what it *did* with the value.
//!
//! Three properties are under test.
//!
//! - **The capability is constructible.** `run --capability publish_change`
//!   selected the capability and then refused with `Unconfigured("[github]")`
//!   until this table existed. It now builds the clients, the effect context and
//!   the executor, and executes.
//! - **`[github.policy]` reaches the executor.** A document saying `deny`
//!   produces a policy refusal naming the effect kind, and — the discriminating
//!   half — the same document *without* the rule does not.
//! - **`inspect` still builds nothing.** For every value of `--capability`, on a
//!   document that describes no forge at all, with no credential exported.
//!
//! # Offline, and credential-free in the sense that matters
//!
//! `[github] cli = { program, args }` is the product seam an operator uses to
//! pin or wrap `gh`; these scenarios point it at a small script that answers
//! from a `case` statement. Nothing here reaches a network, and the token these
//! runs export authenticates nothing — it exists so that the *resolution* of the
//! credential is exercised, and it is asserted to reach no observable surface.

mod support;

use std::path::{Path, PathBuf};
use support::Scenario;

/// The work this milestone's scenarios are about.
const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

/// The variable every document below names. Never a value.
const CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

/// What is exported as that credential: a string that authenticates nothing,
/// and that must appear on no surface.
const SENTINEL: &str = "ghp_sentinel_github_deployment_must_never_print_7c31";

/// A scenario with one open work item and a git repository holding the change
/// that is to be published.
///
/// Real git rather than a bare directory: the commit being published is read out
/// of this worktree's `HEAD` by the binary itself, so a scenario over a
/// non-repository would fail before anything policy-shaped happened.
fn publishable() -> (Scenario, PathBuf) {
    let scenario = Scenario::new();
    scenario.write_work_item(WORK_ID, "open");
    let work = scenario.write_fixture_repo();
    (scenario, work)
}

/// The commit `work` is sitting on, read the way the binary reads it.
fn head_sha(work: &Path) -> String {
    let out = std::process::Command::new("git")
        .current_dir(work)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success(), "could not read HEAD of {work:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A `gh` that answers every request `404`, so nothing this run asks about
/// exists yet.
///
/// Exit 1 rather than 0 because that is what `gh` itself exits with on a 404,
/// and the adapter deliberately reads the status line rather than the exit code
/// for anything that is not authentication, cancellation or a killed child.
fn gh_answering_nothing_exists(dir: &Path) -> PathBuf {
    write_gh(
        dir,
        "empty-gh",
        "printf 'HTTP/1.1 404 Not Found\\r\\n\\r\\n{\"message\":\"Not Found\"}\\n'\nexit 1\n",
    )
}

/// A `gh` reporting that the branch is already published at `sha`, and that no
/// pull request is open for it.
///
/// This is what lets a scenario reach the *second* effect offline: the branch's
/// postcondition already holds, so the executor settles it at its step 3 and
/// never pushes.
fn gh_answering_the_branch_is_published(dir: &Path, sha: &str) -> PathBuf {
    write_gh(
        dir,
        "published-gh",
        &format!(
            "for a in \"$@\"; do path=\"$a\"; done\n\
             case \"$path\" in\n\
             \x20 */git/ref/heads/*)\n\
             \x20   printf 'HTTP/1.1 200 OK\\r\\n\\r\\n{{\"object\":{{\"sha\":\"{sha}\"}}}}\\n' ;;\n\
             \x20 */pulls*)\n\
             \x20   printf 'HTTP/1.1 200 OK\\r\\n\\r\\n[]\\n' ;;\n\
             \x20 *)\n\
             \x20   printf 'HTTP/1.1 404 Not Found\\r\\n\\r\\n{{\"message\":\"Not Found\"}}\\n'\n\
             \x20   exit 1 ;;\n\
             esac\n"
        ),
    )
}

/// Write an executable `sh` script standing in for `gh`, and hand back its path.
#[cfg(unix)]
fn write_gh(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The `[github]` table for `work`, driving `gh`, plus whatever `extra` lines a
/// scenario needs.
///
/// `config_dir` is written down rather than left to its default, and that is a
/// property of the harness rather than of the feature: the default is relative
/// to the working directory, and a test that took it would create a scratch
/// directory inside the package it is being run from. Every path a scenario
/// names lives inside the scenario, so a scenario leaves nothing behind.
fn forge_table(scenario: &Scenario, gh: &Path, work: &Path, extra: &str) -> String {
    format!(
        "[github]\n\
         repo = \"peel/fiddle-effects-acceptance\"\n\
         base = \"main\"\n\
         token = {{ env = \"{CREDENTIAL}\" }}\n\
         cli = {{ program = {gh}, args = [] }}\n\
         git = \"git\"\n\
         work = {work}\n\
         workflow = \"verify.yml\"\n\
         config_dir = {config_dir}\n\
         timeout = \"120s\"\n\
         {extra}",
        gh = toml_path(gh),
        work = toml_path(work),
        config_dir = toml_path(&scenario.dir().join("gh-config")),
    )
}

/// A path as a TOML string, escaped rather than pasted.
fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// `fiddle run <ref> --capability publish_change --json`, with the credential
/// exported, unjudged.
fn publish(scenario: &Scenario) -> std::process::Output {
    scenario
        .run_command(INVOCATION_REF)
        .args(["--capability", "publish_change", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap()
}

/// What the run said about the stage it ran.
///
/// `progress[0].summary` rather than the outcome's reason: a run's own failure
/// text lands there, filed under the stage it happened at, which is the field
/// `report.rs` documents as carrying it.
fn summary_of(payload: &serde_json::Value) -> String {
    payload["progress"][0]["summary"]
        .as_str()
        .unwrap_or_else(|| panic!("a run that executed publishes one progress entry: {payload}"))
        .to_string()
}

/// The `--json` run payload, and the stderr beside it for a failing assertion to
/// quote.
fn payload_of(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr = {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// **The criterion Task 9 could not satisfy.**
///
/// `run --capability publish_change` used to select the capability and then
/// refuse: the `[github]` table did not exist, so `build_capability` had nothing
/// to construct an executor from. With the table written, the binary builds the
/// clients, owns the effect context, hands the capability a borrowed executor,
/// and runs it — and the evidence that it *executed* rather than merely built is
/// that the world was consulted: the run reaches an effect and reports on it.
#[test]
fn run_constructs_and_executes_the_publishing_capability() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let out = publish(&s);
    let payload = payload_of(&out);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "publish_change",
        "the run must execute the capability it was asked for, got {payload}"
    );
    assert_eq!(
        payload["progress"][0]["stage"], "publish",
        "the executed capability names its own stage, got {payload}"
    );
    assert_ne!(
        payload["outcome"],
        serde_json::Value::Null,
        "the run must have reached a conclusion, got {payload}"
    );
    // The refusal this bean removes must be gone, on both surfaces.
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains("[github]"),
        "the capability is configured and must no longer be refused: {stderr}"
    );
    // Row 11 and not row 2: a capability that ran and failed is `Retryable`,
    // which is the mapping every capability failure has had since M1. Row 2 is
    // what a *refused invocation* exits on, and it is what this scenario exited
    // on before the table existed — so the number itself is evidence that the
    // capability was built rather than declined.
    assert_eq!(
        out.status.code(),
        Some(11),
        "with nothing on the far end of `git push`, the run fails — but it \
         fails having executed: {payload}\nstderr = {stderr}"
    );
}

/// **`[github.policy]` reaches the executor.**
///
/// The first effect's own postcondition does not hold — the scripted `gh` says
/// the branch is absent — so the executor gets past its step 3 and asks policy,
/// which is the only place this document's word can be acted on.
#[test]
fn a_deployment_rule_in_the_document_refuses_the_effect_it_names() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(
        &s,
        &gh,
        &work,
        "\n[github.policy]\nensure_branch_published = \"deny\"\n",
    ));

    let out = publish(&s);
    let payload = payload_of(&out);
    let summary = summary_of(&payload);

    // The capability ran and its first effect was refused, which is a capability
    // failure and therefore row 11 — not row 2, which is the row a document
    // fiddle *declined to act on* exits with. The distinction is the point: the
    // deployment's rule was applied by the executor rather than by the CLI.
    assert_eq!(
        out.status.code(),
        Some(11),
        "a refused effect fails the run"
    );
    assert_eq!(payload["capability_executions"][0]["status"], "failed");
    assert!(
        summary.contains("policy denied"),
        "the document's rule must produce a policy refusal, got {payload}"
    );
    assert!(
        summary.contains("EnsureBranchPublished"),
        "the refusal must name the effect kind the rule was written for, got {payload}"
    );
}

/// **The discriminator.** The same world without the rule does not produce a
/// policy refusal.
///
/// Without this, the scenario above would pass on a build that refused every
/// effect for some unrelated reason, and on one that never consulted the
/// document at all.
#[test]
fn the_same_world_without_the_rule_is_not_refused_by_policy() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let payload = payload_of(&publish(&s));
    assert!(
        !summary_of(&payload).contains("policy denied"),
        "nothing in this document denies anything, and something did: {payload}"
    );
}

/// **The rule is per effect kind, not one switch.**
///
/// The scripted `gh` reports the branch as already published at this worktree's
/// `HEAD`, so the branch effect settles at step 3 and the run reaches the pull
/// request — where the *only* rule this document writes is waiting. A build that
/// mapped every kind to one value would have refused at the branch instead.
#[test]
fn a_rule_written_for_one_kind_refuses_that_kind_and_not_the_one_before_it() {
    let (s, work) = publishable();
    let gh = gh_answering_the_branch_is_published(s.dir(), &head_sha(&work));
    s.append_config(&forge_table(
        &s,
        &gh,
        &work,
        "\n[github.policy]\nensure_pull_request = \"deny\"\n",
    ));

    let payload = payload_of(&publish(&s));
    let summary = summary_of(&payload);

    assert!(
        summary.contains("policy denied") && summary.contains("EnsurePullRequest"),
        "the pull request is the kind the document names, got {payload}"
    );
    assert!(
        !summary.contains("EnsureBranchPublished"),
        "the branch carries no rule and must not have been refused, got {payload}"
    );
    // And the effect that did happen left its receipt, which is what proves the
    // run got past the branch rather than never having reached it.
    let evidence = payload["progress"][0]["evidence"].to_string();
    assert!(
        evidence.contains("effect:ensure_branch_published:"),
        "the branch effect must have produced a receipt, got {payload}"
    );
}

/// The credential is resolved on this arm and nowhere else, and it reaches no
/// observable surface — a second sentinel beside `capability_selection.rs`'s.
#[test]
fn the_forge_credential_reaches_no_surface() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let out = publish(&s);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // Asserted first, because everything below it would hold trivially of a run
    // that never resolved a credential at all. This one did: it built both
    // clients from the variable and executed.
    assert_eq!(
        payload_of(&out)["capability_executions"][0]["capability_id"],
        "publish_change",
        "the credential was resolved and the capability ran: {stdout}"
    );
    assert!(
        !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
        "the token reached a surface: {stdout}{stderr}"
    );
    for (path, bytes) in s.project_tree() {
        assert!(
            !String::from_utf8_lossy(&bytes).contains(SENTINEL),
            "the token was written to {path}"
        );
    }
}

/// An absent credential is refused by the name of the variable, before anything
/// is executed.
#[test]
fn a_publication_without_its_credential_names_the_variable() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "publish_change", "--json"])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(out.status.code(), Some(2), "stderr = {stderr}");
    assert!(
        stderr.contains(CREDENTIAL),
        "name the variable, not the value: {stderr}"
    );
    assert!(!s.report_dir().exists(), "a refused run published nothing");
}

/// Each key a publication cannot invent is refused by the name it is written
/// under, at the moment it is needed — the `workspace.fixture` precedent.
#[test]
fn a_publication_names_the_table_or_key_the_document_is_missing() {
    let cases: [(&str, &str); 3] = [
        ("", "[github]"),
        ("no-work", "github.work"),
        ("no-workflow", "github.workflow"),
    ];
    for (shape, expected) in cases {
        let (s, work) = publishable();
        let gh = gh_answering_nothing_exists(s.dir());
        match shape {
            "" => {}
            "no-work" => s.append_config(
                &forge_table(&s, &gh, &work, "")
                    .replace(&format!("work = {}\n", toml_path(&work)), ""),
            ),
            _ => s.append_config(
                &forge_table(&s, &gh, &work, "").replace("workflow = \"verify.yml\"\n", ""),
            ),
        }

        let out = publish(&s);
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(2), "{shape}: stderr = {stderr}");
        assert!(
            stderr.contains(expected),
            "{shape}: the refusal must name {expected}, got {stderr}"
        );
    }
}

/// **`inspect` builds nothing from the id, for every value of the flag.**
///
/// The document here describes no forge at all and no credential is exported,
/// so a build that constructed the capability from the id would refuse. M1 had
/// to repair exactly this once.
#[test]
fn inspect_names_the_publishing_capability_without_building_it() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    let out = s
        .command()
        .args([
            "inspect",
            INVOCATION_REF,
            "--config",
            s.config_path().to_str().unwrap(),
            "--capability",
            "publish_change",
            "--json",
        ])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(out.status.code(), Some(0), "stderr = {stderr}");
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        payload["next_action"]["execute"]["capability_id"], "publish_change",
        "inspect must name the capability it was asked about, got {payload}"
    );
    assert!(
        !s.report_dir().exists(),
        "inspect is read-only and published nothing"
    );
}
