mod support;

use fiddle_runtime::effect::Recurrence;
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
    for arm in ARMS {
        let out = scanner_with(support::wiz_stub(arm)).scan(&image()).await;
        assert!(
            arm_was_exercised(arm, &out),
            "arm {arm} is producible, but the scan came back {out:?}"
        );
        assert_eq!(
            observed_exit(arm),
            arm_exits_with(arm),
            "arm {arm} no longer ends on the status line that defines it"
        );
    }
}

#[tokio::test]
async fn a_scanner_that_is_not_installed_is_its_own_classification() {
    let out = scanner_with(absent_scanner()).scan(&image()).await;
    match out {
        Err(ScanError::Missing { program, .. }) => {
            assert_eq!(program, std::path::PathBuf::from(absent_scanner().program));
        }
        other => panic!("a scanner that is not on disk came back as {other:?}"),
    }
}

#[tokio::test]
async fn a_non_zero_exit_that_wrote_a_parseable_file_is_a_success() {
    let scanner = scanner_with(support::wiz_stub("exit-nonzero-with-file"));
    let report = scanner
        .scan(&image())
        .await
        .expect("a policy hit is not a failed scan");
    assert!(
        !report.findings().is_empty(),
        "the artefact this scan wrote was readable, so it is the scan's result"
    );

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
        (
            "no-daemon",
            ScanError::DaemonUnreachable {
                stderr: String::new(),
            },
        ),
    ];

    let mut variants = HashSet::new();
    let mut messages = BTreeSet::new();
    for (arm, want) in cases {
        let got = scanner_with(support::wiz_stub(arm))
            .scan(&image())
            .await
            .unwrap_err();
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
        5,
        "five causes, five distinguishable classifications"
    );
    assert_eq!(
        messages.len(),
        5,
        "five causes, five distinguishable reasons: {messages:?}"
    );
}

#[tokio::test]
async fn an_unreachable_docker_daemon_is_retryable_and_names_docker_host() {
    let daemon = scanner_with(support::wiz_stub("no-daemon"))
        .scan(&image())
        .await
        .unwrap_err();

    assert_eq!(
        daemon.recurrence(),
        Recurrence::Correctable,
        "the host comes back and the same invocation works: {daemon}"
    );

    assert!(
        daemon.to_string().contains("DOCKER_HOST"),
        "name the operator's remedy: {daemon}"
    );
    assert!(
        daemon
            .to_string()
            .contains("Cannot connect to the Docker daemon"),
        "this is not the diagnostic that arm wrote, so nothing above is about \
         an unreachable daemon: {daemon}"
    );

    let nothing = scanner_with(support::wiz_stub("empty-file"))
        .scan(&image())
        .await
        .unwrap_err();
    assert_ne!(
        discriminant(&daemon),
        discriminant(&nothing),
        "an unreachable daemon is reported as a scanner that wrote an empty \
         report: {daemon:?}"
    );
    assert_ne!(
        daemon.recurrence(),
        nothing.recurrence(),
        "a scanner that wrote an empty report writes the same nothing next time; \
         a daemon that is down does not, and the two must not share an exit row"
    );
    let broken = scanner_with(support::wiz_stub("exit-nonzero-no-file"))
        .scan(&image())
        .await
        .unwrap_err();
    assert_ne!(
        discriminant(&daemon),
        discriminant(&broken),
        "an unreachable daemon is reported as a scanner that ran and gave up, \
         which is the classification this arm exists to leave: {daemon:?}"
    );
}

#[tokio::test]
async fn the_scan_records_the_version_from_the_document_and_the_digest_from_the_output() {
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

    assert_eq!(
        report.document["extraInfo"]["clientVersion"].as_str(),
        Some(report.scanner_version.as_str()),
        "the scanner records its own version in the document it wrote, so the \
         version fiddle files against a scan is the version that produced it"
    );
    assert!(
        !report.document.to_string().contains(&report.image_digest),
        "the digest does not come from the document, so this assertion still \
         tells a scan that recorded what it inspected from one that read the \
         answer back out of the report"
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
    let failed = scanner_with(support::wiz_stub("leaks-its-credential"))
        .scan(&image())
        .await
        .unwrap_err();
    let diagnostic = failed.to_string();

    assert!(
        diagnostic.contains(FIXTURE_CLIENT_ID),
        "this is not the diagnostic that arm wrote, so nothing below is about \
         the credential channel: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(SENTINEL_SECRET),
        "the credential the scanner was given reached a diagnostic: {diagnostic}"
    );
    assert!(
        diagnostic.contains(REDACTED),
        "the credential was never in this diagnostic, so its absence proves \
         nothing: {diagnostic}"
    );
}
