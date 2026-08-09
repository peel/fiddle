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

use std::path::PathBuf;

/// What `rev-parse HEAD` answers, so a fixture with no repository behind it can
/// still complete a publish. A full object name, because the adapter refuses
/// anything that is not one.
const HEAD_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let subcommand = args.first().cloned().unwrap_or_default();
    let dir: PathBuf = std::env::current_dir().expect("the adapter runs its git in the worktree");

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

    if subcommand == "rev-parse" {
        println!("{HEAD_SHA}");
        return;
    }

    match std::fs::read_to_string(dir.join("mode"))
        .unwrap_or_else(|_| "accepted".to_string())
        .trim()
    {
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
        other => panic!("unknown mode {other}"),
    }
}
