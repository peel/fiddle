mod support;

use fiddle_runtime::effect::Recurrence;
use fiddle_runtime::scanner::ScanError;
use fiddle_runtime::Scanner as _;
use std::collections::{BTreeSet, HashSet};
use std::mem::discriminant;
use std::path::PathBuf;
use support::cve::{
    absent_scanner, arm_exits_with, arm_was_exercised, image, observed_exit, scanner_recording_env,
    scanner_with, ARMS, DIGEST_ON_STDOUT, FIXTURE_CLIENT_VERSION, FIXTURE_IMAGE_DIGEST,
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
async fn the_wizcli_environment_is_its_allowlist_and_fiddle_names_no_credential() {
    let observed = scanner_recording_env();
    observed
        .scan(&image())
        .await
        .expect("the recording arm is an ordinary successful scan");

    let names = observed.child_env_names();
    let allowed = ["HOME", "NO_COLOR", "PATH", "WIZ_CONFIG_DIR"];
    assert!(
        names.iter().all(|name| allowed.contains(&name.as_str())),
        "a name outside the allowlist is a change to the security boundary: {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "PATH") && names.iter().any(|name| name == "NO_COLOR"),
        "the record holds no name the adapter sets, so nothing below is about \
         the adapter's environment: {names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|name| name == "WIZ_CLIENT_ID" || name == "WIZ_CLIENT_SECRET"),
        "fiddle reads no scanner credential, so it exports none: {names:?}"
    );

    match std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        Some(home) => assert_eq!(
            observed.child_env().get("HOME").map(String::as_str),
            home.to_str(),
            "the caller's HOME did not reach the scanner, so wizcli cannot find \
             the login the caller left there"
        ),
        None => assert!(
            !names.iter().any(|name| name == "HOME"),
            "this process has no HOME, so the scanner must not receive one: {names:?}"
        ),
    }
}
