//! A recording `git` for the deterministic publish suite.
//!
//! It writes down every invocation it receives — the arguments and the whole
//! environment — and then behaves as a scripted mode says. It is reached through
//! the product's own `program` seam, the one that exists for operators who must
//! pin or wrap `git`, and it is declared with `required-features`, so
//! `cargo build --release` never produces it. Nothing here is compiled into the
//! product.
//!
//! # Why it finds its scratch directory in the working directory
//!
//! Not the environment: the adapter under test runs `env_clear()` and then sets
//! exactly seven names, so no eighth could reach this process — and widening
//! that set to let the fixture work would delete the property the fixture exists
//! to prove. Not `argv` either, because a publish's argument vector is asserted
//! exactly, and a fixture that had to appear in it would be asserting itself.
//!
//! What is left is the one channel the product already uses for its own reasons:
//! the child's working directory is the worktree being published. The fixture
//! records there. That both of the other two channels are unusable *for the
//! test's own plumbing* is the first piece of evidence that they are pinned.
//!
//! # Why it is a binary and the `gh` fixture's reasoning applies twice over
//!
//! A shell script would be shorter and is wrong here for a reason that is easy
//! to miss: `sh` exports `PWD`, `SHLVL` and `_` into its own environment before
//! anything in the script runs. A recording made by a shell therefore reports
//! three names the runtime never set, and the exact-environment assertion —
//! which is the security boundary — would have to be weakened to a filter over
//! the fixture's own noise. A compiled recorder adds nothing, so what it writes
//! down is exactly what the parent passed.
//!
//! # Why some of the modes hand the work to a real `git`
//!
//! Most of what this fixture is asked is about the *invocation*, and a canned
//! answer is enough. The ambiguous-write modes are not: their whole subject is a
//! push that genuinely moved a ref and then genuinely failed to say so, and a
//! fixture that only claimed to have pushed would leave the executor's
//! postcondition read with nothing real to find. So `push_then_killed`,
//! `push_then_waits`, `never_answers` and `delegated` hand the work to the real
//! `git`, against a real repository, and interpose only on how the invocation
//! *ends* — which for `delegated` is not at all. The environment is passed through untouched, so
//! the push that lands still runs under the seven names the product built.

use std::io::Write;
use std::path::PathBuf;

/// What `rev-parse HEAD` answers, so a fixture with no repository behind it can
/// still complete a publish. A full object name, because the adapter refuses
/// anything that is not one.
const HEAD_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Longer than any deadline a test sets, so a mode that never answers is ended
/// by the runtime's own bound and by nothing else.
const FOREVER: std::time::Duration = std::time::Duration::from_secs(120);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let subcommand = args.first().cloned().unwrap_or_default();
    let dir: PathBuf = std::env::current_dir().expect("the adapter runs its git in the worktree");
    let mode = std::fs::read_to_string(dir.join("mode"))
        .unwrap_or_else(|_| "accepted".to_string())
        .trim()
        .to_string();

    // Keyed by subcommand, because a publish runs `git` twice — an
    // unauthenticated local read and then the authenticated push — and the
    // difference between what those two children were given is one of the
    // things the suite asserts.
    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    std::fs::write(
        dir.join(format!("{subcommand}.json")),
        serde_json::json!({ "argv": args, "env": env }).to_string(),
    )
    .unwrap();

    if subcommand == "push" {
        // Append-only, so "how many pushes were dispatched?" is a question about
        // the filesystem. The per-subcommand record above is overwritten by each
        // invocation and could never answer it, and it is the exact number a
        // duplicate hides behind: an `Unknown` resolved by retrying instead of
        // by reading would show up here as two.
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("pushes"))
            .unwrap();
        writeln!(log, "{}", args.join(" ")).unwrap();
    }

    if subcommand == "rev-parse" {
        // The canned answer keeps the recording fixture repository-free, which
        // is what the argument and environment assertions need. The delegating
        // modes below are driving a real repository, so theirs has to come out
        // of it or the sha the adapter reports would name no commit.
        match delegating(&mode) {
            true => delegate(&args),
            false => println!("{HEAD_SHA}"),
        }
        return;
    }

    match mode.as_str() {
        // A first push, as `--porcelain` reports one.
        "accepted" => print!("To stub\n*\tHEAD:refs/heads/fiddle/abc\t[new branch]\nDone\n"),
        // The adversarial one: the configured header comes back on `stderr`,
        // which is what `GIT_TRACE_CURL` or a wrapper would do, and is the same
        // shape as M1's published-key defect. Exit 128 is git's ordinary fatal
        // and must not be read as a killed child.
        "leaks_the_header" => {
            eprintln!(
                "fatal: unable to access remote: {}",
                std::env::var("GIT_CONFIG_VALUE_0").unwrap_or_default()
            );
            std::process::exit(128);
        }
        // The ambiguous write, and the reason these two modes exist: a push
        // whose *answer* was lost is not a push that failed, and the only way
        // to prove a runtime tells those apart is to really lose the answer to
        // a push that really happened.
        //
        // Note the order. The ref is pushed by a real `git`, against a real
        // repository, and the death comes afterwards — a fixture that died
        // first would be testing a failed write, which proves nothing. It ends
        // with `abort`, so the signal leaves no exit code behind and the
        // adapter reaches `GitError::Killed` rather than reading a number.
        "push_then_killed" => {
            delegate(&args);
            // Written between the push and the death, and that placement is the
            // whole of what it is for: its presence is the fixture's own record
            // that the ref *landed* and the answer was lost *afterwards*. A suite
            // that asserted only on the mode it had itself written down would be
            // asserting its own arrangement; this is the observation that the
            // arranged fault actually fired, and a mode that pushed and then
            // returned normally would leave it absent.
            std::fs::write(dir.join("pushed_then_died"), "yes").unwrap();
            std::process::abort();
        }
        // The same ambiguity as `push_then_killed`, lost to a *cancellation*
        // rather than to a death — the provenance no fixture here could reach
        // before, because a `git` that ended itself can only ever produce a
        // killed child or a timeout.
        //
        // Nothing ends this mode: the ref is pushed by a real `git`, the marker
        // records that it landed, and the process then waits to be killed. What
        // kills it is the runtime's own cancellation token signalling the child's
        // process group, which is the only channel a `^C` has to a bounded child.
        "push_then_waits" => {
            delegate(&args);
            std::fs::write(dir.join("pushed_then_waited"), "yes").unwrap();
            std::thread::sleep(FOREVER);
            // Only reachable if nothing ever cancelled — a test that arranged an
            // interrupt and failed to deliver it. Exiting non-zero rather than
            // reporting a push keeps that from looking like a clean one.
            std::process::exit(1);
        }
        // The recording `git`, driving a real repository and interposing on
        // nothing. It exists for the suites whose subject is *another* object's
        // ambiguity: they need the branch to genuinely land, and they still need
        // the push counted, which the canned `accepted` above cannot do because
        // it never pushes. Nothing about how the invocation ends is changed, so
        // this mode adds no behaviour of its own — it only keeps the recording.
        "delegated" => delegate(&args),
        // The other half of the same ambiguity, and the other failure that
        // classifies `Unknown`: nothing is pushed and nothing is answered, so
        // the runtime's own deadline is what ends this — there is no timeout
        // flag in `git` and this is the only thing that can end it.
        "never_answers" => std::thread::sleep(FOREVER),
        other => panic!("unknown mode {other}"),
    }
}

/// Whether a mode is driving a real repository rather than answering from the
/// fixture's own constants.
fn delegating(mode: &str) -> bool {
    matches!(
        mode,
        "push_then_killed" | "push_then_waits" | "never_answers" | "delegated"
    )
}

/// Hand the invocation to the real `git`, unchanged.
///
/// The environment is *not* rebuilt: this child inherits exactly what the
/// adapter handed the fixture, so the push that lands the ref runs under the
/// same seven names the product built and the delegation cannot quietly grant
/// itself something the boundary forbids. Streams are inherited too, so the
/// porcelain report reaches the adapter's own pipes as though nothing were in
/// between.
fn delegate(args: &[String]) {
    let status = std::process::Command::new("git")
        .args(args)
        .status()
        .expect("git is on the PATH the adapter passed through");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
