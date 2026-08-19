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
        ("bogus", "is not an invocation reference"),
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
    assert!(
        stderr.contains("is not an invocation reference"),
        "stderr={stderr}"
    );
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
    valued_mentions(text, "cve")
}

/// [`valued_cve_mentions`] for any scheme, because the property "a scheme that
/// stands alone is never shown carrying a value" is about *whichever* schemes
/// stand alone, and `cve` being the only one today is a fact about this build
/// rather than about the property.
fn valued_mentions(text: &str, scheme: &str) -> Vec<String> {
    let prefix = format!("{scheme}:");
    text.match_indices(&prefix)
        .filter(|(at, _)| {
            text[at + prefix.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '<'))
        })
        .map(|(at, _)| text[at..].chars().take(32).collect())
        .collect()
}

/// Every **shape template** in `text`: a colon-joined pair whose scheme side is
/// not one of `schemes`, and which therefore stands for any scheme at all.
///
/// `beans:fiddle-m0-demo` is not one — it is one scheme's shape, and true of that
/// scheme. `<scheme>:<value>` is: it says what *a* reference is made of, so it is
/// a claim about the whole grammar, and a grammar with two shapes has no single
/// template that is true of it. Which side of that line a pair falls on is
/// decided by the scheme list read off the binary, so nothing here is pinned to
/// today's placeholder spelling — `<source>:<id>` reads the same way, and a sixth
/// real scheme stops being a template the day it is added.
///
/// The `<` and `>` a placeholder is conventionally written in are trimmed before
/// the comparison rather than required, because a template need not be written in
/// angle brackets to be one. A doubled colon is excluded, so an identifier like
/// `fiddle::invocation_ref::malformed` is not read as a shape.
fn shape_templates(text: &str, schemes: &[&str]) -> Vec<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '<' | '>');
    text.match_indices(':')
        .filter(|(at, _)| {
            text[at + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '<'))
        })
        .filter_map(|(at, _)| {
            let head = text[..at].rsplit(|c: char| !word(c)).next()?;
            let scheme = head.trim_matches(|c| matches!(c, '<' | '>'));
            (!scheme.is_empty() && !schemes.contains(&scheme)).then(|| {
                let tail: String = text[at + 1..].chars().take_while(|c| word(*c)).collect();
                format!("{head}:{tail}")
            })
        })
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
///
/// **What this lane cannot see, and what covers it instead: nothing mechanical.**
/// It reads the compiled binary, which is the right subject for an operator-facing
/// promise and structurally blind to source prose. A fifth surface proved that:
/// the doc comment on `a_bare_slug_cannot_collide_with_a_valued_slug` in
/// `fiddle-core`'s `identity.rs` still stated as present fact that the valued form
/// remediates one finding, and this lane passed over it. Two guards were weighed
/// and both rejected on evidence. A **doctest** cannot reach it: rustdoc builds
/// without `cfg(test)`, so a deliberately failing doctest placed in that test
/// module was collected zero times — `cargo test --doc -p fiddle-core` exited 0 —
/// while the identical probe on `InvocationRef::slug` failed as expected, so the
/// blindness is rustdoc's and no harness setting moves it. A **grep** cannot
/// either: when the fifth surface was found, "remediates one finding" stood at five
/// sites and four were correct — framed as history, or ADR 019 stating the claim
/// false — and recording this made more of them. Separating them means reading the
/// framing, and a pattern narrow enough to try is pinned to today's wording: it
/// would pass the next paraphrase and red on the next legitimate history note. Source doc comments are therefore a review matter
/// here, and saying so is worth more than a guard that only ever catches the
/// sentence already found. `docs/BACKLOG.md` records it with both experiments.
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

/// **Every surface that says how a reference is written offers the bare form for
/// every scheme that takes it.**
///
/// This is the sibling of `no_operator_facing_surface_promises_the_valued_form`
/// and the direction that lane cannot see. That one hunts for a *promise of the
/// valued form*; the defect here was a **denial of the bare form** — the
/// `Malformed` diagnostic read "invocation reference must be `<scheme>:<value>`"
/// and its help offered a colon and a valued example and nothing else. Every
/// assertion in that lane was satisfied while this string was live, because
/// saying `cve` needs a value never mentions `cve:`. Two searches aimed at one
/// class, five days apart, and neither pattern caught it: the first looked for the
/// form being promised, the second recorded the *phrase* it had found.
///
/// So this lane hunts for no phrase. It asks each scheme's shape of the binary and
/// then holds the binary's own words to the answer:
///
/// 1. the schemes are read off the `unknown_scheme` diagnostic, which derives its
///    list from `InvocationScheme::ALL`, so a sixth scheme joins this lane the day
///    it becomes something a caller may write;
/// 2. whether each one stands alone is decided by **driving the bare form** — if
///    `fiddle inspect <scheme>` is not refused as malformed, the bare form is
///    something a caller can type;
/// 3. every grammar surface must then name each such scheme, and never in a valued
///    position. Naming it is the half the old wording failed: a help that lists
///    only the valued shape tells the operator this milestone exists for that
///    their invocation is illegal;
/// 4. and no *part* of a surface denies the bare form by itself. A diagnostic is
///    two things an operator reads separately — the line that judges what they
///    typed, and the advice beneath it — so each is held to (3) alone: a part that
///    gives a **shape template** must name the schemes that template is false of,
///    in that part and not in a sentence below it. A template is a colon-joined
///    pair whose scheme side is not one of the schemes (1) read off the binary, so
///    it is recognised by standing for any scheme rather than by its spelling:
///    `<source>:<id>` reads the same as `<scheme>:<value>`, and a sixth real scheme
///    stops being a template the day it is added.
///
/// **(4) is here because (1)–(3) were measured to be blind without it**, and that
/// measurement is the whole reason to believe this lane now guards what it is named
/// for. With the `Malformed` message reverted in place to "invocation reference must
/// be `<scheme>:<value>`" and the corrected help left underneath it, every
/// assertion in (1)–(3) passed: `cve` was named in the advice, so the flattened
/// whole text named it, and the one sentence that told the operator their reference
/// was illegal was free to go on denying the form the binary accepts. The revert
/// was caught only by two lanes that assert the message text — the coupling this
/// lane exists so as not to need. Both halves of the defect it is named for are now
/// covered: the advertised form nothing implements is
/// `no_operator_facing_surface_promises_the_valued_form`'s, and the implemented form
/// a surface denies is (4)'s.
///
/// There is nothing here to keep in step with the wording, which is the point. The
/// oracle is behaviour, not `stands_alone` — a lane reading the enum would agree
/// with whatever the enum said, and this lane instead fails if the enum and the
/// binary disagree with the prose.
///
/// # What it cannot see, and why that is not fixable from here
///
/// **The surface list is enumerated by hand.** A process cannot be asked to render
/// every string it might print: the diagnostics are reachable only by supplying an
/// input that provokes each, and a sixth diagnostic added with a sixth defect is
/// not discoverable from outside. Nor can the surfaces be *filtered* by whether
/// they state the grammar — deciding that means reading meaning, and the two
/// attempts available are both worse than the gap. Requiring **every** surface to
/// name the standing-alone schemes fails honestly-silent text: `fiddle --help`
/// lists subcommands and says nothing about references, and should not have to.
/// Triggering on a pattern like `<scheme>:<value>` is the phrase hunt that let this
/// defect through twice — the wording that misled had `must be` in it, and the next
/// one need not. That last objection is to a template used as a **gate** on whether
/// a surface gets checked, where a rewording quietly opts the surface out. (4) uses
/// one as a **prohibition**, where the failure modes run the other way: a rewording
/// that keeps the template still reds, and one that drops it gives up nothing this
/// lane was holding. They are not the same instrument.
///
/// **A denial written without a shape is not caught.** (4) recognises a universal
/// claim about the grammar by the template it is made of, and "a reference must
/// name the work it acts on" makes that claim in prose. Two stronger rules were
/// weighed and rejected on evidence. Requiring every part to name the
/// standing-alone schemes reds on the corrected verdict itself, which names two
/// shapes and no schemes and is right to — a diagnostic that had to list `cve` in
/// the sentence judging `cvfoo` would be worse text. Requiring the parts to agree
/// with each other means deciding whether two sentences contradict, which is
/// reading meaning. So the boundary is a *shape offered as the shape*, which is
/// what both instances of this defect were made of; a prose denial is a review
/// matter and `docs/BACKLOG.md` records it.
///
/// So the boundary is: **placement** is derived and cannot go stale, while
/// **membership of the list** — and the flag saying which surfaces enumerate the
/// schemes — is a review matter, exactly as it is for
/// `no_operator_facing_surface_promises_the_valued_form`. That is narrower than
/// what was unheld before, where neither was. A new operator-facing surface that
/// states the grammar and is not added below is the gap; `docs/BACKLOG.md` records
/// it.
#[test]
fn every_grammar_surface_offers_the_bare_form_for_every_scheme_that_takes_it() {
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
    // A diagnostic is wrapped to the terminal by miette, so a sentence that reads
    // as one line to an operator arrives here with newlines, padding and gutter
    // marks inside it. Flattened, the words are the words that were written.
    let flatten = |rendered: &str| -> String {
        rendered
            .replace(['│', '×'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };

    // A diagnostic reaches an operator as two things that travel apart: the `×`
    // line that judges what they typed, and the `help:` advice beneath it. They
    // are read separately — a log line, a CI summary and a shell scrollback all
    // keep the first and lose the second — and this lane has measured what
    // reading them as one blob costs. The defect it exists for lived in the
    // verdict; with the advice below it correct, every whole-text assertion in
    // this lane was satisfied by the word `cve` appearing in that advice, and the
    // verdict was free to go on denying the bare form. So the surfaces below are
    // held part by part as well as whole.
    //
    // A surface with no `×` is clap's, which has one part. The code line above
    // the verdict is dropped with the split: it is an identifier for machines,
    // not a sentence to an operator.
    let parts = |rendered: &str| -> Vec<(&'static str, String)> {
        let Some((_, judged)) = rendered.split_once('×') else {
            return vec![("its text", flatten(rendered))];
        };
        let (verdict, advice) = judged.split_once("help:").unwrap_or((judged, ""));
        [
            ("the line that judges the reference", verdict),
            ("the advice beneath that line", advice),
        ]
        .into_iter()
        .map(|(part, rendered)| (part, flatten(rendered)))
        .filter(|(_, rendered)| !rendered.is_empty())
        .collect()
    };

    // Step 1: the schemes, from the one diagnostic whose job is to name them all.
    let unknown = flatten(&refusal("mystery:x", "inspect"));
    let (_, tail) = unknown.split_once("expected one of ").unwrap_or_else(|| {
        panic!("the unknown-scheme diagnostic has to name the set it knows: {unknown}")
    });
    let listed = tail.split_once(" help:").map_or(tail, |(list, _)| list);
    let schemes = listed
        .split(',')
        .map(str::trim)
        .filter(|scheme| !scheme.is_empty())
        .collect::<Vec<_>>();
    assert!(
        schemes.len() > 1,
        "the set was not parsed off the diagnostic, so nothing below is being \
         checked: listed={listed:?}"
    );
    for scheme in &schemes {
        assert!(
            scheme
                .chars()
                .all(|c| c.is_ascii_lowercase() || matches!(c, '-' | '_')),
            "`{scheme}` is not a scheme spelling, so the list was mis-parsed and this \
             lane is checking the wrong words: listed={listed:?}"
        );
    }

    // Step 2: which of them stand alone, asked of the binary rather than of the
    // enum or of a table written here. Anything the grammar does not refuse is a
    // reference an operator can type, whatever happens to it afterwards.
    let (stand_alone, take_a_value): (Vec<&str>, Vec<&str>) = schemes.iter().partition(|scheme| {
        let out = support::fiddle_command()
            .args([
                "inspect",
                scheme,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        !(out.status.code() == Some(2) && stderr.contains("fiddle::invocation_ref::malformed"))
    });
    assert!(
        !stand_alone.is_empty() && !take_a_value.is_empty(),
        "with one kind of scheme there is no partition to hold anything to, and \
         every assertion below passes for the wrong reason: stand_alone={stand_alone:?} \
         take_a_value={take_a_value:?}"
    );

    // Step 3: every surface that tells a caller how a reference is written.
    // `cvfoo` is here because it is the invocation that found this: one letter
    // from the scheme the milestone ships, and it lands in `Malformed` rather
    // than in the empty-value arm that had already learned this lesson.
    //
    // The flag says whether the surface *enumerates* the schemes, and it is
    // written here rather than detected from the text. Detecting it — selecting
    // the surfaces whose advice happens to contain the phrase the split looks
    // for — would mean a reworded help silently left the partition below instead
    // of failing it, and a check that opts itself out when the wording moves is
    // the failure mode this whole lane exists about.
    let surfaces = [
        (
            "fiddle inspect --help",
            text(&["inspect", "--help"], Some(0), false),
            false,
        ),
        (
            "fiddle run --help",
            text(&["run", "--help"], Some(0), false),
            false,
        ),
        (
            "the `bogus` diagnostic from inspect",
            refusal("bogus", "inspect"),
            true,
        ),
        (
            "the `bogus` diagnostic from run",
            refusal("bogus", "run"),
            true,
        ),
        (
            "the `cvfoo` diagnostic from inspect",
            refusal("cvfoo", "inspect"),
            true,
        ),
        (
            "the `cvfoo` diagnostic from run",
            refusal("cvfoo", "run"),
            true,
        ),
        (
            "the `notascheme:` diagnostic from inspect",
            refusal("notascheme:", "inspect"),
            true,
        ),
    ];

    // The template half of the loop below is vacuous on a part that offers no
    // template, and whether any part offers one is a fact about today's wording
    // rather than about the property — a help that dropped its placeholder would
    // silently take the check with it. So the detector is exercised on the two
    // strings the distinction is about: the wording this lane is downstream of,
    // and the concrete example that must not be mistaken for it.
    assert_eq!(
        shape_templates(
            "invocation reference must be <scheme>:<value>, got `x`",
            &schemes
        ),
        vec!["<scheme>:<value>"],
        "the detector no longer sees a shape template, so the check below holds \
         nothing: schemes={schemes:?}"
    );
    assert!(
        shape_templates(
            "take a value, as in `beans:fiddle-m0-demo`: beans, cve",
            &schemes
        )
        .is_empty(),
        "the detector reads one scheme's own example as a claim about every scheme, \
         so the check below reds on correct text: schemes={schemes:?}"
    );

    for (surface, rendered, _) in &surfaces {
        let advice = flatten(rendered);
        for scheme in &stand_alone {
            assert!(
                advice.contains(*scheme),
                "{surface} says how a reference is written and never mentions \
                 `{scheme}`, which is a complete reference on its own — so a caller \
                 who meant it is shown only shapes that require a value and reads \
                 their own invocation as illegal: {advice}"
            );
            let valued = valued_mentions(&advice, scheme);
            assert!(
                valued.is_empty(),
                "{surface} shows `{scheme}` carrying a value, and the bare form is \
                 what this build acts on: {valued:?}"
            );
        }

        // And no part of it denies the bare form on its own. A part that offers a
        // shape template offers it as *the* shape a reference has, so it has to
        // name the schemes that shape is false of, in the same breath and not in a
        // sentence beneath. Both `--help` surfaces do exactly that — they write
        // `<scheme>:<value>` and then name `cve` as standing alone — and so passing
        // this is not a demand to stop using placeholders. It is a demand that a
        // placeholder not be the last word to a caller whose reference has no
        // colon in it.
        for (part, read) in parts(rendered) {
            let templates = shape_templates(&read, &schemes);
            if templates.is_empty() {
                continue;
            }
            for scheme in &stand_alone {
                assert!(
                    read.contains(*scheme),
                    "{surface}: {part} gives {templates:?} as the shape a reference \
                     takes and never names `{scheme}`, which is a complete reference \
                     on its own — so this part, read as an operator reads it, denies \
                     the bare form. Whatever is written elsewhere in the same output \
                     does not travel with it: {read}"
                );
            }
        }
    }

    // Step 4: the two surfaces that offer the whole set are held to the whole
    // partition, so a scheme cannot be placed in the half its own behaviour
    // refuses. The `--help` surfaces are not held to this: they name the shapes
    // without enumerating which schemes take each, which is a legitimate thing for
    // a positional's help to do and not a claim that can be wrong about a scheme.
    for (surface, rendered, _) in surfaces.iter().filter(|(.., enumerates)| *enumerates) {
        let advice = flatten(rendered);
        let Some((valued_half, bare_half)) =
            advice.split_once("discover their own work need none:")
        else {
            panic!(
                "{surface} offers the set of schemes and has to separate the ones that \
                 require a value from the ones that do not, or it is offering one shape \
                 to a set that has two: {advice}"
            );
        };
        for scheme in &stand_alone {
            assert!(
                bare_half.contains(*scheme),
                "{surface} must offer `{scheme}` where no value is needed, because that \
                 is the invocation the binary accepts: {advice}"
            );
            assert!(
                !valued_half.contains(*scheme),
                "{surface} places `{scheme}` among the schemes that take a value, and \
                 the binary accepts it alone — advice that sends an operator who wanted \
                 a sweep to a read of one named item: {advice}"
            );
        }
        for scheme in &take_a_value {
            assert!(
                valued_half.contains(*scheme),
                "{surface} must offer `{scheme}` where a value is required, because the \
                 binary refuses it alone: {advice}"
            );
            assert!(
                !bare_half.contains(*scheme),
                "{surface} lists `{scheme}` among the schemes that need no value, and \
                 the binary refuses `{scheme}` written alone — advice refused when \
                 followed: {advice}"
            );
        }
    }
    assert!(
        surfaces.iter().any(|(.., enumerates)| *enumerates),
        "no surface was marked as enumerating the schemes, so the partition above \
         checked nothing"
    );
}
