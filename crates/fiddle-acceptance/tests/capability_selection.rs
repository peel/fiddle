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
    s.assert_tree_unchanged(
        &before,
        "a refused invocation must have changed nothing at all",
    );
}

#[test]
fn a_commit_in_a_fixture_repository_starts_no_maintenance_that_outlives_it() {
    let s = Scenario::new();
    let repo = s.write_fixture_repo();
    std::fs::write(repo.join("src/lib.rs"), support::REPAIRED_FIXTURE).unwrap();

    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qam",
            "a second commit",
        ])
        .current_dir(&repo)
        .env("GIT_TRACE", "1")
        .output()
        .unwrap();
    let trace = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "the second commit failed: {trace}");
    assert!(
        !trace.contains("maintenance"),
        "git ends a commit with `git maintenance run --auto --quiet --detach`, whose \
         detached grandchild creates `.git/objects/maintenance.lock` and removes it after \
         `git commit` has already exited. That lock sits inside the scenario directory, so \
         it lands in one byte-for-byte snapshot and not the next, and a walk can list it a \
         moment before it is gone. A fixture repository must therefore switch auto \
         maintenance off. The trace of this commit was:\n{trace}"
    );
}

#[test]
fn the_tree_difference_names_an_added_a_removed_and_a_changed_path_and_is_silent_otherwise() {
    let before = vec![
        ("kept".to_string(), b"same".to_vec()),
        ("edited".to_string(), b"one".to_vec()),
        ("gone".to_string(), b"four".to_vec()),
    ];
    let after = vec![
        ("kept".to_string(), b"same".to_vec()),
        ("edited".to_string(), b"eleven".to_vec()),
        ("fresh".to_string(), b"new".to_vec()),
    ];

    assert_eq!(
        support::tree_difference(&before, &before),
        Vec::<String>::new(),
        "a tree compared against itself must report no difference, or every caller of \
         assert_tree_unchanged fails for a reason that is not there"
    );
    assert_eq!(
        support::tree_difference(&before, &after),
        vec![
            "added `fresh` (3 bytes)".to_string(),
            "changed `edited` (3 bytes -> 6 bytes)".to_string(),
            "removed `gone` (4 bytes)".to_string(),
        ],
        "each kind of difference must be reported with the path it happened to, or a \
         failing snapshot comparison says only that something changed"
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
    s.assert_tree_unchanged(
        &before,
        "inspect is read-only, whichever capability it was asked about",
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
    s.assert_tree_unchanged(&before, "a rejected inspection provably did nothing");
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
    s.assert_tree_unchanged(&before, "a rejected invocation provably did nothing");
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

const TOIL_DOCUMENT: &str = "workflows/toil.toml";

const ONE_CHECK: &str = "version = 1\n\
                         name = \"toil\"\n\
                         stage = \"toil\"\n\
                         \n\
                         [[steps]]\n\
                         kind = \"check\"\n\
                         program = \"true\"\n\
                         args = []\n\
                         timeout_secs = 30\n";

fn toiling_tables(scenario: &Scenario, base_url: &str) -> String {
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
         [github]\n\
         repo = \"acme/icecube\"\n\
         base = \"main\"\n\
         token = {{ env = \"{FORGE_TOKEN}\" }}\n\
         config_dir = {config_dir}\n\
         timeout = \"30s\"\n\
         \n\
         [workspace]\n\
         root = {root}\n\
         fixture = {fixture}\n\
         check = {{ program = \"true\" }}\n\
         command_timeout = \"30s\"\n",
        config_dir = support::toml_string(&scenario.dir().join("gh-config")),
        root = support::toml_string(&scenario.dir().join("workspaces")),
        fixture = support::toml_string(&fixture),
    )
}

fn toiling() -> Scenario {
    toiling_against(UNREACHABLE_GATEWAY)
}

fn toiling_against(base_url: &str) -> Scenario {
    let scenario = Scenario::new();
    scenario.write_work_item(WORK_ID, "open");
    let tables = toiling_tables(&scenario, base_url);
    scenario.append_config(&tables);
    scenario
}

fn write_toil_prompt(scenario: &Scenario, name: &str, text: &str) {
    let path = scenario.dir().join("workflows/prompts").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn document_path(scenario: &Scenario) -> std::path::PathBuf {
    scenario.dir().join(TOIL_DOCUMENT)
}

fn write_toil_document(scenario: &Scenario, text: &str) -> std::path::PathBuf {
    let path = document_path(scenario);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn run_toil(scenario: &Scenario) -> std::process::Output {
    scenario
        .run_command(INVOCATION_REF)
        .args(["--capability", "toil", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .env(FORGE_TOKEN, "ghp-a-token-no-forge-would-honour")
        .output()
        .unwrap()
}

fn payload_of(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"))
}

fn squeezed(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{2502}')
        .collect()
}

fn refused_nothing_else_ran(scenario: &Scenario, out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(2),
        "a document this build cannot run is invalid input; stderr = {stderr}"
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "",
        "a refused run reports no payload: {stderr}"
    );
    assert_eq!(
        scenario.read_change_marker(WORK_ID),
        None,
        "no built-in capability may run in the document's place, and `stub_mark` \
         is the one that would leave a marker: {stderr}"
    );
    assert!(
        !scenario.report_dir().exists(),
        "a refused run publishes no bundle: {stderr}"
    );
    stderr
}

#[test]
fn an_absent_workflow_document_refuses_with_the_path_it_looked_for() {
    let s = toiling();
    let looked_for = document_path(&s);
    assert!(
        !looked_for.exists(),
        "this scenario is about a document that is not there"
    );

    let out = run_toil(&s);

    let stderr = refused_nothing_else_ran(&s, &out);
    assert!(
        squeezed(&stderr).contains(&squeezed(&looked_for.display().to_string())),
        "the refusal must name the path it looked for, so an operator can write \
         the document there: {stderr}"
    );
}

#[test]
fn a_workflow_document_that_is_there_reaches_the_capability_the_command_line_named() {
    let s = toiling();
    write_toil_document(&s, ONE_CHECK);

    let out = run_toil(&s);
    let payload = payload_of(&out);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "toil",
        "the run must execute the capability the flag named, and no built-in in \
         its place: {payload}"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "completed",
        "the document names one check that exits zero, so the workflow ran to its \
         end: {payload}"
    );
    assert_eq!(
        payload["progress"][0]["stage"], "toil",
        "a toil run files its progress under its own stage: {payload}"
    );
}

#[test]
fn inspect_and_run_agree_that_toil_is_the_capability_a_toil_run_executes() {
    let s = toiling();
    write_toil_document(&s, ONE_CHECK);

    let foreseen = s.inspect_json_with(&["--capability", "toil"], INVOCATION_REF);
    let ran = payload_of(&run_toil(&s));

    assert_eq!(
        foreseen["next_action"]["execute"]["capability_id"], "toil",
        "inspect must foresee the capability it was asked about: {foreseen}"
    );
    assert_eq!(
        foreseen["next_action"]["execute"]["capability_id"],
        ran["capability_executions"][0]["capability_id"],
        "the capability inspect foresaw and the capability run executed must be \
         the same one: inspect said {foreseen}, run did {ran}"
    );
}

#[test]
fn the_step_the_document_names_is_the_step_that_runs() {
    let passing = toiling();
    write_toil_document(&passing, ONE_CHECK);
    let completed = payload_of(&run_toil(&passing));

    let failing = toiling();
    write_toil_document(&failing, &ONE_CHECK.replace("\"true\"", "\"false\""));
    let out = run_toil(&failing);
    let failed = payload_of(&out);

    assert_eq!(
        completed["capability_executions"][0]["status"], "completed",
        "{completed}"
    );
    assert_eq!(
        failed["capability_executions"][0]["status"], "failed",
        "the only difference between the two documents is the program the check \
         step names, so a build that never reads the step reports the same status \
         twice: {failed}"
    );
    assert_eq!(
        failed["capability_executions"][0]["capability_id"], "toil",
        "the failing run is the workflow failing, not something else running: {failed}"
    );
}

#[test]
fn a_workflow_document_this_build_does_not_read_refuses_with_its_version() {
    let s = toiling();
    let path = write_toil_document(&s, &ONE_CHECK.replace("version = 1", "version = 2"));

    let out = run_toil(&s);

    let stderr = refused_nothing_else_ran(&s, &out);
    assert!(
        squeezed(&stderr).contains(&squeezed(&path.display().to_string())) && stderr.contains('2'),
        "the refusal must name the document and the version it carries: {stderr}"
    );
}

#[test]
fn a_workflow_document_with_no_step_refuses_rather_than_doing_no_work() {
    let s = toiling();
    let path = write_toil_document(
        &s,
        "version = 1\nname = \"toil\"\nstage = \"toil\"\nsteps = []\n",
    );

    let out = run_toil(&s);

    let stderr = refused_nothing_else_ran(&s, &out);
    assert!(
        squeezed(&stderr).contains(&squeezed(&path.display().to_string())),
        "the refusal must name the document: {stderr}"
    );
}

#[test]
fn a_document_naming_another_stage_refuses_rather_than_filing_progress_under_a_third() {
    let s = toiling();
    let path = write_toil_document(
        &s,
        &ONE_CHECK.replace("stage = \"toil\"", "stage = \"triage\""),
    );

    let out = run_toil(&s);

    let stderr = refused_nothing_else_ran(&s, &out);
    assert!(
        squeezed(&stderr).contains(&squeezed(&path.display().to_string()))
            && stderr.contains("triage")
            && stderr.contains("toil"),
        "the refusal must name the document, the stage it declares and the stage \
         this build files a toil run under: {stderr}"
    );
}

#[test]
fn a_step_whose_prompt_is_absent_refuses_with_the_prompt_path() {
    let s = toiling();
    let document = write_toil_document(
        &s,
        "version = 1\n\
         name = \"toil\"\n\
         stage = \"toil\"\n\
         \n\
         [[steps]]\n\
         kind = \"agent\"\n\
         prompt = \"nothing_wrote_this.md\"\n\
         max_turns = 1\n",
    );
    let prompt = s.dir().join("workflows/prompts/nothing_wrote_this.md");

    let out = run_toil(&s);

    let stderr = refused_nothing_else_ran(&s, &out);
    assert!(
        squeezed(&stderr).contains(&squeezed(&prompt.display().to_string())),
        "the refusal must name the prompt it could not read: {stderr}"
    );
    assert!(
        squeezed(&stderr).contains(&squeezed(&document.display().to_string())),
        "and the document that named it: {stderr}"
    );
}

#[test]
fn a_jira_invocation_defaults_to_toil_and_a_cve_invocation_still_defaults_to_mitigate() {
    let jira = support::StubJira::holding_the_issue();
    let s = toiling();
    s.append_config(&format!(
        "\n[jira]\n\
         site = \"https://icecube.atlassian.net\"\n\
         project = \"IDENT\"\n\
         user = {{ env = \"JIRA_USER_EMAIL\" }}\n\
         token = {{ env = \"JIRA_API_TOKEN\" }}\n\
         base_url = \"{}\"\n\
         timeout = \"30s\"\n",
        jira.base_url()
    ));

    let out = s
        .command()
        .args([
            "inspect",
            &format!("jira:{}", support::JIRA_ISSUE_KEY),
            "--json",
            "--config",
        ])
        .arg(s.config_path())
        .env("JIRA_USER_EMAIL", "nobody@example.com")
        .env("JIRA_API_TOKEN", "a-token-no-site-would-honour")
        .output()
        .unwrap();
    let inspected = payload_of(&out);

    assert_eq!(
        inspected["next_action"]["execute"]["capability_id"], "toil",
        "an unqualified jira invocation plans the toil workflow: {inspected}"
    );

    let sweeping = mitigating();
    let swept = sweeping.inspect_json_with(&[], "cve");
    assert_eq!(
        swept["next_action"]["execute"]["capability_id"], "cve_mitigate",
        "a cve invocation still plans the sweep: {swept}"
    );
}

const ONE_EVALUATION: &str = "version = 1\n\
                              name = \"toil\"\n\
                              stage = \"toil\"\n\
                              \n\
                              [[steps]]\n\
                              kind = \"evaluate\"\n\
                              prompt = \"change_evaluate.md\"\n\
                              max_turns = 2\n";

const A_FINDING: &str = "the diff changes a public signature the ticket did not name";

fn verdict_of(finding: Option<&str>) -> serde_json::Value {
    match finding {
        Some(finding) => serde_json::json!({ "verdict": "rejected", "findings": [finding] }),
        None => serde_json::json!({ "verdict": "accepted" }),
    }
}

fn a_run_the_judge(finding: Option<&str>) -> (support::StubGateway, serde_json::Value, i32) {
    let gateway = support::StubGateway::serving(vec![support::accepted(support::reports(
        verdict_of(finding),
    ))]);
    let s = toiling_against(&gateway.base_url());
    write_toil_document(&s, ONE_EVALUATION);
    write_toil_prompt(
        &s,
        "change_evaluate.md",
        "Judge the change this run found, and reply with the structured verdict.\n",
    );

    let out = run_toil(&s);
    let code = out.status.code().expect("the binary exited");
    (gateway, payload_of(&out), code)
}

#[test]
fn a_workflow_the_judge_rejects_exits_twelve_and_a_workflow_it_accepts_does_not() {
    let (rejecting, rejected, rejected_code) = a_run_the_judge(Some(A_FINDING));
    assert_eq!(
        rejecting.served(),
        1,
        "the verdict below has to have come off the wire: {rejected}"
    );
    assert_eq!(
        rejected["capability_executions"][0]["capability_id"], "toil",
        "{rejected}"
    );
    assert_eq!(
        rejected["capability_executions"][0]["status"], "rejected",
        "a judge that rejects stops the run as a rejection: {rejected}"
    );
    assert_eq!(
        rejected["outcome"]["rejected"]["findings"][0], A_FINDING,
        "the finding the judge named reaches the bundle: {rejected}"
    );
    assert_eq!(
        rejected_code, 12,
        "a rejected run exits 12, which is neither a failure at 20 nor a retry \
         at 11: {rejected}"
    );

    let (accepting, accepted, accepted_code) = a_run_the_judge(None);
    assert_eq!(accepting.served(), 1, "{accepted}");
    assert_eq!(
        accepted["capability_executions"][0]["status"], "completed",
        "the same document and the same steps, and only the verdict differs, so a \
         build that exits 12 whatever the judge said reds here: {accepted}"
    );
    assert_ne!(
        accepted_code, 12,
        "an accepted run is not a rejected one: {accepted}"
    );
}
