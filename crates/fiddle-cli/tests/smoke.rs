//! **Tier 1: is the agent loop still wired up?**
//!
//! One test, `#[ignore]`d, driving the compiled `fiddle` binary against a real
//! model. It is the only test in the workspace that exercises the CLI wiring of
//! `fixture_repair` end to end: the deterministic suite in `fiddle-runtime`
//! proves the *shell's* response to a model input, and would stay green over a
//! `main.rs` that built the wrong capability, resolved no credential, or never
//! reached the gateway at all. That is the same failure shape as M0's
//! stale-binary defect, and this file is the only thing standing in front of it.
//!
//! # The rule this file exists to obey
//!
//! **A real-model test never asserts the model succeeded.** Both outcomes are
//! correct behaviour:
//!
//! - repaired → the check passes → `Completed`, exit 0, and the correlation
//!   marker on disk;
//! - not repaired → the check fails → `Retryable`, exit 11, and no marker.
//!
//! Which of the two happened is *data* — printed, never asserted. A weak model
//! on a bad day must not fail anybody's build. What is asserted is protocol:
//! the run reached the capability it was asked for, it concluded on a row of
//! the exit-code table, the exit code is that row's, a bundle was published and
//! parses, the fixture repository is untouched, the marker is present exactly
//! when it was earned, and nothing anywhere holds the credential.
//!
//! # The one thing that is *not* the model's business to fail
//!
//! There is a third possibility neither of the two above covers: the run never
//! got as far as a model turn — the gateway refused the connection, the
//! workspace could not be prepared, the credential was rejected. That is not a
//! weak model, it is an inconclusive run, and a test that reported it as a pass
//! would be the M0 stale-binary defect wearing yet another hat: green, and
//! evidence of nothing. [`classify`] separates the two, and the inconclusive
//! class fails loudly and says so in those words.
//!
//! # Running it
//!
//! ```text
//! ( set -a; . .env; set +a; \
//!   nix develop -c cargo test -p fiddle-cli -- --ignored --nocapture )
//! ```
//!
//! `--nocapture` because the interesting half of this test is the observation
//! block it prints, and cargo swallows stdout for a test that passes.
//!
//! `FIDDLE_TIER1_MODEL` overrides the model; `claude-haiku-4-5` is the default
//! because it is Claude-family — the most exercised translation path on a
//! Claude-centric gateway — and cheap enough that running it on every
//! development pass costs nothing worth measuring.
//!
//! # The fixture, and why it is copied rather than shared
//!
//! `broken_crate` in `crates/fiddle-runtime/tests/fixture.rs` builds exactly
//! this repository, and this file does not use it. It cannot: that file is a
//! `tests/` module of a different package, so it is compiled into
//! `fiddle-runtime`'s test binaries and is reachable from nowhere else. Sharing
//! it would mean promoting it to a published item of some crate — either a
//! `#[cfg(feature = "test-fixtures")]` module in `fiddle-runtime`, which puts
//! test scaffolding in a shipped library, or a fifth workspace member existing
//! only to hold thirty lines of `std::fs::write`. Both are larger changes than
//! the duplication they remove, and both touch crates this task must leave
//! alone. So the builder below is a third copy — after `fixture.rs` and the
//! one inside `capability/repair.rs`'s own unit tests — and is deliberately the
//! smallest of the three.
//!
//! It is *trivial* on purpose: one obviously-wrong constant, and a test naming
//! the value it should have. A model that cannot repair this is telling us
//! something about the model; a model that cannot repair it *and* a run that
//! never reached one are different findings, and a fixture with any real
//! difficulty in it would blur them.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// The variable the configuration names, and the only credential this test
/// knows about.
const CREDENTIAL_VAR: &str = "LITELLM_API_KEY";

/// Overrides for the deployment under test, so a developer can point Tier 1 at
/// another model or another gateway without editing this file.
const MODEL_VAR: &str = "FIDDLE_TIER1_MODEL";
const BASE_URL_VAR: &str = "FIDDLE_TIER1_BASE_URL";

/// The sensible default: Claude-family, and cheap.
const DEFAULT_MODEL: &str = "claude-haiku-4-5";

/// The gateway the epic is verified against.
const DEFAULT_BASE_URL: &str = "https://litellm.firn.snplow.net/v1";

const PROJECT: &str = "icecube";
const WORK_ID: &str = "fiddle-m1-smoke";
const INVOCATION_REF: &str = "beans:fiddle-m1-smoke";

/// The defect the fixture ships with: an off-by-one that compiles cleanly and
/// fails the package's own test, so a repair has to be a real edit rather than
/// something a formatter could have made.
const BROKEN: &str = "pub fn last_index(len: usize) -> usize { len }\n";

/// Tier 1: is the agent loop still wired up?
///
/// `#[ignore]` so the gate stays offline and free. It asserts *protocol
/// conformance*, never that the model succeeded — see the module documentation
/// for why that distinction is the whole design of this test.
#[test]
#[ignore = "tier 1: requires LITELLM_API_KEY; run with --ignored"]
fn the_agent_loop_still_works_against_a_real_model() {
    // Loud, not skipped. A test that passes for want of a key proves nothing
    // and says it proved something, which is worse than not existing.
    let credential = match std::env::var(CREDENTIAL_VAR) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!(
            "tier 1 requires {CREDENTIAL_VAR}; it is opt-in, not skipped \
             silently. Load it without printing it:\n  \
             ( set -a; . .env; set +a; cargo test -p fiddle-cli -- --ignored --nocapture )"
        ),
    };
    let model = env_or(MODEL_VAR, DEFAULT_MODEL);
    let base_url = env_or(BASE_URL_VAR, DEFAULT_BASE_URL);

    let project = Project::new(&model, &base_url);

    // The compiled binary, as a subprocess — never a library call. `cargo`
    // builds it from these sources and hands its path over in
    // `CARGO_BIN_EXE_fiddle`, so this cannot be driving a stale artefact.
    let started = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_fiddle"))
        .args([
            "run",
            INVOCATION_REF,
            "--config",
            project.config_path().to_str().unwrap(),
            "--capability",
            "fixture_repair",
            "--json",
        ])
        .output()
        .expect("could not launch the fiddle binary");
    let latency = started.elapsed();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // ---------------------------------------------------------------------
    // The credential, first and unconditionally.
    //
    // Before any other assertion, because every other assertion in this file
    // quotes stdout or stderr into its failure message. Checking the leak last
    // would mean a run that *did* leak announced it through a panic about
    // something else — printing the secret in the course of failing to notice
    // it. Checking it first means everything below is already known safe to
    // print.
    // ---------------------------------------------------------------------
    assert!(
        !stdout.contains(&credential),
        "the credential reached stdout"
    );
    assert!(
        !stderr.contains(&credential),
        "the credential reached stderr"
    );
    let leaked: Vec<String> = project
        .files()
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(&credential))
        .map(|(path, _)| path)
        .collect();
    assert!(
        leaked.is_empty(),
        "the credential was written to {leaked:?}"
    );

    // ---------------------------------------------------------------------
    // Protocol.
    // ---------------------------------------------------------------------
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout is not the `--json` payload ({e}):\nstdout = {stdout}\nstderr = {stderr}")
    });

    // The run reached the capability it was asked for. This is the assertion
    // the acceptance lane's `the_selected_capability_is_the_one_that_runs`
    // makes offline; here it is made against a real gateway, so a build that
    // resolves a credential and then runs `stub_mark` is caught too.
    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "fixture_repair",
        "the run must execute the capability it was asked for: {payload}"
    );

    let conclusion = Conclusion::read(&payload, &stderr);
    assert_eq!(
        out.status.code(),
        Some(conclusion.exit_code()),
        "the exit code must be the one the exit-code table gives this outcome; \
         stderr = {stderr}"
    );

    // Published, and readable by the path the payload names — which is how a
    // downstream reader would find it, rather than by reconstructing an attempt
    // directory here.
    let bundle_path = project.report_dir().join(
        payload["report"]
            .as_str()
            .unwrap_or_else(|| panic!("the run published no bundle: {payload}")),
    );
    let bundle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&bundle_path)
            .unwrap_or_else(|e| panic!("could not read {} ({e})", bundle_path.display())),
    )
    .unwrap_or_else(|e| panic!("{} is not JSON ({e})", bundle_path.display()));
    assert_eq!(
        bundle["capability_executions"][0]["capability_id"], "fixture_repair",
        "the published bundle must agree with stdout about what ran: {bundle}"
    );
    assert_eq!(
        bundle["invocation_ref"], INVOCATION_REF,
        "the bundle must be about the invocation that was made: {bundle}"
    );

    // **The marker is present exactly when it was earned, and it is a marker
    // fiddle itself recognises.**
    //
    // Not a claim about the model — a claim about the rule that only a passing
    // check writes one. Both directions are asserted, so neither a capability
    // that marked a failed repair nor one that failed to mark a successful one
    // gets through.
    //
    // `next_action` is the derivation the runtime made *after* the run, over
    // the world the run left behind, so pairing it with the file on disk is
    // what turns "a marker was written" into "the marker written is the one the
    // next invocation will accept". A capability that wrote a plausible-looking
    // but wrong key would satisfy the first and fail this.
    let marker = project.change_marker();
    match conclusion {
        Conclusion::Repaired => {
            let key = marker
                .as_deref()
                .unwrap_or_else(|| panic!("a completed run must have written its marker"));
            assert!(
                key.len() == 16 && key.chars().all(|c| c.is_ascii_hexdigit()),
                "the correlation key is 16 hex characters (design §4.3), got {key:?}"
            );
            assert_eq!(
                payload["next_action"], "complete",
                "the marker the run wrote must be the one its own re-derivation \
                 accepts as accounting for the work: {payload}"
            );
        }
        Conclusion::NotRepaired { .. } => {
            assert_eq!(
                marker, None,
                "a repair that did not pass its check earned nothing"
            );
            assert_eq!(
                payload["next_action"]["execute"]["capability_id"], "fixture_repair",
                "an unearned run leaves the work still to do: {payload}"
            );
        }
    }

    // The fixture on disk. M1's whole claim about what survives an attempt is
    // that nothing does: the repair lives and dies in a per-attempt worktree,
    // so the repository it was branched from is byte-identical afterwards and
    // the workspace root is empty.
    assert_eq!(
        project.fixture_status(),
        Vec::<String>::new(),
        "the attempt wrote to the repository it was supposed to branch from"
    );
    assert_eq!(
        dirs_under(&project.workspace_root()),
        Vec::<PathBuf>::new(),
        "the attempt left its worktree behind"
    );

    // ---------------------------------------------------------------------
    // Data. Printed, never asserted.
    // ---------------------------------------------------------------------
    println!("\n─── tier 1 observation ────────────────────────────────────");
    println!("  model            = {model}");
    println!("  gateway          = {base_url}");
    println!("  latency          = {:.1}s", latency.as_secs_f64());
    println!("  exit code        = {:?}", out.status.code());
    println!("  outcome          = {}", payload["outcome"]);
    println!(
        "  execution        = {}",
        payload["capability_executions"][0]["status"]
    );
    println!(
        "  evidence         = {}",
        payload["capability_executions"][0]["evidence"]
    );
    println!("  next action      = {}", payload["next_action"]);
    match &conclusion {
        Conclusion::Repaired => println!("  repair landed    = yes"),
        Conclusion::NotRepaired { reason } => {
            println!("  repair landed    = no");
            println!("  reason           = {reason}");
        }
    }
    println!("  marker written   = {}", marker.is_some());
    println!("  tools called     = unknown — receipts are not published");
    println!("  bundle           = {}", bundle_path.display());
    println!("───────────────────────────────────────────────────────────");
    println!(
        "  Neither answer above is a verdict. `repair landed = no` is correct \
         behaviour and\n  is recorded as data; only a run that never reached a \
         model turn fails this test.\n  `tools called` cannot be answered from \
         outside until ToolReceipts reach the bundle;\n  see `classify` for why \
         that gap matters."
    );
}

/// How the run ended, restricted to the two answers that are *about the model*.
///
/// Anything else — a gateway that refused the connection, a workspace that
/// could not be prepared, a credential the far end rejected — is inconclusive
/// rather than a failure of the model, and [`Conclusion::read`] panics on it
/// saying exactly that. The distinction is the difference between "we exercised
/// the loop and the model lost" and "we never exercised the loop", and a test
/// that ran green on the second would be worthless.
enum Conclusion {
    /// The check passed. The chain marker → assessment → `Completed` held.
    Repaired,
    /// The loop ran and did not earn anything. Correct behaviour.
    NotRepaired { reason: String },
}

impl Conclusion {
    /// The row of design §4.5's exit-code table this conclusion sits on.
    fn exit_code(&self) -> i32 {
        match self {
            Conclusion::Repaired => 0,
            Conclusion::NotRepaired { .. } => 11,
        }
    }

    /// Read the outcome out of a `run --json` payload, refusing anything that
    /// is not evidence about the agent loop.
    fn read(payload: &serde_json::Value, stderr: &str) -> Self {
        if payload["outcome"] == "completed" {
            return Conclusion::Repaired;
        }
        let Some(reason) = payload["outcome"]["retryable"]["reason"].as_str() else {
            panic!(
                "the run concluded on a row this test cannot interpret. \
                 `Completed` and `Retryable` are the two correct answers; \
                 anything else means the run did not reach the capability at \
                 all: {payload}\nstderr = {stderr}"
            );
        };
        classify(reason);
        Conclusion::NotRepaired {
            reason: reason.to_string(),
        }
    }
}

/// Panic if `reason` describes something that happened *before* a model turn.
///
/// The three accepted shapes each require a completion to have come back and
/// been judged — the model's report failed its schema, a bound we set was
/// reached, or the check overruled the model's claim. Every one of them is the
/// model being weak, and every one of them passes.
///
/// Matched on the leading clause of each error's `Display`, which is a fixed
/// string in `fiddle_runtime`: `CapabilityError::Agent` wraps an `AgentError`
/// whose variants are `the attempt was stopped by a bound`,
/// `the model did not hold up its end`, `the provider did not hold up its
/// end`, and `the attempt was cancelled`, and `CapabilityError::CheckFailed`
/// leads with `the check exited`. Only the provider arm and the workspace arm
/// are refused.
///
/// # What this deliberately does *not* claim
///
/// It does not establish that a **tool** ran. It cannot, and the gap is real
/// rather than theoretical: `ToolReceipt`'s own documentation says receipts are
/// published in the evidence bundle, and they are not —
/// `FixtureRepair::execute` builds a `ToolHost`, hands it to `attempt`, and
/// never reads `host.receipts()` back, so nothing outside the process can see
/// which tools were called or whether any were. The first Tier 1 run against
/// this gateway found a defect that lives squarely in that blind spot: with
/// `response_format` pinned to a JSON schema, the gateway's models answer with
/// the structured report on turn one and call no tools at all, so the check
/// fails over an untouched tree and the run reports `Retryable` — which is
/// indistinguishable, from out here, from a model that tried and lost.
///
/// Until receipts reach the bundle, `the check exited …` means *a completion
/// came back and the tree did not satisfy the check*, and no more than that.
fn classify(reason: &str) {
    const REACHED_THE_MODEL: [&str; 3] = [
        "the check exited",
        "the attempt produced no report: the model did not hold up its end",
        "the attempt produced no report: the attempt was stopped by a bound",
    ];
    if REACHED_THE_MODEL
        .iter()
        .any(|prefix| reason.starts_with(prefix))
    {
        return;
    }
    panic!(
        "INCONCLUSIVE — the run did not reach a model turn, so it says nothing \
         about whether the agent loop is wired up. This is not the model \
         failing; it is the run failing to happen. Check the gateway, the \
         credential, and the toolchain the check needs.\n  reason = {reason}"
    );
}

/// `name`'s value, or `fallback` when it is unset or empty.
fn env_or(name: &str, fallback: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => fallback.to_string(),
    }
}

/// A disposable project: the configuration document, the broken crate it points
/// at, the fixture root the marker lands in, and the report directory.
struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    /// A project pointed at `model` on `base_url`, with one open work item and
    /// a repository that fails its own check.
    ///
    /// The bounds are deliberately tight. `max_tokens` covers a one-line file
    /// plus the structured report and nothing more; `max_turns` leaves room for
    /// read, write, check, and report with a couple of retries; the deadline
    /// and the command timeout are minutes rather than the reference
    /// configuration's tens of minutes, because a Tier 1 run that has taken
    /// five minutes over a two-line crate has already told us what it is going
    /// to tell us.
    fn new(model: &str, base_url: &str) -> Self {
        let project = Project {
            dir: tempfile::tempdir().expect("a temporary directory"),
        };

        std::fs::create_dir_all(project.stub_root().join("work")).unwrap();
        std::fs::create_dir_all(project.stub_root().join("changes")).unwrap();
        std::fs::write(
            project.stub_root().join(format!("work/{WORK_ID}.json")),
            format!("{{\"id\":\"{WORK_ID}\",\"status\":\"open\"}}"),
        )
        .unwrap();

        project.write_broken_crate();

        std::fs::write(
            project.config_path(),
            format!(
                "[project]\n\
                 name = \"{PROJECT}\"\n\
                 \n\
                 [stub]\n\
                 root = {stub}\n\
                 \n\
                 [report]\n\
                 dir = {reports}\n\
                 \n\
                 [agent]\n\
                 model = \"{model}\"\n\
                 base_url = \"{base_url}\"\n\
                 api_key = {{ env = \"{CREDENTIAL_VAR}\" }}\n\
                 max_turns = 12\n\
                 max_tokens = 2048\n\
                 deadline = \"5m\"\n\
                 tool_timeout = \"4m\"\n\
                 \n\
                 [workspace]\n\
                 root = {workspaces}\n\
                 fixture = {fixture}\n\
                 check = {{ program = \"cargo\", args = [\"test\", \"--offline\"] }}\n\
                 command_timeout = \"4m\"\n",
                stub = toml_path(&project.stub_root()),
                reports = toml_path(&project.report_dir()),
                workspaces = toml_path(&project.workspace_root()),
                fixture = toml_path(&project.fixture()),
            ),
        )
        .unwrap();

        project
    }

    /// A deliberately broken zero-dependency Rust crate, as a git repository.
    ///
    /// Zero dependencies so the check runs `--offline` against nothing.
    /// `target/` **and** `Cargo.lock` are gitignored: cargo writes a lock file
    /// on the first run even for a package with no dependencies, and a lock
    /// file regenerated by the check is not a repair — counting it would spend
    /// the changed-file cap on noise and put a fabricated path into published
    /// evidence.
    fn write_broken_crate(&self) {
        let repo = self.fixture();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n",
        )
        .unwrap();
        std::fs::write(repo.join("src/lib.rs"), BROKEN).unwrap();
        std::fs::write(
            repo.join("tests/repair.rs"),
            "#[test]\nfn the_last_index_is_one_before_the_length() {\n    \
             assert_eq!(fixture::last_index(3), 2);\n}\n",
        )
        .unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\nCargo.lock\n").unwrap();

        git(&repo, &["init", "-q", "."]);
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "the broken fixture",
            ],
        );
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("fiddle.toml")
    }

    fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    fn workspace_root(&self) -> PathBuf {
        self.dir.path().join("workspaces")
    }

    fn fixture(&self) -> PathBuf {
        self.dir.path().join("fixture")
    }

    /// The marker recorded at `<stub.root>/changes/<work_id>.json`, read the
    /// way the stub change port reads it — so a capability that wrote a file
    /// fiddle could not read back is a miss rather than a hit.
    fn change_marker(&self) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{WORK_ID}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value["marker"].as_str().map(str::to_string)
    }

    /// What `git status --porcelain` reports in the fixture repository. Empty
    /// is the assertion: an attempt branches a worktree and never writes here.
    fn fixture_status(&self) -> Vec<String> {
        let out = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(self.fixture())
            .output()
            .expect("could not run git status");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|line| line[3..].trim().to_string())
            .collect()
    }

    /// Every file anywhere under the whole project, as `(relative path,
    /// bytes)`.
    ///
    /// Deliberately the *whole* project rather than the bundle alone: a
    /// credential could just as easily land in the attempt journal, the
    /// worktree, or a file the check wrote, and "nothing this run produced
    /// holds it" is only a claim worth making if every one of those is looked
    /// at.
    fn files(&self) -> Vec<(String, Vec<u8>)> {
        let root = self.dir.path();
        files_under(root)
            .into_iter()
            .map(|path| {
                let relative = path.strip_prefix(root).unwrap().display().to_string();
                (relative, std::fs::read(&path).unwrap_or_default())
            })
            .collect()
    }
}

/// A path as a TOML basic string.
fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// Run git in `dir`, panicking with its stderr if it fails.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("could not run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Every file under `root`, recursively.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every directory under `root`, recursively. An abandoned worktree is a
/// directory, and an empty one is exactly as much of a leak as a full one.
fn dirs_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.push(path.clone());
                stack.push(path);
            }
        }
    }
    found.sort();
    found
}
