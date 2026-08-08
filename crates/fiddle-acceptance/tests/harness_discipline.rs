//! Structural guards over the acceptance harness itself.
//!
//! Every other test in this package asserts something about `fiddle`. These
//! assert something about the harness that runs them, because the harness has
//! its own way of being silently wrong: a suite that drives *some* binary is
//! exactly as green as a suite that drives the binary built from the sources
//! under test, and only one of the two is evidence.
//!
//! A companion to `crate_boundary.rs`, which does the same job for
//! `fiddle-core`'s purity, and deliberately the same shape: grep the sources for
//! the construct that must not appear, and carry a non-vacuity assertion so the
//! scan cannot pass by having found nothing to look at.

mod support;

use std::path::{Path, PathBuf};

/// The construct no acceptance test may name.
///
/// `assert_cmd`'s `cargo_bin` resolves a *path* under the target directory and
/// trusts that something already put a binary there. Under
/// `cargo test --workspace` nothing does — this package does not depend on
/// `fiddle-cli`, so `main.rs` is only ever compiled as a test harness under
/// `deps/` — so the path holds whatever a previous `cargo build` left, or
/// nothing at all. [`support::fiddle_binary`] carries the full account and the
/// alternative.
///
/// That helper is why no scenario needs the convention. This constant is why the
/// next scenario cannot reach for it anyway: the fix was behavioural, and a
/// behaviour nothing enforces is a convention, which is how the defect entered
/// the first time.
const BANNED: &str = "cargo_bin";

#[test]
fn no_acceptance_test_resolves_the_binary_by_convention() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let sources = rs_files(&root);
    assert!(
        !sources.is_empty(),
        "found no sources under {}; the scan would pass vacuously",
        root.display()
    );

    // The other half of the non-vacuity claim, and the half that is easier to
    // break: `code_only` could make the scan pass by returning nothing at all.
    // Anchor on a declaration that has to survive stripping, so an over-eager
    // stripper fails here instead of reporting a clean tree.
    let support = code_only(&read(&root.join("support/mod.rs")));
    assert!(
        support.contains("pub fn fiddle_binary"),
        "stripping ate the harness's own code; the scan is looking at nothing"
    );

    let mut offenders = Vec::new();
    for path in &sources {
        for line in offending_lines(&read(path)) {
            offenders.push(format!("{}:{line}", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "an acceptance test names `{BANNED}` at {offenders:?}; every scenario \
         must drive the binary `support::fiddle_binary` built from the sources \
         under test, never a path resolved by convention"
    );
}

#[test]
fn the_scan_reads_code_and_not_prose_about_it() {
    // A guard on a clean tree and a guard that sees nothing look identical from
    // the outside. These cases are the difference. The fixtures below hold the
    // banned name as string data, which the scan strips before looking — so this
    // file can describe what it forbids without tripping over itself, and
    // without an allowlist that would go stale on the next edit.
    assert_eq!(
        offending_lines("let name = Command::cargo_bin(\"fiddle\");\n"),
        vec![1],
        "a call in code must be a hit"
    );
    assert_eq!(
        offending_lines("use assert_cmd::cargo::cargo_bin;\n"),
        vec![1],
        "an import must be a hit too: the bare identifier is banned, not the \
         call syntax, so a rename cannot smuggle it past"
    );
    assert!(
        offending_lines("/// `cargo_bin` resolves a path and trusts it\n").is_empty(),
        "the explanation of why it is banned has to be able to name it"
    );
    assert!(
        offending_lines("/* cargo_bin */ /* /* cargo_bin */ */\n").is_empty(),
        "block comments, nested as Rust allows"
    );
    assert!(
        offending_lines("let s = \"cargo_bin\"; let c = '\\''; let q = '\"';\n").is_empty(),
        "string and character literals are not code either"
    );
    assert_eq!(
        offending_lines("// prose\nlet c = cargo_bin();\n"),
        vec![2],
        "stripping must not eat the code that follows a comment, or shift the \
         line numbers a failure reports"
    );
}

#[test]
fn the_binary_under_test_is_built_beside_this_test_binary() {
    // What `fiddle_binary` has to get right and cannot check for itself: the
    // binary it hands out must have been built under the same profile as the
    // test asking for it. A `<target>/<profile>/deps/<name>` test binary and a
    // `<target>/<profile>/fiddle` executable agree exactly when the profile
    // directory is one and the same, so compare the directories rather than
    // re-deriving either name.
    let test_exe = std::env::current_exe().unwrap();
    let profile_dir = test_exe
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("unexpected test binary location: {}", test_exe.display()));
    assert_eq!(
        support::fiddle_binary().parent().unwrap(),
        profile_dir,
        "the harness built `fiddle` under a different profile than this test \
         binary was built under, so the suite is driving a binary compiled with \
         other settings than it is testing"
    );
}

/// Every line number in `src` whose *code* names [`BANNED`].
fn offending_lines(src: &str) -> Vec<usize> {
    code_only(src)
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(BANNED))
        .map(|(index, _)| index + 1)
        .collect()
}

/// `src` with every comment, string literal and character literal removed.
///
/// Comments go because the reason a construct is banned is worth keeping, and it
/// cannot be worth reading without naming the construct — a guard that made
/// `support/mod.rs` delete its explanation to stay green would have traded the
/// account for the enforcement. String and character literals go for the same
/// reason one level up: this file has to hold the banned name as data in order
/// to search for it, and the cases above have to hold fixtures containing it.
///
/// Stripping both is stricter than exempting particular lines, not looser. What
/// remains is only what the compiler resolves as code, which is the only place a
/// call or an import can be, so no line is ever exempt and nothing goes stale
/// when this file is edited. It also means the ban can be on the bare
/// identifier rather than on call syntax, which catches a use declaration or an
/// alias as well as a call.
///
/// Newlines survive, so the line numbers a failure reports point at the real
/// source. The one construct not lexed exactly is the character literal, told
/// apart from a lifetime by whether a closing quote follows within two
/// characters — the same question Rust's own grammar asks.
fn code_only(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        let skip = literal_len(&chars, i).or_else(|| comment_len(&chars, i));
        match skip {
            Some(len) => {
                out.extend(chars[i..i + len].iter().filter(|c| **c == '\n'));
                i += len;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// The length of the string, byte string, raw string or character literal
/// starting at `i`, or `None` if none starts there.
///
/// An unterminated literal runs to the end of the file rather than being
/// reported: this scans sources the compiler has already accepted, so the case
/// only arises for a file that would not build, and swallowing the tail is the
/// safe direction — it cannot turn a real call into a miss without also turning
/// the file into a compile error.
fn literal_len(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    let raw = chars.get(j) == Some(&'r');
    if raw {
        j += 1;
    }
    let mut hashes = 0;
    while raw && chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }

    match chars.get(j) {
        Some('"') => {
            j += 1;
            loop {
                match chars.get(j) {
                    None => return Some(chars.len() - i),
                    Some('\\') if !raw => j += 2,
                    Some('"') if (1..=hashes).all(|k| chars.get(j + k) == Some(&'#')) => {
                        return Some(j + 1 + hashes - i)
                    }
                    Some(_) => j += 1,
                }
            }
        }
        // A tick is a character literal when a closing tick follows the one
        // character it may hold, or when an escape closes further along;
        // otherwise it opens a lifetime and is code.
        Some('\'') if !raw && chars.get(j + 1) == Some(&'\\') => (j + 3..j + 14)
            .find(|k| chars.get(*k) == Some(&'\''))
            .map(|k| k + 1 - i),
        Some('\'') if !raw && chars.get(j + 2) == Some(&'\'') => Some(j + 3 - i),
        _ => None,
    }
}

/// The length of the line or block comment starting at `i`, or `None` if none
/// starts there. Block comments nest, as Rust's do.
fn comment_len(chars: &[char], i: usize) -> Option<usize> {
    match (chars.get(i), chars.get(i + 1)) {
        (Some('/'), Some('/')) => {
            let mut j = i + 2;
            while j < chars.len() && chars[j] != '\n' {
                j += 1;
            }
            Some(j - i)
        }
        (Some('/'), Some('*')) => {
            let mut j = i;
            let mut depth = 0usize;
            while j + 1 < chars.len() {
                if chars[j] == '/' && chars[j + 1] == '*' {
                    depth += 1;
                    j += 2;
                } else if chars[j] == '*' && chars[j + 1] == '/' {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        return Some(j - i);
                    }
                } else {
                    j += 1;
                }
            }
            Some(chars.len() - i)
        }
        _ => None,
    }
}

/// Every `*.rs` path under `root`, recursively.
fn rs_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(rs_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {} ({e})", path.display()))
}
