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

// ---------------------------------------------------------------------------
// `[github.decision]` — who may decide. One key, and the reason it is one is at
// `config::Decision`: a page bound was specified for this table and removed,
// because a constant that cannot disagree with itself is what two reads of one
// conversation need.
// ---------------------------------------------------------------------------

/// The numeric user id the fixtures below nominate. An id and never a login,
/// which is the property the schema enforces by type.
const DECIDER: u64 = 505_401;

/// Which word of the disclosure taxonomy this channel is reported under.
///
/// The build has two, and they are not interchangeable: `accepted-not-enforced`
/// for a key no code path consults, `observed-not-enforced` for one that is read
/// and acted on and still decides nothing. `[github.decision]` is the first —
/// nothing reads either key while `propose_change` is not constructible from a
/// document — so it is disclosed the way `agent.max_capability_attempts` is, which
/// is what makes this scenario the same scenario that one already has rather than
/// a new kind of claim. `render.rs`'s `DECISION_STATUS` carries the argument and
/// is the one line that changes when a capability starts reading the keys.
const DECISION_STATUS: &str = "accepted-not-enforced";

/// The same fact in the plain rendering, which says it in prose rather than in a
/// word a machine keys on — the split `agent.max_capability_attempts` already uses
/// between its payload object and its terminal line.
const DECISION_STATUS_PHRASE: &str = "accepted, not enforced";

/// [`FORGE`] with a `[github.decision]` table carrying `body`.
fn with_decision(body: &str) -> String {
    format!("{FORGE}\n[github.decision]\n{body}\n")
}

/// **The decision channel is disclosed, in both renderings.**
///
/// `config check` is the command an operator runs to confirm what a document
/// means before a run acts on it, and who may promote a change is the most
/// consequential thing this document now says. Asserted field by field rather
/// than by searching for substrings, for the reason the agent table's own
/// scenario gives: a renamed key has to fail here.
///
/// `matched_on` is in the payload because the *kind* of identity is the property,
/// not a detail: a reader who cannot tell whether `505401` is matched as an
/// immutable id or as something a login could become cannot tell whether the
/// allowlist means what they think.
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

    // And the human rendering says the same three things, because an operator at
    // a terminal is the reader this is for.
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
    // **The key as well as the value**, spelled the way the document spells it,
    // because that rendering's whole contract is `<table>.<key> = <value>` for a
    // reader whose next move is to go and edit the file. Asserted on the pair and
    // not on the value alone: a rendering that printed the right ids under a key
    // an operator could not find in their document would satisfy any number of
    // substring checks while telling them nothing they could act on.
    assert!(
        stdout.contains(&format!("github.decision.authorized = {DECIDER}"))
            && stdout.contains("numeric_user_id")
            && stdout.contains(DECISION_STATUS_PHRASE),
        "the plain rendering must disclose the channel under the key the document \
         writes it under: {stdout}"
    );
}

/// **A document naming nobody is refused, and told why.**
///
/// The empty list is the one an operator is most likely to write on the way to
/// filling it in, and reading it as "anybody" is the failure this refusal exists
/// to prevent. Exit 2, because it is a document that cannot be honoured rather
/// than a run that went wrong.
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

/// A login is not a spelling this schema admits, so the loose match a login
/// would force on the code is unreachable from any document.
#[test]
fn config_check_refuses_an_approver_named_by_login() {
    let out = check(&with_decision(r#"authorized = ["peel"]"#));
    assert_eq!(out.status.code(), Some(2), "a login must exit 2");
}

/// The same strictness one table deeper: `[github.decision]` is its own strict
/// table, because `deny_unknown_fields` on `[github]` does not reach into a
/// child — `[github.read_retry]`'s reasoning, applied to the second child table.
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
    // And the same document without the misspelling is accepted, so the refusal
    // above is strictness and not the correct key having gone missing with it.
    assert_eq!(
        checked(&with_decision(&format!("authorized = [{DECIDER}]")))["github"]["decision"]
            ["authorized"],
        serde_json::json!([DECIDER])
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
    // Absent means allow, and it is reported rather than left to be inferred —
    // for every kind this build can be given a rule for, so that a kind added to
    // the table without being added to the rendering fails here rather than being
    // a rule an operator cannot confirm.
    assert_eq!(github["policy"]["ensure_branch_published"], "allow");
    assert_eq!(github["policy"]["ensure_pull_request"], "allow");
    assert_eq!(github["policy"]["ensure_check_requested"], "allow");
    assert_eq!(github["policy"]["publish_decision_request"], "allow");
    assert_eq!(github["policy"]["ensure_pull_request_ready"], "allow");
    // And the channel a deployment has not described is `null` rather than
    // absent, for the reason `work` and `workflow` are: an operator confirming a
    // document should learn that nobody is authorized to promote a change here.
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

// ---------------------------------------------------------------------------
// The ordered check list, each check carrying its own success criterion.
//
// M1 shipped one check and one meaning of success — the process exited zero.
// M4 runs several in order, and they do not agree on what success is: a build
// succeeds by exiting zero, a formatter succeeds by exiting zero *and printing
// nothing*, and a scanner succeeds by *writing its artefact* whatever it exits.
//
// The criterion is therefore written in the document, next to the check it
// judges. The alternative — recognising `go fmt` or `wizcli` by name and
// applying the meaning that program is known to have — would make an
// operator's rename a change of meaning, and a wrapper script the same. There
// is no such recognition anywhere, and the scenarios below are what would
// notice if one appeared.
// ---------------------------------------------------------------------------

/// The three checks the milestone was specified against, each declaring a
/// different criterion, written the way an operator would write them.
///
/// Kept as one constant so the scenarios below can *subtract* from a document
/// that is otherwise known to be accepted, which is how the both-shapes
/// scenario tells a semantic refusal from a syntactic one.
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

/// The list loads, keeps the order it was written in, and each entry reports
/// back the criterion *it* declared rather than one derived from anything.
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

/// And the plain rendering says it too, since an operator running `config
/// check` without `--json` is the reader most likely to be confirming that the
/// scanner is not about to be judged by an exit status.
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
    // The index is in the line because the order is a fact about the list, and
    // an operator confirming a document is confirming that too.
    assert!(
        stdout.find("workspace.checks[0]").unwrap() < stdout.find("workspace.checks[2]").unwrap(),
        "{stdout}"
    );
}

/// **The criterion comes from the document, never from the program's name.**
///
/// Two checks running the *same* command, differing only in what they declare
/// success to be, and the payload keeps them apart. Nothing that inferred a
/// meaning from `go fmt` could produce two different answers for one command,
/// so this is the scenario that would fail the day somebody adds a lookup
/// table keyed on a program name.
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

/// A check that declares nothing is refused, *including* one whose program a
/// reader would happily guess the meaning of. `go fmt` is the most guessable
/// command in the list and it still has to say what it means.
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

/// The set is closed, so a criterion nobody implemented is refused at its line
/// rather than accepted and then never honoured.
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

/// The singular `check` M1 shipped still loads on its own, unchanged.
#[test]
fn config_check_still_accepts_the_singular_check_on_its_own() {
    let workspace = checked(&format!(
        "{AGENTIC}check = {{ program = \"cargo\", args = [\"test\"] }}\n"
    ))["workspace"]
        .clone();
    assert_eq!(workspace["check"]["program"], "cargo", "{workspace}");
    assert_eq!(workspace["checks"], serde_json::json!([]), "{workspace}");
}

/// **A contradiction is refused, never ranked.**
///
/// A document naming both shapes has said two things about what judges a
/// repair, and there is no reading of it that is what the operator meant:
/// running the singular one, running the list, or running both are three
/// different milestones. So it is refused, and the operator picks.
///
/// The hard part of this scenario is not the refusal, it is proving *why* it
/// refused. A malformed document also exits 2, and a test that could not tell
/// the two apart would pass just as happily against a schema that resolved the
/// contradiction by precedence and a document with a typo in it. So the same
/// bytes are run three ways: with the singular line removed, with the list
/// removed, and whole. The first two are accepted, which is what establishes
/// that every byte in the third parses — leaving *naming both* as the only
/// thing that can have caused the refusal.
#[test]
fn config_check_refuses_a_document_naming_both_check_shapes() {
    const SINGULAR: &str = "check = { program = \"cargo\", args = [\"test\"] }\n";
    let both = format!("{AGENTIC}{SINGULAR}{CHECK_LIST}");

    // Half one, alone: accepted.
    assert_eq!(
        check(&both.replace(SINGULAR, "")).status.code(),
        Some(0),
        "the list alone is a document this schema accepts"
    );
    // Half two, alone: accepted. Between them these two runs cover every byte
    // of `both`, so nothing in it is a syntax error.
    assert_eq!(
        check(&both.replace(CHECK_LIST, "")).status.code(),
        Some(0),
        "the singular check alone is a document this schema accepts"
    );

    // Together: refused, and the refusal is the semantic one.
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

// ---------------------------------------------------------------------------
// The two tables M4 adds (Task 20.a)
// ---------------------------------------------------------------------------

/// The scanner and the sweep, written the way an operator would write them.
const SWEEP: &str = r#"
[scanner]
cli = { program = "wizcli", args = ["scan"] }
client_id = { env = "WIZ_CLIENT_ID" }
client_secret = { env = "WIZ_CLIENT_SECRET" }
timeout = "20m"

[orchestration.cve]
image = "ghcr.io/acme/icecube:latest"
max_findings = 3
go = { program = "go", args = [] }
"#;

/// **Both preferences the PRD documents are preferences the document can set.**
///
/// `[orchestration.cve] max_findings` was in the product document's
/// configuration example and in no reader for the whole of M4a, so the number a
/// deployment believed it had set was a constant in the runtime — the same
/// number, which is exactly why nobody noticed. This is what makes the two
/// distinguishable: the document says `3`, and `3` is what comes back.
///
/// `severities` is the other key of that same two-key example, and it survived a
/// pass longer: the table above omits it, so what is asserted here is the
/// **default** — the set a document that says nothing about grades still means,
/// and the set this build acted on for the whole of M4a. The lane that varies it
/// is [`the_grades_a_sweep_acts_on_are_the_grades_the_document_named`].
///
/// The image is asserted beside them and is not decoration: it is the one key in
/// this table with no default, because a guessed image would scan whichever tag
/// this build shipped with.
#[test]
fn the_sweep_table_loads_and_reports_the_bound_the_document_set() {
    let cve = checked(&format!("{AGENTIC}{SWEEP}"))["orchestration"]["cve"].clone();
    assert_eq!(
        cve,
        serde_json::json!({
            "image": "ghcr.io/acme/icecube:latest",
            "severities": ["CRITICAL", "HIGH"],
            "max_findings": 3,
            "go": { "program": "go", "args": [] },
        }),
        "a bound nothing reports back is a bound an operator cannot confirm: {cve}"
    );
}

/// **The `[orchestration.cve]` table in the product manual is a table this
/// binary accepts — read out of the manual, not transcribed from it.**
///
/// The transcription is what failed. `severities = ["HIGH", "CRITICAL"]` sat in
/// the PRD's configuration example while `OrchestrationCve` — `deny_unknown_fields`
/// — admitted three other names, so a deployment that copied the manual exited 2
/// with `unknown field \`severities\``. The table beside this one is *written the
/// way an operator would write them*, which is exactly why it could not catch
/// that: it is this suite's idea of the table, and the divergence was between the
/// manual and the schema.
///
/// So this lane parses `docs/fiddle-agentic-factory-prd.md` and feeds the binary
/// the manual's own bytes. Either document may now move and the other has to
/// follow: a key added to the manual that the schema refuses reds here, and so
/// does a key the schema stops admitting.
///
/// **The extraction is asserted before it is used.** A helper that silently found
/// nothing would make this lane a `config check` over `AGENTIC` alone — green,
/// and evidence for nothing — so the table is required to carry both keys the
/// manual's example documents before a byte of it is handed over.
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
         client_id = {{ env = \"WIZ_CLIENT_ID\" }}\n\
         client_secret = {{ env = \"WIZ_CLIENT_SECRET\" }}\n\
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

/// The `[orchestration.cve]` table of the manual's reference configuration,
/// verbatim.
///
/// Read from the product document rather than held as a constant here, because a
/// constant is a transcription and a transcription is the thing that drifted. The
/// table runs from its own header to the next TOML header or the end of the
/// block, which is what a TOML table is; the comment lines inside it come along,
/// and they are part of what an operator would copy.
///
/// **Which block it comes out of is now said rather than assumed.** When this was
/// written the manual carried one TOML fence, so scanning the whole file for the
/// header could only find that one. The manual now carries two — the reference
/// configuration, which is a boundary map across the whole of V1 and deliberately
/// does not load, and the shorter document beside it, which does — and both
/// declare an `[orchestration.cve]` table. A file-wide scan would take whichever
/// came first, which is a lane whose subject depends on document order, so this
/// asks [`reference_configuration`] for the block it means.
fn documented_sweep_table() -> String {
    documented_table(&reference_configuration(), "[orchestration.cve]")
}

/// **The grades a sweep acts on are the grades the document named.**
///
/// The property `max_findings` has, for the key beside it, and it is asked the
/// only way that distinguishes a wired key from an ignored one: this document
/// names a set the build does **not** default to. `MEDIUM` is in it and
/// `["CRITICAL", "HIGH"]` is what an omitting document means, so an
/// implementation that read the key and threw it away comes back with two grades
/// where this asserts three.
///
/// Reported ranked rather than as written. The value is a *set* — two documents
/// spelling the same grades in different orders describe one deployment — and
/// worst-first is the one spelling both of them share, so an operator comparing
/// two accepted documents is comparing their meaning rather than their typing.
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

/// **A sweep that names no grade at all is refused rather than quietly run.**
///
/// `severities = []` parses as a TOML array and would leave the severity arm
/// selecting nothing, so the run would act only on findings carrying a public
/// exploit *and* a published fix — a sweep almost nothing reaches, presenting to
/// an operator as *the scanner found nothing*. That is the failure this whole
/// table's `deny_unknown_fields` exists to prevent one spelling of, and an empty
/// list is the other spelling.
///
/// The diagnostic has to name the key, and it must **not** be the diagnostic
/// this document used to get. `severities = []` was refused before this key
/// existed too — as an unknown field — so a lane asserting only *exit 2, and the
/// word `severities` appears* is satisfied by a build that never wired the key at
/// all. The second assertion is what makes this one about the empty list.
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

/// **The scanner's two credentials are echoed by name and never by value.**
///
/// The rule every `EnvRef` in this schema is under, and it is sharpest here:
/// `client_secret` is the value the whole of `fiddle_runtime::scanner`'s
/// redaction exists for, so a `config check` that printed it would undo that
/// redaction from the one command an operator runs to confirm a document.
///
/// The variables are *exported with a sentinel* before the check runs, which is
/// what makes the absence mean something: a payload that omits a value nobody
/// set is not evidence about anything.
#[test]
fn the_scanner_table_names_its_credentials_and_prints_neither() {
    let out = check_with_env(
        &format!("{AGENTIC}{SWEEP}"),
        &["--json"],
        &[
            ("WIZ_CLIENT_ID", "wiz-client-id-sentinel-9f21"),
            ("WIZ_CLIENT_SECRET", "wiz-client-secret-sentinel-9f21"),
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        payload["scanner"]["client_id"]["env"], "WIZ_CLIENT_ID",
        "{payload}"
    );
    assert_eq!(
        payload["scanner"]["client_secret"]["env"], "WIZ_CLIENT_SECRET",
        "{payload}"
    );
    for sentinel in [
        "wiz-client-id-sentinel-9f21",
        "wiz-client-secret-sentinel-9f21",
    ] {
        assert!(
            !stdout.contains(sentinel),
            "a credential reached stdout: {stdout}"
        );
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains(sentinel),
            "a credential reached a diagnostic"
        );
    }
}

/// A document that omits both tables still loads, and reports neither.
///
/// The property every optional table in this schema has, asserted for the two
/// new ones: "absent is a legal document" and never "absent is filled in
/// silently". A deployment that never scans has not left these blank — it has
/// described a deployment that does not scan, and a `config check` inventing an
/// image for it would be the guess the schema refuses.
#[test]
fn a_document_that_never_scans_reports_no_scanner_and_no_sweep() {
    let payload = checked(AGENTIC);
    assert!(payload.get("scanner").is_none(), "{payload}");
    assert!(payload.get("orchestration").is_none(), "{payload}");
}

/// `fiddle config check` with `env` restored to the child.
///
/// The mirror of [`check_with`]'s credential-free default, and the half removal
/// alone cannot make: removing a variable shows fiddle does not need it, while
/// supplying one and finding it on no surface shows fiddle does not print it.
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

// ---------------------------------------------------------------------------
// What the manual's configuration example *is* (fiddle-hrjg)
// ---------------------------------------------------------------------------

/// The product manual, read from the repository root.
///
/// `cargo test` runs a test binary with the package directory as its working
/// directory, so the manual is two levels up — the same relative path
/// [`fixture`] takes.
fn manual() -> String {
    std::fs::read_to_string("../../docs/fiddle-agentic-factory-prd.md")
        .expect("the product manual is two levels up from this package")
}

/// The sentence introducing the manual's *reference configuration*: the boundary
/// map across the whole of V1, most of whose tables name milestones that have not
/// shipped.
const REFERENCE_INTRO: &str = "fixes the intended boundaries";

/// The heading introducing the document the manual offers as copyable.
///
/// The `####` is part of the marker deliberately. The sentence a reader meets
/// before the boundary map links to this section by name, and a marker that
/// matched that link text would select the first fence after it — the boundary
/// map, the one block this lane must never be handed.
const COPYABLE_INTRO: &str = "#### The configuration this build loads";

/// The claim the manual makes about the reference configuration, in the manual's
/// own words. Pinned because it is the disposition: a reader who cannot tell a
/// boundary map from a document finds out from an exit code instead.
const COMPOSITE_CLAIM: &str = "it is not a document a deployment can load";

/// The manual's reference configuration, verbatim.
fn reference_configuration() -> String {
    fenced_toml_after(REFERENCE_INTRO)
}

/// The document the manual offers as copyable, verbatim.
fn copyable_configuration() -> String {
    fenced_toml_after(COPYABLE_INTRO)
}

/// The body of the fenced TOML block that the line containing `marker`
/// introduces.
///
/// **Which block a lane reads is said rather than assumed.** The manual carries
/// two TOML fences that mean opposite things — one deliberately does not load,
/// one must — and both declare an `[orchestration.cve]` table. Selecting by a
/// marker in the surrounding prose rather than by ordinal means that moving
/// either block cannot silently point a lane at the other: the panics below fire
/// instead of a plausible wrong answer coming back.
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

/// The table `line` declares, if it declares one.
fn table_header(line: &str) -> Option<&str> {
    let line = line.trim();
    line.strip_prefix('[')
        .filter(|rest| !rest.starts_with('['))
        .and_then(|rest| rest.strip_suffix(']'))
}

/// Every table `document` declares, in order.
fn table_headers(document: &str) -> Vec<&str> {
    document.lines().filter_map(table_header).collect()
}

/// One TOML table of `document`, verbatim: `header` through the line before the
/// next header, or the end.
///
/// The comment lines inside it come along, because they are part of what an
/// operator would copy.
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

/// `document` without the table `header` names, and without its sub-tables.
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

/// The table line `at` (1-based) sits inside, if any.
fn enclosing_table(lines: &[String], at: usize) -> Option<&str> {
    lines[..at].iter().rev().find_map(|line| table_header(line))
}

/// The name serde quoted after `phrase`, as in ``unknown field `repository` ``.
fn refused_name(stderr: &str, phrase: &str) -> Option<String> {
    regex::Regex::new(&format!("{phrase} `([^`]+)`"))
        .unwrap()
        .captures(stderr)
        .map(|found| found[1].to_string())
}

/// The 1-based line the diagnostic points at.
fn refused_line(stderr: &str) -> usize {
    regex::Regex::new(r"fiddle\.toml:(\d+):")
        .unwrap()
        .captures(stderr)
        .unwrap_or_else(|| panic!("every refusal names the line it is about: {stderr}"))[1]
        .parse()
        .unwrap()
}

/// What clearing a document's refusals one at a time cost.
struct Clearing {
    /// One entry per refusal, in the order serde reached them.
    trail: Vec<String>,
    /// Tables the document declared.
    declared: usize,
    /// How many of those were still standing when it finally loaded.
    survived: usize,
}

/// Clear `document`'s refusals one at a time, deleting exactly what the binary's
/// own message points at, until it loads.
///
/// **Mechanical rather than a hand-written list of edits, and that is the whole
/// point.** Strict deserialization reports one unknown or missing field at a
/// time, so every refusal hides the next and the only honest way to count them
/// is to clear each and ask again. A hand-written list would also count, but the
/// number it produced would be a property of the author's choices — dropping a
/// whole section where the message named one key gives a smaller number for the
/// same document. The rule here has no choices in it: an unknown field whose line
/// is a table header costs that table and its sub-tables, any other unknown field
/// costs its own line, and a required table the manual never shows is supplied
/// from the document the manual itself offers as copyable, so nothing in this
/// measurement is invented here.
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

/// **The manual's reference configuration is a composite, and the manual says so
/// where a reader meets it.**
///
/// `fiddle-c64d` made the `[orchestration.cve]` table of that block load and
/// pinned it to the schema. The block as a whole still does not, and it never
/// will: it is the whole of V1 written down at once, so most of its tables name
/// milestones that have not shipped. That leaves two honest dispositions and one
/// dishonest one. Making it load would mean deleting most of the product's own
/// statement of intent, so the manual takes the other: it says plainly that the
/// block is a boundary map rather than a document, and it shows a document
/// beside it. Saying nothing was the third option, and it is the one this lane
/// exists to prevent — after `c64d`, exactly one table of that block is known to
/// be real, and a reader had no way to learn which.
///
/// So this asserts the claim *and* asserts that the claim is true: the block is
/// fed to the binary and must be refused. A manual that called a loadable
/// document a composite would be as wrong as the silence, in the other
/// direction.
#[test]
fn the_manual_says_its_reference_configuration_is_not_a_document_that_loads() {
    let reference = reference_configuration();
    // The extraction is asserted before it is used. `[jira]` is the
    // discriminator rather than decoration: it is in the boundary map and in no
    // loadable document, so a lane that found the copyable block by mistake
    // fails here rather than passing over the wrong bytes.
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

/// **How far the reference configuration is from loadable is measured, and the
/// manual records the measurement.**
///
/// The number is the point. `severities` masked `missing field image` for a whole
/// milestone in this same block, so "it fails at line 13" is never the end of the
/// story — it is the first of however many, and nobody knew how many. This clears
/// them one at a time and makes the manual state the count, so a reader learns
/// the size of the gap rather than the depth of the first hole, and so neither
/// document can move without the other following.
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

/// **The document the manual offers as copyable is one this build loads.**
///
/// The other half of the disposition, and the half that makes it a disposition
/// rather than a disclaimer: a manual whose only example does not load leaves an
/// operator with nothing to start from, so saying "this one is a composite" is
/// only honest beside something that is not.
///
/// The whole block is fed to the binary, not a table of it — that is the
/// difference between this and `c64d`'s per-table lane, and it is what "copyable"
/// has to mean. The extraction is asserted first, and asserted against every
/// table the schema admits: a copyable document that quietly stopped covering
/// `[scanner]` would still load, and would still be a worse starting point than
/// the one this claim was made about.
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
    // Echoed back rather than merely accepted, and each expected value is read
    // out of the block rather than transcribed beside it.
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

/// The string `key` is assigned in `document`, unquoted.
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

/// **The forge table the manual documents names the keys the schema admits.**
///
/// `c64d`'s lane one table over. The `[github]` table of the reference
/// configuration spelled its repository `repository` and its base branch
/// `default_branch`, where the schema shipped `repo` and `base` — the same two
/// settings under different words, which is a transcription defect and not a
/// boundary the build resolved differently. It presented at line 13 of the block
/// as `unknown field \`repository\``, which is the same failure `severities` was.
///
/// Those two are corrected in the manual, and this holds them there. The table is
/// read out of the manual and fed to the binary, so the correction cannot be
/// undone in either document alone: renaming the key back reds here, and so does
/// the schema dropping the name it now admits. Only the table's own keys are
/// taken — `[github.pull_requests]` and `[github.actions]` are tables this build
/// does not have, and they stay in the boundary map for the milestone that brings
/// them.
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

// ---------------------------------------------------------------------------
// The manual that describes the schema, checked against the schema
// ---------------------------------------------------------------------------

/// A document naming every table this schema admits.
///
/// Assembled from the three constants above rather than written out a fourth
/// time, and the forge half is sliced out of [`FORGE`] so the keys with no
/// default are spelled in exactly one place.
fn every_table() -> String {
    let forge = FORGE.split_once("[github]").expect("FORGE names a forge").1;
    format!("{AGENTIC}\n[github]{forge}{SWEEP}")
}

/// **Every table the schema admits is a table the system document names.**
///
/// `docs/technical/SYSTEM.md`'s `fiddle.toml` paragraph is where an operator goes
/// to learn what the deployment document may contain, and this epic edited it to
/// add `[[workspace.checks]]` and nothing else — so `[scanner]`, which carries two
/// of the four credentials, and `[orchestration.cve]`, which decides what a sweep
/// does, were absent from the one paragraph that enumerates the document. A
/// deliver-phase bean recorded that rather than fixing it, which is how a second
/// reader came to find it again.
///
/// The section list comes from the binary rather than from a constant here, for
/// the reason the lane beside it reads the manual's own bytes: a transcription is
/// the thing that drifts. `config check --json` echoes one key per table the
/// document carried, so a table added to the schema and to nobody's prose reds
/// here.
///
/// `[orchestration.cve]` is matched as a prefix because the nesting is the PRD's
/// spelling and the paragraph names the sub-table an operator actually writes,
/// not the parent.
#[test]
fn the_system_document_names_every_table_this_schema_admits() {
    let payload = checked(&every_table());
    // One key per table, and the two scalars — `schema` and `status` — are not
    // tables. Discriminated by *shape* rather than by name, so a third scalar
    // added to the payload does not arrive here as a table nobody documented.
    let sections: Vec<String> = payload
        .as_object()
        .expect("the payload is an object")
        .iter()
        .filter(|(_, value)| value.is_object())
        .map(|(key, _)| key.clone())
        .collect();
    // Non-vacuity: the document above carries every table, so a payload that
    // echoed only the three M0 ones would mean this lane is checking almost
    // nothing.
    for expected in ["agent", "workspace", "github", "scanner", "orchestration"] {
        assert!(
            sections.iter().any(|section| section == expected),
            "the document handed over names `{expected}` and the payload does \
             not echo it, so this lane is checking a shorter schema than there \
             is: {sections:?}"
        );
    }

    let document = std::fs::read_to_string(support::repo_root().join("docs/technical/SYSTEM.md"))
        .expect("the system document is part of the repository");
    let paragraph = document
        .lines()
        .find(|line| line.starts_with("**`fiddle.toml`**"))
        .expect("the system document describes the deployment document");

    for section in &sections {
        assert!(
            paragraph.contains(&format!("[{section}]"))
                || paragraph.contains(&format!("[{section}.")),
            "`[{section}]` is a table this schema admits and the `fiddle.toml` \
             paragraph of docs/technical/SYSTEM.md does not name it"
        );
    }
}
