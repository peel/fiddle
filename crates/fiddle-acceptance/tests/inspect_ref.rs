//! Black-box coverage of the `fiddle inspect <invocation-ref>` contract.
//!
//! The invocation reference is the identity every later command is addressed
//! by, so both halves of its contract are asserted from outside the process:
//! a well-formed reference is echoed back with its scheme, and each malformed
//! shape is rejected with a diagnostic naming *its own* defect.

use assert_cmd::Command;

#[test]
fn inspect_echoes_a_parsed_invocation_ref() {
    let out = Command::cargo_bin("fiddle")
        .unwrap()
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

/// Each malformed shape fails for its own reason, so each must be *told* its own
/// reason. Three identical "invalid invocation ref" messages would satisfy the
/// exit code and still leave the caller guessing, so the diagnostics are also
/// asserted to be pairwise distinct.
#[test]
fn inspect_rejects_a_malformed_invocation_ref() {
    let mut diagnostics = Vec::new();
    for (arg, needle) in [
        ("bogus", "<scheme>:<value>"),
        ("mystery:x", "unknown invocation scheme"),
        ("beans:", "must not be empty"),
    ] {
        let out = Command::cargo_bin("fiddle")
            .unwrap()
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
    let out = Command::cargo_bin("fiddle")
        .unwrap()
        .args(["inspect", "bogus", "--config", "no/such/fiddle.toml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("<scheme>:<value>"), "stderr={stderr}");
    assert!(!stderr.contains("no/such/fiddle.toml"), "stderr={stderr}");
}
