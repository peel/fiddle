//! A scripted `wizcli` for the offline CVE gate.
//!
//! It is reached through the product's own `program`/`args` seam — the one that
//! exists for operators who must pin or wrap a scanner — and it is declared with
//! `required-features`, so `cargo build --release` never produces it. Nothing
//! here is compiled into the product.
//!
//! # It is permanent, not a placeholder
//!
//! Unlike `gh_stub` and `git_stub`, this fixture does not stand in for something
//! the suite could reach if it tried harder. Wiz is testable only in CI, where
//! the tenant credentials live, so **this milestone's gate never calls a real
//! `wizcli` at all**: the scanner arm of every offline test is this program, for
//! good. That makes its arm list the adapter's real contract rather than a
//! convenience, which is why the arms are named for *situations in the world*
//! and not for the errors they are expected to produce.
//!
//! # The arms, and the one that matters
//!
//! Five of the six are ordinary. `exit-nonzero-with-file` is the one this
//! fixture exists for: `wizcli` exits non-zero when an organisation policy flags
//! any finding in the tenant, including findings that have nothing to do with
//! this scan, and it writes a perfectly good report while doing it. An adapter
//! that read the status line first would report that as a failed scan. Producing
//! it here — a real non-zero exit over a real, parseable document — is what makes
//! "success is the artefact" a fact about a process rather than a claim about
//! one, and it is why `exit-nonzero-no-file` sits beside it: the two differ only
//! in the artefact, so an adapter that collapsed them would be caught.
//!
//! # Why it selects its arm from `argv`
//!
//! Not the environment. The adapter's environment is an allowlist, so a variable
//! carrying the test's own plumbing could not reach this process without
//! widening the boundary the fixture exists to prove. The arm arrives as the
//! first argument, through the same `args` seam an operator would wrap a real
//! scanner with — `gh_stub` is arranged the same way and its header gives the
//! same reason.
//!
//! # Why it prints the shared documents rather than embedding one
//!
//! `tests/support/document.rs` is where a scanner document is written down, and
//! its bytes are what the projection lanes assert against. A second copy here
//! would drift from those, and the drift would present as a projection bug in a
//! lane that never touched this file. So the module is included rather than
//! imitated — which is the whole reason those builders are in a file of their
//! own; see that file's header.

// Only the document builders are used here, and only some of them. The module
// is shared with the test suites, which use the rest.
#[allow(dead_code)]
#[path = "../support/document.rs"]
mod document;

use document::{libraries, os_packages, report_with, DEFAULT_LIBRARY_CVES, DEFAULT_OS_CVES};
use std::path::PathBuf;

/// The version this scanner announces. Not a version any real `wizcli` has, so
/// an assertion that finds it cannot have been satisfied by a scanner somebody
/// actually installed.
const STUB_VERSION: &str = "0.0.0-fiddle-stub";

/// What the scanner resolves an image reference to.
///
/// A full 64-hex digest, because that is what the adapter is entitled to expect
/// and a short one would let a reader that truncates pass.
const STUB_DIGEST: &str = "sha256:6f1b0d2c9a4e7385bd1c05fa9e37642c8b0d5713ae629f04c8d17b6a3e59042d";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arm = args
        .first()
        .cloned()
        .expect("the arm is the first argument, passed through the adapter's `args` seam");
    let report = output_file(&args)
        .expect("--json-output-file <path> must be passed by the adapter under test");

    match arm.as_str() {
        // A scan that worked. Both package arrays are populated, because the
        // projection has to read both and a document with one of them empty
        // would let a reader that found only `libraries` pass.
        "ok" => {
            banner(&args);
            write(&report, document());
        }
        // The reason this fixture exists. The document is written *first* and
        // the non-zero exit comes after, in that order and for that reason: a
        // stub that exited before writing would be testing a failed scan, which
        // proves nothing about whether the artefact or the status line decides.
        "exit-nonzero-with-file" => {
            banner(&args);
            write(&report, document());
            eprintln!("wizcli: policy 'default-vulnerabilities' matched 3 findings in this tenant");
            // 3 rather than 1, because 1 is what almost anything exits with and
            // an assertion that happens to pass against a generic failure is not
            // yet an assertion about a policy hit.
            std::process::exit(3);
        }
        // The other half of that pair: the same non-zero ending with nothing
        // written, so the two cases differ by the artefact alone.
        "exit-nonzero-no-file" => {
            banner(&args);
            eprintln!("wizcli: internal error while analysing layers");
            std::process::exit(3);
        }
        // The file is created and left empty, which is what a scanner killed
        // between opening its output and filling it leaves behind. Exit 0, so
        // nothing but the artefact can be what the adapter refuses on.
        "empty-file" => {
            banner(&args);
            write(&report, String::new());
        }
        // Written, non-empty, and not a document. A truncated JSON object rather
        // than prose, because a truncation is what a scanner that ran out of
        // disk or was killed mid-write actually produces, and because an adapter
        // testing for a leading `{` would pass on prose and fail here.
        "unparseable-file" => {
            banner(&args);
            write(&report, "{\"result\": {\"libraries\": [".to_string());
        }
        // Nothing to scan. No banner, because there was no image to resolve a
        // digest for, and the diagnostic is the daemon's own wording — it is the
        // only thing that separates this from `exit-nonzero-no-file`, which ends
        // identically.
        "no-such-image" => {
            eprintln!(
                "wizcli: failed to inspect {}: Error response from daemon: no such image",
                image(&args)
            );
            std::process::exit(3);
        }
        other => panic!("unknown arm {other}"),
    }
}

/// The document the successful arms write.
///
/// Both arrays, one advisory each, from the shared builders — so the bytes this
/// program puts on disk and the bytes the projection lanes assert against are
/// one value. The record is the scanner's own shape: nested under a package,
/// carrying `hasExploit` and a free-form `description` that no projected finding
/// admits, which is what leaves the injection boundary something to strip.
fn document() -> String {
    report_with(
        libraries(&DEFAULT_LIBRARY_CVES),
        os_packages(&DEFAULT_OS_CVES),
    )
    .raw()
    .to_string()
}

/// Announce what ran and what it resolved the image to.
///
/// On `stdout`, in two lines a real scanner would print for a person, because
/// the report itself carries neither: which scanner looked and what the tag
/// resolved to are facts about the *scan*. Printed by every arm that got as far
/// as having an image, so an arm that fails later still has its provenance
/// recorded ahead of the failure.
fn banner(args: &[String]) {
    println!("wizcli {STUB_VERSION}");
    println!("scanning {} at {STUB_DIGEST}", image(args));
}

/// Where the report goes: the adapter's `--json-output-file`.
fn output_file(args: &[String]) -> Option<PathBuf> {
    let at = args.iter().position(|arg| arg == "--json-output-file")?;
    args.get(at + 1).map(PathBuf::from)
}

/// The image reference, which the adapter passes last.
fn image(args: &[String]) -> String {
    args.last().cloned().unwrap_or_default()
}

/// Write the report, failing loudly.
///
/// A fixture that could not write its artefact would surface as whichever
/// classification the adapter reaches for a missing file, which is a passing
/// test of the wrong arm.
fn write(report: &PathBuf, body: String) {
    std::fs::write(report, body)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", report.display()));
}
