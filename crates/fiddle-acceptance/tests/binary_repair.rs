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

use support::{a_real_repair, refused, Reply, Scenario, StubGateway};

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

/// The variable this scenario's documents name. Never a value.
const CREDENTIAL: &str = "LITELLM_API_KEY";

/// What is exported as the credential: a string that authenticates nothing,
/// because the endpoint it is sent to is this test's own socket.
const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

// ---------------------------------------------------------------------------
// The gateway, and the script it answers from
// ---------------------------------------------------------------------------
//
// [`StubGateway`], its [`Reply`] script and [`a_real_repair`] live in
// `support` rather than here, so the decision-walk lane drives the same
// endpoint this one does. This file established them and is still their
// largest caller; only the refusal below is local, because it quotes this
// file's own sentinel.

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
/// not used here. `Workspace::run` builds a child's environment from
/// `env_clear` plus four names — `HOME`, `LANG`, `PATH`, and `RUSTUP_HOME` when
/// the parent has one — of which the last two are the only inherited locators,
/// and on macOS under this
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

/// **The attempt a published evidence reference names is the attempt the
/// bundle is filed under.**
///
/// `repair:<changed>:<attempt>` is a cross-reference: its last field is there so
/// a reader holding the evidence can go and find the record of the same attempt.
/// It was not one. `main.rs` minted an id for `RepairConfig` and
/// `fiddle_runtime::attempt` minted the bundle's separately, so the reference
/// named an id that appeared in no bundle and on no disk — a format implying a
/// tie that did not hold, which is worse than carrying no identifier at all.
///
/// Both halves are read out of the published document, because that is the
/// artefact a downstream reader has: whatever the process said on stdout, the
/// tie either exists in the bundle on disk or it does not exist.
#[test]
fn the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under() {
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

    let bundle = s.read_bundle(&payload);
    let attempt_id = bundle["attempt_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the bundle must record its attempt: {bundle}"));
    let evidence = bundle["capability_executions"][0]["evidence"][0]
        .as_str()
        .unwrap_or_else(|| panic!("a completed repair earns an evidence reference: {bundle}"));

    assert_eq!(
        evidence,
        format!("repair:1:{attempt_id}"),
        "the evidence names an attempt, so it must name *this* one — the bundle \
         a reader holding this reference would go and open"
    );

    // And the bundle really is filed under that id, so following the reference
    // reaches a document rather than a name.
    let reported = payload["report"]
        .as_str()
        .unwrap_or_else(|| panic!("the run payload must name its bundle: {payload}"));
    assert!(
        reported.contains(attempt_id),
        "the published path must be the one the reference leads to, got \
         {reported} for attempt {attempt_id}"
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

/// **What the model was actually offered, read off the wire.**
///
/// Every other assertion about the tool protocol in this workspace is made
/// against the *builder* — `ReadFile.parameters()`, `WriteFile.description()`,
/// the four `Tool` impls inspected one at a time inside `agent::tools`. Those
/// are claims about our own constructors, and they hold whether or not the
/// constructors are what reaches a provider. This one is made against the
/// serialized chat-completions request body the compiled binary put on a
/// socket, so a rig release that started composing tool definitions
/// differently, or that began folding host context into arguments on the way
/// out, fails here rather than passing.
///
/// # The offered set is exactly four, and that is a measurement
///
/// It is not what reading `agent::attempt` would suggest. That function asks
/// for `OutputMode::Tool`, whose documented behaviour is to register the
/// structured-output schema as a synthetic tool the model calls to finalise —
/// which would make the advertised set five names. It does not happen: rig
/// 0.41's `prompt_typed` pins `OutputMode::Native` over whatever the builder
/// asked for, so no synthetic tool is ever advertised and the native
/// `response_format` constraint is sent instead, on the finalising turn only.
/// The shape is pinned here in both directions — four tools every turn, and
/// the constraint appearing exactly once — because it is the shape that was
/// measured to work against the real gateway, and nothing else in the gate
/// could see it change.
///
/// The set is asserted exactly rather than by `contains`. A fifth capability
/// tool nobody argued for, and rig beginning to advertise a helper of its own,
/// are both things a menu assertion exists to catch.
///
/// # The positive search
///
/// Absence of host-only values is asserted by *looking for them*, not inferred
/// from the fact that `ToolHost` is not a serializable field. Two things are
/// searched for across the whole body: both spellings of the scenario root, so
/// that macOS's `/var` → `/private/var` symlink cannot make the search vacuous,
/// and the credential's value. The credential is legitimately in the
/// `authorization` header of the same request and is deliberately outside what
/// [`StubGateway::request_bodies`] keeps, so this is a claim about the model's
/// view rather than about the transport.
#[test]
fn the_serialized_request_offers_four_tools_and_carries_no_host_fact() {
    let gateway = StubGateway::serving(a_real_repair());
    let s = scenario(&gateway, 4);

    let out = repair(&s);
    let payload = payload(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the run must complete, or the requests below are not the requests a \
         working attempt sends: payload = {payload} stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bodies = gateway.request_bodies();
    assert_eq!(
        bodies.len(),
        2,
        "both turns must have reached the socket, or there is nothing here to \
         inspect"
    );

    // Both spellings of the disposable project, which is the ancestor of the
    // ephemeral worktree, the fixture repository and the report directory
    // alike: one host path found anywhere below it is one too many.
    let mut roots = vec![s.dir().display().to_string()];
    if let Ok(canonical) = s.dir().canonicalize() {
        let canonical = canonical.display().to_string();
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }

    let mut constrained = 0;
    for (turn, body) in bodies.iter().enumerate() {
        let request: serde_json::Value = serde_json::from_str(body)
            .unwrap_or_else(|e| panic!("turn {turn} is not JSON ({e}): {body}"));

        // The premise. Without it the searches below could pass over a request
        // that never carried a prompt or a tool at all.
        assert!(
            body.contains("You are repairing one small Rust project"),
            "turn {turn} carries no preamble, so this is not the request the \
             agent sends: {body}"
        );

        let tools = request["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("turn {turn} advertises no tools: {request}"));
        let mut offered: Vec<&str> = tools
            .iter()
            .map(|tool| {
                tool["function"]["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("a tool with no name on turn {turn}: {tool}"))
            })
            .collect();
        offered.sort_unstable();
        assert_eq!(
            offered,
            ["list_files", "read_file", "run_check", "write_file"],
            "turn {turn} must offer the capability's four tools and nothing \
             else — see this test's note on the synthetic output tool that is \
             not here: {request}"
        );

        // A schema is a menu: anything named on one is something the model may
        // fill in, so a host handle appearing here is a host handle granted.
        // `"/` is a JSON string that begins at the filesystem root, which is
        // the shape of every absolute path and of nothing a relative one.
        for tool in tools {
            let name = tool["function"]["name"].as_str().unwrap();
            let advertised = tool["function"].to_string();
            for banned in ["workspace", "cancel", "receipts", "\"/"] {
                assert!(
                    !advertised.contains(banned),
                    "`{banned}` reaches the advertised schema of `{name}` on \
                     turn {turn}: {advertised}"
                );
            }
        }

        if request.get("response_format").is_some() {
            constrained += 1;
            assert_eq!(
                request["response_format"]["json_schema"]["name"], "RepairReport",
                "the only structured-output constraint fiddle asks for is its \
                 own report: {request}"
            );
        }

        for root in &roots {
            assert!(
                !body.contains(root.as_str()),
                "turn {turn} shows the model the host path {root}: {body}"
            );
        }
        assert!(
            !body.contains(SENTINEL),
            "turn {turn} carries the credential in what the model is shown; it \
             belongs in the authorization header and nowhere else: {body}"
        );
    }

    assert_eq!(
        constrained, 1,
        "the native structured-output constraint belongs on the finalising \
         turn alone: a first turn carrying it is the shape measured to stop \
         this gateway calling tools at all, and every turn carrying none is a \
         report nothing validates"
    );
}
