mod support;

fn fixture() -> &'static str {
    "../../tests/fixtures/fiddle.toml"
}

#[test]
fn config_check_accepts_the_documented_fixture() {
    let out = support::fiddle_command()
        .args(["config", "check", "--config", fixture(), "--json"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "valid");
    assert_eq!(v["project"]["name"], "icecube");
}

const AGENTIC: &str = r#"[project]
name = "icecube"

[stub]
root = "."

[report]
dir = "."

[agent]
model = "claude-sonnet-5"
base_url = "https://litellm.firn.snplow.net/v1"
api_key = { env = "LITELLM_API_KEY" }
max_turns = 12

[workspace]
root = ".fiddle/workspaces"
isolation = "git-worktree"
"#;

const CREDENTIAL: &str = "LITELLM_API_KEY";

const SENTINEL: &str = "sk-sentinel-config-check-must-never-print-4b19";

fn check(text: &str) -> std::process::Output {
    check_with(text, &[])
}

fn check_with(text: &str, extra: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, text).unwrap();
    support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .args(extra)
        .env_remove(CREDENTIAL)
        .env_remove(FORGE_CREDENTIAL)
        .output()
        .unwrap()
}

fn checked(text: &str) -> serde_json::Value {
    let out = check_with(text, &["--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

#[test]
fn config_check_accepts_a_document_naming_an_agent_and_a_workspace() {
    let out = check(AGENTIC);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_check_reports_the_agent_table_it_accepted() {
    let agent = checked(AGENTIC)["agent"].clone();
    assert_eq!(agent["model"], "claude-sonnet-5", "{agent}");
    assert_eq!(
        agent["base_url"], "https://litellm.firn.snplow.net/v1",
        "{agent}"
    );
    assert_eq!(agent["api_key"]["env"], CREDENTIAL, "{agent}");
    assert_eq!(agent["max_turns"], 12, "{agent}");
    assert_eq!(agent["max_tokens"], 8192, "{agent}");
    assert_eq!(agent["max_changed_files"], 16, "{agent}");
    assert_eq!(agent["deadline"], "45m", "{agent}");
    assert_eq!(agent["tool_timeout"], "15m", "{agent}");
}

const ENDPOINT: &str = "FIDDLE_MODEL_BASE_URL";

const A_RESOLVED_ENDPOINT: &str = "https://gateway.resolved-from-the-environment.invalid/v1";

fn naming_the_endpoint() -> String {
    AGENTIC.replace(
        "base_url = \"https://litellm.firn.snplow.net/v1\"",
        &format!("base_url = {{ env = \"{ENDPOINT}\" }}"),
    )
}

#[test]
fn config_check_reports_the_variable_that_names_the_endpoint() {
    let out = check_with_env(
        &naming_the_endpoint(),
        &["--json"],
        &[(ENDPOINT, A_RESOLVED_ENDPOINT)],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        payload["agent"]["base_url"]["env"], ENDPOINT,
        "a named endpoint is reported under the key the document writes it as, \
         as `agent.api_key.env` already is: {stdout}"
    );
    assert!(
        !stdout.contains(A_RESOLVED_ENDPOINT),
        "the resolved endpoint must not be reported as though the document \
         wrote it: {stdout}"
    );
}

#[test]
fn the_plain_rendering_names_the_endpoint_variable_an_operator_goes_and_sets() {
    let out = check_with_env(
        &naming_the_endpoint(),
        &[],
        &[(ENDPOINT, A_RESOLVED_ENDPOINT)],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains(&format!("agent.base_url.env = {ENDPOINT}")),
        "an operator at a terminal cannot confirm which variable carries the \
         endpoint: {stdout}"
    );
    assert!(
        !stdout.contains(A_RESOLVED_ENDPOINT),
        "the resolved endpoint must not be rendered as a written value: {stdout}"
    );
}

#[test]
fn config_check_accepts_a_named_endpoint_that_nothing_exports() {
    let out = check(&naming_the_endpoint());
    assert_eq!(
        out.status.code(),
        Some(0),
        "the schema check reads the document, and the run reads the \
         environment: stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_check_reports_the_workspace_table_it_accepted() {
    let workspace = checked(AGENTIC)["workspace"].clone();
    assert_eq!(workspace["root"], ".fiddle/workspaces", "{workspace}");
    assert_eq!(workspace["isolation"], "git-worktree", "{workspace}");
    assert_eq!(workspace["command_timeout"], "15m", "{workspace}");
    assert_eq!(workspace["cleanup"], "always", "{workspace}");
    assert_eq!(workspace["fixture"], serde_json::Value::Null, "{workspace}");
    assert_eq!(workspace["check"], serde_json::Value::Null, "{workspace}");
}

#[test]
fn config_check_reports_the_fixture_under_repair_and_the_check_that_judges_it() {
    let workspace = checked(&AGENTIC.replace(
        "isolation = \"git-worktree\"",
        "isolation = \"git-worktree\"\nfixture = \"fixtures/m1-demo\"\n\
         check = { program = \"cargo\", args = [\"test\", \"--offline\"] }",
    ))["workspace"]
        .clone();
    assert_eq!(workspace["fixture"], "fixtures/m1-demo", "{workspace}");
    assert_eq!(workspace["check"]["program"], "cargo", "{workspace}");
    assert_eq!(
        workspace["check"]["args"],
        serde_json::json!(["test", "--offline"]),
        "{workspace}"
    );
}

#[test]
fn config_check_reports_the_attempt_bound_it_enforces_and_where_the_count_lives() {
    let document = AGENTIC.replace(
        "max_turns = 12",
        "max_turns = 12\nmax_capability_attempts = 5",
    );
    let agent = checked(&document)["agent"].clone();
    let bound = &agent["max_capability_attempts"];
    assert_eq!(bound["configured"], 5, "{agent}");
    assert!(
        bound.get("enforced").is_none(),
        "a document writing 5 gets 5, so no key may name a number it does not \
         get: {agent}"
    );
    assert_eq!(bound["status"], "enforced-per-pull-request", "{agent}");
    assert_eq!(
        bound["counted_in"], "pull-request-body",
        "the bound is spent across processes, so the surface has to say where \
         the count a person can edit is held: {agent}"
    );
    assert_eq!(
        bound["decision"], "037-the-attempt-bound-is-per-pull-request",
        "the surface must lead a reader to the decision: {agent}"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, &document).unwrap();
    let out = support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(0), "stdout: {stdout}");
    assert!(
        stdout.contains("agent.max_capability_attempts = 5")
            && stdout.contains("enforced per pull request")
            && stdout.contains("037-the-attempt-bound-is-per-pull-request"),
        "the plain rendering must say the same as the payload beside it: {stdout}"
    );
    assert!(
        !stdout.contains("not enforced"),
        "this document names no unenforced key, so the phrase must appear \
         nowhere in its rendering: {stdout}"
    );
}

#[test]
fn config_check_reports_the_credentials_variable_name_and_never_its_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, AGENTIC).unwrap();
    for extra in [vec!["--json"], vec![]] {
        let out = support::fiddle_command()
            .args(["config", "check", "--config", path.to_str().unwrap()])
            .args(&extra)
            .env(CREDENTIAL, SENTINEL)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(0), "{extra:?}: stderr = {stderr}");
        assert!(
            !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
            "{extra:?} resolved the credential and printed it: {stdout}{stderr}"
        );
        assert!(
            stdout.contains(CREDENTIAL),
            "{extra:?} must still name the variable, so an operator can confirm \
             which one the document points at: {stdout}"
        );
    }
}

#[test]
fn an_m0_shaped_document_produces_exactly_the_payload_it_always_did() {
    let payload =
        checked("[project]\nname = \"icecube\"\n\n[stub]\nroot = \".\"\n\n[report]\ndir = \".\"\n");
    assert_eq!(
        payload,
        serde_json::json!({
            "schema": "fiddle.config_check.v0",
            "status": "valid",
            "project": { "name": "icecube" },
            "stub": { "root": "." },
            "report": { "dir": "." },
        }),
        "a document describing no agent and no workspace must report exactly \
         what it always has"
    );
}

#[test]
fn config_check_refuses_a_credential_written_into_the_document() {
    let out = check(&AGENTIC.replace(
        r#"api_key = { env = "LITELLM_API_KEY" }"#,
        r#"api_key = "sk-literal-secret""#,
    ));
    assert_eq!(
        out.status.code(),
        Some(2),
        "a literal credential must be rejected"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("env"),
        "the diagnostic must say what the schema wanted instead, got: {stderr}"
    );
    assert!(
        stderr.contains("fiddle.toml:13"),
        "the diagnostic must point at the offending line, got: {stderr}"
    );
    assert!(
        !stderr.contains("sk-literal-secret"),
        "the diagnostic repeated the credential it was refusing: {stderr}"
    );
}

#[test]
fn config_check_rejects_an_unknown_key_inside_the_agent_table() {
    let out = check(&AGENTIC.replace("max_turns = 12", "temperature = 0.7"));
    assert_eq!(out.status.code(), Some(2), "unknown key must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("temperature") && stderr.contains("unknown field"),
        "the diagnostic must name the offending key and why, got: {stderr}"
    );
    assert!(
        stderr.contains("fiddle.toml:14"),
        "the diagnostic must point at the offending line, got: {stderr}"
    );
}

#[test]
fn config_check_rejects_an_unknown_key_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(
        &path,
        "[project]\nname = \"icecube\"\nnickname = \"nope\"\n\n[stub]\nroot = \".\"\n\n[report]\ndir = \".\"\n",
    )
    .unwrap();
    let out = support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "unknown key must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("nickname"),
        "diagnostic must name the offending key, got: {stderr}"
    );
    assert!(
        stderr.contains("nickname = \"nope\"") || stderr.contains("fiddle.toml:3"),
        "diagnostic must locate the key in the source, got: {stderr}"
    );
}

const FORGE_CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

const FORGE: &str = r#"[project]
name = "icecube"

[stub]
root = "."

[report]
dir = "."

[github]
repo = "peel/fiddle-effects-acceptance"
base = "main"
token = { env = "FIDDLE_GITHUB_TOKEN" }
"#;

#[test]
fn config_check_accepts_a_document_naming_a_forge() {
    let out = check(FORGE);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_check_refuses_a_forge_credential_written_into_the_document() {
    let out = check(&FORGE.replace(
        r#"token = { env = "FIDDLE_GITHUB_TOKEN" }"#,
        r#"token = "ghp_a_literal_secret""#,
    ));
    assert_eq!(out.status.code(), Some(2), "a literal token must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("env"),
        "the diagnostic must say what the schema wanted instead, got: {stderr}"
    );
    assert!(
        stderr.contains("fiddle.toml:13"),
        "the diagnostic must point at the offending line, got: {stderr}"
    );
    assert!(
        !stderr.contains("ghp_a_literal_secret"),
        "the refusal repeated the credential it was refusing: {stderr}"
    );
}

#[test]
fn config_check_rejects_an_unknown_key_inside_the_github_table() {
    let out = check(&FORGE.replace("base = \"main\"", "reviewers = [\"someone\"]"));
    assert_eq!(out.status.code(), Some(2), "unknown key must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("reviewers") && stderr.contains("unknown field"),
        "the diagnostic must name the offending key and why, got: {stderr}"
    );
    assert!(
        stderr.contains("fiddle.toml:12"),
        "the diagnostic must point at the offending line, got: {stderr}"
    );
}

#[test]
fn config_check_rejects_an_unknown_key_inside_the_policy_table() {
    let out = check(&format!(
        "{FORGE}\n[github.policy]\nensure_everything = \"deny\"\n"
    ));
    assert_eq!(out.status.code(), Some(2), "unknown key must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("ensure_everything") && stderr.contains("unknown field"),
        "the diagnostic must name the offending key and why, got: {stderr}"
    );
}

const DECIDER: u64 = 505_401;

const DECISION_STATUS: &str = "accepted-not-enforced";

const DECISION_STATUS_PHRASE: &str = "accepted, not enforced";

fn with_decision(body: &str) -> String {
    format!("{FORGE}\n[github.decision]\n{body}\n")
}

#[test]
fn config_check_reports_the_decision_channel_and_its_authorized_set() {
    let document = with_decision(&format!("authorized = [{DECIDER}]"));
    let github = checked(&document)["github"].clone();
    let decision = &github["decision"];
    assert_eq!(
        decision["authorized"],
        serde_json::json!([DECIDER]),
        "{github}"
    );
    assert_eq!(decision["matched_on"], "numeric_user_id", "{github}");
    assert_eq!(decision["status"], DECISION_STATUS, "{github}");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, &document).unwrap();
    let out = support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .env_remove(FORGE_CREDENTIAL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(0), "stdout: {stdout}");
    assert!(
        stdout.contains(&format!("github.decision.authorized = {DECIDER}"))
            && stdout.contains("numeric_user_id")
            && stdout.contains(DECISION_STATUS_PHRASE),
        "the plain rendering must disclose the channel under the key the document \
         writes it under: {stdout}"
    );
}

#[test]
fn config_check_refuses_a_decision_table_that_names_nobody() {
    let out = check(&with_decision("authorized = []"));
    assert_eq!(out.status.code(), Some(2), "an empty list must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("authorized") && stderr.contains("nobody"),
        "the diagnostic must name the key and say why, got: {stderr}"
    );
    assert!(
        stderr.contains("fiddle.toml:15"),
        "the diagnostic must point at the table it is about, got: {stderr}"
    );
}

#[test]
fn config_check_refuses_an_approver_named_by_login() {
    let out = check(&with_decision(r#"authorized = ["peel"]"#));
    assert_eq!(out.status.code(), Some(2), "a login must exit 2");
}

#[test]
fn config_check_rejects_an_unknown_key_inside_the_decision_table() {
    let out = check(&with_decision(&format!(
        "authorized = [{DECIDER}]\nauthorised = [42]"
    )));
    assert_eq!(out.status.code(), Some(2), "unknown key must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("authorised") && stderr.contains("unknown field"),
        "the diagnostic must name the offending key and why, got: {stderr}"
    );
    assert_eq!(
        checked(&with_decision(&format!("authorized = [{DECIDER}]")))["github"]["decision"]
            ["authorized"],
        serde_json::json!([DECIDER])
    );
}

#[test]
fn config_check_rejects_a_deployment_rule_it_does_not_know() {
    let out = check(&format!(
        "{FORGE}\n[github.policy]\nensure_pull_request = \"probably\"\n"
    ));
    assert_eq!(out.status.code(), Some(2), "an unknown rule must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("probably") || stderr.contains("unknown variant"),
        "the diagnostic must name what it could not read, got: {stderr}"
    );
}

#[test]
fn config_check_rejects_a_repository_that_is_not_owner_and_name() {
    for spelling in ["fiddle", "peel/", "/fiddle", "peel/fiddle/extra"] {
        let out = check(&FORGE.replace(
            r#"repo = "peel/fiddle-effects-acceptance""#,
            &format!("repo = {spelling:?}"),
        ));
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{spelling:?} is not owner/name and must be refused"
        );
        assert!(
            stderr.contains("owner/name") && stderr.contains("fiddle.toml:11"),
            "{spelling:?} must be refused at the line `repo` is written on, \
             saying what was wanted instead, got: {stderr}"
        );
    }
}

#[test]
fn config_check_does_not_read_the_variable_the_forge_names() {
    let out = check(FORGE);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a missing credential is not a configuration error: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_check_reports_the_github_table_it_accepted() {
    let github = checked(FORGE)["github"].clone();
    assert_eq!(github["repo"], "peel/fiddle-effects-acceptance", "{github}");
    assert_eq!(github["base"], "main", "{github}");
    assert_eq!(github["token"]["env"], FORGE_CREDENTIAL, "{github}");
    assert_eq!(github["cli"]["program"], "gh", "{github}");
    assert_eq!(github["cli"]["args"], serde_json::json!([]), "{github}");
    assert_eq!(github["git"], "git", "{github}");
    assert_eq!(github["timeout"], "5m", "{github}");
    assert_eq!(
        github["required_checks"]["configured"],
        serde_json::json!([]),
        "{github}"
    );
    assert_eq!(github["work"], serde_json::Value::Null, "{github}");
    assert_eq!(github["workflow"], serde_json::Value::Null, "{github}");
    assert_eq!(github["policy"]["ensure_branch_published"], "allow");
    assert_eq!(github["policy"]["ensure_pull_request"], "allow");
    assert_eq!(github["policy"]["ensure_check_requested"], "allow");
    assert_eq!(github["policy"]["publish_decision_request"], "allow");
    assert_eq!(github["policy"]["ensure_pull_request_ready"], "allow");
    assert_eq!(github["decision"], serde_json::Value::Null, "{github}");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, FORGE).unwrap();
    for extra in [vec!["--json"], vec![]] {
        let out = support::fiddle_command()
            .args(["config", "check", "--config", path.to_str().unwrap()])
            .args(&extra)
            .env(FORGE_CREDENTIAL, SENTINEL)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(0), "{extra:?}: stderr = {stderr}");
        assert!(
            !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
            "{extra:?} resolved the forge credential and printed it: {stdout}{stderr}"
        );
        assert!(
            stdout.contains(FORGE_CREDENTIAL),
            "{extra:?} must still name the variable: {stdout}"
        );
    }
}

#[test]
fn config_check_reports_the_gh_program_the_document_pins() {
    let document = format!(
        "{FORGE}cli = {{ program = \"/opt/gh/bin/gh\", args = [\"--stub-dir\", \"/tmp/x\"] }}\n\
         git = \"/opt/git/bin/git\"\nworkflow = \"verify.yml\"\n\
         required_checks = [\"build\"]\ntimeout = \"90s\"\n"
    );
    let github = checked(&document)["github"].clone();
    assert_eq!(github["cli"]["program"], "/opt/gh/bin/gh", "{github}");
    assert_eq!(
        github["cli"]["args"],
        serde_json::json!(["--stub-dir", "/tmp/x"]),
        "{github}"
    );
    assert_eq!(github["git"], "/opt/git/bin/git", "{github}");
    assert_eq!(github["workflow"], "verify.yml", "{github}");
    let checks = &github["required_checks"];
    assert_eq!(
        checks["configured"],
        serde_json::json!(["build"]),
        "{github}"
    );
    assert_eq!(
        checks["enforced"],
        serde_json::json!([]),
        "a document naming a required check gets no check required of it, and \
         this is where it finds that out: {github}"
    );
    assert_eq!(checks["status"], "observed-not-enforced", "{github}");
    assert_eq!(
        checks["decision"], "017-required-checks-are-observed-not-enforced",
        "the surface must lead a reader to the decision: {github}"
    );
    assert!(
        github["timeout"].is_string() && github["policy"]["ensure_pull_request"].is_string(),
        "a value that fires is a plain scalar: {github}"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, &document).unwrap();
    let out = support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .env(FORGE_CREDENTIAL, SENTINEL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("not enforced") && stdout.contains("017-required-checks"),
        "the plain rendering must disclose it too: {stdout}"
    );
    assert_eq!(github["timeout"], "90s", "{github}");
}

const CHECK_LIST: &str = r#"
[[workspace.checks]]
program = "go"
args = ["build", "./..."]
success = "exit-zero"

[[workspace.checks]]
program = "go"
args = ["fmt", "./..."]
success = "exit-zero-and-no-output"

[[workspace.checks]]
program = "wizcli"
args = ["scan"]
success = "artefact-written"
"#;

const COMMAND_LIST: &str = r#"
[[workspace.commands]]
program = "go"
args = ["mod", "tidy"]

[[workspace.commands]]
program = "go"
args = ["mod", "edit"]
extend = "arguments"
"#;

#[test]
fn config_check_reports_the_programs_a_deployment_declared_and_what_may_vary() {
    let commands = checked(&format!("{AGENTIC}{COMMAND_LIST}"))["workspace"]["commands"].clone();
    assert_eq!(
        commands,
        serde_json::json!([
            { "program": "go", "args": ["mod", "tidy"], "extend": "none" },
            { "program": "go", "args": ["mod", "edit"], "extend": "arguments" },
        ]),
        "an operator reading the effective configuration must see every program \
         an attempt may run, and which of them the attempt may add to: {commands}"
    );
}

#[test]
fn a_deployment_declaring_no_program_says_so_rather_than_leaving_it_open() {
    let workspace = checked(AGENTIC)["workspace"].clone();
    assert_eq!(
        workspace["commands"],
        serde_json::json!([]),
        "an absent declaration is an empty list, never a default program: {workspace}"
    );
}

#[test]
fn the_plain_rendering_discloses_each_declared_program_and_its_extension_rule() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, format!("{AGENTIC}{COMMAND_LIST}")).unwrap();
    let out = support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("workspace.commands[0] = \"go\" \"mod\" \"tidy\" (extend: none)"),
        "a declaration an attempt cannot add to must read as one: {stdout}"
    );
    assert!(
        stdout.contains("workspace.commands[1] = \"go\" \"mod\" \"edit\" (extend: arguments)"),
        "and one it can must read as one: {stdout}"
    );
}

#[test]
fn config_check_reports_each_check_with_the_criterion_it_declared() {
    let checks = checked(&format!("{AGENTIC}{CHECK_LIST}"))["workspace"]["checks"].clone();
    assert_eq!(
        checks,
        serde_json::json!([
            { "program": "go", "args": ["build", "./..."], "success": "exit-zero" },
            { "program": "go", "args": ["fmt", "./..."], "success": "exit-zero-and-no-output" },
            { "program": "wizcli", "args": ["scan"], "success": "artefact-written" },
        ]),
        "the list is ordered and each criterion is the declared one: {checks}"
    );
}

#[test]
fn the_plain_rendering_discloses_each_check_and_its_criterion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, format!("{AGENTIC}{CHECK_LIST}")).unwrap();
    let out = support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("workspace.checks[0] = \"go\" \"build\" \"./...\" (success: exit-zero)"),
        "each check is rendered with the criterion it declared: {stdout}"
    );
    assert!(
        stdout.contains("workspace.checks[2] = \"wizcli\" \"scan\" (success: artefact-written)"),
        "including the one whose success is not its exit status: {stdout}"
    );
    assert!(
        stdout.find("workspace.checks[0]").unwrap() < stdout.find("workspace.checks[2]").unwrap(),
        "{stdout}"
    );
}

#[test]
fn one_command_declared_two_ways_keeps_two_meanings() {
    let document = format!(
        "{AGENTIC}\n\
         [[workspace.checks]]\nprogram = \"go\"\nargs = [\"fmt\", \"./...\"]\n\
         success = \"exit-zero\"\n\n\
         [[workspace.checks]]\nprogram = \"go\"\nargs = [\"fmt\", \"./...\"]\n\
         success = \"exit-zero-and-no-output\"\n"
    );
    let checks = checked(&document)["workspace"]["checks"].clone();
    assert_eq!(checks[0]["success"], "exit-zero", "{checks}");
    assert_eq!(checks[1]["success"], "exit-zero-and-no-output", "{checks}");
}

#[test]
fn config_check_refuses_a_check_that_declares_no_criterion() {
    let out = check(&format!(
        "{AGENTIC}\n[[workspace.checks]]\nprogram = \"go\"\nargs = [\"fmt\", \"./...\"]\n"
    ));
    assert_eq!(out.status.code(), Some(2), "a criterion is not optional");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("success"),
        "the diagnostic must name the missing key, got: {stderr}"
    );
}

#[test]
fn config_check_refuses_a_criterion_outside_the_closed_set() {
    let out = check(&format!(
        "{AGENTIC}\n[[workspace.checks]]\nprogram = \"go\"\nsuccess = \"no-output\"\n"
    ));
    assert_eq!(out.status.code(), Some(2), "the set of criteria is closed");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("no-output") && stderr.contains("exit-zero-and-no-output"),
        "the diagnostic must name what was written and what was available, \
         got: {stderr}"
    );
}

#[test]
fn config_check_still_accepts_the_singular_check_on_its_own() {
    let workspace = checked(&format!(
        "{AGENTIC}check = {{ program = \"cargo\", args = [\"test\"] }}\n"
    ))["workspace"]
        .clone();
    assert_eq!(workspace["check"]["program"], "cargo", "{workspace}");
    assert_eq!(workspace["checks"], serde_json::json!([]), "{workspace}");
}

#[test]
fn config_check_refuses_a_document_naming_both_check_shapes() {
    const SINGULAR: &str = "check = { program = \"cargo\", args = [\"test\"] }\n";
    let both = format!("{AGENTIC}{SINGULAR}{CHECK_LIST}");

    assert_eq!(
        check(&both.replace(SINGULAR, "")).status.code(),
        Some(0),
        "the list alone is a document this schema accepts"
    );
    assert_eq!(
        check(&both.replace(CHECK_LIST, "")).status.code(),
        Some(0),
        "the singular check alone is a document this schema accepts"
    );

    let out = check(&both);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("check") && stderr.contains("checks"),
        "the diagnostic must name both shapes so the operator knows which two \
         lines are in conflict, got: {stderr}"
    );
    assert!(
        !stderr.contains("expected") || stderr.contains("checks"),
        "a bare TOML syntax complaint would mean this scenario proved nothing, \
         got: {stderr}"
    );
}

const SWEEP: &str = r#"
[scanner]
cli = { program = "wizcli", args = ["scan"] }
timeout = "20m"

[orchestration.cve]
image = "ghcr.io/acme/icecube:latest"
max_findings = 3
"#;

#[test]
fn the_sweep_table_loads_and_reports_the_bound_the_document_set() {
    let cve = checked(&format!("{AGENTIC}{SWEEP}"))["orchestration"]["cve"].clone();
    assert_eq!(
        cve,
        serde_json::json!({
            "image": "ghcr.io/acme/icecube:latest",
            "severities": ["CRITICAL", "HIGH"],
            "max_findings": 3,
        }),
        "a bound nothing reports back is a bound an operator cannot confirm: {cve}"
    );
}

#[test]
fn the_sweep_table_the_product_manual_documents_is_one_the_schema_accepts() {
    let documented = documented_sweep_table();
    for key in ["severities", "max_findings", "image"] {
        assert!(
            documented.contains(key),
            "the manual's `[orchestration.cve]` example must name {key}, or this \
             lane is checking a document the manual does not contain: {documented}"
        );
    }
    let out = check(&format!(
        "{AGENTIC}[scanner]\n\
         cli = {{ program = \"wizcli\", args = [\"scan\"] }}\n\
         timeout = \"20m\"\n\
         \n{documented}"
    ));
    assert_eq!(
        out.status.code(),
        Some(0),
        "the manual's own sweep table was refused, so a deployment that copies \
         the manual cannot start. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn documented_sweep_table() -> String {
    documented_table(&reference_configuration(), "[orchestration.cve]")
}

#[test]
fn the_grades_a_sweep_acts_on_are_the_grades_the_document_named() {
    let document = format!(
        "{AGENTIC}{}",
        SWEEP.replace(
            "max_findings = 3",
            "severities = [\"HIGH\", \"MEDIUM\", \"CRITICAL\"]\nmax_findings = 3",
        )
    );
    let cve = checked(&document)["orchestration"]["cve"].clone();
    assert_eq!(
        cve["severities"],
        serde_json::json!(["CRITICAL", "HIGH", "MEDIUM"]),
        "a grade set nothing reports back is one an operator cannot confirm: {cve}"
    );
}

#[test]
fn a_sweep_that_names_no_grade_is_refused() {
    let out = check(&format!(
        "{AGENTIC}{}",
        SWEEP.replace("max_findings = 3", "severities = []\nmax_findings = 3")
    ));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("severities"),
        "the refusal must name the key an operator would fix, got: {stderr}"
    );
    assert!(
        !stderr.contains("unknown field"),
        "an empty list must be refused as an empty list; refusing it as an \
         unknown key means the key is not wired, got: {stderr}"
    );
    assert!(
        stderr.contains("at least one grade"),
        "the refusal must say what to write instead, got: {stderr}"
    );
}

#[test]
fn a_scanner_table_that_names_a_credential_is_refused() {
    for field in ["client_id", "client_secret"] {
        let document = format!(
            "{AGENTIC}{}{field} = {{ env = \"WIZ_A_VARIABLE\" }}\n",
            SWEEP.replace("timeout = \"20m\"", "")
        );
        let out = check(&document);
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(
            out.status.code(),
            Some(2),
            "fiddle reads no scanner credential, so a document that names one \
             must say so at load rather than at the scan. stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains(field),
            "the refusal must name the key to delete: {stderr}"
        );
    }

    let out = check(&format!("{AGENTIC}{SWEEP}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "the same table without those keys loads, so the refusals above are the \
         keys and not the table. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_document_that_never_scans_reports_no_scanner_and_no_sweep() {
    let payload = checked(AGENTIC);
    assert!(payload.get("scanner").is_none(), "{payload}");
    assert!(payload.get("orchestration").is_none(), "{payload}");
}

fn check_with_env(text: &str, extra: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, text).unwrap();
    let mut command = support::fiddle_command();
    command
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .args(extra)
        .env_remove(CREDENTIAL);
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn manual() -> String {
    std::fs::read_to_string("../../docs/fiddle-agentic-factory-prd.md")
        .expect("the product manual is two levels up from this package")
}

const REFERENCE_INTRO: &str = "fixes the intended boundaries";

const COPYABLE_INTRO: &str = "#### The configuration this build loads";

const COMPOSITE_CLAIM: &str = "it is not a document a deployment can load";

fn reference_configuration() -> String {
    fenced_toml_after(REFERENCE_INTRO)
}

fn copyable_configuration() -> String {
    fenced_toml_after(COPYABLE_INTRO)
}

fn fenced_toml_after(marker: &str) -> String {
    let manual = manual();
    let lines: Vec<&str> = manual.lines().collect();
    let intro = lines
        .iter()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| {
            panic!(
                "the manual no longer carries the line that introduces this block, \
                 so this lane cannot say which fence it would be reading: {marker}"
            )
        });
    let open = intro
        + lines[intro..]
            .iter()
            .position(|line| line.trim() == "```toml")
            .unwrap_or_else(|| panic!("a ```toml fence must follow: {marker}"));
    let close = open
        + 1
        + lines[open + 1..]
            .iter()
            .position(|line| line.trim().starts_with("```"))
            .unwrap_or_else(|| panic!("the fence opened after {marker} is never closed"));
    lines[open + 1..close].join("\n")
}

fn table_header(line: &str) -> Option<&str> {
    let line = line.trim();
    line.strip_prefix('[')
        .filter(|rest| !rest.starts_with('['))
        .and_then(|rest| rest.strip_suffix(']'))
}

fn table_headers(document: &str) -> Vec<&str> {
    document.lines().filter_map(table_header).collect()
}

fn documented_table(document: &str, header: &str) -> String {
    let wanted = header
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .expect("a header is written with its brackets");
    let mut lines = document
        .lines()
        .skip_while(|line| table_header(line) != Some(wanted));
    let first = lines
        .next()
        .unwrap_or_else(|| panic!("the document must declare a {header} table"));
    let body = lines.take_while(|line| table_header(line).is_none());
    std::iter::once(first)
        .chain(body)
        .collect::<Vec<_>>()
        .join("\n")
}

fn without_table(lines: &[String], header: &str) -> Vec<String> {
    let mut kept = Vec::new();
    let mut dropping = false;
    for line in lines {
        if let Some(declared) = table_header(line) {
            dropping = declared == header || declared.starts_with(&format!("{header}."));
        }
        if !dropping {
            kept.push(line.clone());
        }
    }
    kept
}

fn enclosing_table(lines: &[String], at: usize) -> Option<&str> {
    lines[..at].iter().rev().find_map(|line| table_header(line))
}

fn refused_name(stderr: &str, phrase: &str) -> Option<String> {
    regex::Regex::new(&format!("{phrase} `([^`]+)`"))
        .unwrap()
        .captures(stderr)
        .map(|found| found[1].to_string())
}

fn refused_line(stderr: &str) -> usize {
    regex::Regex::new(r"fiddle\.toml:(\d+):")
        .unwrap()
        .captures(stderr)
        .unwrap_or_else(|| panic!("every refusal names the line it is about: {stderr}"))[1]
        .parse()
        .unwrap()
}

struct Clearing {
    trail: Vec<String>,
    declared: usize,
    survived: usize,
}

fn clear_one_refusal_at_a_time(document: &str) -> Clearing {
    let copyable = copyable_configuration();
    let declared: Vec<String> = table_headers(document)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut lines: Vec<String> = document.lines().map(str::to_owned).collect();
    let mut trail: Vec<String> = Vec::new();
    loop {
        let out = check(&format!("{}\n", lines.join("\n")));
        if out.status.code() == Some(0) {
            break;
        }
        assert_eq!(
            out.status.code(),
            Some(2),
            "a document is refused with exit 2 or accepted with 0, and this was \
             neither. stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let at = refused_line(&stderr);
        assert!(
            at <= lines.len(),
            "a refusal points at line {at} of a {}-line document, so this helper \
             is reading a line number it did not write: {stderr}",
            lines.len()
        );
        if let Some(name) = refused_name(&stderr, "unknown field") {
            match table_header(&lines[at - 1]) {
                Some(header) => {
                    let header = header.to_owned();
                    trail.push(format!(
                        "unknown field `{name}` at line {at}: delete [{header}] and its sub-tables"
                    ));
                    lines = without_table(&lines, &header);
                }
                None => {
                    trail.push(format!(
                        "unknown field `{name}` at line {at}: delete that key"
                    ));
                    lines.remove(at - 1);
                }
            }
        } else if let Some(name) = refused_name(&stderr, "missing field") {
            let absent = !lines
                .iter()
                .any(|line| table_header(line) == Some(name.as_str()));
            if absent && table_headers(&copyable).contains(&name.as_str()) {
                trail.push(format!(
                    "missing field `{name}`: supply the [{name}] table from the \
                     document the manual offers as copyable"
                ));
                lines.push(String::new());
                lines.extend(
                    documented_table(&copyable, &format!("[{name}]"))
                        .lines()
                        .map(str::to_owned),
                );
            } else {
                let enclosing = enclosing_table(&lines, at)
                    .unwrap_or_else(|| {
                        panic!("`missing field {name}` belongs to a table: {stderr}")
                    })
                    .to_owned();
                trail.push(format!(
                    "missing field `{name}` inside [{enclosing}]: delete [{enclosing}]"
                ));
                lines = without_table(&lines, &enclosing);
            }
        } else {
            panic!("this measurement can only clear an unknown or a missing field: {stderr}");
        }
        assert!(
            trail.len() < 64,
            "64 refusals is not a document with defects in it, it is a runaway \
             loop in this helper. Trail:\n{}",
            trail.join("\n")
        );
    }
    let loaded = lines.join("\n");
    let standing = table_headers(&loaded);
    let survived = declared
        .iter()
        .filter(|header| standing.contains(&header.as_str()))
        .count();
    Clearing {
        trail,
        declared: declared.len(),
        survived,
    }
}

#[test]
fn the_manual_says_its_reference_configuration_is_not_a_document_that_loads() {
    let reference = reference_configuration();
    for header in ["project", "github", "jira", "orchestration.cve"] {
        assert!(
            table_headers(&reference).contains(&header),
            "this is not the manual's reference configuration — that block \
             declares [{header}]: {reference}"
        );
    }
    let out = check(&format!("{reference}\n"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "the reference configuration now loads, which is a better world than the \
         one this lane was written for: retire the composite note it pins and let \
         the block be the copyable one. stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let manual = manual();
    for claim in [COMPOSITE_CLAIM, COPYABLE_INTRO] {
        assert!(
            manual.contains(claim),
            "an example that cannot load is a defect or a deliberate composite, \
             and a reader cannot tell which without being told. The manual must \
             say: {claim}"
        );
    }
}

#[test]
fn the_refusals_the_reference_configuration_reaches_are_the_number_the_manual_records() {
    let reference = reference_configuration();
    assert!(
        table_headers(&reference).contains(&"jira"),
        "this is not the manual's reference configuration: {reference}"
    );
    let clearing = clear_one_refusal_at_a_time(&reference);
    let trail = clearing.trail.join("\n");
    let passes = format!("takes {} passes", clearing.trail.len());
    let deleted = format!(
        "{} of its {} tables have to be deleted",
        clearing.declared - clearing.survived,
        clearing.declared
    );
    let manual = manual();
    for claim in [&passes, &deleted] {
        assert!(
            manual.contains(claim.as_str()),
            "the manual must record what this document costs a reader who tries \
             to load it, and say `{claim}`. Measured here, one line per refusal:\n\
             {trail}"
        );
    }
}

#[test]
fn the_document_the_manual_offers_as_copyable_is_one_this_build_loads() {
    let copyable = copyable_configuration();
    let declared = table_headers(&copyable);
    for header in [
        "project",
        "stub",
        "report",
        "agent",
        "workspace",
        "github",
        "scanner",
        "orchestration.cve",
    ] {
        assert!(
            declared.contains(&header),
            "the copyable document must show [{header}] — it is the whole of what \
             a deployment can say today, and a table it omits is one an operator \
             has to discover elsewhere: {copyable}"
        );
    }
    assert!(
        !declared.contains(&"jira"),
        "this is the boundary map, not the copyable document: {copyable}"
    );
    let out = check_with(&format!("{copyable}\n"), &["--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the document the manual offers as copyable was refused, so the manual \
         now has no example a deployment can copy at all. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(payload["status"], "valid", "{payload}");
    assert_eq!(
        payload["project"]["name"],
        documented_scalar(&copyable, "name"),
        "{payload}"
    );
    assert_eq!(
        payload["github"]["repo"],
        documented_scalar(&copyable, "repo"),
        "{payload}"
    );
    assert_eq!(
        payload["orchestration"]["cve"]["image"],
        documented_scalar(&copyable, "image"),
        "{payload}"
    );
}

fn documented_scalar(document: &str, key: &str) -> String {
    let assignment = format!("{key} = ");
    document
        .lines()
        .find_map(|line| line.trim().strip_prefix(&assignment))
        .unwrap_or_else(|| panic!("the document must assign {key}: {document}"))
        .trim()
        .trim_matches('"')
        .to_owned()
}

#[test]
fn the_forge_table_the_product_manual_documents_names_the_keys_the_schema_admits() {
    let documented = documented_table(&reference_configuration(), "[github]");
    for key in ["repo", "base", "token"] {
        assert!(
            documented
                .lines()
                .any(|line| line.starts_with(&format!("{key} = "))),
            "the manual's `[github]` example must name {key}, or this lane is \
             checking a document the manual does not contain: {documented}"
        );
    }
    let out = check(&format!("{AGENTIC}{documented}\n"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "the manual's own forge table was refused, so an operator who copies it \
         cannot publish. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn every_table() -> String {
    let forge = FORGE.split_once("[github]").expect("FORGE names a forge").1;
    format!("{AGENTIC}{CHECK_LIST}{COMMAND_LIST}\n[github]{forge}{SWEEP}")
}

fn admitted_tables() -> Vec<String> {
    let refused = "surely_not_a_table_this_schema_admits";
    let out = check(&format!("{AGENTIC}\n[{refused}]\nx = 1\n"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(2),
        "a table the schema does not admit is invalid input; stderr = {stderr}"
    );
    let (_, listed) = stderr.split_once("expected one of").unwrap_or_else(|| {
        panic!("the refusal must enumerate the tables the schema admits: {stderr}")
    });
    let tables: Vec<String> = regex::Regex::new("`([a-z][a-z0-9_]*)`")
        .unwrap()
        .captures_iter(listed)
        .map(|found| found[1].to_string())
        .collect();
    for mandatory in ["project", "stub", "report"] {
        assert!(
            tables.iter().any(|table| table == mandatory),
            "the parsed set must be the set the binary enumerated, and `{mandatory}` \
             is not optional in any build; got {tables:?} from {stderr}"
        );
    }
    tables
}

#[test]
fn the_system_document_names_every_table_this_schema_admits() {
    let tables = admitted_tables();
    let document = std::fs::read_to_string(support::repo_root().join("docs/technical/SYSTEM.md"))
        .expect("the system document is part of the repository");
    let paragraph = document
        .lines()
        .find(|line| line.starts_with("**`fiddle.toml`**"))
        .expect("the system document describes the deployment document");

    for table in &tables {
        assert!(
            paragraph.contains(&format!("[{table}]")) || paragraph.contains(&format!("[{table}.")),
            "`[{table}]` is a table this schema admits and the `fiddle.toml` \
             paragraph of docs/technical/SYSTEM.md does not name it"
        );
    }
}

#[test]
fn config_check_echoes_every_table_the_schema_admits() {
    let tables = admitted_tables();
    let payload = checked(&every_table());
    let echoed: Vec<String> = payload
        .as_object()
        .expect("the payload is an object")
        .iter()
        .filter(|(_, value)| value.is_object())
        .map(|(key, _)| key.clone())
        .collect();

    for table in &tables {
        assert!(
            echoed.iter().any(|section| section == table),
            "`[{table}]` is a table this schema admits and `config check --json` \
             did not echo it for a document meant to name every table: either \
             `render::config_check_json` has no arm for it, so an operator cannot \
             read it back, or `every_table()` in this file does not carry it. \
             Echoed: {echoed:?}"
        );
    }

    for section in &echoed {
        assert!(
            tables.iter().any(|table| table == section),
            "`config check --json` echoed `{section}` as a table and the schema's \
             own refusal does not enumerate it. Either the payload carries a key \
             that is not a top-level field of `config::Config`, or \
             `admitted_tables` read the refusal short — in which case every lane \
             over that set has been checking a smaller schema than there is. \
             Enumerated: {tables:?}"
        );
    }
}

fn plain(text: &str) -> String {
    let out = check(text);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn the_plain_rendering_names_the_image_grades_and_bound_a_sweep_will_act_on() {
    let stdout = plain(&format!("{AGENTIC}{SWEEP}"));
    for line in [
        "orchestration.cve.image = ghcr.io/acme/icecube:latest",
        "orchestration.cve.severities = CRITICAL HIGH",
        "orchestration.cve.max_findings = 3",
    ] {
        assert!(
            stdout.contains(line),
            "an operator at a terminal cannot confirm `{line}`: {stdout}"
        );
    }
    for line in ["scanner.cli = \"wizcli\" \"scan\"", "scanner.timeout = 20m"] {
        assert!(
            stdout.contains(line),
            "an operator at a terminal cannot confirm `{line}`: {stdout}"
        );
    }
}

#[test]
fn the_plain_rendering_covers_every_table_and_key_the_payload_echoes() {
    let document = every_table();
    let payload = checked(&document);
    let stdout = plain(&document);

    let rendered: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.split_once(" = "))
        .map(|(key, _)| key.trim())
        .filter_map(|key| {
            let mut parts = key.splitn(3, '.');
            let table = parts.next()?;
            let field = parts.next()?;
            let field = field.split_once('[').map_or(field, |(name, _)| name);
            Some(format!("{table}.{field}"))
        })
        .collect();

    for table in admitted_tables() {
        let echoed = payload[&table]
            .as_object()
            .unwrap_or_else(|| {
                panic!(
                    "`[{table}]` is a table this schema admits and the payload for a \
                     document meant to name every table did not echo it as an object — \
                     see `config_check_echoes_every_table_the_schema_admits`: {payload}"
                )
            })
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in echoed {
            assert!(
                rendered
                    .iter()
                    .any(|line| line == &format!("{table}.{key}")),
                "`config check --json` echoes `{table}.{key}` and the plain \
                 rendering an operator gets by default does not name it, so the \
                 two surfaces do not confirm the same document. Either \
                 `render::config_check_human` has no arm for it, or it spells the \
                 key differently from the document. Rendered: {rendered:?}"
            );
        }
    }

    for key in &rendered {
        let (table, field) = key.split_once('.').expect("a rendered key is dotted");
        assert!(
            payload[table].get(field).is_some(),
            "the plain rendering names `{key}` and `config check --json` echoes no \
             such key, so a reader at a terminal is being told something a reader \
             parsing the payload cannot confirm: {payload}"
        );
    }
}
