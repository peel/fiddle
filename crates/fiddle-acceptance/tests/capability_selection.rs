mod support;

use support::Scenario;

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

const CREDENTIAL: &str = "LITELLM_API_KEY";

const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

const UNREACHABLE_GATEWAY: &str = "http://127.0.0.1:9/v1";

fn agentic_tables(scenario: &Scenario, base_url: &str) -> String {
    let fixture = scenario.write_fixture_repo();
    format!(
        "[agent]\n\
         model = \"a-model\"\n\
         base_url = \"{base_url}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n\
         max_turns = 1\n\
         max_tokens = 64\n\
         deadline = \"30s\"\n\
         tool_timeout = \"30s\"\n\
         \n\
         [workspace]\n\
         root = {}\n\
         fixture = {}\n\
         check = {{ program = \"true\" }}\n\
         command_timeout = \"30s\"\n",
        support::toml_string(&scenario.dir().join("workspaces")),
        support::toml_string(&fixture),
    )
}

fn repairable() -> Scenario {
    let scenario = Scenario::new();
    scenario.write_work_item(WORK_ID, "open");
    let tables = agentic_tables(&scenario, UNREACHABLE_GATEWAY);
    scenario.append_config(&tables);
    scenario
}

#[test]
fn the_selected_capability_is_the_one_that_runs() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"));

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "fixture_repair",
        "the run must execute the capability it was asked for, got {payload}"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "failed",
        "the gateway is unreachable, so the selected capability must have \
         tried and failed rather than something else having succeeded: {payload}"
    );
    assert_eq!(
        out.status.code(),
        Some(11),
        "a capability that tried and failed is retryable, stderr = {stderr}"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID),
        None,
        "nothing earned a correlation marker: `stub_mark` never ran, and the \
         repair never passed a check"
    );
}

#[test]
fn a_repair_files_its_progress_under_a_stage_of_its_own() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"));
    let bundle = s.read_bundle(&payload);

    assert_eq!(
        bundle["progress"][0]["capability_id"], "fixture_repair",
        "the bundle must record the capability that ran: {bundle}"
    );
    assert_eq!(
        bundle["progress"][0]["stage"], "repair",
        "a repair's progress must be filed under a stage describing the repair, \
         never under M0's `mark`: {bundle}"
    );
}

#[test]
fn an_absent_flag_still_selects_the_deterministic_capability() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    let payload = s.run_json(INVOCATION_REF, 0);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "stub_mark",
        "an unqualified run must select `stub_mark`, got {payload}"
    );
    assert_eq!(payload["outcome"], "completed");
}

#[test]
fn naming_the_deterministic_capability_selects_it() {
    let s = repairable();

    let payload = s.run_json_with(&["--capability", "stub_mark"], INVOCATION_REF, 0);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "stub_mark",
        "got {payload}"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "the deterministic capability writes the marker it always wrote"
    );
}

#[test]
fn an_absent_credential_is_a_configuration_error_naming_the_variable() {
    let s = repairable();
    let before = s.project_tree();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "an absent credential is invalid configuration, stdout = {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(CREDENTIAL),
        "the diagnostic must name the variable to set: {stderr}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a refused invocation must have changed nothing at all"
    );
}

#[test]
fn the_deterministic_capability_needs_no_credential_at_all() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .arg("--json")
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(0),
        "the deterministic capability must not resolve a credential, stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_document_with_no_agent_table_says_so() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    let out = s.run_raw_with(
        &["--capability", "fixture_repair", "--json"],
        INVOCATION_REF,
    );

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[agent]"),
        "the diagnostic must name the table to add: {stderr}"
    );
}

#[test]
fn a_document_with_no_workspace_table_says_so() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    s.append_config(&format!(
        "[agent]\nmodel = \"a-model\"\nbase_url = \"{UNREACHABLE_GATEWAY}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n"
    ));

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[workspace]"),
        "the diagnostic must name the table to add: {stderr}"
    );
}

#[test]
fn a_proposal_names_the_table_it_is_missing_and_a_complete_document_is_not_refused() {
    const FORGE: &str = "\n[github]\nrepo = \"peel/fiddle\"\nbase = \"main\"\n\
                         token = { env = \"FIDDLE_GITHUB_TOKEN\" }\n";
    const DECISION: &str = "\n[github.decision]\nauthorized = [505401]\n";

    for (extra, expected) in [("", Some("[github.decision]")), (DECISION, None)] {
        let s = Scenario::new();
        s.write_work_item(WORK_ID, "open");
        let tables = agentic_tables(&s, UNREACHABLE_GATEWAY);
        s.append_config(&tables);
        s.append_config(FORGE);
        s.append_config(extra);

        let out = s
            .run_command(INVOCATION_REF)
            .args(["--capability", "propose_change", "--json"])
            .env(CREDENTIAL, SENTINEL)
            .env("FIDDLE_GITHUB_TOKEN", SENTINEL)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();

        match expected {
            Some(table) => {
                assert_eq!(out.status.code(), Some(2), "stderr = {stderr}");
                assert!(
                    stderr.contains(table),
                    "the diagnostic must name the table to add: {stderr}"
                );
                assert!(
                    stderr.contains("propose_change"),
                    "and the capability that wanted it: {stderr}"
                );
            }
            None => {
                assert_ne!(
                    out.status.code(),
                    Some(2),
                    "a document naming every table propose_change needs must not be \
                     refused as a configuration error: stderr = {stderr}"
                );
                for table in ["[github.decision]", "[workspace]", "[agent]"] {
                    assert!(
                        !stderr.contains(table),
                        "the document carries {table} — a refusal naming it sends \
                         the reader to a line that is already correct: {stderr}"
                    );
                }
            }
        }

        assert!(
            !stderr.contains(SENTINEL) && !String::from_utf8_lossy(&out.stdout).contains(SENTINEL),
            "a credential reached a stream a caller reads: {stderr}"
        );
    }
}

#[test]
fn inspect_names_the_capability_a_run_with_the_same_flags_executes() {
    let s = repairable();

    let inspected = s.inspect_json_with(&["--capability", "fixture_repair"], INVOCATION_REF);
    let foreseen = &inspected["next_action"]["execute"]["capability_id"];
    assert_eq!(
        foreseen, "fixture_repair",
        "inspect must report the capability it was asked about: {inspected}"
    );

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let ran: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr = {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });

    assert_eq!(
        &ran["capability_executions"][0]["capability_id"], foreseen,
        "the capability inspect foresaw and the capability run executed must be \
         the same one: inspect said {foreseen}, run did {ran}"
    );
}

#[test]
fn an_unqualified_inspect_and_an_unqualified_run_agree_on_the_default() {
    let s = repairable();

    let inspected = s.inspect_json_with(&[], INVOCATION_REF);
    let ran = s.run_json(INVOCATION_REF, 0);

    assert_eq!(
        inspected["next_action"]["execute"]["capability_id"], "stub_mark",
        "the default plan is unchanged: {inspected}"
    );
    assert_eq!(
        inspected["next_action"]["execute"]["capability_id"],
        ran["capability_executions"][0]["capability_id"],
        "unqualified, the two commands must still name the same capability"
    );
}

#[test]
fn selecting_a_capability_leaves_inspect_read_only() {
    let s = repairable();
    let before = s.project_tree();

    let payload = s.inspect_json_with(&["--capability", "fixture_repair"], INVOCATION_REF);

    assert_eq!(
        payload["next_action"]["execute"]["capability_id"], "fixture_repair",
        "the scenario must have reached the derivation to prove anything: {payload}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "inspect is read-only, whichever capability it was asked about"
    );
    assert!(
        !s.report_dir().exists(),
        "inspect must publish no bundle, whichever capability it was asked about"
    );
}

#[test]
fn an_unknown_capability_is_refused_by_inspect_too() {
    let s = repairable();
    let before = s.project_tree();

    let out = s.inspect_raw_with(&["--capability", "nope", "--json"], INVOCATION_REF);

    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout = {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("stub_mark") && stderr.contains("fixture_repair"),
        "the diagnostic must list every id this build can execute: {stderr}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a rejected inspection provably did nothing"
    );
}

#[test]
fn an_unknown_capability_is_a_usage_error_listing_the_known_ids() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let before = s.project_tree();

    let out = s.run_raw_with(&["--capability", "nope"], INVOCATION_REF);

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("stub_mark") && stderr.contains("fixture_repair"),
        "the diagnostic must list every id this build can execute: {stderr}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a rejected invocation provably did nothing"
    );
}

#[test]
fn nothing_a_run_that_reaches_no_gateway_produces_contains_the_credential() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains(SENTINEL),
        "the payload echoed it: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL),
        "the diagnostic echoed it: {stderr}"
    );

    let leaked: Vec<String> = s
        .project_tree()
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(SENTINEL))
        .map(|(path, _)| path)
        .collect();
    assert!(
        leaked.is_empty(),
        "the credential was written to {leaked:?}"
    );
}

#[test]
fn a_credential_the_client_rejects_is_not_repeated_in_the_diagnostic() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, format!("{SENTINEL}\nx"))
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "a credential no client can carry is invalid configuration"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains(SENTINEL),
        "the refusal repeated the credential: {stderr}"
    );
    assert!(
        stderr.contains(CREDENTIAL),
        "the refusal must still name the variable, so an operator knows which \
         one to fix: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn an_interrupt_cancels_the_attempt_rather_than_killing_the_runner_under_it() {
    use std::io::Read;

    let gateway = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = gateway.local_addr().unwrap().port();

    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let tables = agentic_tables(&s, &format!("http://127.0.0.1:{port}/v1"));
    s.append_config(&tables);

    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS {
        command.env_remove(name);
    }
    let child = command
        .args([
            "run",
            INVOCATION_REF,
            "--config",
            s.config_path().to_str().unwrap(),
            "--capability",
            "fixture_repair",
            "--json",
        ])
        .env(CREDENTIAL, SENTINEL)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let (mut held, _) = gateway.accept().expect("the attempt must dial the gateway");

    interrupt(child.id());

    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(11),
        "a cancelled attempt did not do the work and repeating it may well \
         succeed, stdout = {stdout} stderr = {stderr}"
    );

    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"));
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a cancelled run must conclude retryably: {payload}"));
    assert!(
        reason.contains("cancelled"),
        "the run must conclude that it was cancelled rather than that it \
         failed: {reason}"
    );
    assert!(
        payload["report"].is_string(),
        "an interrupted run still records what it concluded: {payload}"
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
        "an interrupted attempt must still take its worktree down with it"
    );

    let mut nothing = [0u8; 1];
    let _ = held.read(&mut nothing);
}

#[cfg(unix)]
fn interrupt(pid: u32) {
    let status = std::process::Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .unwrap();
    assert!(status.success(), "could not interrupt process {pid}");
}

const COUNT_WORDS: [&str; 9] = [
    "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
];

fn advertised_capabilities() -> Vec<String> {
    let scenario = Scenario::new();
    let out = scenario.run_raw_with(&["--capability", "not-a-capability"], INVOCATION_REF);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown id is invalid input; stderr = {stderr}"
    );
    let (_, listed) = stderr.split_once("can execute:").unwrap_or_else(|| {
        panic!("the diagnostic must list what this build can execute: {stderr}")
    });
    let ids: Vec<String> = listed
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|token| {
            !token.is_empty()
                && token
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .map(str::to_string)
        .collect();
    assert!(
        ids.contains(&"stub_mark".to_string()) && ids.len() >= 2,
        "the parsed list must be the list the binary printed, got {ids:?} from {stderr}"
    );
    ids
}

#[test]
fn the_system_document_names_every_capability_this_build_registers() {
    let ids = advertised_capabilities();
    let document = std::fs::read_to_string(support::repo_root().join("docs/technical/SYSTEM.md"))
        .expect("the system document is part of the repository");
    let census = document
        .lines()
        .find(|line| line.contains("capabilities are registered"))
        .unwrap_or_else(|| {
            panic!(
                "the system document must carry a capability census for this lane \
                 to check; nothing in it says `capabilities are registered`"
            )
        });

    for id in &ids {
        assert!(
            census.contains(id),
            "`{id}` is advertised by the binary and missing from the census in \
             docs/technical/SYSTEM.md: {census}"
        );
    }

    let expected = COUNT_WORDS
        .get(ids.len() - 2)
        .unwrap_or_else(|| panic!("{} capabilities is outside COUNT_WORDS", ids.len()));
    assert!(
        census.contains(&format!("{expected} capabilities are registered")),
        "the census must state the number of capabilities there are — {} — as \
         `{expected} capabilities are registered`: {census}",
        ids.len()
    );
}

const FORGE_TOKEN: &str = "FIDDLE_GITHUB_TOKEN";

fn mitigating_tables(scenario: &Scenario) -> String {
    let fixture = scenario.write_fixture_repo();
    format!(
        "[agent]\n\
         model = \"a-model\"\n\
         base_url = \"{UNREACHABLE_GATEWAY}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n\
         max_turns = 1\n\
         max_tokens = 64\n\
         deadline = \"30s\"\n\
         tool_timeout = \"30s\"\n\
         \n\
         [github]\n\
         repo = \"acme/icecube\"\n\
         base = \"main\"\n\
         token = {{ env = \"{FORGE_TOKEN}\" }}\n\
         config_dir = {config_dir}\n\
         timeout = \"30s\"\n\
         \n\
         [scanner]\n\
         cli = {{ program = \"wizcli\", args = [\"scan\"] }}\n\
         timeout = \"30s\"\n\
         \n\
         [orchestration.cve]\n\
         image = \"ghcr.io/acme/icecube:latest\"\n\
         \n\
         [workspace]\n\
         root = {root}\n\
         fixture = {fixture}\n\
         command_timeout = \"30s\"\n\
         \n\
         [[workspace.checks]]\n\
         program = \"true\"\n\
         args = []\n\
         success = \"exit-zero\"\n",
        config_dir = support::toml_string(&scenario.dir().join("gh-config")),
        root = support::toml_string(&scenario.dir().join("workspaces")),
        fixture = support::toml_string(&fixture),
    )
}

fn mitigating() -> Scenario {
    let scenario = Scenario::new();
    let tables = mitigating_tables(&scenario);
    scenario.append_config(&tables);
    scenario
}

#[test]
fn an_absent_forge_credential_names_the_forge_and_not_the_model() {
    let s = mitigating();

    let out = s
        .run_command("cve")
        .args(["--capability", "cve_mitigate", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .env_remove(FORGE_TOKEN)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(out.status.code(), Some(2), "stderr = {stderr}");
    assert!(
        stderr.contains(FORGE_TOKEN),
        "the refusal must name the variable to export: {stderr}"
    );
    assert!(
        stderr.contains("forge credential") && stderr.contains("[github]"),
        "a forge credential is the forge's, and `[github]` is where it is \
         named: {stderr}"
    );
    assert!(
        !stderr.contains("model"),
        "the forge credential must not be described as the model's: {stderr}"
    );
}
