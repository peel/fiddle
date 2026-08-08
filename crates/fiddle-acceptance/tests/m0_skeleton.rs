//! The M0 mandatory proof: one cumulative scenario, in order, through the
//! public CLI only.
//!
//! The other files in this package each isolate one property. This one is
//! deliberately *not* isolated: it is a single ordered walk through the whole
//! milestone — validate configuration, look before touching, run, read the
//! evidence the run published, then run again from a fresh process — sharing
//! one fixture project from the first step to the last. That ordering is the
//! claim. Five independent `#[test]` functions could each pass while the
//! sequence they describe does not work, because nothing would force the
//! bundle assertion to be about the run that just happened, or the second
//! invocation to see the world the first one left. Here every step observes
//! what its predecessor did.
//!
//! Everything is observed from outside the process: an exit code, a `--json`
//! payload, or a file on disk. Nothing calls a library function.
//!
//! The scenario is also **credential-free by construction**. `Scenario`
//! removes every name in [`support::CREDENTIAL_VARS`] from each subprocess it
//! launches, so the milestone cannot quietly come to depend on a secret that
//! happens to be exported on a developer's machine or defined by a CI runner.
//! The last step closes the other half of that guarantee: with those same
//! variables *present*, fiddle behaves identically, so it neither requires nor
//! consults them.

mod support;

use support::Scenario;

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

#[test]
fn m0_executable_skeleton_scenario() {
    let s = Scenario::new();

    // The list is pinned here, in the scenario itself, so shortening it in the
    // harness fails this test rather than silently weakening the guarantee the
    // milestone rests on.
    assert_eq!(
        support::CREDENTIAL_VARS,
        [
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "ANTHROPIC_API_KEY",
            "JIRA_API_TOKEN"
        ],
        "the M0 lane must stay credential-free"
    );

    // ---- 1. the configuration this whole scenario runs against is valid ----
    let checked = s.config_check();
    assert_eq!(
        checked.status.code(),
        Some(0),
        "config check must accept the scenario's own document, stderr = {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let checked: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(checked["status"], "valid");
    assert_eq!(checked["project"]["name"], support::PROJECT_NAME);

    // ---- 2. the fixture state is observable, unstarted, and unharmed by looking ----
    s.write_work_item(WORK_ID, "open");
    let before_inspect = s.stub_snapshot();

    let pre = s.inspect_json(INVOCATION_REF);

    assert_eq!(pre["invocation_ref"], INVOCATION_REF);
    assert_eq!(pre["scheme"], "beans");
    assert_eq!(
        pre["observations"]["work_item"]["available"]["value"]["status"],
        "open"
    );
    assert!(
        pre["assessment"]["not_started"].is_object(),
        "unstarted work must assess as not_started, got {}",
        pre["assessment"]
    );
    assert_eq!(pre["next_action"]["execute"]["capability_id"], "stub_mark");
    assert_eq!(
        s.stub_snapshot(),
        before_inspect,
        "inspect must not mutate fixture state"
    );
    assert!(
        !s.report_dir().exists(),
        "inspect must not publish a report bundle"
    );

    // ---- 3. the capability the inspection named executes, with a typed outcome ----
    let first = s.run_json(INVOCATION_REF, 0);

    assert_eq!(first["outcome"], "completed");
    assert_eq!(first["invocation_ref"], INVOCATION_REF);
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "the run must write this project and invocation's correlation key"
    );

    // ---- 4. the evidence bundle is read from disk, not from the process output ----
    let b1 = s.read_bundle(&first);

    assert_eq!(b1["schema"], "fiddle.report.v0");
    assert!(
        b1["fiddle"]["package_version"].is_string(),
        "the bundle must identify the build that produced it, got {}",
        b1["fiddle"]
    );
    assert!(
        b1["fiddle"]["source_revision"].is_string(),
        "the bundle must identify the source it was built from, got {}",
        b1["fiddle"]
    );
    assert_eq!(b1["invocation_ref"], INVOCATION_REF);
    assert_eq!(b1["work_ref"], serde_json::json!(INVOCATION_REF));
    assert!(b1["attempt_id"].is_string());
    // The bundle records the mode the run was *invoked* with, and these steps
    // pass no `--mode`, so it must be the default the CLI documents.
    assert_eq!(b1["mode"], "unattended");
    assert_eq!(b1["outcome"], "completed");
    assert_eq!(b1["next_action"], serde_json::json!("complete"));
    assert_eq!(b1["capability_executions"][0]["capability_id"], "stub_mark");
    assert_eq!(b1["progress"][0]["capability_id"], "stub_mark");
    assert_eq!(
        b1["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(s.expected_marker(INVOCATION_REF)),
        "the bundle must record the world the run left behind, got {}",
        b1["observations"]["changes"]
    );

    // ---- 5. a second, genuinely fresh process finds nothing left to do ----
    let after_first = s.stub_snapshot();
    let second = s.run_json(INVOCATION_REF, 0);
    let b2 = s.read_bundle(&second);

    assert_eq!(
        s.stub_snapshot(),
        after_first,
        "the second invocation must leave the fixture bytes identical"
    );
    assert_eq!(
        s.change_files(WORK_ID).len(),
        1,
        "exactly one marker file may exist, got {:?}",
        s.change_files(WORK_ID)
    );
    assert!(
        b2["capability_executions"].as_array().unwrap().is_empty(),
        "the second invocation must execute nothing, got {}",
        b2["capability_executions"]
    );
    assert_eq!(b2["outcome"], "completed");
    // Still a real attempt over the same work, not a replay of the first bundle.
    assert_ne!(
        b1["attempt_id"], b2["attempt_id"],
        "the second invocation must publish its own attempt"
    );
    assert_eq!(
        b1["work_ref"], b2["work_ref"],
        "work identity must be stable across attempts"
    );

    // ---- 6. credentials are neither required nor consulted ----
    //
    // Steps 1-5 ran with every credential variable removed, which shows fiddle
    // does not *need* one. Running the same command with those variables
    // present, and getting the same answer, shows it does not *use* one either.
    let with_credentials = s.run_raw_with_env(
        &support::CREDENTIAL_VARS.map(|name| (name, "poison-not-a-real-secret")),
        INVOCATION_REF,
    );
    assert_eq!(
        with_credentials.status.code(),
        Some(0),
        "M0 must behave the same whether or not credentials are present, stderr = {}",
        String::from_utf8_lossy(&with_credentials.stderr)
    );
    let third: serde_json::Value = serde_json::from_slice(&with_credentials.stdout).unwrap();
    assert_eq!(third["outcome"], "completed");
    assert!(
        third["capability_executions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a credentialed environment must not change what fiddle does, got {}",
        third["capability_executions"]
    );
    assert_eq!(
        s.stub_snapshot(),
        after_first,
        "and it must not change what fiddle writes"
    );
}
