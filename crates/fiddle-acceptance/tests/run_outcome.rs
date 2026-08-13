//! Black-box coverage of `fiddle run` — the first command that changes the
//! world.
//!
//! Everything here is asserted from outside the process: an exit code, a
//! `--json` payload, and the fixture file the run left behind. Each scenario
//! builds its own temporary project, so no test depends on another's state and
//! none of them touches the tracked fixtures.

mod support;

use std::path::{Path, PathBuf};
use support::Scenario;

/// The happy path of design §4.4: unstarted work derives `Execute`, the
/// capability writes the correlation key, and the run completes.
#[test]
fn run_executes_the_stub_capability_and_completes() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.run_json("beans:fiddle-m0-demo", 0);

    assert_eq!(v["outcome"], "completed");
    assert_eq!(
        v["capability_executions"][0]["capability_id"], "stub_mark",
        "got {}",
        v["capability_executions"]
    );
    assert_eq!(v["capability_executions"][0]["status"], "completed");

    let marker = s
        .read_change_marker("fiddle-m0-demo")
        .expect("capability must write the marker");
    assert_eq!(
        marker.len(),
        16,
        "correlation key must be 16 hex chars, got {marker:?}"
    );
    assert!(marker.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(
        marker,
        s.expected_marker("beans:fiddle-m0-demo"),
        "the marker written must be this project and invocation's correlation key"
    );
}

/// The fail-closed arm of design §4.3, asserted where it matters most: a world
/// fiddle cannot observe must not be acted on. Exit 20 is the numeric row of
/// the exit-code table, read from outside the process.
#[test]
fn run_on_an_unobservable_source_fails_closed_with_exit_20() {
    let s = Scenario::new();
    s.remove_stub_root();

    let v = s.run_json("beans:fiddle-m0-demo", 20);

    assert!(v["outcome"]["failed"].is_object(), "got {}", v["outcome"]);
    assert!(
        v["capability_executions"].as_array().unwrap().is_empty(),
        "a blocked derivation must never execute the capability"
    );
    assert!(
        v["progress"].as_array().unwrap().is_empty(),
        "nothing ran, so nothing can have made progress"
    );
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "a blocked run must leave no marker behind"
    );
}

/// A completed run must describe the state it left behind. The `execute` it
/// derived on entry is no longer true by the time it reports, and echoing it
/// would send the caller round the loop for work that is already done.
/// Design §4.7.
#[test]
fn run_reports_complete_after_a_successful_execution() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.run_json("beans:fiddle-m0-demo", 0);

    assert_eq!(
        v["next_action"],
        serde_json::json!("complete"),
        "a completed run must not advertise work still to do, got {}",
        v["next_action"]
    );
    // The observations reported must be the post-execution ones the action was
    // derived from — otherwise `complete` would rest on a view that does not
    // show the marker.
    assert_eq!(
        v["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(s.expected_marker("beans:fiddle-m0-demo")),
        "got {}",
        v["observations"]["changes"]
    );
}

/// The whole `run` surface design §4.5 documents, not a subset of it.
#[test]
fn run_accepts_the_documented_mode_and_capability_flags() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.run_json_with(
        &["--mode", "attended", "--capability", "stub_mark"],
        "beans:fiddle-m0-demo",
        0,
    );

    assert_eq!(v["outcome"], "completed");
    assert_eq!(
        v["mode"], "attended",
        "the bundle must record the mode it ran under"
    );

    // The other mode is accepted too, and is what a run defaults to.
    let d = Scenario::new();
    d.write_work_item("fiddle-m0-demo", "open");
    assert_eq!(
        d.run_json_with(&["--mode", "unattended"], "beans:fiddle-m0-demo", 0)["mode"],
        "unattended"
    );

    let o = Scenario::new();
    o.write_work_item("fiddle-m0-demo", "open");
    assert_eq!(
        o.run_json("beans:fiddle-m0-demo", 0)["mode"],
        "unattended",
        "omitting --mode must be the same as naming the default"
    );
}

/// An unknown capability id is a usage error, not a silent no-op: a run asked
/// to do something this build has never heard of, and that exited 0 having done
/// nothing, would be indistinguishable from a run that did the work.
#[test]
fn run_rejects_an_unknown_capability_id_rather_than_no_opping() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.run_raw_with(
        &["--capability", "not-a-capability"],
        "beans:fiddle-m0-demo",
    );

    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown capability id is a usage error"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("not-a-capability") && stderr.contains("stub_mark"),
        "diagnostic must name the bad value and the known ids: {stderr}"
    );
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "a rejected invocation must not have executed anything"
    );
}

/// A `--mode` fiddle does not know is rejected by the same row of the table,
/// and the diagnostic names the alternatives.
#[test]
fn run_rejects_an_unknown_mode() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.run_raw_with(&["--mode", "supervised"], "beans:fiddle-m0-demo");

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("supervised") && stderr.contains("attended"),
        "got {stderr}"
    );
    assert!(s.read_change_marker("fiddle-m0-demo").is_none());
}

/// Row `11` of the exit-code table, asserted numerically from outside the
/// process like the rest of them.
///
/// The world stays observable — the derivation has to reach `Execute` for the
/// capability to fail at all — so the failure is injected as a change directory
/// that can be listed but not written to. That is a Unix permission, hence the
/// gate; an identity that ignores permission bits makes the case unbuildable,
/// hence the early return.
#[cfg(unix)]
#[test]
fn a_capability_that_cannot_write_exits_11_and_records_the_failed_execution() {
    use std::os::unix::fs::PermissionsExt;

    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let changes = s.stub_root().join("changes");

    std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o500)).unwrap();
    let out = s.run_raw_with(&["--json"], "beans:fiddle-m0-demo");
    std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o755)).unwrap();

    if out.status.code() == Some(0) {
        return; // running with an identity that ignores the permission bits
    }

    assert_eq!(
        out.status.code(),
        Some(11),
        "stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["outcome"]["retryable"].is_object(),
        "got {}",
        v["outcome"]
    );
    assert_eq!(v["capability_executions"][0]["status"], "failed");
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "a failed execution must leave no marker and no debris"
    );
}

// ---------------------------------------------------------------------------
// Row 10, which had no producer until M3
// ---------------------------------------------------------------------------
//
// **What these scenarios prove, and what they do not.**
//
// They prove that exit 10 is reachable from the compiled binary, that a run
// reaching it is neither 11 nor 20, and that both renderings of it agree. That
// is the whole of the row's contract, and none of it was true before: `10` was
// a `match` arm in `exit_code_for` with nothing on the far side of it, unit
// tested since M0 against a hand-built `RunOutcome` and never once produced by
// a run.
//
// They do not prove that a *conversation* is named, because on this path there
// is none. `propose_change` — the capability that publishes a question and
// hands back a `CapabilityError::AwaitingDecision` carrying the conversation it
// published on — is a later bean, so the honest producer available here is a
// deployment document that says a person must decide before the branch is
// published. `EffectError::HumanDecisionRequired` is the *other* condition ADR
// 016 assigned to this row, so the route is not a stand-in for the real one; it
// is one of the two things the row is for. The conversation half is asserted
// one layer down, in `orchestration.rs`'s
// `a_capability_awaiting_a_decision_suspends_rather_than_failing_or_retrying`,
// against a capability that returns the variant carrying an `InteractionRef`.
//
// Everything below is offline. `[github] cli = { program, args }` is the
// product seam an operator uses to pin or wrap `gh`, and it points at a shell
// script; the exported token authenticates nothing.

/// The variable the documents below name. Never a value.
const CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

/// A `gh` that answers every request `404`, so the branch this run would publish
/// does not exist yet and the executor gets past its postcondition read to the
/// point where the document's rule is consulted.
///
/// Exit 1 rather than 0 because that is what `gh` exits with on a 404, and the
/// adapter reads the status line rather than the exit code.
#[cfg(unix)]
fn gh_answering_nothing_exists(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("empty-gh");
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf 'HTTP/1.1 404 Not Found\\r\\n\\r\\n{\"message\":\"Not Found\"}\\n'\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A path as a TOML string, escaped rather than pasted.
fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// A scenario whose deployment says a person must decide before the first
/// effect may happen, and whose world is otherwise ordinary.
///
/// `config_dir` is written down rather than defaulted: the default is relative
/// to the working directory, so a test taking it would leave a scratch
/// directory inside the package it was run from.
#[cfg(unix)]
fn awaiting_a_decision() -> Scenario {
    let scenario = Scenario::new();
    scenario.write_work_item("fiddle-m0-demo", "open");
    let work = scenario.write_fixture_repo();
    let gh = gh_answering_nothing_exists(scenario.dir());
    scenario.append_config(&format!(
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
         \n[github.policy]\n\
         ensure_branch_published = \"require_human\"\n",
        gh = toml_path(&gh),
        work = toml_path(&work),
        config_dir = toml_path(&scenario.dir().join("gh-config")),
    ));
    scenario
}

/// `fiddle run … --capability publish_change`, with the credential exported and
/// the exit code left unjudged.
#[cfg(unix)]
fn suspending_run(scenario: &Scenario, extra: &[&str]) -> std::process::Output {
    scenario
        .run_command("beans:fiddle-m0-demo")
        .args(["--capability", "publish_change"])
        .args(extra)
        .env(CREDENTIAL, "ghp_sentinel_authenticates_nothing_4b19")
        .output()
        .unwrap()
}

/// **The row that had no producer, driven by a run rather than by a unit test
/// of the mapping function.**
///
/// `exit_code_for`'s `Suspended => 10` arm has been unit tested since M0
/// against a `RunOutcome` built by hand, and the row still had nothing on the
/// far side of it. This is the far side.
#[cfg(unix)]
#[test]
fn a_run_awaiting_a_decision_exits_ten_and_says_what_it_waits_for() {
    let s = awaiting_a_decision();

    let out = suspending_run(&s, &["--json"]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        out.status.code(),
        Some(10),
        "stdout {stdout}\nstderr {stderr}"
    );

    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let reason = v["outcome"]["suspended"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a suspended run carries a reason: {}", v["outcome"]));
    assert!(
        reason.contains("awaiting"),
        "the reason must say the run is waiting rather than that it failed: {reason}"
    );
    assert!(
        reason.contains("EnsureBranchPublished"),
        "and must name what it is waiting about: {reason}"
    );

    // The run executed, and the bundle must not contradict its own outcome. It
    // said `failed` on every capability `Err` until this row existed, which
    // would have had a reader of the progress entry conclude the opposite of
    // what the exit code told them.
    assert_eq!(
        v["capability_executions"][0]["capability_id"],
        "publish_change"
    );
    assert_eq!(v["capability_executions"][0]["status"], "awaiting");
    let progress = v["progress"].as_array().unwrap();
    assert_eq!(progress.len(), 1, "one capability per run: {progress:?}");
    assert_eq!(progress[0]["stage"], "publish");
    assert_eq!(progress[0]["status"], "awaiting");
    assert!(
        progress[0]["summary"]
            .as_str()
            .unwrap()
            .contains("awaiting"),
        "the published bundle must say the same thing the outcome does: {progress:?}"
    );
}

/// **Not retryable, and not failed, and this is the point of the row.**
///
/// Automation retrying on 11 would loop on a question nobody has answered yet,
/// and automation treating 20 as final would abandon a run that is merely
/// waiting. Both exclusions are asserted rather than implied by the equality,
/// because a build that regressed either one regresses a caller's behaviour and
/// not only a number.
#[cfg(unix)]
#[test]
fn a_suspended_run_is_neither_retryable_nor_failed() {
    let s = awaiting_a_decision();

    let out = suspending_run(&s, &[]);

    assert_ne!(
        out.status.code(),
        Some(11),
        "a repeat asks the same question"
    );
    assert_ne!(
        out.status.code(),
        Some(20),
        "an answer would finish this run"
    );
    assert_eq!(out.status.code(), Some(10));
}

/// **The discriminator.** The same world, the same capability, the same
/// scripted `gh` — without the rule the document is the one M2 already gates,
/// and the run does not suspend.
///
/// Without this the scenario above would pass on a build that suspended every
/// run, or that suspended for some reason unrelated to the document.
#[cfg(unix)]
#[test]
fn the_same_world_without_the_rule_does_not_suspend() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let work = s.write_fixture_repo();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&format!(
        "[github]\n\
         repo = \"peel/fiddle-effects-acceptance\"\n\
         base = \"main\"\n\
         token = {{ env = \"{CREDENTIAL}\" }}\n\
         cli = {{ program = {gh}, args = [] }}\n\
         git = \"git\"\n\
         work = {work}\n\
         workflow = \"verify.yml\"\n\
         config_dir = {config_dir}\n\
         timeout = \"120s\"\n",
        gh = toml_path(&gh),
        work = toml_path(&work),
        config_dir = toml_path(&s.dir().join("gh-config")),
    ));

    let out = suspending_run(&s, &[]);

    assert_ne!(
        out.status.code(),
        Some(10),
        "nothing in this document asks a person anything: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The human rendering says the same thing the payload says. Two renderings of
/// one outcome that disagree is a defect an operator finds at the worst moment
/// — and the outcome line is the only place a reader at a terminal learns that
/// waiting is what happened.
#[cfg(unix)]
#[test]
fn the_human_rendering_says_the_run_is_waiting_too() {
    let s = awaiting_a_decision();

    let out = suspending_run(&s, &[]);
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert_eq!(out.status.code(), Some(10), "got {stdout}");
    assert!(
        stdout.contains("outcome     = suspended"),
        "the outcome line must name the row: {stdout}"
    );
    assert!(
        stdout.to_lowercase().contains("awaiting"),
        "and must say what the run is waiting for: {stdout}"
    );
    assert!(
        stdout.contains("publish_change awaiting"),
        "the execution line must not say `failed` about a run that is waiting: {stdout}"
    );
}

/// A reader at a terminal is entitled to the same conclusions the payload
/// carries, not only to the exit code.
#[test]
fn the_human_rendering_names_the_outcome_the_mode_and_what_ran() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.run_raw_with(&[], "beans:fiddle-m0-demo");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(stdout.contains("outcome     = completed"), "got {stdout}");
    assert!(stdout.contains("mode        = unattended"), "got {stdout}");
    assert!(stdout.contains("next action = complete"), "got {stdout}");
    assert!(stdout.contains("stub_mark completed"), "got {stdout}");
}
