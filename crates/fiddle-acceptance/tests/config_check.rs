//! Black-box acceptance for `fiddle config check`.
//!
//! Every assertion here drives the compiled binary as a subprocess and reads its
//! exit code, stdout, and stderr; nothing calls a library function directly.

mod support;

/// The documented fixture, relative to this package's root — `cargo test` runs a
/// test binary with the package directory as its working directory, so two
/// levels up is the repository root.
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

/// A document naming a model and a workspace, as M1's own configuration does.
/// Written as a constant so each scenario below can mutate exactly one line of
/// a document that is otherwise known to be accepted.
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

/// The variable [`AGENTIC`] names. Never a value.
const CREDENTIAL: &str = "LITELLM_API_KEY";

/// What is exported as that credential where a scenario needs it to be set: a
/// string that authenticates nothing, and that must appear on no surface.
const SENTINEL: &str = "sk-sentinel-config-check-must-never-print-4b19";

/// Run `config check` over `text` and hand back the whole process result.
///
/// The document is written into a fresh temporary directory each time, so no
/// scenario can be reading a file another one left behind.
fn check(text: &str) -> std::process::Output {
    check_with(text, &[])
}

/// As [`check`], with `extra` flags appended.
fn check_with(text: &str, extra: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fiddle.toml");
    std::fs::write(&path, text).unwrap();
    support::fiddle_command()
        .args(["config", "check", "--config", path.to_str().unwrap()])
        .args(extra)
        // The credential is *named* by this document and must not be needed to
        // validate it: an environment holding no key must still exit 0. Both
        // names, because both tables carry an `EnvRef` and a helper that
        // removed only one would prove the property for only one of them.
        .env_remove(CREDENTIAL)
        .env_remove(FORGE_CREDENTIAL)
        .output()
        .unwrap()
}

/// The `--json` payload of `config check` over `text`, requiring exit 0.
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

/// **The command whose purpose is confirming which document was accepted has
/// to show the operators it accepted.**
///
/// Asserted field by field rather than by searching the payload for substrings:
/// a renamed key has to fail here, and a payload that happened to contain the
/// model identifier somewhere else would satisfy a substring search while
/// telling a caller nothing it could parse.
#[test]
fn config_check_reports_the_agent_table_it_accepted() {
    let agent = checked(AGENTIC)["agent"].clone();
    assert_eq!(agent["model"], "claude-sonnet-5", "{agent}");
    assert_eq!(
        agent["base_url"], "https://litellm.firn.snplow.net/v1",
        "{agent}"
    );
    // The *name* of the variable, which is all the document can hold.
    assert_eq!(agent["api_key"]["env"], CREDENTIAL, "{agent}");
    assert_eq!(agent["max_turns"], 12, "{agent}");
    // The bounds the document left to their defaults are reported as the
    // values that will actually apply, not omitted: an operator confirming a
    // document needs to see what it means, not only what it says.
    assert_eq!(agent["max_tokens"], 8192, "{agent}");
    assert_eq!(agent["max_changed_files"], 16, "{agent}");
    assert_eq!(agent["deadline"], "45m", "{agent}");
    assert_eq!(agent["tool_timeout"], "15m", "{agent}");
}

/// The other table M1 added, including the two keys a repair refuses by name
/// when they are missing — reported as `null` so an operator learns *before*
/// the run which refusal is waiting for them.
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

/// The repository under repair and the command that decides the milestone's
/// central property, echoed in the shape the document writes them.
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

/// **A key that parses, defaults, and fires nothing must say so at runtime.**
///
/// Design §6.6 promises that under `deny_unknown_fields` a deferred key is a
/// loud error rather than a silently ignored one.
/// `agent.max_capability_attempts` reaches the state that forbids by a route it
/// did not name — *known* rather than unknown, and therefore silent. ADR 013 is
/// right not to build the retry loop; this is the cheap remedy it never weighed.
///
/// The document asks for five attempts and gets one. Both numbers are in the
/// payload, under names a machine reader can key on, and the shape itself
/// distinguishes this bound from every other one: the enforced bounds are plain
/// scalars.
#[test]
fn config_check_marks_the_attempt_bound_it_accepts_and_does_not_enforce() {
    let agent = checked(&AGENTIC.replace(
        "max_turns = 12",
        "max_turns = 12\nmax_capability_attempts = 5",
    ))["agent"]
        .clone();
    let bound = &agent["max_capability_attempts"];
    assert_eq!(bound["configured"], 5, "{agent}");
    assert_eq!(
        bound["enforced"], 1,
        "a document writing 5 gets one attempt, and this is where it finds \
         that out: {agent}"
    );
    assert_eq!(bound["status"], "accepted-not-enforced", "{agent}");
    assert_eq!(
        bound["decision"], "013-one-attempt-bound-not-two",
        "the surface must lead a reader to the decision: {agent}"
    );
    assert!(
        agent["max_turns"].is_number() && agent["max_changed_files"].is_number(),
        "a bound that fires is a plain scalar, so the shape alone tells the two \
         kinds apart: {agent}"
    );
}

/// **No credential value on any surface**, asserted with the variable actually
/// exported — which is the only state in which a leak is possible at all.
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

/// **M0's payload is byte-for-byte what it was before the tables existed.**
///
/// The absent tables produce no key at all rather than a `null` one, which is
/// what keeps `crates/fiddle-acceptance/tests/m0_skeleton.rs` — whose first step
/// asserts this very payload, and whose document has neither table — unaffected
/// by this change, and what keeps `fiddle.config_check.v0` an honest version:
/// nothing a v0 reader ever saw has moved or changed meaning.
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
    // The one property this milestone's configuration exists to have: the
    // schema admits the *name* of a variable, so a resolved secret cannot be
    // carried in a file that gets committed. Mutating an accepted document is
    // what makes the refusal attributable to `api_key` and nothing else.
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
    // The refusal must not undo itself. A line-aware diagnostic quotes the
    // line it is about, and here that line is the secret; fiddle prints a
    // redacted placeholder in its place, so `config check` on a mistaken
    // document is safe to run in a CI log.
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
    // Design §4.6: the diagnostic is line-aware, not just a message. miette
    // renders the source line containing the offending key.
    assert!(
        stderr.contains("nickname = \"nope\"") || stderr.contains("fiddle.toml:3"),
        "diagnostic must locate the key in the source, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// The `[github]` deployment table.
//
// The same three properties `[agent]` already has, asserted again because two
// `deny_unknown_fields` attributes and two `EnvRef` fields are two separate
// chances to forget one: a credential is named and never written, an unknown
// key is refused at its line, and validating a document reads no credential.
// ---------------------------------------------------------------------------

/// The variable [`FORGE`] names. Never a value.
const FORGE_CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

/// A document naming a forge, written the way an operator would write one.
///
/// Deliberately minimal: `repo`, `base` and `token` are the three keys with no
/// defensible default, and everything else is left out so that each scenario
/// below adds exactly the line it is about.
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

/// **The token deserializes only from `{ env = "NAME" }`.**
///
/// The same rule `agent.api_key` already follows, and for the same reason: a
/// document that could hold a forge credential is a document that gets
/// committed holding one.
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

/// `deny_unknown_fields` is not relaxed for the new table.
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

/// The same, one table deeper: `[github.policy]` is its own strict table.
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

/// A rule this build cannot honour is refused at its line rather than defaulted
/// to something permissive.
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

/// `repo` is `owner/name` and is refused at the line that is not, because the
/// head a pull request is opened from is derived from the owner half — a value
/// that cannot be derived is a run that fails after it has already pushed.
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
        // Attributed to the key rather than merely rejected: without this the
        // scenario would pass on a build where `[github]` itself is unknown.
        assert!(
            stderr.contains("owner/name") && stderr.contains("fiddle.toml:11"),
            "{spelling:?} must be refused at the line `repo` is written on, \
             saying what was wanted instead, got: {stderr}"
        );
    }
}

/// **Validating the document reads no credential.**
///
/// `config check` is the command that runs before work starts, on a machine
/// that may legitimately not hold the token yet. A missing credential is not a
/// configuration error — [`check`] removes both named variables from the
/// subprocess, so this passes only if nothing resolved either.
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

/// The table is echoed back, defaults included, and the credential is echoed as
/// the name the document wrote — with the value exported, which is the only
/// state in which a leak is possible at all.
#[test]
fn config_check_reports_the_github_table_it_accepted() {
    let github = checked(FORGE)["github"].clone();
    assert_eq!(github["repo"], "peel/fiddle-effects-acceptance", "{github}");
    assert_eq!(github["base"], "main", "{github}");
    assert_eq!(github["token"]["env"], FORGE_CREDENTIAL, "{github}");
    // Defaults reported as the values that will apply, the same way the agent
    // bounds are.
    assert_eq!(github["cli"]["program"], "gh", "{github}");
    assert_eq!(github["cli"]["args"], serde_json::json!([]), "{github}");
    assert_eq!(github["git"], "git", "{github}");
    assert_eq!(github["timeout"], "5m", "{github}");
    assert_eq!(
        github["required_checks"]["configured"],
        serde_json::json!([]),
        "{github}"
    );
    // The two keys a publication refuses by name when they are absent, reported
    // as `null` so an operator learns which refusal is waiting for them.
    assert_eq!(github["work"], serde_json::Value::Null, "{github}");
    assert_eq!(github["workflow"], serde_json::Value::Null, "{github}");
    // Absent means allow, and it is reported rather than left to be inferred.
    assert_eq!(github["policy"]["ensure_branch_published"], "allow");
    assert_eq!(github["policy"]["ensure_pull_request"], "allow");
    assert_eq!(github["policy"]["ensure_check_requested"], "allow");

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

/// The seam the deterministic suite substitutes through, echoed in the shape the
/// document writes it — the same shape `[workspace] check` already uses.
///
/// It also carries **the second key that is reported and does not decide**.
/// `github.required_checks` reaches `Executor::observe_checks` and populates
/// `observations.verification`, and nothing branches on the answer:
/// `fiddle_core::assess` matches on the work item and the change set alone. A
/// list named `required_checks` that requires nothing is exactly the shape
/// `agent.max_capability_attempts` shipped, so it is disclosed the same way —
/// object rather than scalar, both values, a `status` a machine keys on, and the
/// decision that explains it.
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
    // The discriminating half, and the same one the attempt bound's own scenario
    // makes: a key that *does* decide is a plain scalar, so the shape alone
    // tells the two kinds apart without reading any word.
    assert!(
        github["timeout"].is_string() && github["policy"]["ensure_pull_request"].is_string(),
        "a value that fires is a plain scalar: {github}"
    );
    // And the human rendering says it too, since an operator running `config
    // check` without `--json` is the reader this is for.
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
