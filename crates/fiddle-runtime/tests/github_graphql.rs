//! `GhCli::graphql`: the one call whose verdict is not on the status line.
//!
//! [ADR 018](../../../docs/technical/decisions/018-a-graphql-200-is-not-a-success.md)
//! is the decision these cases enforce, and `scripts/verify-graphql-ready.sh` is
//! where every response shape below was measured against real GitHub. A refused
//! mutation answers 200 with `errors[]` and `gh` exits 1, so `api`'s rule —
//! `status >= 400` — would read a refusal as a success. Everything here is about
//! that one difference and about the fact that it is the *only* difference:
//! the environment, the bound, the credential and the status-line parse are
//! `api`'s own, shared rather than reimplemented, so this is a second call shape
//! and not a second spawn site.
//!
//! Driven through the product's `cli.program` seam against the scripted `gh` in
//! `tests/gh_stub/`, like `github_cli`. Nothing here reaches GitHub.

use fiddle_runtime::effect::EffectOutcome;
use fiddle_runtime::github::{GhCli, GhError};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// A generous bound for a stub that answers immediately. No case here is about
/// the deadline; `github_cli` owns that one, and it is the same bound.
const PATIENT: Duration = Duration::from_secs(30);

/// The credential the client is built with. Sentinel-shaped so that "it did not
/// appear in `argv`" is a claim about this value rather than about an empty
/// string that could not have appeared anywhere.
const TEST_TOKEN: &str = "ghp_graphql_sentinel_must_not_appear";

/// The mutation this method exists for, parameterised exactly as the product
/// must send it.
///
/// The node id is `$id` and never interpolated into the text. That is not style:
/// a node id is a value from GitHub that this process passes on, and a value
/// spliced into a query is a value that can rewrite the query. `-f id=…` binding
/// as a GraphQL variable rather than as a form field was measured, not assumed —
/// see step 0 of the probe script.
const MUTATION: &str = "mutation($id: ID!) { markPullRequestReadyForReview(input: \
                        {pullRequestId: $id}) { pullRequest { isDraft } } }";

/// A run nobody interrupted. Named for what it is rather than `token()`, because
/// this module's other token is a credential and the two must not read alike.
fn uncancelled() -> CancellationToken {
    CancellationToken::new()
}

/// A scratch world: a scripted `gh`, its answers, and the requests it recorded.
struct World {
    dir: TempDir,
}

impl World {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    /// A `GhCli` pointed at the scripted `gh`.
    ///
    /// The stub's scratch directory arrives through `cli.args`, not through the
    /// environment, for the reason the fixture's own header gives: the adapter
    /// clears the environment and sets exactly five names, so a sixth could not
    /// reach the child even if the test wanted one.
    fn gh(&self) -> GhCli {
        // Empty, and stays empty: an empty GH_CONFIG_DIR beside an absent HOME
        // is what makes a real `gh` refuse rather than reach a stored
        // credential.
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

    /// Script the next GraphQL answer, in call order. The status and the body
    /// are given separately because for GraphQL they are independent facts.
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

    /// The environment the child actually received, as `NAME=value` entries.
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

/// The environment's names alone, sorted, so a set can be compared to a set.
fn names(env: &[String]) -> Vec<&str> {
    let mut names: Vec<&str> = env
        .iter()
        .map(|entry| entry.split_once('=').unwrap().0)
        .collect();
    names.sort_unstable();
    names
}

/// The defect this method exists to prevent, stated as a test: a refused
/// mutation arrives with HTTP 200, and `api`'s rule (`status >= 400`) would read
/// it as a success. The body below is the one the probe recorded off real
/// GitHub, `path` and `locations` trimmed.
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
        err.outcome(),
        EffectOutcome::NotCommitted,
        "GitHub could not resolve the node, so nothing was reached to mutate: {err:?}"
    );
    assert!(
        !err.is_worth_reading_again(),
        "and a node that was not found will not be found by asking again: {err:?}"
    );
}

/// Each kind, and the reason each is where it is. `FORBIDDEN` joins `NOT_FOUND`
/// because both are conclusions about the request; `UNPROCESSABLE` joins the 422
/// because it covers a refusal and an "already in that state" with one word, and
/// only a postcondition read can tell those apart.
#[tokio::test]
async fn each_error_kind_classifies_by_what_it_settles() {
    for (kind, expected) in [
        ("NOT_FOUND", EffectOutcome::NotCommitted),
        ("FORBIDDEN", EffectOutcome::NotCommitted),
        ("UNPROCESSABLE", EffectOutcome::Unknown),
        ("SERVICE_UNAVAILABLE", EffectOutcome::Unknown),
        // The row that cannot be measured, and the one most likely to matter
        // later: GitHub's error-type set is GitHub's to extend.
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

        assert_eq!(err.outcome(), expected, "{kind}: got {err:?}");
        assert_eq!(
            err.is_worth_reading_again(),
            expected == EffectOutcome::Unknown,
            "{kind}: only an unsettled refusal is worth looking at again"
        );
    }
}

/// An unrecognised kind is `Unknown`, so this classification errs toward looking
/// again rather than toward believing an outcome. Asserted of an error with no
/// `type` at all, which is the default arm's own case rather than one more
/// example of a name nobody knows.
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
        err.outcome(),
        EffectOutcome::Unknown,
        "a refusal this build cannot name is not evidence about the world: {err:?}"
    );
}

/// The same rule for an `errors` that is present and not an array at all. It
/// costs a read rather than a duplicate write, which is the direction every
/// mistake in this classification is meant to fall.
#[tokio::test]
async fn an_errors_field_that_is_not_an_array_is_a_refusal_and_unknown() {
    let world = World::new();
    world.script_graphql(200, r#"{"data":null,"errors":"something unexpected"}"#);

    let err = world
        .gh()
        .graphql(MUTATION, &[], &uncancelled())
        .await
        .expect_err("a body this client cannot read is not a success");

    assert_eq!(err.outcome(), EffectOutcome::Unknown, "got {err:?}");
}

/// **And the same rule for a 200 that is not an object at all**, which is the
/// asymmetry this case exists to close: a wrongly-shaped `errors` field above
/// already cost a read, while a `null`, an array, a string or a number came back
/// as `Ok(Null)` — a claimed success. Both are the same situation, a response
/// that did not say what happened, and only one of them was treated as one.
///
/// `Unknown` is the honest answer and the direction ADR 018 argues for: being
/// wrong this way costs one postcondition read, and being wrong the other way
/// costs a duplicate external effect. Classified alongside the unreadable
/// `errors` field rather than as `GhError::Malformed`, and the difference is not
/// cosmetic — `Malformed` is `Unknown` and deliberately *not* worth reading
/// again, on the reasoning that a program which is not `gh` will not become one.
/// That reasoning does not hold here: `gh` answered, with a readable status line
/// and a body that parsed as JSON, and the next answer to the same question may
/// well be readable.
///
/// A `null` body is where an **empty** 200 lands too — `parse_body` reads a body
/// that is not there as `Null`, since a 204 from a workflow dispatch is the
/// ordinary case — so the two are one case by the time the verdict is read, and
/// the fixture scripts the one it can express.
#[tokio::test]
async fn a_200_whose_body_cannot_be_interpreted_is_unknown_and_not_a_success() {
    for body in ["null", "[]", r#""a string""#, "42", "true"] {
        let world = World::new();
        world.script_graphql(200, body);

        let Err(err) = world.gh().graphql(MUTATION, &[], &uncancelled()).await else {
            panic!("body {body} must not read as a success");
        };

        assert_eq!(
            err.outcome(),
            EffectOutcome::Unknown,
            "body {body} is a lost answer, not a mutation that did not happen: {err:?}"
        );
        assert!(
            err.is_worth_reading_again(),
            "body {body} left the question open, so asking it again may answer it: {err:?}"
        );
    }
}

/// And the neighbouring case stays where it is: a 200 that *is* an object is
/// read for its verdict, never rejected for its shape. Fixing the hole above
/// must not turn a legitimate answer into an error.
///
/// Deliberately close to `a_200_with_data_and_no_errors_returns_the_data`, and
/// not the same subject: that one is about what a success *returns*, this one is
/// the boundary of the shape check — the smallest body GitHub would send for a
/// mutation that was not refused. **Measured, and it is not load-bearing**: with
/// the check inverted to reject every body, nine tests fail and this is one of
/// them, so it localises nothing the suite could not already see. It is here
/// because the boundary is worth stating beside the case that motivated it.
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

/// **The fixture cannot answer a call nobody scripted**, and this is the case
/// that keeps that true.
///
/// The route used to answer an unscripted call with a silent `200 {"data":{}}`,
/// which made a test that forgot to script an answer indistinguishable from one
/// that meant that answer — and for this route the omission is worse than
/// elsewhere, because a GraphQL verdict lives in the body, so a fabricated 200 is
/// a fabricated verdict. It applied a mutation to the stub's world on the way
/// past, too.
///
/// Written as a test rather than left to the fixture's own good behaviour because
/// nothing else can notice it. Measured, not assumed: with the silent default
/// restored, this is the **only** test in the workspace that fails — so until it
/// existed the property was asserted nowhere, and withdrawing the default broke
/// nothing precisely because nothing was looking.
///
/// It asserts the *filename* reaches the diagnostic, which is the whole of what
/// makes the failure actionable: the panic leaves stdout empty, so the client
/// reports `GhError::Malformed` — the one failure that quotes `stderr` — and the
/// stub prints the missing path ahead of the panic so that the client's
/// 120-character bound does not truncate it away.
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

/// A success returns the data and nothing else. It is still only a claim — the
/// executor's step 8 decides — and this asserts the method does not pretend
/// otherwise by returning a receipt or an outcome of its own.
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

/// An empty errors array is not an error. GitHub does not send one, and a
/// classifier that treated `"errors": []` as a refusal would fail every success
/// from a server that did.
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

/// The transport half is `api`'s, unchanged: a 5xx is `Unknown` and worth
/// reading again, and this method inherits that rather than reimplementing it.
/// A status at or above 400 is still `GhError::Http`, with the same message and
/// the same advice, because nothing about GraphQL changes what a 502 means.
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
    assert_eq!(err.outcome(), EffectOutcome::Unknown);
    assert!(err.is_worth_reading_again());
}

/// A cancellation still refuses before any child exists, which is the check that
/// makes a cancellation knowledge rather than an ambiguity. Shared with `api`
/// rather than repeated, and this is what proves the sharing reaches this call.
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
    assert_eq!(err.outcome(), EffectOutcome::NotCommitted);
    assert!(
        !world.dir.path().join("requests").exists(),
        "a cancelled mutation must not have reached the child at all"
    );
}

/// Not a new spawn site. The same five names with no `HOME` that
/// `github_cli::the_gh_environment_is_exactly_five_names_and_no_home` pins for
/// `api`, asserted here against what the child actually received.
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

/// The credential never reaches `argv`, and neither does anything else that
/// could grow into one. The query does, because a query is not a secret.
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

/// A variable is its own argument and never part of the query text.
///
/// This is the property that keeps a node id from being able to rewrite the
/// query it is used in. A node id is a value GitHub gave this process and this
/// process passes back; interpolating one into the query would make a value
/// containing a quote into syntax. The query that goes out must therefore be the
/// constant it was written as, with `$id` still unresolved in it.
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
