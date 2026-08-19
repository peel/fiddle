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
/// malformed. Every row that was here before is therefore unchanged, which is the
/// point of running it again rather than of editing it.
///
/// `cve:` is a row of its own all the same, and not because it fails differently
/// — it fails identically, and the pairwise check below is what makes it earn its
/// place. Two callers who wrote an empty value have *different* repairs available
/// when one of their schemes stands alone, so `beans:` and `cve:` must not be
/// answered with the same words.
#[test]
fn inspect_rejects_a_malformed_invocation_ref() {
    let mut diagnostics = Vec::new();
    for (arg, needle) in [
        ("bogus", "<scheme>:<value>"),
        ("mystery:x", "unknown invocation scheme"),
        ("beans:", "must not be empty"),
        ("cve:", "must not be empty"),
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

/// **The advice for an empty value is the advice for the scheme it was given
/// for.**
///
/// For every scheme but one there is a single legal repair: name the work. `cve`
/// stands alone, so the repair a caller who wrote `cve:` needs is not the one the
/// others need — drop the separator and sweep, rather than name the work — and
/// those are different work. A sweep scans the configured image and opens what it
/// finds. Advice that named only the appending repair sent an operator who wanted
/// the sweep to the wrong one, silently and with an exit code of 2 either way.
///
/// The two halves are asserted together, and the second is what makes the first
/// worth having: the bare form has to be *offered* for `cve:`, and it has to be
/// **absent** for `beans:`, where writing `beans` alone is refused by the grammar.
/// A help string that offered both repairs to everybody would pass the first
/// assertion and be a new defect.
///
/// # Why the valued form is asserted *absent* here
///
/// This assertion used to be its mirror image: it required the diagnostic to
/// contain `cve:<identifier>` and called it "the valued form, which remediates
/// one finding". Nothing in this build remediates one named finding, so that was
/// a lane pinning a sentence rather than a behaviour — and it kept the sentence
/// alive by pinning it. A repair has to be an invocation that works, and this one
/// is refused; see `no_operator_facing_surface_promises_the_valued_form` for the
/// property over every surface, and `the_valued_form_of_a_self_discovering_scheme
/// _is_refused` for the refusal itself.
#[test]
fn an_empty_value_is_told_every_repair_its_own_scheme_admits() {
    let refuse = |arg: &str| {
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
        String::from_utf8(out.stderr).unwrap()
    };

    let sweep = refuse("cve:");
    assert!(
        sweep.contains("discovers its own work"),
        "a scheme that stands alone must be told so, or the bare form is a repair \
         the operator has no way to learn about: {sweep}"
    );
    assert!(
        sweep.contains("`cve`"),
        "the bare form is one of the two repairs and must be spelled: {sweep}"
    );
    assert!(
        valued_cve_mentions(&sweep).is_empty(),
        "the valued form is not implemented in this build, so offering it as a repair \
         sends a caller from one refusal to another: {sweep}"
    );

    let tracked = refuse("beans:");
    assert!(
        !tracked.contains("discovers its own work"),
        "`beans` names a work item and `beans` alone is refused, so offering the \
         bare form here would be advice that does not parse: {tracked}"
    );
    assert!(
        tracked.contains("beans:fiddle-m0-demo"),
        "the one repair a tracked scheme has is to name the work: {tracked}"
    );
}

/// **A scheme fiddle does not know is never described as recognised.**
///
/// `notascheme:` is two defects at once, and the grammar reports the empty value
/// because that is the more specific one. The advice, though, was written for the
/// schemes that *are* recognised: it told this caller their scheme was fine and
/// sent them to append an identifier — the half of the reference that is not
/// wrong, and a repair that cannot work, because `notascheme:anything` is
/// refused for a different reason.
///
/// So the empty-value complaint is asserted to still be the one made — the parse
/// order is unchanged and is not what this fixes — while the advice is asserted
/// to name the defect the caller can act on. The legal schemes are spelled here
/// rather than read from `fiddle-core`, for the reason this crate has no
/// dependency on it: the list is a promise the design makes to an operator, and
/// a lane that derived it from the enum would agree with any list the enum
/// happened to hold.
///
/// # Why naming the set is not enough
///
/// Listing the schemes and then saying what to write after one of them is two
/// claims, and the second is not true of the whole set: `cve` discovers its own
/// work and needs no value (ADR 019). A version of this help that named all five
/// and then said to append "the work it names" satisfied every assertion about
/// *recognition* above while telling a caller who had mistyped `cve` to write
/// `cve:<identifier>` — a read of one named finding, which the `cve:` help two
/// tests up is careful to call different work from a sweep. Advice that is wrong
/// about the milestone's own scheme is no smaller a defect than advice that is
/// wrong about recognition.
///
/// So each scheme is asserted against the shape the design gives *it*: the four
/// that name work must appear where a value is required, `cve` must appear where
/// none is, and neither may appear in the other's half. That is also how every
/// scheme still gets checked for being offered at all — one in neither half is a
/// scheme the operator was not told about. The split is spelled here for the same
/// reason the list is: it is `stands_alone`'s promise to an operator, and a lane
/// that read `stands_alone` would ratify whatever the enum happened to say.
#[test]
fn an_empty_value_after_an_unknown_scheme_is_not_told_its_scheme_is_recognised() {
    let out = support::fiddle_command()
        .args([
            "inspect",
            "notascheme:",
            "--config",
            "../../tests/fixtures/fiddle.toml",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();

    assert!(
        stderr.contains("must not be empty"),
        "the empty value is still the defect reported, before the scheme is \
         looked up: {stderr}"
    );
    assert!(
        !stderr.contains("the scheme is recognised"),
        "`notascheme` is not one of the schemes, so telling this caller it is \
         recognised is a confident and wrong diagnosis: {stderr}"
    );
    assert!(
        stderr.contains("not one fiddle knows"),
        "the defect the caller can act on is the scheme, and it has to be \
         named: {stderr}"
    );
    // The help is wrapped to the terminal by the renderer, so a sentence that
    // reads as one line to an operator arrives here with newlines and padding
    // inside it. Flattened, the words are the words that were written.
    let advice = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some((takes_a_value, needs_none)) = advice.split_once("discover their own work need none:")
    else {
        panic!(
            "the advice has to separate the schemes that require a value from the ones \
             that do not, or it is offering one shape to a set that has two: {advice}"
        );
    };
    // Every scheme with the shape the design gives it, in one table: a scheme
    // that appeared in neither half would be a scheme the caller is not offered,
    // which is the same defect as naming four of five.
    for (scheme, stands_alone) in [
        ("beans", false),
        ("jira", false),
        ("scheduled", false),
        ("scanner", false),
        ("cve", true),
    ] {
        if stands_alone {
            assert!(
                needs_none.contains(scheme),
                "`{scheme}` discovers its own work, so a caller who meant it has to be \
                 told the bare form: {advice}"
            );
            assert!(
                !takes_a_value.contains(scheme),
                "`{scheme}` stands alone, so advising this caller to write \
                 `{scheme}:<identifier>` sends someone who wanted a sweep to a read of \
                 one named finding — different work, and the reason the scheme carries \
                 its own advice at all: {advice}"
            );
        } else {
            assert!(
                takes_a_value.contains(scheme),
                "`{scheme}` names a work item and is refused without one, so it belongs \
                 where a value is required: {advice}"
            );
            assert!(
                !needs_none.contains(scheme),
                "`{scheme}` written alone does not parse, so listing it among the \
                 schemes that need no value is advice refused when followed: {advice}"
            );
        }
    }
}

/// **The valued form of a self-discovering scheme is refused, not silently
/// rescoped.**
///
/// `cve:CVE-2026-1234` parses, and ADR 019 keeps it parsing — the grammar is
/// what a milestone implementing narrowing builds on. What does not exist is a
/// capability that acts on one named finding: `MitigateConfig` declares no
/// advisory field and the sweep scans `[orchestration.cve] image` alone. So the
/// two things a run over this reference could do are both wrong. It can block on
/// a work-item read that has no source, which is what it did; or, handed a stub
/// work file, it can sweep the entire image while deriving its identity from the
/// narrowed reference — a second branch and a second pull request for work
/// already covered, which is the duplicate-effect hazard ADR 019's own context
/// names, arriving by the other door.
///
/// Refused before the configuration is read, and refused whatever
/// `--capability` asked for: the *form* is what this build cannot act on, and no
/// capability changes that. Both invocations are driven because they failed
/// differently and neither failure mentioned the form — the operator's own
/// `fiddle run cve:CVE-2026-1234` exited 2 naming a table it should never have
/// got as far as needing, and `--capability stub_mark` exited 20 having asked
/// the stub port for `stub:work/CVE-2026-1234.json`.
///
/// The filesystem is asserted for the same reason
/// `a_traversing_reference_creates_nothing_anywhere` asserts it: a non-zero exit
/// is not evidence that nothing was attempted.
#[test]
fn the_valued_form_of_a_self_discovering_scheme_is_refused() {
    let s = Scenario::new();
    let before = s.project_tree();

    for extra in [Vec::new(), vec!["--capability", "stub_mark"]] {
        let out = s.run_raw_with(&extra, "cve:CVE-2026-1234");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "a form this build cannot act on is invalid input; extra={extra:?} \
             stderr={stderr}"
        );
        assert!(
            stderr.contains("not implemented in this build"),
            "the refusal has to say the form is unimplemented, or its reader takes it \
             for a configuration defect and goes looking for the table to add; \
             extra={extra:?} stderr={stderr}"
        );
        assert!(
            stderr.contains("cve:CVE-2026-1234"),
            "and it has to name the reference it refused; extra={extra:?} \
             stderr={stderr}"
        );
        assert!(
            stderr.contains("write `cve`"),
            "the one invocation this build does implement is the sweep, and it has to \
             be spelled; extra={extra:?} stderr={stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "a refused invocation must write nothing to stdout, got {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    assert_eq!(
        s.project_tree(),
        before,
        "a refused form must leave the project tree byte for byte as it found it"
    );
    assert!(
        !s.report_dir().exists(),
        "a refused form must not bring `<report.dir>` into existence"
    );
}

/// Every occurrence in `text` of a `cve` reference *carrying a value*, with a
/// little of what follows it, so a failure names the sentence it found.
///
/// A `cve:` written with nothing usable after it — as in "`cve:` is still an
/// error" — is not a mention of the valued form, so the character after the
/// colon is what decides. The set is the value grammar ADR 011 fixes, spelled
/// here rather than read from `fiddle-core` for the reason this crate depends on
/// nothing of it, plus `<`, which is how help text writes a placeholder.
fn valued_cve_mentions(text: &str) -> Vec<String> {
    text.match_indices("cve:")
        .filter(|(at, _)| {
            text[at + "cve:".len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '<'))
        })
        .map(|(at, _)| text[at..].chars().take(32).collect())
        .collect()
}

/// **No operator-facing surface promises a form this build does not implement.**
///
/// This is the lane that was missing, and its absence is why four surfaces came
/// to advertise `cve:CVE-2026-1234` while nothing implemented it. Two of the
/// four were introduced by a commit whose subject was "help text describes the
/// grammar the binary has, not the one it had": help written from an ADR
/// describes what was *decided*, and only something driving the binary can say
/// what was *built*.
///
/// The lane that should have caught it did the opposite. It asserted that the
/// `cve:` diagnostic **contained** `cve:<identifier>` and called it "the valued
/// form, which remediates one finding" — a test whose subject is a sentence,
/// which cannot notice that the sentence is false. So every surface here is read
/// off the compiled binary: `--help` as clap renders it, and each diagnostic as
/// miette renders it.
///
/// The refusal is checked too, and it is the *only* surface allowed to name the
/// valued form, because naming what it refuses is its job. Without that half the
/// property would be satisfiable by saying nothing anywhere, which is how an
/// operator ends up with an invocation that fails for no stated reason.
#[test]
fn no_operator_facing_surface_promises_the_valued_form() {
    let text = |args: &[&str], expected: Option<i32>, stderr: bool| -> String {
        let out = support::fiddle_command().args(args).output().unwrap();
        if let Some(expected) = expected {
            assert_eq!(
                out.status.code(),
                Some(expected),
                "args={args:?} stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        String::from_utf8(if stderr { out.stderr } else { out.stdout }).unwrap()
    };
    let help = |args: &[&str]| text(args, Some(0), false);
    let refusal = |reference: &str, command: &str| {
        text(
            &[
                command,
                reference,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ],
            Some(2),
            true,
        )
    };

    for (surface, rendered) in [
        ("fiddle --help", help(&["--help"])),
        ("fiddle inspect --help", help(&["inspect", "--help"])),
        ("fiddle run --help", help(&["run", "--help"])),
        (
            "the `cve:` diagnostic from inspect",
            refusal("cve:", "inspect"),
        ),
        ("the `cve:` diagnostic from run", refusal("cve:", "run")),
    ] {
        let promised = valued_cve_mentions(&rendered);
        assert!(
            promised.is_empty(),
            "{surface} offers a valued `cve` reference, and no capability in this \
             build acts on one — an operator following it reaches a refusal: \
             {promised:?}"
        );
    }

    for command in ["inspect", "run"] {
        let refused = refusal("cve:CVE-2026-1234", command);
        assert!(
            !valued_cve_mentions(&refused).is_empty(),
            "the surface that refuses the form has to name it, or its reader cannot \
             tell which half of their reference was the problem: {command} {refused}"
        );
        assert!(
            refused.contains("not implemented in this build"),
            "and it has to say why, in the words every other surface is silent in: \
             {command} {refused}"
        );
    }
}
