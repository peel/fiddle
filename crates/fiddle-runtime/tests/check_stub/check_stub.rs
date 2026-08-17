//! A scripted check for the offline evaluation gate.
//!
//! `cve_evaluate` drives a [`Tree`] that answers out of a table. This program is
//! what the *other* half of that suite drives — `cve_evaluate_spawn`, where the
//! tree is a real worktree and every check is a real child — and it exists
//! because the five checks of design §2.6 are `go build`, `go fmt`, `go vet`,
//! `docker build` and a `wizcli` rescan, and this project's dev shell declares
//! `[rustToolchain, alejandra, gh, jq]`. There is no Go toolchain here, no
//! container daemon and no scanner. What the gate needs is not those programs:
//! it needs *a program*, started by the adapter, whose exit status and output
//! the adapter reads back.
//!
//! [`Tree`]: fiddle_runtime::evaluate::Tree
//!
//! # Why a compiled fixture and not `/bin/echo`
//!
//! Two reasons, and the second is the one that decided it. The first is that the
//! four situations the contract needs — exit zero and silent, exit zero and
//! talkative, exit non-zero, and never finish — are four different system
//! programs on a good day and four different *paths* to them on a bad one; one
//! fixture that can be asked for any of them assumes nothing about what a host
//! keeps in `/bin`.
//!
//! The second is [`RECORD`]. The claim the suite has to hold is that
//! `check.program` and `check.args` decide what is executed, and stdout can only
//! evidence that for a program that happens to print its own name. So every
//! invocation writes down its `argv`, its working directory and its whole
//! environment, exactly as `wiz_stub` does and for the same reason: an assertion
//! about what a child received can only be made against a record a child wrote.
//! It is written *unconditionally* — before the arm runs and before any exit —
//! so the record comes from the same spawn as the check under test rather than
//! from a special recording invocation nothing else shares.
//!
//! # Why the flags are not positional
//!
//! `wiz_stub` and `go_stub` take their arm as the first argument, because the
//! adapter appends to their `args` and the arm has to survive that. Here the
//! whole of `args` is the check's own declaration and nothing is appended, so
//! the fixture can read named flags — which is what lets one lane declare two
//! checks that differ only in what they print, and lets the record show that the
//! difference arrived through `argv`.
//!
//! `required-features` keeps it out of `cargo build --release`; see the
//! `[[bin]]` block in `Cargo.toml`, whose comments argue for every line of the
//! arrangement.

use std::path::{Path, PathBuf};

/// What this process was given, written where the suite can read it back.
///
/// Named by the caller through `--record`, because unlike a scan there is no
/// scratch directory the adapter and the test already agree about: a check is
/// run in the worktree, and a record written there would be a file the
/// attempt's own changed-file derivation then reports as work somebody did.
const RECORD: &str = "--record";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Before anything else, so that an arm which sleeps until it is killed, or
    // exits non-zero, has still said what it was started with.
    if let Some(path) = flag(&args, RECORD) {
        record(Path::new(&path));
    }

    // Long enough to be killed by any deadline a test would set, rather than a
    // duration chosen to be *just* longer than one: a fixture that raced its own
    // timeout would fail on a loaded machine and pass on a quiet one.
    if flag(&args, "--hang").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(600));
    }

    // Both streams, because the formatter criterion counts either: a tool that
    // complains on stderr has still complained.
    if let Some(text) = flag(&args, "--say") {
        println!("{text}");
    }
    if let Some(text) = flag(&args, "--warn") {
        eprintln!("{text}");
    }

    // Last, and zero when nobody asked, so an unadorned invocation is the
    // silent success the contract's ordinary checks are.
    if let Some(code) = flag(&args, "--exit") {
        std::process::exit(code.parse().expect("--exit takes a number"));
    }
}

/// The value after `name`, or `None` when it was not passed.
fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == name)?;
    match args.get(at + 1) {
        Some(value) => Some(value.clone()),
        None => panic!("{name} takes a value"),
    }
}

/// Write down what this process was started with, where it was started, and
/// what it can see from there.
///
/// `argv` includes this program's own path, because that is what `argv` is — and
/// it is the whole point here: an adapter that ignored `check.program` and ran
/// something of its own choosing is caught by the first element of this array
/// and by nothing else the suite can observe.
///
/// The whole environment rather than the names the workspace is expected to set,
/// for `wiz_stub`'s reason: an assertion that a fifth name arrived can only be
/// made against a record that would have carried a fifth name.
///
/// `cwd` and the entries beside it are what make "in the tree under judgement" a
/// fact rather than a hope. A check that ran in the runner's own directory would
/// report a different path and a different listing, and both are asserted.
fn record(path: &Path) {
    let cwd = std::env::current_dir().expect("a working directory");
    let mut entries: Vec<String> = std::fs::read_dir(&cwd)
        .unwrap_or_else(|source| panic!("could not list {}: {source}", cwd.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    entries.sort();

    let argv: Vec<String> = std::env::args().collect();
    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();

    let record = serde_json::json!({
        "argv": argv,
        "cwd": cwd.to_string_lossy(),
        "entries": entries,
        "env": env,
    });
    write(path.to_path_buf(), record.to_string());
}

/// Write the record, failing loudly.
///
/// A fixture that could not write its record would surface as a missing-file
/// panic in the assertion rather than here, which is a failure naming the wrong
/// thing.
fn write(path: PathBuf, body: String) {
    std::fs::write(&path, body)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", path.display()));
}
