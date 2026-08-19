mod support;

use std::path::{Path, PathBuf};

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

fn offending_lines(src: &str) -> Vec<usize> {
    code_only(src)
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(BANNED))
        .map(|(index, _)| index + 1)
        .collect()
}

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
        Some('\'') if !raw && chars.get(j + 1) == Some(&'\\') => (j + 3..j + 14)
            .find(|k| chars.get(*k) == Some(&'\''))
            .map(|k| k + 1 - i),
        Some('\'') if !raw && chars.get(j + 2) == Some(&'\'') => Some(j + 3 - i),
        _ => None,
    }
}

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
