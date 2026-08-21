use fiddle_runtime::github::{observe_genuine_failure, GenuineFailure};
use fiddle_runtime::GhCli;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const EARLIER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

const PATIENT: Duration = Duration::from_secs(60);

struct Forge {
    dir: TempDir,
}

impl Forge {
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        Self { dir }
    }

    fn check(&self, name: &str, status: &str, conclusion: Option<&str>, head_sha: &str) {
        let path = self.dir.path().join("checks_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({
            "name": name,
            "status": status,
            "conclusion": conclusion,
            "head_sha": head_sha,
            "details_url": details_url(name),
        }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    fn answers_runs_for_every_sha(&self) {
        std::fs::write(self.dir.path().join("checks_unfiltered"), "yes").unwrap();
    }

    fn gh(&self) -> GhCli {
        GhCli::new(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            vec![
                "--stub-dir".to_string(),
                self.dir.path().display().to_string(),
            ],
            "ghp_never_reaches_a_network".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            self.dir.path().join("config"),
            PATIENT,
        )
    }
}

fn details_url(name: &str) -> String {
    format!("https://github.com/{REPO}/runs/{name}")
}

async fn ask(forge: &Forge, head_sha: &str) -> Option<GenuineFailure> {
    let cancel = CancellationToken::new();
    observe_genuine_failure(&forge.gh(), REPO, head_sha, &cancel)
        .await
        .value()
        .expect("the check runs were read")
        .clone()
}

fn blamed_names(genuine: &GenuineFailure) -> Vec<&str> {
    genuine
        .blamed
        .iter()
        .map(|check| check.name.as_str())
        .collect()
}

#[tokio::test]
async fn a_check_that_failed_against_the_head_sha_is_a_genuine_failure() {
    let forge = Forge::empty();
    forge.check("cve-verify", "completed", Some("failure"), HEAD);

    let genuine = ask(&forge, HEAD).await.expect("the head sha failed");

    assert_eq!(genuine.head_sha, HEAD);
    assert_eq!(blamed_names(&genuine), ["cve-verify"]);
    assert_eq!(
        genuine.blamed[0].details_url,
        Some(details_url("cve-verify")),
        "the attempt needs to know where the check said so"
    );
}

#[tokio::test]
async fn a_failure_against_an_earlier_sha_says_nothing_about_the_head() {
    let forge = Forge::empty();
    forge.check("cve-verify", "completed", Some("failure"), EARLIER);
    forge.answers_runs_for_every_sha();

    assert_eq!(
        ask(&forge, HEAD).await,
        None,
        "the fix moved on to {HEAD}, and the failure tested {EARLIER}"
    );

    let earlier = ask(&forge, EARLIER)
        .await
        .expect("the same forge really does hold a failure at the earlier sha");
    assert_eq!(blamed_names(&earlier), ["cve-verify"]);
}

#[tokio::test]
async fn a_pending_check_has_not_answered_yet_and_blames_nothing() {
    let forge = Forge::empty();
    forge.check("cve-verify", "queued", None, HEAD);
    forge.check("cve-rescan", "in_progress", None, HEAD);

    assert_eq!(ask(&forge, HEAD).await, None);
}

#[tokio::test]
async fn a_cancelled_skipped_timed_out_or_neutral_check_reached_no_verdict_about_the_fix() {
    for conclusion in ["cancelled", "skipped", "timed_out", "neutral"] {
        let forge = Forge::empty();
        forge.check("cve-verify", "completed", Some(conclusion), HEAD);

        assert_eq!(
            ask(&forge, HEAD).await,
            None,
            "{conclusion} does not say the fix is wrong"
        );
    }
}

#[tokio::test]
async fn a_green_head_leaves_nothing_to_attempt() {
    let forge = Forge::empty();
    forge.check("cve-verify", "completed", Some("success"), HEAD);
    forge.check("cve-rescan", "completed", Some("success"), HEAD);

    assert_eq!(ask(&forge, HEAD).await, None);
}

#[tokio::test]
async fn one_failure_among_green_checks_is_still_genuine() {
    let forge = Forge::empty();
    forge.check("cve-verify", "completed", Some("success"), HEAD);
    forge.check("cve-rescan", "completed", Some("failure"), HEAD);

    let genuine = ask(&forge, HEAD).await.expect("one check failed");

    assert_eq!(blamed_names(&genuine), ["cve-rescan"]);
}
