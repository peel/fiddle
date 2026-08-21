mod support;

use fiddle_runtime::effect::Recurrence;
use fiddle_runtime::scanner::{ScanError, REDACTED};
use fiddle_runtime::Scanner as _;
use std::collections::{BTreeSet, HashSet};
use std::mem::discriminant;
use std::path::PathBuf;
use support::cve::{
    absent_scanner, arm_exits_with, arm_was_exercised, image, observed_exit, scanner_of,
    scanner_reading_the_default_directory, scanner_recording_env, scanner_with, Caller, ARMS,
    DIGEST_ON_STDOUT, FIXTURE_CLIENT_ID, FIXTURE_CLIENT_VERSION, FIXTURE_IMAGE_DIGEST,
    FIXTURE_LOGIN_FILE, SENTINEL_SECRET,
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
async fn a_document_that_records_no_scanner_version_is_refused() {
    let recorded = scanner_with(support::wiz_stub("clean-image"))
        .scan(&image())
        .await
        .expect("a document that records its version is a scan that succeeded");
    assert_eq!(
        recorded.scanner_version, FIXTURE_CLIENT_VERSION,
        "the neighbouring arm writes the same document with the field present"
    );

    let refused = scanner_with(support::wiz_stub("no-client-version"))
        .scan(&image())
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, ScanError::Unparseable { .. }),
        "one field decides the two arms, and a document that cannot say what \
         produced it is one fiddle cannot account for: {refused:?}"
    );
    assert!(
        refused.to_string().contains("clientVersion"),
        "name the field the document does not carry: {refused}"
    );
    assert_eq!(
        refused.recurrence(),
        Recurrence::Permanent,
        "the same document records the same nothing on the next read"
    );

    let blank = scanner_with(support::wiz_stub("blank-client-version"))
        .scan(&image())
        .await
        .unwrap_err();
    assert!(
        matches!(&blank, ScanError::Unparseable { .. }),
        "a field that carries no version is a field that records none: {blank:?}"
    );
}

#[tokio::test]
async fn a_document_that_records_no_image_digest_is_refused() {
    let recorded = scanner_with(support::wiz_stub("clean-image"))
        .scan(&image())
        .await
        .expect("a document that records the image it read is a scan that succeeded");
    assert_eq!(
        recorded.image_digest, FIXTURE_IMAGE_DIGEST,
        "the neighbouring arm writes the same document with the field present"
    );

    let refused = scanner_with(support::wiz_stub("no-scan-origin"))
        .scan(&image())
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, ScanError::Unparseable { .. }),
        "one field decides the two arms, and a document that cannot say which \
         image it read is one fiddle cannot account for: {refused:?}"
    );
    assert!(
        refused.to_string().contains("scanOriginResource"),
        "name the field the document does not carry: {refused}"
    );
    assert_eq!(
        refused.recurrence(),
        Recurrence::Permanent,
        "the same document records the same nothing on the next read"
    );

    let blank = scanner_with(support::wiz_stub("blank-scan-origin"))
        .scan(&image())
        .await
        .unwrap_err();
    assert!(
        matches!(&blank, ScanError::Unparseable { .. }),
        "a field that carries no digest is a field that records none: {blank:?}"
    );
}

#[tokio::test]
async fn the_scan_records_the_version_and_the_digest_from_the_document() {
    let scanner = scanner_with(support::wiz_stub("ok"));
    let report = scanner.scan(&image()).await.unwrap();
    assert!(
        !report.scanner_version.is_empty(),
        "an unrecorded scanner is weak evidence: a clean report that cannot say \
         what produced it is not attributable to anything"
    );
    assert!(
        !report.image_digest.is_empty(),
        "the digest is what a bundle names the verdicts against: a tag is a \
         name somebody can move"
    );

    assert_eq!(
        report.document["extraInfo"]["clientVersion"].as_str(),
        Some(report.scanner_version.as_str()),
        "the scanner records its own version in the document it wrote, so the \
         version fiddle files against a scan is the version that produced it"
    );
    assert_eq!(
        report.document["scanOriginResource"]["id"].as_str(),
        Some(report.image_digest.as_str()),
        "the scanner records the image it resolved in the same document, so the \
         digest fiddle publishes is the digest of the image that produced the \
         findings"
    );
    assert_ne!(
        report.image_digest, DIGEST_ON_STDOUT,
        "the arm prints one digest on its output and records another in the \
         document; the recorded one is the one the findings belong to, and a \
         digest scraped from console prose can name a layer, a base image, or \
         nothing at all"
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
        ["NO_COLOR", "PATH", "WIZ_CONFIG_DIR"],
        "a fourth name here is a change to the security boundary"
    );

    assert!(
        observed.child_login_secret() == SENTINEL_SECRET,
        "the scanner read no credential, so its absence from argv and from the \
         environment below would prove nothing: {:?}",
        observed.child_login_secret()
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
        !observed
            .child_env()
            .values()
            .any(|value| value.contains(SENTINEL_SECRET)),
        "the credential is never a variable fiddle exports: {:?}",
        observed.child_env()
    );

    assert_eq!(
        observed.child_env().get("WIZ_CONFIG_DIR").cloned(),
        Some(observed.caller().to_string()),
        "the scanner was pointed somewhere other than the directory the caller \
         logged in to: {:?}",
        observed.child_env().get("WIZ_CONFIG_DIR")
    );
    assert!(
        !observed
            .child_env()
            .get("WIZ_CONFIG_DIR")
            .is_some_and(|directory| directory.starts_with(observed.scratch())),
        "the configuration source is fiddle's own scratch, which holds no login: {:?}",
        observed.child_env().get("WIZ_CONFIG_DIR")
    );
}

#[tokio::test]
async fn a_scan_without_the_callers_login_is_refused_and_names_the_cause() {
    let logged_in = Caller::new();
    logged_in.logs_in();
    let reached = scanner_of(support::wiz_stub("ok"), logged_in)
        .scan(&image())
        .await;
    assert!(
        reached.is_ok(),
        "the login is the only input the refusal below turns on, so a scan that \
         fails with it present proves nothing: {reached:?}"
    );

    let never_logged_in = Caller::new();
    let refused = scanner_of(support::wiz_stub("ok"), never_logged_in)
        .scan(&image())
        .await
        .unwrap_err();

    assert!(
        matches!(&refused, ScanError::Unauthenticated { .. }),
        "one login decides the two scans, and a scanner nobody logged in to \
         reads no image: {refused:?}"
    );
    assert!(
        refused.to_string().contains("wizcli auth"),
        "name the command that authenticates the scanner: {refused}"
    );
    assert!(
        refused.to_string().contains("caller"),
        "name who runs it: {refused}"
    );
    assert_eq!(
        refused.recurrence(),
        Recurrence::Permanent,
        "a retry runs the same scanner against the same absent login, so it \
         reads the same nothing: {refused}"
    );

    let banner = scanner_with(support::wiz_stub("exit-nonzero-no-file"))
        .scan(&image())
        .await
        .unwrap_err();
    assert_ne!(
        discriminant(&refused),
        discriminant(&banner),
        "an unauthenticated scanner is reported as a scanner that ran and gave \
         up, which is the diagnostic this refusal exists to replace: {refused:?}"
    );
    assert_ne!(
        refused.recurrence(),
        banner.recurrence(),
        "the run that found this exited retryable, and no retry writes a login"
    );
}

#[tokio::test]
async fn the_login_fiddle_reads_is_the_one_wizcli_reads_when_no_directory_is_named() {
    let caller = Caller::new();
    caller.logs_in_at_the_default_directory();
    let observed = scanner_reading_the_default_directory(support::wiz_stub("ok"), caller);
    observed
        .scan(&image())
        .await
        .expect("a caller who logged in at wizcli's own directory reached a scan");

    assert_eq!(
        observed.child_env_names(),
        ["HOME", "NO_COLOR", "PATH"],
        "fiddle names no configuration directory, so wizcli chooses its own"
    );
    assert_eq!(
        observed.child_login_secret(),
        SENTINEL_SECRET,
        "the scanner read no credential from the directory it chose"
    );

    let empty = Caller::new();
    std::fs::create_dir_all(empty.path().join(".wiz")).expect("an empty default directory");
    let refused = scanner_reading_the_default_directory(support::wiz_stub("ok"), empty)
        .scan(&image())
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, ScanError::Unauthenticated { .. }),
        "the directory exists and holds no {FIXTURE_LOGIN_FILE}: {refused:?}"
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
