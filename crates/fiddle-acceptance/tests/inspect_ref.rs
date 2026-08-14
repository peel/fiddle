//! Black-box coverage of the `fiddle <command> <invocation-ref>` contract.
//!
//! The invocation reference is the identity every later command is addressed
//! by, so both halves of its contract are asserted from outside the process:
//! a well-formed reference is echoed back with its scheme, and each malformed
//! shape is rejected with a diagnostic naming *its own* defect.
//!
//! The grammar is one gate rather than one command's gate, which is why the
//! last test here drives `run` instead of `inspect`: `run` is the command that
//! derives paths from a reference, so it is the one that has to be *shown*
//! creating nothing when the reference is refused.

mod support;

use support::Scenario;

#[test]
fn inspect_echoes_a_parsed_invocation_ref() {
    let out = support::fiddle_command()
        .args([
            "inspect",
            "beans:fiddle-m0-demo",
            "--config",
            "../../tests/fixtures/fiddle.toml",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["invocation_ref"], "beans:fiddle-m0-demo");
    assert_eq!(v["scheme"], "beans");
}

/// **A scheme that discovers its own work is accepted with no value.**
///
/// ADR 019. Asserted from outside the process because the round trip is a claim
/// about what a caller can *type*: `inspect` echoes the reference it parsed, so
/// a bare reference rendered as `cve:` would be text the binary prints and then
/// refuses. The unit tests pin `as_str`; this pins that the text reaching a
/// caller is that same text.
///
/// The exit code is asserted as "not the usage code" rather than as `0`. What
/// this test owns is the grammar gate — and stdout is only written once the
/// reference has parsed, so the echoed fields are unreachable without it —
/// while what happens *after* the gate belongs to the CVE orchestration that
/// has not landed yet.
#[test]
fn inspect_accepts_a_self_discovering_scheme_with_no_value() {
    let out = support::fiddle_command()
        .args([
            "inspect",
            "cve",
            "--config",
            "../../tests/fixtures/fiddle.toml",
            "--json",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_ne!(
        out.status.code(),
        Some(2),
        "`cve` is a complete reference and must not be refused as usage; stderr = {stderr}"
    );
    assert!(
        !stderr.contains("<scheme>:<value>"),
        "the grammar must not demand a value it has no use for; stderr = {stderr}"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("a parsed reference is echoed on stdout: {e}; stderr = {stderr}")
    });
    assert_eq!(v["invocation_ref"], "cve", "echoed bare, never as `cve:`");
    assert_eq!(v["scheme"], "cve");
}

/// Each malformed shape fails for its own reason, so each must be *told* its own
/// reason. Four identical "invalid invocation ref" messages would satisfy the
/// exit code and still leave the caller guessing, so the diagnostics are also
/// asserted to be pairwise distinct.
///
/// Admitting the bare form added a fifth *shape* and no fifth defect: `cve:` is
/// still an empty value, and a scheme written without its value is still
/// malformed. Every row below is therefore unchanged, which is the point of
/// running it again rather than of editing it.
#[test]
fn inspect_rejects_a_malformed_invocation_ref() {
    let mut diagnostics = Vec::new();
    for (arg, needle) in [
        ("bogus", "<scheme>:<value>"),
        ("mystery:x", "unknown invocation scheme"),
        ("beans:", "must not be empty"),
        ("beans:../../../pwned", "ASCII letters, digits"),
    ] {
        let out = support::fiddle_command()
            .args([
                "inspect",
                arg,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "arg={arg}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains(needle), "arg={arg} stderr={stderr}");
        assert!(
            out.stdout.is_empty(),
            "a rejected reference must write nothing to stdout; arg={arg}"
        );
        diagnostics.push((arg, stderr));
    }

    for (i, (arg, stderr)) in diagnostics.iter().enumerate() {
        for (other_arg, other) in &diagnostics[i + 1..] {
            assert_ne!(
                stderr, other,
                "`{arg}` and `{other_arg}` fail for different reasons and must not \
                 be reported with the same diagnostic"
            );
        }
    }
}

/// The reference is validated before anything else, so the defect a caller is
/// told about is the one in the argument they typed — not a missing
/// configuration document they never mentioned.
#[test]
fn a_malformed_reference_is_reported_without_reference_to_configuration() {
    let out = support::fiddle_command()
        .args(["inspect", "bogus", "--config", "no/such/fiddle.toml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("<scheme>:<value>"), "stderr={stderr}");
    assert!(!stderr.contains("no/such/fiddle.toml"), "stderr={stderr}");
}

/// **A reference can never name a place outside the configured roots.**
///
/// `run` derives three paths from the reference's slug — the published bundle,
/// the attempt journal, and the sources the stub ports read — so a value
/// carrying `..` reaches outside both `<report.dir>` and `<stub.root>`. It did:
/// before the grammar constrained the value, this exact invocation exited 20
/// having written `<report.dir>/../pwned/<attempt>/report.json`, a bundle two
/// levels above the directory the configuration names.
///
/// Asserted against the filesystem rather than against the exit code, because
/// the escape *also* exited non-zero: an exit-code assertion alone would have
/// been green against the bug. The whole project tree is compared before and
/// after, so a file created anywhere — inside the roots, beside them, or above
/// them — fails this test.
#[test]
fn a_traversing_reference_creates_nothing_anywhere() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let before = s.project_tree();

    let out = s.run_raw_with(&["--json"], "beans:../../../pwned");

    assert_eq!(
        out.status.code(),
        Some(2),
        "a reference that is not an identifier is invalid input, stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "a rejected reference must write nothing to stdout, got {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a refused reference must leave the project tree byte for byte as it found it"
    );
    assert!(
        !s.report_dir().exists(),
        "a refused reference must not bring `<report.dir>` into existence"
    );
    // The escape landed *above* `<report.dir>`, so the containment claim is
    // only worth making if the test looks there too.
    assert!(
        !s.dir().join("pwned").exists(),
        "nothing may be created outside the roots the configuration names"
    );
}
