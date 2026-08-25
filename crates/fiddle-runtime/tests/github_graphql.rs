use fiddle_runtime::effect::{AdapterError, EffectOutcome, EffectPhase};
use fiddle_runtime::github::{GhCli, GhError};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(30);

const TEST_TOKEN: &str = "ghp_graphql_sentinel_must_not_appear";

const MUTATION: &str = "mutation($id: ID!) { markPullRequestReadyForReview(input: \
                        {pullRequestId: $id}) { pullRequest { isDraft } } }";

fn uncancelled() -> CancellationToken {
    CancellationToken::new()
}

struct World {
    dir: TempDir,
}

impl World {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    fn gh(&self) -> GhCli {
        let config = self.dir.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        GhCli::new(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            vec![
                "--stub-dir".to_string(),
                self.dir.path().display().to_string(),
            ],
            TEST_TOKEN.to_string(),
            "FIDDLE_GITHUB_TOKEN",
            config,
            PATIENT,
        )
    }

    fn script_graphql(&self, status: u16, body: &str) {
        let scripts = self.dir.path().join("graphql");
        std::fs::create_dir_all(&scripts).unwrap();
        let n = std::fs::read_dir(&scripts).unwrap().count();
        let body: serde_json::Value =
            serde_json::from_str(body).expect("a scripted body is JSON, as GitHub's is");
        std::fs::write(
            scripts.join(format!("{n}.json")),
            serde_json::json!({ "status": status, "body": body }).to_string(),
        )
        .unwrap();
    }

    fn recorded(&self, n: usize) -> serde_json::Value {
        let request = std::fs::read_to_string(
            self.dir
                .path()
                .join("requests")
                .join(format!("{n:04}.json")),
        )
        .expect("the stub records every request it receives");
        serde_json::from_str(&request).unwrap()
    }

    fn recorded_env(&self, n: usize) -> Vec<String> {
        strings(&self.recorded(n)["env"])
    }

    fn recorded_argv(&self, n: usize) -> Vec<String> {
        strings(&self.recorded(n)["argv"])
    }
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("the stub records this as an array")
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .collect()
}

fn names(env: &[String]) -> Vec<&str> {
    let mut names: Vec<&str> = env
        .iter()
        .map(|entry| entry.split_once('=').unwrap().0)
        .collect();
    names.sort_unstable();
    names
}

#[tokio::test]
async fn a_200_carrying_errors_is_a_refusal_and_not_a_success() {
    let world = World::new();
    world.script_graphql(
        200,
        r#"{"data":{"markPullRequestReadyForReview":null},
            "errors":[{"type":"NOT_FOUND",
                       "message":"Could not resolve to a node with the global id of 'PR_x'"}]}"#,
    );

    let err = world
        .gh()
        .graphql(MUTATION, &[("id", "PR_x")], &uncancelled())
        .await
        .expect_err("a 200 that refused the mutation is not a success");

    assert!(
        matches!(&err, GhError::GraphQl { kind, .. } if kind == "NOT_FOUND"),
        "the classification is the error's own type and never the status: got {err:?}"
    );
    assert_eq!(
        err.outcome(EffectPhase::Apply),
        EffectOutcome::NotCommitted,
        "GitHub could not resolve the node, so nothing was reached to mutate: {err:?}"
    );
    assert!(
        !err.is_worth_reading_again(),
        "and a node that was not found will not be found by asking again: {err:?}"
    );
}

#[tokio::test]
async fn each_error_kind_classifies_by_what_it_settles() {
    for (kind, expected) in [
        ("NOT_FOUND", EffectOutcome::NotCommitted),
        ("FORBIDDEN", EffectOutcome::NotCommitted),
        ("UNPROCESSABLE", EffectOutcome::Unknown),
        ("SERVICE_UNAVAILABLE", EffectOutcome::Unknown),
        ("SOMETHING_GITHUB_ADDS_LATER", EffectOutcome::Unknown),
    ] {
        let world = World::new();
        world.script_graphql(
            200,
            &format!(r#"{{"data":null,"errors":[{{"type":"{kind}","message":"m"}}]}}"#),
        );

        let err = world
            .gh()
            .graphql(MUTATION, &[], &uncancelled())
            .await
            .unwrap_err();

        assert_eq!(
            err.outcome(EffectPhase::Apply),
            expected,
            "{kind}: got {err:?}"
        );
        assert_eq!(
            err.is_worth_reading_again(),
            expected == EffectOutcome::Unknown,
            "{kind}: only an unsettled refusal is worth looking at again"
        );
    }
}

#[tokio::test]
async fn an_error_with_no_type_at_all_is_unknown() {
    let world = World::new();
    world.script_graphql(200, r#"{"errors":[{"message":"no type field here"}]}"#);

    let err = world
        .gh()
        .graphql(MUTATION, &[], &uncancelled())
        .await
        .unwrap_err();

    assert_eq!(
        err.outcome(EffectPhase::Apply),
        EffectOutcome::Unknown,
        "a refusal this build cannot name is not evidence about the world: {err:?}"
    );
}

#[tokio::test]
async fn an_errors_field_that_is_not_an_array_is_a_refusal_and_unknown() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":null,"errors":"something unexpected"}"#);

    let err = world
        .gh()
        .graphql(MUTATION, &[], &uncancelled())
        .await
        .expect_err("a body this client cannot read is not a success");

    assert_eq!(
        err.outcome(EffectPhase::Apply),
        EffectOutcome::Unknown,
        "got {err:?}"
    );
}

#[tokio::test]
async fn a_200_whose_body_cannot_be_interpreted_is_unknown_and_not_a_success() {
    for body in ["null", "[]", r#""a string""#, "42", "true"] {
        let world = World::new();
        world.script_graphql(200, body);

        let Err(err) = world.gh().graphql(MUTATION, &[], &uncancelled()).await else {
            panic!("body {body} must not read as a success");
        };

        assert_eq!(
            err.outcome(EffectPhase::Apply),
            EffectOutcome::Unknown,
            "body {body} is a lost answer, not a mutation that did not happen: {err:?}"
        );
        assert!(
            err.is_worth_reading_again(),
            "body {body} left the question open, so asking it again may answer it: {err:?}"
        );
    }
}

#[tokio::test]
async fn a_200_with_data_is_still_a_claimed_success() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":{"x":1}}"#);

    let value = world
        .gh()
        .graphql(MUTATION, &[], &uncancelled())
        .await
        .expect("an object carrying data and no errors is a claimed success");
    assert_eq!(value["x"], 1);
}

#[tokio::test]
async fn an_unscripted_graphql_call_cannot_pass_for_an_answer() {
    let world = World::new();

    let Err(err) = world.gh().graphql(MUTATION, &[], &uncancelled()).await else {
        panic!("a call the fixture never scripted must not read as an answer");
    };

    assert!(
        matches!(err, GhError::Malformed(_)),
        "an unanswered call is a fixture fault and not a classified refusal: {err:?}"
    );
    let reported = err.to_string();
    assert!(
        reported.contains("nothing scripted at") && reported.contains("0.json"),
        "the diagnostic has to name the file a test forgot to write: {reported}"
    );
}

#[tokio::test]
async fn a_200_with_data_and_no_errors_returns_the_data() {
    let world = World::new();
    world.script_graphql(
        200,
        r#"{"data":{"markPullRequestReadyForReview":{"pullRequest":{"isDraft":false}}}}"#,
    );

    let value = world
        .gh()
        .graphql(MUTATION, &[("id", "PR_x")], &uncancelled())
        .await
        .expect("a 200 with data and no errors is a success");

    assert_eq!(
        value["markPullRequestReadyForReview"]["pullRequest"]["isDraft"], false,
        "the data is returned unwrapped, so a caller reads the mutation's own \
         field rather than reaching through an envelope"
    );
}

#[tokio::test]
async fn an_empty_errors_array_is_not_a_refusal() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":{"x":1},"errors":[]}"#);

    let value = world
        .gh()
        .graphql(MUTATION, &[], &uncancelled())
        .await
        .expect("an empty errors array says nothing was refused");
    assert_eq!(value["x"], 1);
}

#[tokio::test]
async fn a_transport_failure_is_unknown_exactly_as_it_is_for_api() {
    let world = World::new();
    world.script_graphql(502, r#"{"message":"Bad gateway"}"#);

    let err = world
        .gh()
        .graphql(MUTATION, &[], &uncancelled())
        .await
        .unwrap_err();

    assert!(
        matches!(err, GhError::Http { status: 502, .. }),
        "a status at or above 400 is read from the status line as it always was: got {err:?}"
    );
    assert_eq!(err.outcome(EffectPhase::Apply), EffectOutcome::Unknown);
    assert!(err.is_worth_reading_again());
}

#[tokio::test]
async fn a_cancelled_mutation_never_reaches_the_child() {
    let world = World::new();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = world
        .gh()
        .graphql(MUTATION, &[("id", "PR_x")], &cancel)
        .await
        .unwrap_err();

    assert!(matches!(err, GhError::CancelledBeforeSpawn), "got {err:?}");
    assert_eq!(err.outcome(EffectPhase::Apply), EffectOutcome::NotCommitted);
    assert!(
        !world.dir.path().join("requests").exists(),
        "a cancelled mutation must not have reached the child at all"
    );
}

#[tokio::test]
async fn the_graphql_environment_is_the_same_five_names_and_no_home() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":{}}"#);
    world
        .gh()
        .graphql(MUTATION, &[("id", "PR_x")], &uncancelled())
        .await
        .expect("a scripted mutation answers 200");

    let env = world.recorded_env(0);
    assert_eq!(
        names(&env),
        [
            "GH_CONFIG_DIR",
            "GH_PROMPT_DISABLED",
            "GH_TOKEN",
            "NO_COLOR",
            "PATH"
        ],
        "a second call shape must not be a second environment: {env:?}"
    );
    assert!(
        !env.iter().any(|entry| entry.starts_with("HOME=")),
        "HOME reopens the operator's keyring: {env:?}"
    );
    assert!(
        env.iter()
            .any(|entry| entry == &format!("GH_TOKEN={TEST_TOKEN}")),
        "the credential reaches the child through the environment: {env:?}"
    );
}

#[tokio::test]
async fn no_credential_reaches_argv() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":{}}"#);
    world
        .gh()
        .graphql(MUTATION, &[("id", "PR_x")], &uncancelled())
        .await
        .expect("a scripted mutation answers 200");

    let argv = world.recorded_argv(0);
    assert!(
        argv.iter().any(|arg| arg == "graphql"),
        "the endpoint is `graphql`, or this ran some other call: {argv:?}"
    );
    assert!(
        argv.iter().any(|arg| arg == "-i"),
        "`-i` is what makes the status line readable, and it is not optional here: {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg.contains(TEST_TOKEN)),
        "the credential appeared in argv, which /proc/<pid>/cmdline makes \
         world-readable: {argv:?}"
    );
}

#[tokio::test]
async fn a_variable_is_its_own_argument_and_never_spliced_into_the_query() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":{}}"#);
    world
        .gh()
        .graphql(MUTATION, &[("id", "PR_kwDOabc\" }")], &uncancelled())
        .await
        .expect("a scripted mutation answers 200");

    let argv = world.recorded_argv(0);
    assert!(
        argv.iter().any(|arg| arg == &format!("query={MUTATION}")),
        "the query goes out exactly as written, `$id` unresolved: {argv:?}"
    );
    assert!(
        argv.iter().any(|arg| arg == "id=PR_kwDOabc\" }"),
        "and the value travels beside it as its own `-f` argument: {argv:?}"
    );
    assert!(
        !argv
            .iter()
            .any(|arg| arg.starts_with("query=") && arg.contains("PR_kwDOabc")),
        "a value spliced into the query is a value that can rewrite it: {argv:?}"
    );
}
