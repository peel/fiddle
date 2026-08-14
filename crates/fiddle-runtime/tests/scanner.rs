//! The scanner port, gated against the scripted `wizcli`.
//!
//! Every test here drives a compiled fixture binary through the same seam an
//! operator would pin a real `wizcli` at, so what is asserted is a *subprocess
//! contract*: what the adapter does with an artefact, an exit code and a stream
//! of diagnostics it did not write. Nothing imports the stub as a library, and
//! nothing here reaches a network, a credential or a Wiz tenant — the gate is
//! offline by construction, and the scripted scanner is permanent rather than a
//! placeholder for one.

mod support;

// `Scanner` is imported for its method and not named again: the suite drives a
// scan through the port, which is the seam a capability will hold, rather than
// through whatever concrete adapter `scanner_with` happened to build.
use fiddle_runtime::scanner::{ScanError, REDACTED};
use fiddle_runtime::Scanner as _;
use std::collections::{BTreeSet, HashSet};
use std::mem::discriminant;
use std::path::PathBuf;
use support::cve::{
    absent_scanner, arm_exits_with, arm_was_exercised, image, observed_exit, scanner_recording_env,
    scanner_with, ARMS, FIXTURE_CLIENT_ID, SENTINEL_SECRET,
};

#[tokio::test]
async fn the_stub_can_produce_each_unsuccessful_arm() {
    // The stub is the gate's scanner. It must be able to produce every arm the
    // adapter has to discriminate, or the failure tests built on it cannot
    // exist: a suite whose fixture can only ever succeed proves that the
    // successful path works and says nothing at all about the other five.
    //
    // Iterated over `ARMS` rather than over a list written here, so the arms
    // this asserts about and the arms `arm_was_exercised` knows how to check
    // are one list. `ARMS` is a fixed-length array for the reason `all_shapes`
    // is: dropping an entry has to be a compile error rather than a quietly
    // shorter loop.
    for arm in ARMS {
        let out = scanner_with(support::wiz_stub(arm)).scan(&image()).await;
        assert!(
            arm_was_exercised(arm, &out),
            "arm {arm} is producible, but the scan came back {out:?}"
        );
        // The outcome above is not enough on its own. `ok` and
        // `exit-nonzero-with-file` are both a successful report, and that is
        // the adapter behaving correctly rather than an oversight — so an arm
        // that stopped exiting non-zero would satisfy every assertion above it
        // while no longer being the situation it is named for. The status is
        // the only thing that separates those two, and `empty-file` and
        // `unparseable-file` likewise mean nothing unless they end cleanly, so
        // it is pinned for all six. See `arm_exits_with` for each arm's.
        assert_eq!(
            observed_exit(arm),
            arm_exits_with(arm),
            "arm {arm} no longer ends on the status line that defines it"
        );
    }
}

#[tokio::test]
async fn a_scanner_that_is_not_installed_is_its_own_classification() {
    // The sixth `ScanError`, and the only one with no arm above it: the loop
    // covers what a running scanner can do, and this covers there not being one.
    // Left untested it would be the one variant whose remedy — install the
    // scanner, or fix the path it was pinned to — is reachable only through a
    // classification nothing has ever seen the adapter produce.
    let out = scanner_with(absent_scanner()).scan(&image()).await;
    // The program is asserted as well as the variant. `Missing` is reached from
    // a `NotFound` raised somewhere inside spawning, and a build that resolved
    // the seam against the wrong path — a working directory, an inherited
    // `PATH` — would raise exactly the same error about a different program and
    // pass a test that only matched the variant.
    match out {
        Err(ScanError::Missing { program, .. }) => {
            assert_eq!(program, std::path::PathBuf::from(absent_scanner().program));
        }
        other => panic!("a scanner that is not on disk came back as {other:?}"),
    }
}

#[tokio::test]
async fn a_non_zero_exit_that_wrote_a_parseable_file_is_a_success() {
    // `wizcli` exits non-zero when an organisation policy flags a finding, and
    // those findings need have nothing to do with the image this scan named.
    // Success is the artefact, not the status. This is the inverse of the usual
    // rule, which is exactly why it is asserted first: an adapter that read the
    // status line would report a perfectly good report as a failed scan, and the
    // honest handling of a failed scan is to stop — so one unrelated policy hit
    // anywhere in a tenant would silently switch this capability off.
    let scanner = scanner_with(support::wiz_stub("exit-nonzero-with-file"));
    let report = scanner
        .scan(&image())
        .await
        .expect("a policy hit is not a failed scan");
    assert!(
        !report.findings().is_empty(),
        "the artefact this scan wrote was readable, so it is the scan's result"
    );

    // The flag that keeps the *unrelated* half of the story from growing. It is
    // asserted here rather than only commented in the adapter, because it is the
    // difference between an exit code this build tolerates and one it has asked
    // the scanner not to produce. See the adapter for why it is load-bearing.
    assert!(
        scanner
            .child_argv()
            .iter()
            .any(|argument| argument == "--by-policy-hits=DISABLED"),
        "the scanner was not asked to stop failing on policy hits: {:?}",
        scanner.child_argv()
    );
}

#[tokio::test]
async fn every_unsuccessful_arm_is_its_own_error() {
    // Four situations in the world, four classifications, and no two of them the
    // same. The property is *pairwise* — that a caller can tell a broken scanner
    // from a mistyped tag — and a per-arm `matches!` cannot state it: four
    // assertions that each accept one variant would all pass if the adapter had
    // collapsed three of the four into a fourth's neighbour, because each
    // assertion only ever looks at its own arm.
    let cases = [
        (
            "exit-nonzero-no-file",
            ScanError::Failed {
                status: String::new(),
                stderr: String::new(),
            },
        ),
        (
            "empty-file",
            ScanError::NoOutput {
                path: PathBuf::new(),
            },
        ),
        (
            "unparseable-file",
            ScanError::Unparseable {
                path: PathBuf::new(),
                reason: String::new(),
            },
        ),
        (
            "no-such-image",
            ScanError::ImageAbsent {
                image: String::new(),
                stderr: String::new(),
            },
        ),
    ];

    // Two sets, and only the first of them pins the property. `variants` holds
    // discriminants, so an entry that fails to insert is genuinely a second arm
    // reaching a variant some earlier arm already reached. `messages` holds what
    // a reader is actually told, and it is the weaker of the two on purpose: two
    // `Failed`s carrying different stderr are two distinct strings, so a count
    // over messages alone would be satisfied by an adapter that discriminated
    // nothing. Both are counted, and the count is printed rather than implied.
    let mut variants = HashSet::new();
    let mut messages = BTreeSet::new();
    for (arm, want) in cases {
        let got = scanner_with(support::wiz_stub(arm))
            .scan(&image())
            .await
            .unwrap_err();
        // Distinctness before identity, so that a collapse is reported as the
        // thing it is. With the order the other way round a reader is told an
        // arm reached the wrong variant, which is true and is the smaller half
        // of it: what has actually happened is that two situations in the world
        // became one classification.
        assert!(
            variants.insert(discriminant(&got)),
            "arm {arm} reaches a classification another arm already reached: {got:?}"
        );
        assert!(
            messages.insert(got.to_string()),
            "arm {arm} tells a reader what some other arm already told them: {got}"
        );
        assert_eq!(
            discriminant(&got),
            discriminant(&want),
            "arm {arm} came back as {got:?}"
        );
    }
    assert_eq!(
        variants.len(),
        4,
        "four causes, four distinguishable classifications"
    );
    assert_eq!(
        messages.len(),
        4,
        "four causes, four distinguishable reasons: {messages:?}"
    );
}

#[tokio::test]
async fn the_scan_records_what_it_scanned_before_parsing_anything() {
    let scanner = scanner_with(support::wiz_stub("ok"));
    let report = scanner.scan(&image()).await.unwrap();
    assert!(
        !report.scanner_version.is_empty(),
        "an unrecorded scanner is weak evidence: a clean report that cannot say \
         what produced it is not attributable to anything"
    );
    assert!(
        !report.image_digest.is_empty(),
        "the digest is what makes a rescan comparable: a tag is a name somebody \
         can move"
    );

    // What makes the *ordering* observable rather than merely written down. Both
    // values are taken from what the child announced, before the artefact is
    // opened — so neither of them can have come out of the document, and this
    // asserts that they could not have: the document contains neither string.
    // An adapter that read its provenance out of the parsed report would fail
    // here, and would also have nothing to record on an arm that wrote a
    // document it could not parse.
    let document = report.document.to_string();
    assert!(
        !document.contains(&report.scanner_version),
        "the version is in the document, so this test cannot tell a scan that \
         recorded its provenance from one that read it back out of the report"
    );
    assert!(
        !document.contains(&report.image_digest),
        "the digest is in the document; see above"
    );
}

#[tokio::test]
async fn the_wizcli_environment_is_exactly_its_allowlist_and_no_credential_reaches_argv() {
    let observed = scanner_recording_env();
    observed
        .scan(&image())
        .await
        .expect("the recording arm is an ordinary successful scan");

    let names = observed.child_env_names();
    assert_eq!(
        names,
        [
            "NO_COLOR",
            "PATH",
            "WIZ_CLIENT_ID",
            "WIZ_CLIENT_SECRET",
            "WIZ_CONFIG_DIR"
        ],
        "a sixth name here is a change to the security boundary"
    );

    // The credential IS in that set, and that is the point: it travels by
    // environment precisely so that it never reaches argv, because
    // /proc/<pid>/cmdline is world-readable on Linux and a secret in an argument
    // is a secret every user on the box can read for as long as the process
    // lives. Asserted against what the child *received*, not against what the
    // adapter meant to set — a `Command` this suite never spawned would prove
    // only that the builder was called.
    assert_eq!(
        observed.child_env().get("WIZ_CLIENT_SECRET").cloned(),
        Some(SENTINEL_SECRET.to_string()),
        "the credential did not arrive by the channel it is supposed to travel \
         on, so its absence from argv below would prove nothing"
    );
    assert!(
        !observed
            .child_argv()
            .iter()
            .any(|argument| argument.contains(SENTINEL_SECRET)),
        "the credential is never an argument: {:?}",
        observed.child_argv()
    );

    // `HOME` is absent for the reason it is absent from the `gh` environment:
    // with `HOME` gone and `WIZ_CONFIG_DIR` pointed at a scratch directory this
    // adapter owns, the child cannot reach an operator's ambient credential — so
    // "it used the credential it was given and no other" is a fact about the
    // process rather than a claim about it.
    assert!(
        !names.iter().any(|name| name == "HOME"),
        "no ambient configuration source: {names:?}"
    );
    assert!(
        observed
            .child_env()
            .get("WIZ_CONFIG_DIR")
            .is_some_and(|directory| directory.starts_with(observed.scratch())),
        "the configuration source is not pinned to this scan's scratch: {:?}",
        observed.child_env().get("WIZ_CONFIG_DIR")
    );
}

#[tokio::test]
async fn no_diagnostic_quotes_the_credential_the_scanner_was_given() {
    // A scanner that quotes its own configuration back at you is not a strange
    // thing to meet — plenty of tools print what they authenticated with when
    // authentication fails. The adapter passes a child's diagnostics through
    // into `ScanError`, so that stream is a real path out of this process for
    // the one value that must never take it.
    let failed = scanner_with(support::wiz_stub("leaks-its-credential"))
        .scan(&image())
        .await
        .unwrap_err();
    let diagnostic = failed.to_string();

    // Three assertions, and the first two are what make the third mean anything.
    // A sentinel is only evidence if something planted it: this arm really does
    // print the secret, and the diagnostic really is the one it printed.
    assert!(
        diagnostic.contains(FIXTURE_CLIENT_ID),
        "this is not the diagnostic that arm wrote, so nothing below is about \
         the credential channel: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(SENTINEL_SECRET),
        "the credential the scanner was given reached a diagnostic: {diagnostic}"
    );
    // Last, and it is the assertion that keeps the one above from being empty:
    // a marker is here only because a secret was taken out, so a build that
    // stopped planting one — or an arm that stopped printing it — fails here
    // rather than passing for the want of anything to redact.
    assert!(
        diagnostic.contains(REDACTED),
        "the credential was never in this diagnostic, so its absence proves \
         nothing: {diagnostic}"
    );
}
