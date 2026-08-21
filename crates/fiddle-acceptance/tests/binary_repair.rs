mod support;

use support::{a_real_repair, refused, Reply, Scenario, StubGateway};

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

const CREDENTIAL: &str = "LITELLM_API_KEY";

const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

fn a_refusal_quoting_the_credential() -> Vec<Reply> {
    vec![refused(
        401,
        "Unauthorized",
        serde_json::json!({
            "error": {
                "message": format!(
                    "Incorrect API key provided: {SENTINEL}. \
                     You can find your API key at https://example.invalid/keys."
                ),
                "type": "invalid_request_error",
                "param": null,
                "code": "invalid_api_key",
            }
        }),
    )]
}

fn scenario(gateway: &StubGateway, max_turns: usize) -> Scenario {
    scenario_checked(gateway, max_turns, PASSES_ONCE_REPAIRED)
}

const PASSES_ONCE_REPAIRED: &str =
    "{ program = \"grep\", args = [\"-q\", \"len - 1\", \"src/lib.rs\"] }";

fn scenario_checked(gateway: &StubGateway, max_turns: usize, check: &str) -> Scenario {
    scenario_endpoint(&format!("\"{}\"", gateway.base_url()), max_turns, check)
}

const ENDPOINT: &str = "FIDDLE_MODEL_BASE_URL";

fn a_named_endpoint() -> String {
    format!("{{ env = \"{ENDPOINT}\" }}")
}

fn scenario_endpoint(base_url: &str, max_turns: usize, check: &str) -> Scenario {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let fixture = s.write_fixture_repo();
    s.append_config(&format!(
        "[agent]\n\
         model = \"a-model\"\n\
         base_url = {base_url}\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n\
         max_turns = {max_turns}\n\
         max_tokens = 512\n\
         max_changed_files = 4\n\
         deadline = \"300s\"\n\
         tool_timeout = \"300s\"\n\
         \n\
         [workspace]\n\
         root = {}\n\
         fixture = {}\n\
         check = {check}\n\
         command_timeout = \"300s\"\n",
        support::toml_string(&s.dir().join("workspaces")),
        support::toml_string(&fixture),
    ));
    s
}

fn repair(s: &Scenario) -> std::process::Output {
    s.run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap()
}

fn payload(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"))
}

#[test]
fn the_binary_drives_a_repair_that_passes_its_check_and_records_the_marker() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a repair whose check passed completed, payload = {payload} stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");
    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "fixture_repair",
        "{payload}"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "completed",
        "{payload}"
    );
    assert_eq!(
        payload["next_action"],
        serde_json::json!("complete"),
        "the run must report the world it left behind: {payload}"
    );
    assert_eq!(
        gateway.served(),
        2,
        "the binary must have taken both scripted turns through the real \
         gateway client"
    );

    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "a repair that passed its check accounts for the work"
    );

    let bundle = s.read_bundle(&payload);
    assert_eq!(bundle["progress"][0]["stage"], "repair", "{bundle}");
    assert_eq!(bundle["progress"][0]["status"], "completed", "{bundle}");
    let evidence = bundle["capability_executions"][0]["evidence"][0]
        .as_str()
        .unwrap_or_else(|| panic!("a completed repair earns an evidence reference: {bundle}"));
    assert!(
        evidence.starts_with("repair:1:"),
        "the evidence must name what git saw change — one file — rather than \
         what the model claimed: {evidence}"
    );

    let workspaces = s.dir().join("workspaces");
    assert!(
        workspaces.exists(),
        "the attempt never prepared a workspace, so nothing was proven about \
         tearing one down"
    );
    assert_eq!(
        support::walkdir_dirs(&workspaces),
        Vec::<std::path::PathBuf>::new(),
        "an ephemeral worktree must not outlive the attempt that made it"
    );
    assert_eq!(
        std::fs::read_to_string(s.dir().join("fixture/src/lib.rs")).unwrap(),
        support::BROKEN_FIXTURE,
        "the repository under repair is branched from, never written to"
    );
}

#[test]
fn a_document_that_names_its_endpoint_drives_the_same_repair_through_the_same_gateway() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario_endpoint(&a_named_endpoint(), 4, PASSES_ONCE_REPAIRED);

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .env(ENDPOINT, gateway.base_url())
        .output()
        .unwrap();
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a named endpoint reaches the same gateway a written one does, \
         payload = {payload} stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");
    assert_eq!(
        gateway.served(),
        2,
        "the binary must have taken both scripted turns through the endpoint \
         the variable named"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "a repair that passed its check accounts for the work"
    );
}

#[test]
fn a_named_endpoint_that_resolves_to_nothing_refuses_and_reaches_no_gateway() {
    for exported in [None, Some(""), Some("   ")] {
        let gateway = StubGateway::serving(a_real_repair());
        let s = scenario_endpoint(&a_named_endpoint(), 4, PASSES_ONCE_REPAIRED);

        let mut command = s.run_command(INVOCATION_REF);
        command
            .args(["--capability", "fixture_repair", "--json"])
            .env(CREDENTIAL, SENTINEL);
        match exported {
            Some(value) => command.env(ENDPOINT, value),
            None => command.env_remove(ENDPOINT),
        };
        let out = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();

        assert_eq!(
            out.status.code(),
            Some(2),
            "`{exported:?}` is not an endpoint, and the run must refuse rather \
             than reach a default: stderr = {stderr}"
        );
        assert!(
            stderr.contains(ENDPOINT) && stderr.contains("agent.base_url"),
            "the refusal must name the variable to export and the key that \
             names it: {stderr}"
        );
        assert_eq!(
            gateway.served(),
            0,
            "a document with no endpoint must reach no gateway"
        );
        assert_eq!(
            s.read_change_marker(WORK_ID),
            None,
            "a refused run accounts for no work"
        );
    }
}

#[test]
fn the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a repair whose check passed completed, payload = {payload} stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bundle = s.read_bundle(&payload);
    let attempt_id = bundle["attempt_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the bundle must record its attempt: {bundle}"));
    let evidence = bundle["capability_executions"][0]["evidence"][0]
        .as_str()
        .unwrap_or_else(|| panic!("a completed repair earns an evidence reference: {bundle}"));

    assert_eq!(
        evidence,
        format!("repair:1:{attempt_id}"),
        "the evidence names an attempt, so it must name *this* one — the bundle \
         a reader holding this reference would go and open"
    );

    let reported = payload["report"]
        .as_str()
        .unwrap_or_else(|| panic!("the run payload must name its bundle: {payload}"));
    assert!(
        reported.contains(attempt_id),
        "the published path must be the one the reference leads to, got \
         {reported} for attempt {attempt_id}"
    );
}

#[test]
fn a_turn_budget_of_one_stops_the_same_repair_before_it_earns_anything() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 1);

    let out = repair(&s);
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(11),
        "an attempt stopped by a bound did not do the work, and repeating it \
         under a larger bound may well succeed: payload = {payload}"
    );
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a bounded attempt concludes retryably: {payload}"));
    assert!(
        reason.contains("turn budget of 1"),
        "the run must name the bound the document set, so an operator knows \
         which number to raise: {reason}"
    );
    assert_eq!(
        gateway.served(),
        1,
        "one turn was configured, so exactly one completion may be requested"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID),
        None,
        "no check ever ran, so nothing was earned"
    );
}

#[test]
fn a_gateway_refusal_never_reaches_what_the_run_publishes() {
    let gateway = StubGateway::serving(a_refusal_quoting_the_credential());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(
        gateway.served(),
        1,
        "the binary must have dialled the gateway and been refused, or this \
         scenario proves nothing: payload = {payload} stderr = {stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(11),
        "a refused provider call did not do the work, and repeating it may well \
         succeed: payload = {payload} stderr = {stderr}"
    );

    assert!(
        !stdout.contains(SENTINEL),
        "the payload republished the gateway's copy of the credential: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL),
        "the diagnostic republished the gateway's copy of the credential: {stderr}"
    );
    let leaked: Vec<String> = s
        .project_tree()
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(SENTINEL))
        .map(|(path, _)| path)
        .collect();
    assert!(
        leaked.is_empty(),
        "the gateway's copy of the credential was written to {leaked:?}"
    );

    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a refused provider call concludes retryably: {payload}"));
    assert!(
        reason.contains("401"),
        "the run must still name what the gateway did, so an operator knows \
         whether to fix a key or wait out a rate limit: {reason}"
    );
    assert!(
        !reason.contains("Incorrect API key provided"),
        "the response body is authored outside this process and must not be \
         quoted at all: {reason}"
    );
}

const PUBLISHED_TEXT_LIMIT: usize = 2048;

const A_LOUD_FAILING_CHECK: &str = "{ program = \"sh\", args = [\"-c\", \
     \"pwd >&2; i=0; while [ $i -lt 400 ]; do printf '0123456789' >&2; \
     i=$((i+1)); done; exit 1\"] }";

#[test]
fn a_failing_checks_output_is_bounded_and_names_no_workspace_path() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario_checked(&gateway, 4, A_LOUD_FAILING_CHECK);

    let out = repair(&s);
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(11),
        "the check exited non-zero, so nothing was earned: payload = {payload} \
         stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a failing check concludes retryably: {payload}"));
    let bundle = s.read_bundle(&payload);
    let summary = bundle["progress"][0]["summary"]
        .as_str()
        .unwrap_or_else(|| panic!("a failed execution still records progress: {bundle}"));

    let workspaces = s.dir().join("workspaces");
    let spellings = [
        workspaces.display().to_string(),
        std::fs::canonicalize(&workspaces)
            .unwrap_or_else(|_| workspaces.clone())
            .display()
            .to_string(),
    ];

    for (field, text) in [("reason", reason), ("summary", summary)] {
        for spelling in &spellings {
            assert!(
                !text.contains(spelling.as_str()),
                "the published `{field}` names the host's workspace path \
                 {spelling}"
            );
        }
        assert!(
            text.chars().count() <= PUBLISHED_TEXT_LIMIT,
            "the published `{field}` is {} characters, and the bound is \
             {PUBLISHED_TEXT_LIMIT}",
            text.chars().count()
        );
        assert!(
            text.contains("exited"),
            "an operator must still learn that the check is what refused this \
             run: {field} = {text}"
        );
    }

    assert_eq!(
        s.read_change_marker(WORK_ID),
        None,
        "the check failed, so nothing was earned"
    );
}

#[test]
fn the_serialized_request_offers_four_tools_and_carries_no_host_fact() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the run must complete, or the requests below are not the requests a \
         working attempt sends: payload = {payload} stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bodies = gateway.request_bodies();
    assert_eq!(
        bodies.len(),
        2,
        "both turns must have reached the socket, or there is nothing here to \
         inspect"
    );

    let mut roots = vec![s.dir().display().to_string()];
    if let Ok(canonical) = s.dir().canonicalize() {
        let canonical = canonical.display().to_string();
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }

    let mut constrained = 0;
    for (turn, body) in bodies.iter().enumerate() {
        let request: serde_json::Value = serde_json::from_str(body)
            .unwrap_or_else(|e| panic!("turn {turn} is not JSON ({e}): {body}"));

        assert!(
            body.contains("You are repairing one small Rust project"),
            "turn {turn} carries no preamble, so this is not the request the \
             agent sends: {body}"
        );

        let tools = request["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("turn {turn} advertises no tools: {request}"));
        let mut offered: Vec<&str> = tools
            .iter()
            .map(|tool| {
                tool["function"]["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("a tool with no name on turn {turn}: {tool}"))
            })
            .collect();
        offered.sort_unstable();
        assert_eq!(
            offered,
            ["list_files", "read_file", "run_check", "write_file"],
            "turn {turn} must offer the capability's four tools and nothing \
             else — see this test's note on the synthetic output tool that is \
             not here: {request}"
        );

        for tool in tools {
            let name = tool["function"]["name"].as_str().unwrap();
            let advertised = tool["function"].to_string();
            for banned in ["workspace", "cancel", "receipts", "\"/"] {
                assert!(
                    !advertised.contains(banned),
                    "`{banned}` reaches the advertised schema of `{name}` on \
                     turn {turn}: {advertised}"
                );
            }
        }

        if request.get("response_format").is_some() {
            constrained += 1;
            assert_eq!(
                request["response_format"]["json_schema"]["name"], "RepairReport",
                "the only structured-output constraint fiddle asks for is its \
                 own report: {request}"
            );
        }

        for root in &roots {
            assert!(
                !body.contains(root.as_str()),
                "turn {turn} shows the model the host path {root}: {body}"
            );
        }
        assert!(
            !body.contains(SENTINEL),
            "turn {turn} carries the credential in what the model is shown; it \
             belongs in the authorization header and nowhere else: {body}"
        );
    }

    assert_eq!(
        constrained, 1,
        "the native structured-output constraint belongs on the finalising \
         turn alone: a first turn carrying it is the shape measured to stop \
         this gateway calling tools at all, and every turn carrying none is a \
         report nothing validates"
    );
}
