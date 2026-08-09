//! Which capability an invocation selects, what it takes to build one, and what
//! a refusal is allowed to say.
//!
//! With one capability registered, `--capability` could only ever name the one
//! the derivation would have chosen anyway, so validating the value and acting
//! on it were indistinguishable. With two, they are not, and the difference is
//! the worst kind: a run asked for `fixture_repair`, silently ran `stub_mark`,
//! and exited 0 reporting `completed`. Every scenario here is about that
//! difference.
//!
//! The other half is the credential. It is read in exactly one place, only for
//! a capability that needs a model, and it must reach neither stdout, nor
//! stderr, nor anything the run writes to disk — so the scenarios that supply
//! one supply a sentinel and then go looking for it everywhere.
//!
//! The third part is that `inspect` asks the same question. A read-only command
//! that reports which capability is next is making the claim `run` acts on, so
//! the two are driven here together and compared against *each other* rather
//! than each against a literal — an assertion on two constants would still pass
//! if both commands moved to a third capability together, and would say nothing
//! about the pair.
//!
//! Everything is observed from outside the process. Nothing calls a library
//! function.

mod support;

use support::Scenario;

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

/// The variable this scenario's documents name. Never a value — the name is
/// what configuration is allowed to carry.
const CREDENTIAL: &str = "LITELLM_API_KEY";

/// A value that is unmistakable if it ever appears anywhere it should not.
///
/// Shaped like a credential on purpose: what is being asserted is not that this
/// particular string is absent but that whatever the variable held is.
const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

/// An endpoint nothing can be listening on.
///
/// Port 9 is `discard`, and on the loopback interface a connection to it is
/// refused immediately rather than routed anywhere. That is what makes these
/// scenarios free and offline while still driving the real gateway client: the
/// capability builds its model, reaches for it, and fails at the socket.
const UNREACHABLE_GATEWAY: &str = "http://127.0.0.1:9/v1";

/// The `[agent]` and `[workspace]` tables a repairing capability needs, bounded
/// so tightly that a scenario cannot hang on a network stack that behaves
/// unexpectedly.
fn agentic_tables(scenario: &Scenario, base_url: &str) -> String {
    let fixture = scenario.write_fixture_repo();
    format!(
        "[agent]\n\
         model = \"a-model\"\n\
         base_url = \"{base_url}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n\
         max_turns = 1\n\
         max_tokens = 64\n\
         deadline = \"30s\"\n\
         tool_timeout = \"30s\"\n\
         \n\
         [workspace]\n\
         root = {}\n\
         fixture = {}\n\
         check = {{ program = \"true\" }}\n\
         command_timeout = \"30s\"\n",
        support::toml_string(&scenario.dir().join("workspaces")),
        support::toml_string(&fixture),
    )
}

/// A scenario with an open work item and the tables a repairing capability
/// needs.
fn repairable() -> Scenario {
    let scenario = Scenario::new();
    scenario.write_work_item(WORK_ID, "open");
    let tables = agentic_tables(&scenario, UNREACHABLE_GATEWAY);
    scenario.append_config(&tables);
    scenario
}

/// **The regression.** `--capability` selects; it does not merely validate.
///
/// Verified against the release binary before this test existed:
///
/// ```text
/// fiddle run beans:demo --capability fixture_repair --json
///   -> "capability_executions":[{"capability_id":"stub_mark", … "completed"}]
///   -> "outcome":"completed", exit 0
/// ```
///
/// Asked for one capability, ran another, and reported success. So the
/// assertion here is on *what ran*, never on the exit code alone: the id in
/// `capability_executions` has to be the id that was asked for, and the
/// deterministic capability's marker must not be on disk.
///
/// The gateway is unreachable, so the selected capability fails — which is the
/// point. A failed execution is still recorded as an execution, under the id of
/// the capability that attempted it, and that record is the evidence that
/// selection is real.
#[test]
fn the_selected_capability_is_the_one_that_runs() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"));

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "fixture_repair",
        "the run must execute the capability it was asked for, got {payload}"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "failed",
        "the gateway is unreachable, so the selected capability must have \
         tried and failed rather than something else having succeeded: {payload}"
    );
    assert_eq!(
        out.status.code(),
        Some(11),
        "a capability that tried and failed is retryable, stderr = {stderr}"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID),
        None,
        "nothing earned a correlation marker: `stub_mark` never ran, and the \
         repair never passed a check"
    );
}

/// **A repair's progress is filed under the repair's own stage.**
///
/// The published bundle is what a downstream reader consumes, and every
/// [`ProgressEntry`] in it carries a `stage`. Before this test, that field was a
/// single constant in the orchestration — `"mark"`, the name of M0's one
/// observable step — so a `fixture_repair` run published
/// `{"capability_id":"fixture_repair","stage":"mark", …}`: a bundle labelled
/// with the wrong capability's vocabulary. Nothing caught it because every
/// existing assertion on `stage` is over a `stub_mark` bundle, where the
/// constant happened to be right.
///
/// Asserted over the *published bundle* rather than over stdout, because that is
/// the artefact the field is a contract to; and on the failing arm because the
/// gateway is unreachable, which is exactly the arm an operator reads a stage
/// name on. The stage names what ran, not whether it worked.
#[test]
fn a_repair_files_its_progress_under_a_stage_of_its_own() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"));
    let bundle = s.read_bundle(&payload);

    assert_eq!(
        bundle["progress"][0]["capability_id"], "fixture_repair",
        "the bundle must record the capability that ran: {bundle}"
    );
    assert_eq!(
        bundle["progress"][0]["stage"], "repair",
        "a repair's progress must be filed under a stage describing the repair, \
         never under M0's `mark`: {bundle}"
    );
}

/// The other direction of the same rule: with the flag absent, the run selects
/// the deterministic capability, exactly as M0 does.
///
/// This is what keeps M0's lane byte-identical — the default did not move when
/// a second capability was registered.
#[test]
fn an_absent_flag_still_selects_the_deterministic_capability() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    let payload = s.run_json(INVOCATION_REF, 0);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "stub_mark",
        "an unqualified run must select `stub_mark`, got {payload}"
    );
    assert_eq!(payload["outcome"], "completed");
}

/// Naming the deterministic capability explicitly selects it too, so the flag
/// is a selection rather than a switch that only means something for one value.
#[test]
fn naming_the_deterministic_capability_selects_it() {
    let s = repairable();

    let payload = s.run_json_with(&["--capability", "stub_mark"], INVOCATION_REF, 0);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "stub_mark",
        "got {payload}"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(s.expected_marker(INVOCATION_REF).as_str()),
        "the deterministic capability writes the marker it always wrote"
    );
}

/// A capability that needs a model, in an environment holding no credential,
/// is a configuration error that names the variable to set.
///
/// It must not fall back to anything. An exit 0 here would be the original
/// defect wearing a different hat.
#[test]
fn an_absent_credential_is_a_configuration_error_naming_the_variable() {
    let s = repairable();
    let before = s.project_tree();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "an absent credential is invalid configuration, stdout = {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(CREDENTIAL),
        "the diagnostic must name the variable to set: {stderr}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a refused invocation must have changed nothing at all"
    );
}

/// The credential is resolved only for a capability that needs a model.
///
/// This is the property M0's whole acceptance lane rests on: it runs with no
/// credential in the environment, and it must keep doing so even though the
/// binary now knows how to read one.
#[test]
fn the_deterministic_capability_needs_no_credential_at_all() {
    // The tables are present and name a variable that is *not* set, so the only
    // thing keeping this run alive is that nothing asked for the credential.
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .arg("--json")
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(0),
        "the deterministic capability must not resolve a credential, stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A capability that needs a model over a document that configures none says
/// which table is missing, rather than failing at the credential for a
/// document that was never going to work.
#[test]
fn a_document_with_no_agent_table_says_so() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    let out = s.run_raw_with(
        &["--capability", "fixture_repair", "--json"],
        INVOCATION_REF,
    );

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[agent]"),
        "the diagnostic must name the table to add: {stderr}"
    );
}

/// The same, one table over: `[agent]` alone is not enough, because a repair
/// needs somewhere to work and something to be judged by.
#[test]
fn a_document_with_no_workspace_table_says_so() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    s.append_config(&format!(
        "[agent]\nmodel = \"a-model\"\nbase_url = \"{UNREACHABLE_GATEWAY}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n"
    ));

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[workspace]"),
        "the diagnostic must name the table to add: {stderr}"
    );
}

/// **`inspect` and `run` never disagree about what will happen.**
///
/// The defect this closes: `inspect` took no `--capability` and reported the
/// plan for `stub_mark` unconditionally, so over a repair-configured project
///
/// ```text
/// fiddle inspect beans:x --json  -> next_action: execute stub_mark
/// fiddle run     beans:x --capability fixture_repair -> executes fixture_repair
/// ```
///
/// — a read-only command whose whole purpose is to say what a run would do,
/// saying the wrong thing.
///
/// The two halves are driven here in one scenario and compared against *each
/// other* rather than against a literal, because the property is agreement: an
/// assertion on two constants would still pass if both commands moved to a
/// third capability together, and would say nothing about the pair.
#[test]
fn inspect_names_the_capability_a_run_with_the_same_flags_executes() {
    let s = repairable();

    let inspected = s.inspect_json_with(&["--capability", "fixture_repair"], INVOCATION_REF);
    let foreseen = &inspected["next_action"]["execute"]["capability_id"];
    assert_eq!(
        foreseen, "fixture_repair",
        "inspect must report the capability it was asked about: {inspected}"
    );

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let ran: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr = {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });

    assert_eq!(
        &ran["capability_executions"][0]["capability_id"], foreseen,
        "the capability inspect foresaw and the capability run executed must be \
         the same one: inspect said {foreseen}, run did {ran}"
    );
}

/// The default did not move: an `inspect` with no flag reports `stub_mark`,
/// exactly as M0's lane asserts, even over a document that configures a repair.
///
/// This is the other half of the agreement, and it is why adding the flag was
/// enough on its own: `run` with no flag also selects `stub_mark`, so the two
/// commands agree at the default as well as at every explicit selection. There
/// is no configuration left where they differ without the caller having asked
/// them different questions.
#[test]
fn an_unqualified_inspect_and_an_unqualified_run_agree_on_the_default() {
    let s = repairable();

    let inspected = s.inspect_json_with(&[], INVOCATION_REF);
    let ran = s.run_json(INVOCATION_REF, 0);

    assert_eq!(
        inspected["next_action"]["execute"]["capability_id"], "stub_mark",
        "the default plan is unchanged: {inspected}"
    );
    assert_eq!(
        inspected["next_action"]["execute"]["capability_id"],
        ran["capability_executions"][0]["capability_id"],
        "unqualified, the two commands must still name the same capability"
    );
}

/// `inspect` gained a selection and not a side effect.
///
/// The command is read-only by contract — it never writes fixture state and
/// never publishes a bundle — and a flag that reaches the derivation must not
/// have quietly reached the capability builder as well. Asserted over the whole
/// project tree, so an escape of any kind is seen rather than only the two
/// artefacts a run is known to write.
#[test]
fn selecting_a_capability_leaves_inspect_read_only() {
    let s = repairable();
    let before = s.project_tree();

    let payload = s.inspect_json_with(&["--capability", "fixture_repair"], INVOCATION_REF);

    assert_eq!(
        payload["next_action"]["execute"]["capability_id"], "fixture_repair",
        "the scenario must have reached the derivation to prove anything: {payload}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "inspect is read-only, whichever capability it was asked about"
    );
    assert!(
        !s.report_dir().exists(),
        "inspect must publish no bundle, whichever capability it was asked about"
    );
}

/// The selection `inspect` accepts is the same selection `run` accepts, down to
/// the refusal: an id this build cannot execute is a usage error naming what it
/// can, not a plan for something that does not exist.
///
/// Refused without a credential in the environment, because `inspect` needs
/// none — the flag names the capability the plan is *about*, and nothing is
/// built from it.
#[test]
fn an_unknown_capability_is_refused_by_inspect_too() {
    let s = repairable();
    let before = s.project_tree();

    let out = s.inspect_raw_with(&["--capability", "nope", "--json"], INVOCATION_REF);

    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout = {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("stub_mark") && stderr.contains("fixture_repair"),
        "the diagnostic must list every id this build can execute: {stderr}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a rejected inspection provably did nothing"
    );
}

/// An unknown id is a usage error listing what this build can run — unchanged
/// from M0, except that the list now has two entries because a second
/// capability was registered rather than because anyone retyped it.
#[test]
fn an_unknown_capability_is_a_usage_error_listing_the_known_ids() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let before = s.project_tree();

    let out = s.run_raw_with(&["--capability", "nope"], INVOCATION_REF);

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("stub_mark") && stderr.contains("fixture_repair"),
        "the diagnostic must list every id this build can execute: {stderr}"
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a rejected invocation provably did nothing"
    );
}

/// **The credential never leaves the process.**
///
/// The run above is driven again, and this time everything it produced is
/// searched: stdout, stderr, and every byte of every file anywhere under the
/// project — the published bundle, the attempt journal, the fixture, the
/// workspace root. Configuration already cannot hold a credential; this is the
/// other half, that a run holding one does not write it down.
#[test]
fn nothing_a_run_produces_contains_the_credential() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains(SENTINEL),
        "the payload echoed it: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL),
        "the diagnostic echoed it: {stderr}"
    );

    let leaked: Vec<String> = s
        .project_tree()
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(SENTINEL))
        .map(|(path, _)| path)
        .collect();
    assert!(
        leaked.is_empty(),
        "the credential was written to {leaked:?}"
    );
}

/// A credential the gateway client cannot even be built with is refused without
/// being repeated.
///
/// A value carrying a newline cannot become an HTTP header, so the client
/// refuses to build. That refusal is the closest thing to "the credential is
/// wrong" this milestone can reach offline, and it is exactly the moment a
/// naive `{error}` would print the secret — the same defect the configuration
/// loader already had to fix when it refused a literal `api_key`.
#[test]
fn a_credential_the_client_rejects_is_not_repeated_in_the_diagnostic() {
    let s = repairable();

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, format!("{SENTINEL}\nx"))
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "a credential no client can carry is invalid configuration"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains(SENTINEL),
        "the refusal repeated the credential: {stderr}"
    );
    assert!(
        stderr.contains(CREDENTIAL),
        "the refusal must still name the variable, so an operator knows which \
         one to fix: {stderr}"
    );
}

/// **An interrupt reaches the running attempt.**
///
/// The obligation this discharges is written down in
/// `fiddle_runtime::workspace::command`: a workspace command is put in a
/// process group of its own so that a timed-out `cargo test` is reaped together
/// with the test binaries it spawned, and the price of that is that the child no
/// longer shares this process's group. A terminal `^C` therefore no longer
/// reaches it, and a runner that simply died on the signal would leave a build
/// running over a worktree that is about to be deleted underneath it.
/// Cancellation is the only channel that still gets there, so `SIGINT` has to
/// flip it.
///
/// The window is opened deterministically rather than by sleeping: the scenario
/// listens on a port of its own, points the gateway at it, and **accepts the
/// connection without ever answering it**. The moment `accept` returns, the
/// attempt is provably inside its model call, which is exactly where an
/// interrupt has to be able to reach it.
///
/// What is asserted is the whole consequence, not just that the process died:
/// the run *concluded* — retryably, naming cancellation — and the worktree came
/// down with it. A handler that killed the process outright would leave the
/// bundle unpublished and the workspace root full.
#[cfg(unix)]
#[test]
fn an_interrupt_cancels_the_attempt_rather_than_killing_the_runner_under_it() {
    use std::io::Read;

    let gateway = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = gateway.local_addr().unwrap().port();

    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let tables = agentic_tables(&s, &format!("http://127.0.0.1:{port}/v1"));
    s.append_config(&tables);

    // A plain `std::process::Command`, because this scenario has to hold the
    // child while it runs rather than wait for it to finish.
    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS {
        command.env_remove(name);
    }
    let child = command
        .args([
            "run",
            INVOCATION_REF,
            "--config",
            s.config_path().to_str().unwrap(),
            "--capability",
            "fixture_repair",
            "--json",
        ])
        .env(CREDENTIAL, SENTINEL)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Held for the rest of the scenario: dropping it would close the socket and
    // hand the attempt an answer of sorts, which is not the state under test.
    let (mut held, _) = gateway.accept().expect("the attempt must dial the gateway");

    interrupt(child.id());

    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(11),
        "a cancelled attempt did not do the work and repeating it may well \
         succeed, stdout = {stdout} stderr = {stderr}"
    );

    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"));
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a cancelled run must conclude retryably: {payload}"));
    assert!(
        reason.contains("cancelled"),
        "the run must conclude that it was cancelled rather than that it \
         failed: {reason}"
    );
    assert!(
        payload["report"].is_string(),
        "an interrupted run still records what it concluded: {payload}"
    );

    let workspaces = s.dir().join("workspaces");
    assert!(
        workspaces.exists(),
        "the attempt never prepared a workspace, so nothing was proven about \
         tearing one down"
    );
    assert_eq!(
        support::walkdir_dirs(&workspaces),
        Vec::<std::path::PathBuf>::new(),
        "an interrupted attempt must still take its worktree down with it"
    );

    // The socket outlived the process it was serving, which is the other half of
    // the claim: fiddle stopped because it was asked to, not because the far end
    // went away.
    let mut nothing = [0u8; 1];
    let _ = held.read(&mut nothing);
}

/// Send `SIGINT` to `pid`, the way a terminal does to its foreground process.
///
/// Through `kill(1)` rather than through a signalling crate: the acceptance
/// package deliberately depends on nothing that could let a scenario reach
/// inside the binary it is testing, and a shell utility is the same thing an
/// operator at a terminal has.
#[cfg(unix)]
fn interrupt(pid: u32) {
    let status = std::process::Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .unwrap();
    assert!(status.success(), "could not interrupt process {pid}");
}
