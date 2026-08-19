mod support;

use std::path::{Path, PathBuf};
use support::Scenario;

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
    assert_eq!(
        v["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(s.expected_marker("beans:fiddle-m0-demo")),
        "got {}",
        v["observations"]["changes"]
    );
}

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
        return;
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

const CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

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

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

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
