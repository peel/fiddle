//! The `gh` adapter: what the child sees, and what a lost answer means.
//!
//! Every case here is driven through the product's own `cli.program` seam
//! against the scripted `gh` in `tests/gh_stub/`, so the suite is offline,
//! credential-free and deterministic. Nothing here reaches GitHub.
//!
//! Two properties are being defended, and they are not the same one. The first
//! is *containment*: exactly five environment names reach the child and `HOME`
//! is not among them, which is what makes "this adapter used the credential it
//! was given and no other" a fact rather than a promise. The second is
//! *honesty about ambiguity*: when a mutating request's answer is lost, the
//! adapter says it does not know, so the caller goes and looks instead of
//! guessing. Every classification that turns an unknown into a confident wrong
//! answer produces a duplicate external effect.

use fiddle_runtime::effect::EffectOutcome;
use fiddle_runtime::github::{GhCli, GhError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// A generous bound for a stub that answers immediately. The one test that is
/// about the deadline sets its own.
const PATIENT: Duration = Duration::from_secs(30);

/// A `GhCli` pointed at the scripted `gh`, with a scratch directory for both the
/// script and `GH_CONFIG_DIR`.
///
/// The stub's own scratch directory arrives through `cli.args` and not through
/// the environment, and that is not an accident of convenience: the environment
/// the adapter builds has room for exactly five names, so a sixth could not
/// reach the child even if the fixture wanted one. The plumbing being forced
/// into `argv` is the first piece of evidence that the boundary below is real.
fn gh(dir: &Path, token: &str, timeout: Duration) -> GhCli {
    // Empty, and stays empty: an empty GH_CONFIG_DIR beside an absent HOME is
    // what makes a real `gh` refuse rather than fall back to a stored
    // credential.
    let config = dir.join("config");
    std::fs::create_dir_all(&config).unwrap();
    GhCli::new(
        PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
        vec!["--stub-dir".to_string(), dir.display().to_string()],
        token.to_string(),
        "FIDDLE_GITHUB_TOKEN",
        config,
        timeout,
    )
}

/// Script one request key with `<status> <exit> <mode>`.
fn script(dir: &Path, key: &str, spec: &str) {
    std::fs::create_dir_all(dir.join("script")).unwrap();
    std::fs::write(dir.join("script").join(key), spec).unwrap();
}

fn body() -> serde_json::Value {
    serde_json::json!({ "title": "a change", "head": "fiddle/abc", "base": "main" })
}

/// Run one scripted `POST /repos/o/r/pulls` and return what the adapter made of
/// it.
async fn post_scripted(
    dir: &Path,
    token: &str,
    spec: &str,
) -> Result<fiddle_runtime::github::GhResponse, GhError> {
    script(dir, "POST_repos_o_r_pulls", spec);
    gh(dir, token, PATIENT)
        .api(
            "POST",
            "/repos/o/r/pulls",
            Some(&body()),
            &CancellationToken::new(),
        )
        .await
}

/// The environment of the first request the stub recorded, by name.
fn recorded_environment(dir: &Path) -> BTreeMap<String, String> {
    let request = std::fs::read_to_string(dir.join("requests").join("0000.json"))
        .expect("the stub records every request it receives");
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    request["env"]
        .as_array()
        .expect("the stub records the environment it was given")
        .iter()
        .map(|entry| {
            let entry = entry.as_str().unwrap();
            let (name, value) = entry.split_once('=').unwrap();
            (name.to_string(), value.to_string())
        })
        .collect()
}

/// The environment is the security boundary, so it is pinned exactly — the same
/// move `workspace::a_workspace_command_inherits_no_credential` makes for the
/// four-name workspace set. A sixth name cannot be added without changing this
/// assertion, and a fifth cannot be dropped either.
///
/// Asserted against what the *child* received rather than against what the
/// builder was asked to set, because those are different claims and only the
/// first one is the guarantee.
#[tokio::test]
async fn the_gh_environment_is_exactly_five_names_and_no_home() {
    let dir = TempDir::new().unwrap();
    gh(dir.path(), "ghp_whatever", PATIENT)
        .api("GET", "/repos/o/r/pulls", None, &CancellationToken::new())
        .await
        .expect("a scripted read answers 200");

    let seen = recorded_environment(dir.path());
    let names: Vec<&str> = seen.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "GH_CONFIG_DIR",
            "GH_PROMPT_DISABLED",
            "GH_TOKEN",
            "NO_COLOR",
            "PATH"
        ],
        "HOME is deliberately absent: it is what makes the operator's keyring \
         unreachable, and a sixth name here is a change to the security boundary"
    );
    assert_eq!(
        seen.get("GH_TOKEN").map(String::as_str),
        Some("ghp_whatever"),
        "the credential reaches the child through the environment and not through argv"
    );
}

/// The rule the previous test states, applied to the one variable that would
/// undo it. `HOME` is what `gh` follows to `~/.config/gh`, so its absence is the
/// difference between a credential source that is pinned and one that merely
/// happens to be pinned today.
#[tokio::test]
async fn no_credential_of_this_process_survives_into_gh() {
    let dir = TempDir::new().unwrap();
    gh(dir.path(), "ghp_whatever", PATIENT)
        .api("GET", "/repos/o/r/pulls", None, &CancellationToken::new())
        .await
        .expect("a scripted read answers 200");

    let seen = recorded_environment(dir.path());
    assert!(
        !seen.contains_key("HOME"),
        "HOME reopens the operator's keyring: {seen:?}"
    );
    // The two the runner itself holds, named so that a regression is legible
    // rather than only a set mismatch.
    for credential in ["LITELLM_API_KEY", "GITHUB_TOKEN", "GH_STUB_DIR"] {
        assert!(
            !seen.contains_key(credential),
            "{credential} reached gh: {seen:?}"
        );
    }
}

/// The credential must never be an argument. `/proc/<pid>/cmdline` is
/// world-readable on Linux, so a token in `argv` is a token any user on the box
/// can read for as long as the process lives.
#[tokio::test]
async fn the_credential_is_never_an_argument() {
    const TOKEN: &str = "ghp_argv_sentinel_must_not_appear";
    let dir = TempDir::new().unwrap();
    let _ = post_scripted(dir.path(), TOKEN, "201 0 normal").await;

    let request = std::fs::read_to_string(dir.path().join("requests").join("0000.json")).unwrap();
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert!(
        !request["argv"].to_string().contains(TOKEN),
        "the credential appeared in argv: {}",
        request["argv"]
    );
    assert_eq!(
        request["body"].as_str().unwrap(),
        body().to_string(),
        "the request body goes to stdin behind `--input -`, not into argv"
    );
}

/// `gh` documents exit 1 for every HTTP failure regardless of status, so the
/// status must come from the `-i` status line. An adapter that branches on the
/// exit code has read the wrong surface — and would report a 404, a 422 and a
/// 500 as the same thing, which is three different outcomes collapsed into one.
#[tokio::test]
async fn the_http_status_comes_from_the_status_line_not_the_exit_code() {
    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), "ghp_whatever", "422 1 normal")
        .await
        .unwrap_err();
    assert!(
        matches!(err, GhError::Http { status: 422, .. }),
        "got {err:?}"
    );

    // The same exit code, a different status. If the exit code were being read,
    // these two would be indistinguishable.
    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), "ghp_whatever", "500 1 normal")
        .await
        .unwrap_err();
    assert!(
        matches!(err, GhError::Http { status: 500, .. }),
        "got {err:?}"
    );
}

/// A success is a success on the status line too, and what it carries is the
/// parsed body rather than the raw stream.
#[tokio::test]
async fn a_successful_call_returns_the_parsed_status_and_body() {
    let dir = TempDir::new().unwrap();
    let response = gh(dir.path(), "ghp_whatever", PATIENT)
        .api("GET", "/repos/o/r/pulls", None, &CancellationToken::new())
        .await
        .expect("a scripted read answers 200");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, serde_json::json!([]));
}

/// The two exit codes that mean something on their own, and the only two the
/// adapter is allowed to read as answers.
#[tokio::test]
async fn exit_four_is_authentication_and_exit_two_is_cancellation() {
    let dir = TempDir::new().unwrap();
    // A 200 status line beside a non-zero exit, so this asserts the exit code
    // wins for these two rather than merely agreeing with the status.
    let err = post_scripted(dir.path(), "ghp_whatever", "200 4 normal")
        .await
        .unwrap_err();
    assert!(matches!(err, GhError::Auth), "got {err:?}");

    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), "ghp_whatever", "200 2 normal")
        .await
        .unwrap_err();
    assert!(matches!(err, GhError::Cancelled), "got {err:?}");
}

/// A cancelled attempt does not start a mutating request. Checked before the
/// spawn, because a request that has already been sent cannot be un-sent.
#[tokio::test]
async fn a_cancelled_attempt_never_reaches_the_network() {
    let dir = TempDir::new().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = gh(dir.path(), "ghp_whatever", PATIENT)
        .api("POST", "/repos/o/r/pulls", Some(&body()), &cancel)
        .await
        .unwrap_err();
    assert!(matches!(err, GhError::Cancelled), "got {err:?}");
    assert!(
        !dir.path().join("requests").exists(),
        "a cancelled call must not have reached the child at all"
    );
}

/// The classification the milestone turns on. A lost answer is `Unknown`; an
/// explicit refusal is `NotCommitted`.
#[test]
fn a_lost_answer_is_unknown_and_a_refusal_is_not_committed() {
    let http = |status| GhError::Http {
        status,
        message: String::new(),
    };

    assert_eq!(
        GhError::Timeout(Duration::from_secs(1)).outcome(),
        EffectOutcome::Unknown
    );
    assert_eq!(http(500).outcome(), EffectOutcome::Unknown);
    assert_eq!(http(502).outcome(), EffectOutcome::Unknown);
    assert_eq!(http(403).outcome(), EffectOutcome::NotCommitted);
    assert_eq!(http(404).outcome(), EffectOutcome::NotCommitted);
    assert_eq!(http(401).outcome(), EffectOutcome::NotCommitted);
    // 422 is overloaded — malformed input, invalid ref syntax, spam protection
    // and "already exists" all wear it — and is never classified on its face: it
    // is Unknown so that the caller is forced into the postcondition read that
    // can actually tell a refusal from a duplicate.
    assert_eq!(http(422).outcome(), EffectOutcome::Unknown);

    assert_eq!(GhError::Auth.outcome(), EffectOutcome::NotCommitted);
    assert_eq!(GhError::Cancelled.outcome(), EffectOutcome::NotCommitted);
    assert_eq!(
        GhError::Duplicate { count: 2 }.outcome(),
        EffectOutcome::Unknown
    );
}

/// The specific case the exactly-once harness depends on. A child killed on the
/// way back is `Unknown`, and it must not be mistaken for a malformed response —
/// which classifies `NotCommitted` and would report a landed write as failed,
/// producing the duplicate on the next attempt.
///
/// Both spellings of a dead child are driven, because the adapter must not
/// depend on which one it happens to get: an exit code at or above 128 is what a
/// wrapper passes on, and `None` is what a real signal leaves behind.
#[tokio::test]
async fn a_child_that_died_before_answering_is_unknown_not_malformed() {
    for mode in ["commit_then_die", "commit_then_abort"] {
        let dir = TempDir::new().unwrap();
        let err = post_scripted(dir.path(), "ghp_whatever", &format!("201 0 {mode}"))
            .await
            .unwrap_err();

        assert!(matches!(err, GhError::Killed(_)), "{mode}: got {err:?}");
        assert_eq!(err.outcome(), EffectOutcome::Unknown, "{mode}");
        assert_ne!(
            err.outcome(),
            GhError::Malformed(String::new()).outcome(),
            "{mode}: a killed child and a garbled response are different states, \
             and classifying them alike is what turns a landed write into a \
             duplicate on the retry"
        );

        // The half that makes this a real ambiguity rather than a simulated one:
        // the mutation is on disk, and the answer is gone.
        let world = std::fs::read_to_string(dir.path().join("world")).unwrap_or_default();
        assert!(
            world.contains("POST_repos_o_r_pulls"),
            "{mode}: the write must have landed before the child died, or this \
             test is asserting about a failed write"
        );
    }
}

/// `gh` has no timeout flag, so the runtime owns it — in its own process group,
/// through the same bounded runner M1's workspace commands use.
///
/// The two marker files are the point. A parent that merely stopped waiting
/// would also return `Timeout`; only a child that was actually killed leaves no
/// marker behind, and an orphaned `gh` still holding a credential is exactly
/// what this prevents.
///
/// The *descendant's* marker is the one that pins the process group. `gh` is
/// free to fork, and `kill_on_drop` reaps only the process this runtime holds a
/// handle to — so without a kill aimed at the whole group, the grandchild
/// outlives the deadline and writes it.
#[tokio::test]
async fn a_gh_that_never_returns_is_killed_and_reported_as_unknown() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("sleep_ms"), "2000").unwrap();

    let started = std::time::Instant::now();
    let err = gh(dir.path(), "ghp_whatever", Duration::from_millis(150))
        .api(
            "POST",
            "/repos/o/r/pulls",
            Some(&body()),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, GhError::Timeout(_)), "got {err:?}");
    assert_eq!(err.outcome(), EffectOutcome::Unknown);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the deadline is the runtime's, so it fires without waiting for the child"
    );

    // Well past the point the children would have finished sleeping had they
    // lived.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !dir.path().join("survived_the_deadline").exists(),
        "the timed-out gh was left running: a killed child cannot write this"
    );
    assert!(
        !dir.path().join("descendant_survived_the_deadline").exists(),
        "the timed-out gh's own child outlived the deadline: killing the direct \
         process is not enough, the whole process group has to go"
    );
}

/// The credential must not reach a diagnostic, which is the surface that reaches
/// a bundle. Same sentinel discipline as `capability_selection.rs`, and the same
/// defect class M1 shipped: a response body that echoed the key it received.
///
/// The stub echoes the token into the response body on purpose. An adapter that
/// carried a body into its error — which is the natural thing to write — fails
/// here rather than passing because nothing happened to be echoed.
#[tokio::test]
async fn the_token_value_appears_in_no_error_message() {
    const SENTINEL: &str = "ghp_sentinel_must_not_appear_anywhere";
    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), SENTINEL, "401 1 echo_token")
        .await
        .unwrap_err();

    assert!(
        matches!(err, GhError::Http { status: 401, .. }),
        "got {err:?}"
    );
    assert!(
        !format!("{err}").contains(SENTINEL),
        "Display leaked the credential: {err}"
    );
    assert!(
        !format!("{err:?}").contains(SENTINEL),
        "Debug leaked the credential: {err:?}"
    );
    // Proof the stub really did echo it, so the assertions above are testing the
    // redaction rather than an empty body.
    let request = std::fs::read_to_string(dir.path().join("requests").join("0000.json")).unwrap();
    assert!(
        request.contains(SENTINEL),
        "the stub must have received the credential for this test to mean anything"
    );
}

/// The other rendering that reaches an operator. `Debug` on the client itself is
/// what a `dbg!` or a tracing attribute reaches for by default, so it names the
/// variable the credential came from and never the credential.
#[test]
fn the_client_names_the_variable_it_read_and_never_the_value() {
    const SENTINEL: &str = "ghp_sentinel_must_not_appear_anywhere";
    let dir = TempDir::new().unwrap();
    let cli = gh(dir.path(), SENTINEL, PATIENT);

    let rendered = format!("{cli:?}");
    assert!(
        !rendered.contains(SENTINEL),
        "Debug on the client leaked the credential: {rendered}"
    );
    assert!(
        rendered.contains("FIDDLE_GITHUB_TOKEN"),
        "an operator has to learn which variable to fix: {rendered}"
    );
    assert_eq!(cli.variable(), "FIDDLE_GITHUB_TOKEN");
}

/// `-i` is what makes a CLI workable here rather than a compromise: it yields
/// everything a native HTTP client would have had, and these two are the ones a
/// backoff will need.
///
/// Read by header *name*, which is the part worth testing. GitHub's real
/// response carries an `Access-Control-Expose-Headers` whose value lists both of
/// these names — a parser that searched the block for the strings would report a
/// retry delay nobody sent. The header block the stub emits here is the shape
/// taken from a probe of the real binary, including that trap.
#[tokio::test]
async fn the_retry_and_rate_limit_headers_are_read_by_name() {
    let dir = TempDir::new().unwrap();
    let response = post_scripted(dir.path(), "ghp_whatever", "201 0 rate_limited")
        .await
        .expect("a 201 is a success whatever its headers say");

    assert_eq!(response.retry_after, Some(Duration::from_secs(60)));
    assert_eq!(response.rate_limit_remaining, Some(0));
}

/// Something that is not a response is the runner being wrong, not GitHub
/// refusing — so it is `NotCommitted`, and it must stay distinguishable from the
/// killed child above.
///
/// The process ran to a normal completion and produced garbage, which is what
/// `cli.program` pointing at something that is not `gh` looks like. That is a
/// misconfiguration to fix, not an ambiguous write to go and investigate.
#[tokio::test]
async fn a_garbled_response_is_a_broken_runner_and_not_an_ambiguous_write() {
    const SENTINEL: &str = "ghp_sentinel_must_not_appear_anywhere";
    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), SENTINEL, "200 1 garbage")
        .await
        .unwrap_err();

    assert!(matches!(err, GhError::Malformed(_)), "got {err:?}");
    assert_eq!(err.outcome(), EffectOutcome::NotCommitted);
    // The diagnostic has to be actionable — stdout alone is silent about a
    // `program` that is not `gh`, so stderr is quoted — and quoting a second
    // stream is a second place the credential could escape.
    assert!(
        format!("{err}").contains("could not authenticate"),
        "an operator cannot fix this without what the program actually said: {err}"
    );
    for rendered in [format!("{err}"), format!("{err:?}")] {
        assert!(
            !rendered.contains(SENTINEL),
            "the credential reached a diagnostic through stderr: {rendered}"
        );
    }
}
