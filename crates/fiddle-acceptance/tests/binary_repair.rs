//! **The compiled binary, driven all the way through a repair that works.**
//!
//! # The gap this closes
//!
//! Everything else that proves the repairing capability proves it from inside a
//! process. `fiddle-runtime`'s `repair_protocol` suite substitutes a scripted
//! model for the gateway and drives the real tools, the real worktree and the
//! real check — deliberately, because its claim is about the shell's response to
//! a model input rather than about the assembled binary. The black-box lane,
//! `capability_selection`, reaches only the rejection and failure arms: it points
//! the gateway at a port nothing listens on, so the run always dies at the
//! socket.
//!
//! Between the two, one thing was proven by nothing that gates: **the wiring in
//! `main.rs` that turns a configuration document into a capability.** Which
//! table becomes which bound, which check command is handed over, which fixture
//! is branched, and that a passing check reaches exit 0 with a marker on disk —
//! all of it was covered only by the out-of-gate Tier 1 run against a real model.
//! A `build_capability` that mapped `deadline` onto `tool_timeout`, or handed
//! over the wrong repository, would have left every gate command green.
//!
//! # Why this is honest, and offline, and credential-free
//!
//! The one thing the failing lane cannot do is answer. So this lane answers: a
//! [`StubGateway`] binds a loopback port, speaks the OpenAI chat-completions
//! wire format the real gateway speaks, and replies from a fixed script. That is
//! the same technique `capability_selection` already uses when it accepts a
//! connection and never answers it, moved one step further along.
//!
//! What is *not* here matters more. Nothing in `src/` knows a test is happening:
//! there is no transcript provider, no test-only runtime mode and no seam that
//! exists for a test's benefit. The binary builds its real gateway client from
//! the document, dials it over a real socket, parses a real response, and runs
//! the real tool loop, the real ephemeral worktree and the real check. The
//! credential is a sentinel string that authenticates nothing — the endpoint is
//! ours — so the lane needs no secret and reaches no network beyond loopback.
//!
//! # The other thing only an answering endpoint can prove
//!
//! What a gateway *says back*. `capability_selection` searches everything a run
//! produced for the credential sentinel, and finds nothing there because its
//! endpoint refuses the connection and produces no response body at all. An
//! endpoint that answers can be scripted to answer *badly* — a `401` whose body
//! quotes the key it rejected, which is what an OpenAI-compatible
//! `invalid_api_key` envelope contains — and that is the only way to drive the
//! path from a foreign response body to a published bundle. Same for the
//! check's own output: reaching `CheckFailed` at all needs an attempt that got
//! as far as producing a report.
//!
//! # What a scripted model can and cannot prove
//!
//! It cannot prove a model is any good at repairing things; that is Tier 1's
//! job and Tier 1 deliberately never asserts success. What it proves is the
//! chain between a document and an outcome, which is not the model's business at
//! all: the same script is run twice here, and the only difference between the
//! run that earns a marker and the run that does not is one integer in the
//! configuration document.
//!
//! Everything is observed from outside the process. Nothing calls a library
//! function.

mod support;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use support::Scenario;

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

/// The variable this scenario's documents name. Never a value.
const CREDENTIAL: &str = "LITELLM_API_KEY";

/// What is exported as the credential: a string that authenticates nothing,
/// because the endpoint it is sent to is this test's own socket.
const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

// ---------------------------------------------------------------------------
// A gateway that answers
// ---------------------------------------------------------------------------

/// One scripted answer: the status line it is sent under, and its body.
///
/// The status is scripted rather than fixed at 200 because the interesting
/// half of a gateway's behaviour is the half that refuses. A gateway that
/// answers `401` with a body quoting the key it was sent is an ordinary
/// deployment accident — and until it could be scripted here, nothing in the
/// suite could reach the code that decides what a refusal is allowed to say.
struct Reply {
    status: u16,
    phrase: &'static str,
    body: serde_json::Value,
}

/// A reply the client is meant to accept.
fn accepted(body: serde_json::Value) -> Reply {
    Reply {
        status: 200,
        phrase: "OK",
        body,
    }
}

/// A reply that refuses, carrying whatever the gateway felt like saying.
fn refused(status: u16, phrase: &'static str, body: serde_json::Value) -> Reply {
    Reply {
        status,
        phrase,
        body,
    }
}

/// A loopback endpoint that answers `POST <base>/chat/completions` from a fixed
/// script of replies.
///
/// One reply per connection, in order, and then the listener is dropped. A run
/// that asked for more turns than the script holds therefore fails at the socket
/// with a diagnostic rather than hanging, and [`StubGateway::served`] says how
/// many turns were actually taken — which is how a scenario asserts that the
/// binary really dialled out rather than concluding some other way.
///
/// Written against `TcpStream` rather than an HTTP crate on purpose: the
/// acceptance package depends on nothing that could let a scenario reach inside
/// the binary it is testing, and one request-response exchange over HTTP/1.1 is
/// smaller than the dependency would be.
struct StubGateway {
    port: u16,
    served: Arc<AtomicUsize>,
}

impl StubGateway {
    /// An endpoint that will answer with each of `script` in turn.
    fn serving(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let served = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&served);
        // Detached rather than joined. A scenario that drove fewer turns than
        // the script holds leaves this thread blocked in `accept`, and joining
        // it would turn "the binary stopped early" — a perfectly good assertion
        // failure — into a hang.
        std::thread::spawn(move || {
            for reply in script {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                if answer(stream, &reply).is_err() {
                    return;
                }
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        StubGateway { port, served }
    }

    /// The `agent.base_url` a document must name to reach this endpoint.
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// How many completions this endpoint has answered.
    fn served(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }
}

/// Read one whole HTTP request off `stream` and answer it with `reply`.
///
/// The request is drained in full — headers *and* body — before anything is
/// written back, because a server that replies while the client is still
/// sending gets its answer thrown away with a connection reset on some
/// platforms.
///
/// `connection: close` on the response, so the client opens a fresh connection
/// per turn and this function never has to multiplex one.
fn answer(mut stream: TcpStream, reply: &Reply) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];

    let head = loop {
        if let Some(at) = find(&request, b"\r\n\r\n") {
            break at + 4;
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&chunk[..read]);
    };

    let length = content_length(&request[..head]);
    while request.len() < head + length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }

    let body = reply.body.to_string();
    stream.write_all(
        format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{body}",
            reply.status,
            reply.phrase,
            body.len(),
        )
        .as_bytes(),
    )?;
    stream.flush()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);
    Ok(())
}

/// The `content-length` a request's head declares, or zero when it declares
/// none.
fn content_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}

/// The index of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// The script
// ---------------------------------------------------------------------------

/// One chat-completions response carrying `message`.
///
/// Every field the wire format declares is present, including the ones a real
/// gateway may omit, so this stays a description of the protocol rather than of
/// which fields one client happens to tolerate.
fn completion(message: serde_json::Value, finish_reason: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-stub",
        "object": "chat.completion",
        "created": 0,
        "model": "a-model",
        "system_fingerprint": null,
        "choices": [{
            "index": 0,
            "message": message,
            "logprobs": null,
            "finish_reason": finish_reason,
        }],
        "usage": null,
    })
}

/// A turn in which the model calls one tool.
fn calls(tool: &str, arguments: serde_json::Value) -> serde_json::Value {
    completion(
        serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": { "name": tool, "arguments": arguments },
            }],
        }),
        "tool_calls",
    )
}

/// A turn in which the model returns its final report.
fn reports(report: serde_json::Value) -> serde_json::Value {
    completion(
        serde_json::json!({ "role": "assistant", "content": report.to_string() }),
        "stop",
    )
}

/// The script: write the fix, then report it.
///
/// Two turns, and the first one is the whole point — the repair has to be made
/// through the binary's own `write_file` tool, into the binary's own ephemeral
/// worktree, or the check that follows has nothing to pass over.
fn a_real_repair() -> Vec<Reply> {
    vec![
        accepted(calls(
            "write_file",
            serde_json::json!({
                "path": "src/lib.rs",
                "contents": support::REPAIRED_FIXTURE,
            }),
        )),
        accepted(reports(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "summary": "corrected the off-by-one",
            "claimed_complete": true,
        }))),
    ]
}

/// The gateway refuses, and quotes the key it was sent while doing it.
///
/// Not an invented shape. An OpenAI-compatible gateway's `invalid_api_key`
/// envelope names the credential it rejected, which is the whole reason the
/// response body of a *failed* call is the most dangerous string in this
/// system: it is authored by something outside the process, and it routinely
/// contains the one secret fiddle holds.
fn a_refusal_quoting_the_credential() -> Vec<Reply> {
    vec![refused(
        401,
        "Unauthorized",
        serde_json::json!({
            "error": {
                "message": format!(
                    "Incorrect API key provided: {SENTINEL}. \
                     You can find your API key at https://example.invalid/keys."
                ),
                "type": "invalid_request_error",
                "param": null,
                "code": "invalid_api_key",
            }
        }),
    )]
}

/// A scenario with an open work item, the broken fixture repository, and an
/// `[agent]` pointed at `gateway` with `max_turns` turns to spend.
///
/// # The check, and why it compiles nothing
///
/// It is `grep` for the repaired text: an ordinary external program named by
/// the document, run by the binary inside the worktree, whose exit code decides
/// the outcome — which is the whole of what this lane is about. It fails over
/// the tree as branched and passes only once the attempt has actually written
/// the fix, so exit 0 here still means a repair happened.
///
/// The obvious alternative is `cargo test --offline`, which is what
/// `fiddle-runtime`'s `repair_protocol` suite already gates. It is deliberately
/// not used here. `Workspace::run` builds a child's environment from an
/// allowlist of two locators, `PATH` and `RUSTUP_HOME`, and on macOS under this
/// project's Nix dev shell that is not enough for a *compiler*: the shell also
/// exports `DEVELOPER_DIR` and `SDKROOT`, and stripped of them a nested
/// `cargo test` warns `unable to find sdk: 'macosx'` and links against whatever
/// it can find. Driving this scenario that way was seen failing nine runs in a
/// row and then passing twenty-nine — a toolchain-environment problem recorded
/// in BACKLOG, and not something a lane about *wiring a document to a
/// capability* should be able to fail on. Nothing is given up by avoiding it:
/// which program the check is remains the operator's business, and the `cargo`
/// flavour of it is proven elsewhere.
fn scenario(gateway: &StubGateway, max_turns: usize) -> Scenario {
    scenario_checked(gateway, max_turns, PASSES_ONCE_REPAIRED)
}

/// The default check: `grep` for the repaired text. See [`scenario`].
const PASSES_ONCE_REPAIRED: &str =
    "{ program = \"grep\", args = [\"-q\", \"len - 1\", \"src/lib.rs\"] }";

/// As [`scenario`], with the `workspace.check` value written out in full.
///
/// The check is a parameter because two of the properties this lane proves are
/// properties *of what a check printed*: that the workspace it printed is not
/// republished, and that however much it printed, the run's published reason
/// stays inside its bound. Both need a check that prints, and the repairing
/// one deliberately prints nothing.
fn scenario_checked(gateway: &StubGateway, max_turns: usize, check: &str) -> Scenario {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let fixture = s.write_fixture_repo();
    s.append_config(&format!(
        "[agent]\n\
         model = \"a-model\"\n\
         base_url = \"{}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n\
         max_turns = {max_turns}\n\
         max_tokens = 512\n\
         max_changed_files = 4\n\
         deadline = \"300s\"\n\
         tool_timeout = \"300s\"\n\
         \n\
         [workspace]\n\
         root = {}\n\
         fixture = {}\n\
         check = {check}\n\
         command_timeout = \"300s\"\n",
        gateway.base_url(),
        support::toml_string(&s.dir().join("workspaces")),
        support::toml_string(&fixture),
    ));
    s
}

/// `fiddle run … --capability fixture_repair --json`, with the credential
/// exported, handed back unjudged.
fn repair(s: &Scenario) -> std::process::Output {
    s.run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap()
}

/// The `--json` payload of a run, with the whole process result in the panic
/// message when it is not JSON — which is where a run that died early says why.
fn payload(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"))
}

// ---------------------------------------------------------------------------
// The scenarios
// ---------------------------------------------------------------------------

/// **The compiled binary repairs a fixture and earns the marker for it.**
///
/// Every link of the chain is asserted from outside the process: the run reached
/// the gateway, executed the capability it was asked for, concluded on row 0 of
/// the exit-code table, published a bundle saying so, left the correlation
/// marker the *next* invocation's assessment will recognise, and took its
/// worktree down behind it. The fixture repository is untouched, because a
/// repair happens in a branch of it and never in it.
#[test]
fn the_binary_drives_a_repair_that_passes_its_check_and_records_the_marker() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a repair whose check passed completed, payload = {payload} stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");
    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "fixture_repair",
        "{payload}"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "completed",
        "{payload}"
    );
    assert_eq!(
        payload["next_action"],
        serde_json::json!("complete"),
        "the run must report the world it left behind: {payload}"
    );
    assert_eq!(
        gateway.served(),
        2,
        "the binary must have taken both scripted turns through the real \
         gateway client"
    );

    // The marker is the strongest claim fiddle makes, and it is only ever
    // written after a check exits 0.
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "a repair that passed its check accounts for the work"
    );

    // The published bundle, not only stdout: this is the artefact a downstream
    // reader consumes.
    let bundle = s.read_bundle(&payload);
    assert_eq!(bundle["progress"][0]["stage"], "repair", "{bundle}");
    assert_eq!(bundle["progress"][0]["status"], "completed", "{bundle}");
    let evidence = bundle["capability_executions"][0]["evidence"][0]
        .as_str()
        .unwrap_or_else(|| panic!("a completed repair earns an evidence reference: {bundle}"));
    assert!(
        evidence.starts_with("repair:1:"),
        "the evidence must name what git saw change — one file — rather than \
         what the model claimed: {evidence}"
    );

    // What survives, and what does not.
    let workspaces = s.dir().join("workspaces");
    assert!(
        workspaces.exists(),
        "the attempt never prepared a workspace, so nothing was proven about \
         tearing one down"
    );
    assert_eq!(
        support::walkdir_dirs(&workspaces),
        Vec::<std::path::PathBuf>::new(),
        "an ephemeral worktree must not outlive the attempt that made it"
    );
    assert_eq!(
        std::fs::read_to_string(s.dir().join("fixture/src/lib.rs")).unwrap(),
        support::BROKEN_FIXTURE,
        "the repository under repair is branched from, never written to"
    );
}

/// **A bound in the document is a bound on the run.**
///
/// The same endpoint, the same script, the same fixture, the same check — and
/// one integer different in `fiddle.toml`. With a single turn to spend the
/// attempt is stopped after the model's first tool call, so the repair is never
/// finished, the check never runs, and nothing is earned.
///
/// That difference is the assertion. It is what makes the scenario above
/// evidence about `build_capability`'s mapping rather than evidence that the
/// binary can reach a socket: a `max_turns` that was parsed and then dropped on
/// the floor would leave both runs identical, and one of these two tests would
/// fail.
#[test]
fn a_turn_budget_of_one_stops_the_same_repair_before_it_earns_anything() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 1);

    let out = repair(&s);
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(11),
        "an attempt stopped by a bound did not do the work, and repeating it \
         under a larger bound may well succeed: payload = {payload}"
    );
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a bounded attempt concludes retryably: {payload}"));
    assert!(
        reason.contains("turn budget of 1"),
        "the run must name the bound the document set, so an operator knows \
         which number to raise: {reason}"
    );
    assert_eq!(
        gateway.served(),
        1,
        "one turn was configured, so exactly one completion may be requested"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID),
        None,
        "no check ever ran, so nothing was earned"
    );
}

/// **What the gateway said is not what fiddle publishes.**
///
/// The channel this closes, traced hop by hop: rig preserves a non-2xx
/// response body verbatim, its `Display` renders `status <n>: <body>`,
/// `AgentError::Provider` used to carry that rendering, `CapabilityError::Agent`
/// wrapped it, and `orchestration::run` copied `error.to_string()` into both
/// `RunOutcome::Retryable.reason` and `ProgressEntry.summary` — which are
/// printed on stdout and written into the published bundle.
///
/// The body here quotes the credential, which is what an OpenAI-compatible
/// gateway's `invalid_api_key` envelope actually does. So the scenario is the
/// design invariant stated as a run: a secret reaches the provider, the
/// provider hands it back, and nothing fiddle publishes may contain it.
///
/// Asserted over every surface the run produced, not only the payload: stdout,
/// stderr, and every byte of every file under the project — the bundle, the
/// journal, the fixture, the workspace root.
///
/// The reason is still asserted to *say something*. Dropping the text would
/// pass this test and fail the operator: a 401 and a 503 are different things
/// to do about a run, and the reason is where the difference is legible.
#[test]
fn a_gateway_refusal_never_reaches_what_the_run_publishes() {
    let gateway = StubGateway::serving(a_refusal_quoting_the_credential());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(
        gateway.served(),
        1,
        "the binary must have dialled the gateway and been refused, or this \
         scenario proves nothing: payload = {payload} stderr = {stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(11),
        "a refused provider call did not do the work, and repeating it may well \
         succeed: payload = {payload} stderr = {stderr}"
    );

    assert!(
        !stdout.contains(SENTINEL),
        "the payload republished the gateway's copy of the credential: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL),
        "the diagnostic republished the gateway's copy of the credential: {stderr}"
    );
    let leaked: Vec<String> = s
        .project_tree()
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(SENTINEL))
        .map(|(path, _)| path)
        .collect();
    assert!(
        leaked.is_empty(),
        "the gateway's copy of the credential was written to {leaked:?}"
    );

    // The other half: an operator still learns what happened. `401` is the one
    // fact about a refusal that decides what to do next, and it is fiddle's to
    // report because the status line is not the response body.
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a refused provider call concludes retryably: {payload}"));
    assert!(
        reason.contains("401"),
        "the run must still name what the gateway did, so an operator knows \
         whether to fix a key or wait out a rate limit: {reason}"
    );
    assert!(
        !reason.contains("Incorrect API key provided"),
        "the response body is authored outside this process and must not be \
         quoted at all: {reason}"
    );
}

/// The ceiling on any single string this run promotes into a published field.
///
/// Written here as a literal rather than imported, because the acceptance lane
/// checks the binary against the contract and not against itself.
const PUBLISHED_TEXT_LIMIT: usize = 2048;

/// A check that says where it is working and then says far too much.
///
/// `pwd` first, because the workspace root is exactly the string a published
/// reason must not carry — a linked worktree's absolute path is the operator's
/// directory layout and this attempt's identity. Then four thousand characters,
/// which is what an ordinary failing `cargo test` looks like from outside and
/// is comfortably past [`PUBLISHED_TEXT_LIMIT`]. Then a non-zero exit, so the
/// capability reports `CheckFailed` and the whole lot is promoted.
const A_LOUD_FAILING_CHECK: &str = "{ program = \"sh\", args = [\"-c\", \
     \"pwd >&2; i=0; while [ $i -lt 400 ]; do printf '0123456789' >&2; \
     i=$((i+1)); done; exit 1\"] }";

/// **A check that printed a megabyte still publishes a bounded reason, and one
/// that names no host path.**
///
/// The second seam of the same defect. `CapabilityError::CheckFailed` embeds
/// the check's stderr, and the check is an arbitrary operator-configured
/// program run over a tree a model has been writing to: its output is unbounded
/// in size and, until the workspace relativised its own root out of what it
/// hands back, carried the absolute worktree path. Both then reached
/// `RunOutcome::Retryable.reason` and `ProgressEntry.summary` verbatim.
///
/// Both surfaces are asserted, and the bundle as well as stdout, because the
/// bundle is the artefact a downstream reader consumes.
#[test]
fn a_failing_checks_output_is_bounded_and_names_no_workspace_path() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario_checked(&gateway, 4, A_LOUD_FAILING_CHECK);

    let out = repair(&s);
    let payload = payload(&out);

    assert_eq!(
        out.status.code(),
        Some(11),
        "the check exited non-zero, so nothing was earned: payload = {payload} \
         stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a failing check concludes retryably: {payload}"));
    let bundle = s.read_bundle(&payload);
    let summary = bundle["progress"][0]["summary"]
        .as_str()
        .unwrap_or_else(|| panic!("a failed execution still records progress: {bundle}"));

    // The workspace root is `<workspaces>/<attempt>`, so the parent's own path
    // is a prefix of it: a reason carrying either spelling of that prefix is
    // carrying the host's directory layout.
    let workspaces = s.dir().join("workspaces");
    let spellings = [
        workspaces.display().to_string(),
        std::fs::canonicalize(&workspaces)
            .unwrap_or_else(|_| workspaces.clone())
            .display()
            .to_string(),
    ];

    for (field, text) in [("reason", reason), ("summary", summary)] {
        for spelling in &spellings {
            assert!(
                !text.contains(spelling.as_str()),
                "the published `{field}` names the host's workspace path \
                 {spelling}"
            );
        }
        assert!(
            text.chars().count() <= PUBLISHED_TEXT_LIMIT,
            "the published `{field}` is {} characters, and the bound is \
             {PUBLISHED_TEXT_LIMIT}",
            text.chars().count()
        );
        assert!(
            text.contains("exited"),
            "an operator must still learn that the check is what refused this \
             run: {field} = {text}"
        );
    }

    assert_eq!(
        s.read_change_marker(WORK_ID),
        None,
        "the check failed, so nothing was earned"
    );
}
