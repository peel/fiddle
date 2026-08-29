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
fn a_document_naming_an_unknown_effect_is_refused_by_config_check() {
    let out = check(&format!(
        "{FORGE}\n[github.policy]\nensure_everything = \"deny\"\nensure_pull_requst = \"deny\"\n"
    ));
    assert_eq!(out.status.code(), Some(2), "unknown key must exit 2");
    let stderr = String::from_utf8(out.stderr).unwrap();
    for key in ["ensure_everything", "ensure_pull_requst"] {
        assert!(
            stderr.contains(key),
            "the diagnostic must name {key}, not only the first offender: {stderr}"
        );
    }
    assert!(
        stderr.contains("ensure_pull_request_body"),
        "and must say what a rule may be written for, got: {stderr}"
    );
}

#[test]
fn a_document_leaving_an_effect_out_is_accepted_and_reports_it_ungated() {
    let github = checked(&format!(
        "{FORGE}\n[github.policy]\nensure_pull_request = \"deny\"\n"
    ))["github"]
        .clone();
    assert_eq!(github["policy"]["ensure_pull_request"], "deny", "{github}");
    assert_eq!(
        github["policy"]["ensure_branch_published"], "allow",
        "a row the document leaves out is reported as adding no gate: {github}"
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
    assert_eq!(github["policy"]["ensure_pull_request_body"], "allow");
    assert_eq!(github["policy"]["jira.issue_filed"], "allow");
    assert_eq!(github["policy"]["jira.comment_added"], "allow");
    assert_eq!(github["policy"]["jira.issue_transitioned"], "allow");
    assert_eq!(github["policy"]["jira.pull_request_linked"], "allow");
    assert_eq!(
        github["policy"].as_object().unwrap().len(),
        10,
        "one row per effect this build performs, and no more: {github}"
    );
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

fn every_table() -> String {
    let forge = FORGE.split_once("[github]").expect("FORGE names a forge").1;
    format!("{AGENTIC}{CHECK_LIST}{COMMAND_LIST}\n[github]{forge}{SWEEP}{TRACKER}{TRACKER_FILING}")
}

const TRACKER: &str = r#"
[jira]
site = "https://example.atlassian.net"
project = "IDENT"
user = { env = "JIRA_USER_EMAIL" }
token = { env = "JIRA_API_TOKEN" }

[jira.workflow]
ready = "Ready"
in_progress = "In Progress"
in_review = "In Review"
blocked = "Blocked"
done = "Done"
"#;

const TRACKER_FILING: &str = r#"
[jira.filing]
project = "SEC"
issue_type = "Task"
ledger_issue = "SEC-1"
"#;

const TRACKER_CREDENTIAL: &str = "JIRA_API_TOKEN";

#[test]
fn config_check_echoes_the_project_a_deployment_files_advisories_into() {
    let filing = checked(&format!("{AGENTIC}{TRACKER}{TRACKER_FILING}"))["jira"]["filing"].clone();
    assert_eq!(
        filing,
        serde_json::json!({
            "project": "SEC",
            "issue_type": "Task",
            "ledger_issue": "SEC-1",
        }),
        "the project a deployment files into is not the project it reads work \
         items from, and an operator has to be able to read back which is which"
    );
    assert_eq!(
        checked(&format!("{AGENTIC}{TRACKER}{TRACKER_FILING}"))["jira"]["project"],
        "IDENT",
        "the observed project is untouched by the filing table"
    );
}

#[test]
fn a_document_naming_no_filing_table_files_nothing_and_says_so() {
    assert_eq!(
        checked(&format!("{AGENTIC}{TRACKER}"))["jira"]["filing"],
        serde_json::Value::Null,
        "a tracker read for work items files no advisory until a deployment asks \
         it to, and `config check` is where an operator confirms that"
    );
}

#[test]
fn config_check_echoes_the_tracker_and_names_its_credential_without_resolving_it() {
    let document = format!("{AGENTIC}{TRACKER}");
    assert_eq!(
        checked(&document)["jira"],
        serde_json::json!({
            "site": "https://example.atlassian.net",
            "project": "IDENT",
            "user": { "env": "JIRA_USER_EMAIL" },
            "token": { "env": "JIRA_API_TOKEN" },
            "timeout": "5m",
            "base_url": null,
            "workflow": {
                "ready": "Ready",
                "in_progress": "In Progress",
                "in_review": "In Review",
                "blocked": "Blocked",
                "done": "Done",
            },
            "filing": null,
        }),
        "an operator must read back the site, the project, the bound and the \
         variables the document names, and a tracker this deployment files no \
         advisory into echoes that absence rather than omitting the key"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, &document).unwrap();
    for extra in [vec!["--json"], vec![]] {
        let out = support::fiddle_command()
            .args(["config", "check", "--config", path.to_str().unwrap()])
            .args(&extra)
            .env(TRACKER_CREDENTIAL, SENTINEL)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(0), "{extra:?}: stderr = {stderr}");
        assert!(
            !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
            "{extra:?} resolved the tracker credential and printed it: {stdout}{stderr}"
        );
        assert!(
            stdout.contains(TRACKER_CREDENTIAL),
            "{extra:?} must still name the variable the document points at: {stdout}"
        );
    }
}

#[test]
fn a_document_naming_no_tracker_echoes_no_tracker() {
    assert!(
        checked(AGENTIC).get("jira").is_none(),
        "an absent table describes a deployment that reads no tracker, and it is \
         never a blank filled in silently"
    );
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

const SWEEPING: &str = r#"[project]
name = "icecube"

[stub]
root = "."

[report]
dir = "reports"

[agent]
model = "claude-sonnet-5"
base_url = "https://gateway.example/v1"
api_key = { env = "FIDDLE_MODEL_API_KEY" }

[github]
repo = "acme/r"
base = "main"
token = { env = "FIDDLE_GITHUB_TOKEN" }

[scanner]
cli = { program = "wizcli", args = ["scan", "container-image"] }

[orchestration.cve]
image = "icecube:scan"

[workspace]
root = "/tmp/w"
fixture = "."

[[workspace.checks]]
program = "go"
args = ["build", "./..."]
success = "exit-zero"
"#;

#[test]
fn config_check_accepts_a_document_that_run_cve_can_run() {
    let out = check_with(SWEEPING, &["--capability", "cve_mitigate"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_check_refuses_a_sweep_document_that_run_cve_would_refuse() {
    for (absent, without) in [
        (
            "workspace.fixture",
            SWEEPING.replace("fixture = \".\"\n", ""),
        ),
        (
            "[[workspace.checks]]",
            SWEEPING
                .split("[[workspace.checks]]")
                .next()
                .unwrap()
                .to_string(),
        ),
        ("[scanner]", strip_table(SWEEPING, "[scanner]")),
        ("[agent]", strip_table(SWEEPING, "[agent]")),
        ("[github]", strip_table(SWEEPING, "[github]")),
    ] {
        let out = check_with(&without, &["--capability", "cve_mitigate"]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "a document `run cve` refuses for want of {absent} must not pass the \
             pre-flight the host workflow relies on, or the host builds an image and \
             scans a container before finding out — stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains(absent) && said.contains("cve_mitigate"),
            "the refusal names the field and the capability that needs it: {said}"
        );
    }
}

#[test]
fn config_check_asked_about_nothing_still_answers_about_the_schema_alone() {
    let out = check(&SWEEPING.replace("fixture = \".\"\n", ""));
    assert_eq!(
        out.status.code(),
        Some(0),
        "without a capability this command answers about the schema, which is the \
         weaker question it has always answered: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn strip_table(text: &str, table: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        if line.trim() == table {
            skipping = true;
            continue;
        }
        if skipping && line.starts_with('[') {
            skipping = false;
        }
        if !skipping {
            kept.push(line);
        }
    }
    kept.join("\n")
}
