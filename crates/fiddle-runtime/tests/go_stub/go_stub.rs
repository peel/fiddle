//! A scripted `go` for the offline CVE gate.
//!
//! It is reached through the product's own `program`/`args` seam — the one that
//! exists for an operator who has to pin a toolchain or wrap it in a launcher —
//! and it is declared with `required-features`, so `cargo build --release` never
//! produces it. Nothing here is compiled into the product.
//!
//! # What it is for, and why the crate needs one at all
//!
//! `fiddle_runtime::cve::go::Go` is production code: it spawns a real `go` under
//! `crate::process::run_bounded`, in an environment built from nothing, and it is
//! what runs when this capability mitigates a CVE in a host repository's CI. There
//! is no Go toolchain in this project's development shell and no module proxy
//! behind one, so the offline gate cannot call that adapter against the real
//! thing. It can call it against this — which leaves the spawn, the environment,
//! the deadline, the output reading and the `go.mod`/`go.sum` restore all under
//! test, with only the toolchain scripted.
//!
//! That is the arrangement `wiz_stub` is under, and the reason it is worth the
//! `[[bin]]`: without it, M4a would ship a port whose only implementation is a
//! test double, and nothing in the gate would ever notice.
//!
//! # It selects nothing from its `argv` except the subcommand
//!
//! Unlike `wiz_stub`, there are no arms. `go` is a program with subcommands and
//! this answers them: which document comes back is a function of the tree it is
//! run in, not of a fixture switch, because rule 2's probe is precisely the
//! observation that the *same* command answers differently after a bump. An arm
//! would let a lane select that answer directly, which is the thing the probe is
//! supposed to measure.
//!
//! # Where the answers come from
//!
//! `tests/support/go_proxy.rs`, included rather than imitated, so the tree this
//! program writes and the tree the in-process stand-in writes are one
//! implementation. See that file's header for why a `[[bin]]` can share it at all.
//!
//! # Why it records its own environment and `argv`, and why into `HOME`
//!
//! The adapter's environment is an allowlist, and the only honest way to assert
//! an allowlist is against what a child *received* — a `Command` nobody spawned
//! proves that a builder was called and nothing more. So every invocation writes
//! [`CHILD_RECORD`] before it answers, exactly as `wiz_stub` does.
//!
//! It goes into `HOME` because that is the only writable place this child is told
//! about: there is no scratch flag on a `go` command line, the environment is
//! three names and cannot carry the test's own plumbing, and the remaining
//! candidate — the module root — is the tree whose cleanliness the probe's revert
//! is asserted by. A fixture that recorded there would dirty the very thing it is
//! standing next to. That the record lands in `HOME` at all is itself the
//! property: a toolchain's caches belong beside the worktree and not in it.

// The proxy is shared with the test suites, which use parts of it this program
// does not name.
#[allow(dead_code)]
#[path = "../support/go_proxy.rs"]
mod go_proxy;

/// What this process was given, written where the suite can read it back.
///
/// The name matches `wiz_stub`'s, because it is the same record answering the
/// same question; only the directory it lands in differs, for the reason in this
/// module's header.
const CHILD_RECORD: &str = "child.json";

fn main() {
    // The working directory is the module root, because that is how `go` is run:
    // the adapter sets it with `current_dir`, and a `go` that took the tree from
    // an argument would not be answering the question the adapter asks.
    let root = std::env::current_dir().expect("a working directory to be a module root in");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();

    // Before the answer, so that a command which exits non-zero still leaves a
    // record: what was handed to this process is as worth asking of a refusal as
    // of a document.
    record();

    let answer = go_proxy::run(&root, &borrowed);
    print!("{}", answer.stdout);
    eprint!("{}", answer.stderr);
    std::process::exit(answer.code);
}

/// Write down every argument and every environment variable this process was
/// started with.
///
/// The whole environment, not the names the adapter is expected to have set: an
/// assertion that a fourth name arrived can only be made against a record that
/// would have carried a fourth name.
///
/// Panics when there is no `HOME`, rather than skipping the record. An adapter
/// that stopped setting one is exactly the change this is here to notice, and a
/// silently absent record would read as "no child ran" in whichever assertion
/// went looking.
fn record() {
    let home = std::env::var("HOME")
        .expect("the adapter gives its child a HOME; without one there is nowhere to record");
    let argv: Vec<String> = std::env::args().collect();
    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let record = std::path::Path::new(&home).join(CHILD_RECORD);
    std::fs::write(
        &record,
        serde_json::json!({ "argv": argv, "env": env }).to_string(),
    )
    .unwrap_or_else(|source| panic!("could not write {}: {source}", record.display()));
}
