mod support;

use support::Scenario;

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

#[test]
fn m0_executable_skeleton_scenario() {
    let s = Scenario::new();

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

    let checked = s.config_check();
    assert_eq!(
        checked.status.code(),
        Some(0),
        "config check must accept the scenario's own document, stderr = {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let checked: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(
        checked["schema"], "fiddle.config_check.v0",
        "the config check payload must declare its schema, got {checked}"
    );
    assert_eq!(checked["status"], "valid");
    assert_eq!(checked["project"]["name"], support::PROJECT_NAME);

    let bad = s.write_config_variant(
        "bad.toml",
        &s.config_text()
            .replacen("[project]\n", "[project]\nnickname = \"nope\"\n", 1),
    );
    let rejected = s.config_check_raw(&bad);
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "an unknown configuration key must exit 2"
    );
    let diagnostic = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        diagnostic.contains("nickname"),
        "the diagnostic must name the offending key, got {diagnostic}"
    );
    assert!(
        diagnostic.contains("unknown field"),
        "the diagnostic must say why the key was rejected, got {diagnostic}"
    );
    assert!(
        diagnostic.contains("bad.toml:2"),
        "the diagnostic must point at the offending line, got {diagnostic}"
    );

    s.write_work_item(WORK_ID, "open");
    let before_inspect = s.stub_snapshot();

    let pre = s.inspect_json(INVOCATION_REF);

    assert_eq!(
        pre["schema"], "fiddle.inspect.v0",
        "the inspect payload must declare its schema, got {pre}"
    );
    assert_eq!(pre["invocation_ref"], INVOCATION_REF);
    assert_eq!(pre["scheme"], "beans");
    assert_eq!(
        pre["observations"]["work_item"]["available"]["value"]["status"],
        "open"
    );
    assert!(
        pre["observations"]["changes"]["available"].is_object()
            && pre["observations"]["changes"]["available"]["value"]["marker"].is_null(),
        "the change set must be observed as available and unmarked, got {}",
        pre["observations"]["changes"]
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

    s.hide_stub_root();
    let blocked = s.run_json(INVOCATION_REF, 20);
    s.restore_stub_root();

    assert_eq!(
        blocked["schema"], "fiddle.run.v0",
        "a failing run's payload must declare its schema too, got {blocked}"
    );
    assert!(
        blocked["capability_executions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a blocked run must execute nothing, got {}",
        blocked["capability_executions"]
    );
    assert!(
        blocked["progress"].as_array().unwrap().is_empty(),
        "nothing ran, so nothing can have made progress, got {}",
        blocked["progress"]
    );
    assert!(
        blocked["observations"]["work_item"]["unavailable"].is_object(),
        "an unreadable source must be reported unavailable, not empty, got {}",
        blocked["observations"]["work_item"]
    );
    assert!(
        blocked["outcome"]["failed"]["error"]
            .as_str()
            .is_some_and(|error| error.contains("unavailable")),
        "a blocked run must carry a typed failure naming the unavailable source, got {}",
        blocked["outcome"]
    );
    assert!(
        blocked["next_action"]["blocked"]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "a blocked run must derive a blocked next action with a reason, got {}",
        blocked["next_action"]
    );

    let first_raw = s.run_raw_with(&["--json"], INVOCATION_REF);
    assert_eq!(
        first_raw.status.code(),
        Some(0),
        "stderr = {}",
        String::from_utf8_lossy(&first_raw.stderr)
    );
    let first_stdout = String::from_utf8(first_raw.stdout).unwrap();
    assert!(
        first_stdout.starts_with("{\"schema\":\"fiddle.run.v0\""),
        "design §3.2 leads the run payload with its schema, got {first_stdout}"
    );
    let first: serde_json::Value = serde_json::from_str(&first_stdout).unwrap();

    assert_eq!(first["outcome"], "completed");
    assert_eq!(first["invocation_ref"], INVOCATION_REF);
    assert_eq!(
        first["next_action"],
        serde_json::json!("complete"),
        "a satisfied change set leaves nothing to do, got {}",
        first["next_action"]
    );
    assert_eq!(
        first["capability_executions"][0]["capability_id"], "stub_mark",
        "the first run must execute the capability inspect named, got {}",
        first["capability_executions"]
    );
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "the run must write this project and invocation's correlation key"
    );

    let b1 = s.read_bundle(&first);

    assert_eq!(b1["schema"], "fiddle.report.v0");
    let version = b1["fiddle"]["package_version"].as_str().unwrap_or_else(|| {
        panic!(
            "the bundle must identify the build that produced it, got {}",
            b1["fiddle"]
        )
    });
    assert!(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
        "package_version must be an x.y.z version, got {version:?}"
    );
    let revision = b1["fiddle"]["source_revision"].as_str().unwrap_or_else(|| {
        panic!(
            "the bundle must identify the source it was built from, got {}",
            b1["fiddle"]
        )
    });
    assert!(
        revision == "unknown"
            || (revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit())),
        "source_revision must be a 40-hex sha or the literal `unknown`, got {revision:?}"
    );
    assert_eq!(b1["invocation_ref"], INVOCATION_REF);
    assert_eq!(b1["work_ref"], serde_json::json!(INVOCATION_REF));
    assert!(
        b1["attempt_id"].as_str().is_some_and(|id| !id.is_empty()),
        "the bundle must record its attempt, got {}",
        b1["attempt_id"]
    );
    assert_eq!(b1["mode"], "unattended");
    assert_eq!(b1["outcome"], "completed");
    assert_eq!(b1["next_action"], serde_json::json!("complete"));
    assert_eq!(b1["capability_executions"][0]["capability_id"], "stub_mark");
    assert_eq!(b1["progress"][0]["capability_id"], "stub_mark");
    assert_eq!(
        b1["progress"][0]["stage"], "mark",
        "progress must name the observable stage, got {}",
        b1["progress"]
    );
    assert_eq!(
        b1["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(s.expected_marker(INVOCATION_REF)),
        "the bundle must record the world the run left behind, got {}",
        b1["observations"]["changes"]
    );

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
    assert!(
        b2["progress"].as_array().unwrap().is_empty(),
        "an empty execution list implies empty progress, got {}",
        b2["progress"]
    );
    assert_eq!(b2["outcome"], "completed");
    assert_ne!(
        b1["attempt_id"], b2["attempt_id"],
        "the second invocation must publish its own attempt"
    );
    assert_eq!(
        b1["work_ref"], b2["work_ref"],
        "work identity must be stable across attempts"
    );

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

    let before_traversal = s.project_tree();
    let traversing = s.run_raw_with(&["--json"], "beans:../../../pwned");
    assert_eq!(
        traversing.status.code(),
        Some(2),
        "a reference that is not an identifier is invalid input, stderr = {}",
        String::from_utf8_lossy(&traversing.stderr)
    );
    let refusal = String::from_utf8_lossy(&traversing.stderr);
    assert!(
        refusal.contains("ASCII letters, digits"),
        "the diagnostic must say what a value may be written with, got {refusal}"
    );
    assert!(
        traversing.stdout.is_empty(),
        "a refused reference must write nothing to stdout, got {}",
        String::from_utf8_lossy(&traversing.stdout)
    );
    assert_eq!(
        s.project_tree(),
        before_traversal,
        "a refused reference must create nothing — not under `<report.dir>`, not \
         under `<stub.root>`, and above all not beside them"
    );
    assert!(
        !s.dir().join("pwned").exists(),
        "the escape landed here; nothing may be created outside the configured roots"
    );
}
