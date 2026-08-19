use fiddle_runtime::effect::EffectOutcome;
use fiddle_runtime::github::{GhCli, GhError, RetryAdvice};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(30);

fn gh(dir: &Path, token: &str, timeout: Duration) -> GhCli {
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

fn script(dir: &Path, key: &str, spec: &str) {
    std::fs::create_dir_all(dir.join("script")).unwrap();
    std::fs::write(dir.join("script").join(key), spec).unwrap();
}

fn body() -> serde_json::Value {
    serde_json::json!({ "title": "a change", "head": "fiddle/abc", "base": "main" })
}

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
    for credential in ["LITELLM_API_KEY", "GITHUB_TOKEN", "GH_STUB_DIR"] {
        assert!(
            !seen.contains_key(credential),
            "{credential} reached gh: {seen:?}"
        );
    }
}

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

    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), "ghp_whatever", "500 1 normal")
        .await
        .unwrap_err();
    assert!(
        matches!(err, GhError::Http { status: 500, .. }),
        "got {err:?}"
    );
}

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

#[tokio::test]
async fn exit_four_is_authentication_and_exit_two_is_cancellation() {
    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), "ghp_whatever", "200 4 normal")
        .await
        .unwrap_err();
    assert!(matches!(err, GhError::Auth), "got {err:?}");
    assert_eq!(
        err.outcome(),
        EffectOutcome::NotCommitted,
        "exit 4 is `gh` refusing before it dispatches: {err:?}"
    );

    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), "ghp_whatever", "200 2 normal")
        .await
        .unwrap_err();
    assert!(matches!(err, GhError::CancelledAfterSpawn), "got {err:?}");
    assert_eq!(
        err.outcome(),
        EffectOutcome::Unknown,
        "a child that ran and reported a cancellation instead of an answer is \
         an ambiguous write: {err:?}"
    );
}

#[tokio::test]
async fn a_cancelled_attempt_never_reaches_the_network() {
    let dir = TempDir::new().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = gh(dir.path(), "ghp_whatever", PATIENT)
        .api("POST", "/repos/o/r/pulls", Some(&body()), &cancel)
        .await
        .unwrap_err();
    assert!(matches!(err, GhError::CancelledBeforeSpawn), "got {err:?}");
    assert!(
        !dir.path().join("requests").exists(),
        "a cancelled call must not have reached the child at all"
    );
    assert_eq!(
        err.outcome(),
        EffectOutcome::NotCommitted,
        "nothing ran, so this is the one cancellation that is knowledge: {err:?}"
    );
    assert!(
        !err.is_worth_reading_again(),
        "and there is nothing to go and look for: {err:?}"
    );
}

#[tokio::test]
async fn a_cancellation_after_the_child_was_spawned_is_an_ambiguous_write() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("sleep_ms"), "10000").unwrap();
    script(dir.path(), "POST_repos_o_r_pulls", "201 0 normal");

    let cancel = CancellationToken::new();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        canceller.cancel();
    });

    let err = gh(dir.path(), "ghp_whatever", PATIENT)
        .api("POST", "/repos/o/r/pulls", Some(&body()), &cancel)
        .await
        .expect_err("a cancelled call is a failure");

    assert!(
        dir.path().join("requests").join("0000.json").exists(),
        "the child must have run before the token cancelled, or this is the \
         pre-spawn case wearing the other name"
    );
    assert!(matches!(err, GhError::CancelledAfterSpawn), "got {err:?}");
    assert_eq!(
        err.outcome(),
        EffectOutcome::Unknown,
        "a request that may already have landed is not a refusal: {err:?}"
    );
    assert!(
        err.is_worth_reading_again(),
        "and the only thing that settles it is looking: {err:?}"
    );
    assert_ne!(
        err.outcome(),
        GhError::CancelledBeforeSpawn.outcome(),
        "the two provenances of one cancellation must not classify alike"
    );
}

#[test]
fn a_lost_answer_is_unknown_and_a_refusal_is_not_committed() {
    let http = |status| GhError::Http {
        status,
        message: String::new(),
        advice: RetryAdvice::default(),
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
    assert_eq!(http(422).outcome(), EffectOutcome::Unknown);

    assert_eq!(
        GhError::Killed("137".to_string()).outcome(),
        EffectOutcome::Unknown
    );
    assert_eq!(
        GhError::Duplicate { count: 2 }.outcome(),
        EffectOutcome::Unknown
    );

    assert_eq!(GhError::Auth.outcome(), EffectOutcome::NotCommitted);
    assert_eq!(
        GhError::CancelledBeforeSpawn.outcome(),
        EffectOutcome::NotCommitted
    );
    assert_eq!(
        GhError::NotSent(String::new()).outcome(),
        EffectOutcome::NotCommitted
    );

    assert_eq!(
        GhError::CancelledAfterSpawn.outcome(),
        EffectOutcome::Unknown
    );
    assert_eq!(
        GhError::Malformed(String::new()).outcome(),
        EffectOutcome::Unknown
    );
}

#[tokio::test]
async fn a_child_that_died_before_answering_is_unknown() {
    for mode in ["commit_then_die", "commit_then_abort"] {
        let dir = TempDir::new().unwrap();
        let err = post_scripted(dir.path(), "ghp_whatever", &format!("201 0 {mode}"))
            .await
            .unwrap_err();

        assert!(matches!(err, GhError::Killed(_)), "{mode}: got {err:?}");
        assert_eq!(err.outcome(), EffectOutcome::Unknown, "{mode}");
        assert!(
            err.is_worth_reading_again(),
            "{mode}: and the lost answer is settled by looking, which is what \
             separates it from a runner that will not repair itself"
        );

        let world = std::fs::read_to_string(dir.path().join("world")).unwrap_or_default();
        assert!(
            world.contains("POST_repos_o_r_pulls"),
            "{mode}: the write must have landed before the child died, or this \
             test is asserting about a failed write"
        );
    }
}

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
    let request = std::fs::read_to_string(dir.path().join("requests").join("0000.json")).unwrap();
    assert!(
        request.contains(SENTINEL),
        "the stub must have received the credential for this test to mean anything"
    );
}

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

#[tokio::test]
async fn the_retry_and_rate_limit_headers_are_read_by_name() {
    let dir = TempDir::new().unwrap();
    let response = post_scripted(dir.path(), "ghp_whatever", "201 0 rate_limited")
        .await
        .expect("a 201 is a success whatever its headers say");

    assert_eq!(response.retry_after, Some(Duration::from_secs(60)));
    assert_eq!(response.rate_limit_remaining, Some(0));
}

#[tokio::test]
async fn a_garbled_response_is_a_lost_answer_and_not_a_refusal() {
    const SENTINEL: &str = "ghp_sentinel_must_not_appear_anywhere";
    let dir = TempDir::new().unwrap();
    let err = post_scripted(dir.path(), SENTINEL, "200 1 garbage")
        .await
        .unwrap_err();

    assert!(matches!(err, GhError::Malformed(_)), "got {err:?}");
    assert_eq!(
        err.outcome(),
        EffectOutcome::Unknown,
        "a client that cannot read the answer has not learned that the write \
         failed: {err:?}"
    );
    assert!(
        !err.is_worth_reading_again(),
        "and a program that is not `gh` will not become one: {err:?}"
    );
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
