mod fixture;

use fiddle_core::AttemptId;
use fiddle_runtime::evaluate::{evaluate, Check, Contract, InWorkspace, Outcome, Rescan, Success};
use fiddle_runtime::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const AMPLE: Duration = Duration::from_secs(60);

const IMAGE: &str = "ghcr.io/acme/widget:fiddle-fixture";

const CHILD_RECORD: &str = "child.json";

struct World {
    workspace: Workspace,
    dir: tempfile::TempDir,
}

impl World {
    fn new(cancel: CancellationToken) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let repo = fixture::trivial_repo(dir.path());
        let attempt = AttemptId("01JQZX0000000000000000000".to_string());
        let workspace = Workspace::create(&repo, &dir.path().join("ws"), &attempt, cancel)
            .expect("a worktree of the fixture");
        std::fs::create_dir_all(dir.path().join("records")).expect("somewhere to keep records");
        std::fs::create_dir_all(dir.path().join("scan")).expect("a scratch for the rescan");
        World { workspace, dir }
    }

    fn tree(&self, timeout: Duration) -> InWorkspace<'_> {
        InWorkspace::new(
            &self.workspace,
            timeout,
            Rescan {
                scratch: self.dir.path().join("scan"),
                image: IMAGE.to_string(),
            },
        )
    }

    fn scripted(&self, program: &Path, label: &str, extra: &[&str], success: Success) -> Check {
        let mut args = vec![
            "--record".to_string(),
            self.record_path(label).display().to_string(),
        ];
        args.extend(extra.iter().map(|arg| (*arg).to_string()));
        Check {
            program: program.display().to_string(),
            args,
            success,
        }
    }

    fn rescan(&self, program: &Path, arm: &str) -> Check {
        Check {
            program: program.display().to_string(),
            args: vec![arm.to_string()],
            success: Success::ArtefactWritten,
        }
    }

    fn record_path(&self, label: &str) -> PathBuf {
        self.dir.path().join("records").join(label)
    }

    fn record(&self, label: &str) -> serde_json::Value {
        let path = self.record_path(label);
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|source| {
            panic!(
                "no child recorded itself as {label} at {}: {source}",
                path.display()
            )
        });
        serde_json::from_str(&raw).expect("the scripted check writes a JSON record")
    }

    fn scanner_record(&self) -> serde_json::Value {
        let path = self.dir.path().join("scan").join(CHILD_RECORD);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|source| panic!("no scanner recorded itself: {source}"));
        serde_json::from_str(&raw).expect("the scripted scanner writes a JSON record")
    }

    fn a_scanner_ran(&self) -> bool {
        self.dir.path().join("scan").join(CHILD_RECORD).exists()
    }

    fn copy_of(&self, program: &str, at: &str) -> PathBuf {
        let path = self.dir.path().join(at);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory to copy to");
        std::fs::copy(program, &path).expect("a copy of the fixture program");
        path
    }
}

fn stub() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_check_stub"))
}

fn scanner() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_wiz_stub"))
}

fn argv(record: &serde_json::Value) -> Vec<String> {
    record["argv"]
        .as_array()
        .expect("a record carries an argv array")
        .iter()
        .map(|arg| arg.as_str().expect("an argument is a string").to_string())
        .collect()
}

fn declared(check: &Check) -> Vec<String> {
    std::iter::once(check.program.clone())
        .chain(check.args.iter().cloned())
        .collect()
}

#[tokio::test]
async fn each_check_becomes_its_own_child_started_from_its_own_declaration() {
    let world = World::new(CancellationToken::new());
    let wrapper = world.copy_of(&stub().display().to_string(), "opt/acme/bin/tidy-sources");
    let contract = Contract::of(vec![
        world.scripted(&stub(), "build", &[], Success::ExitZero),
        world.scripted(
            &wrapper,
            "fmt",
            &["--say", "main.go"],
            Success::ExitZeroAndNoOutput,
        ),
        world.scripted(&stub(), "vet", &[], Success::ExitZero),
        world.scripted(&stub(), "docker", &[], Success::ExitZero),
        world.rescan(&scanner(), "ok"),
    ]);

    let r = evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect("an evaluation that was not cancelled");

    assert_eq!(
        r.checks()
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>(),
        contract.checks.iter().map(Check::name).collect::<Vec<_>>()
    );
    for (label, check) in ["build", "fmt", "vet", "docker"]
        .iter()
        .zip(&contract.checks)
    {
        assert_eq!(
            argv(&world.record(label)),
            declared(check),
            "the child started for {label} was not the one its declaration named"
        );
    }
    assert!(!wrapper.display().to_string().contains("fmt"));

    let failed: Vec<&str> = r
        .checks()
        .iter()
        .filter(|c| !c.passed)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        failed,
        vec![contract.checks[1].name()],
        "only the formatter fails, and it fails on its output rather than its status"
    );
    assert!(
        matches!(&r.checks()[4].outcome, Outcome::Scanned(report)
            if !report.scanner_version.is_empty()),
        "the rescan carries the report a real scanner wrote"
    );
}

#[tokio::test]
async fn a_check_runs_inside_the_tree_under_judgement() {
    let world = World::new(CancellationToken::new());
    let contract = Contract::of(vec![world.scripted(
        &stub(),
        "where",
        &[],
        Success::ExitZero,
    )]);

    evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect("an evaluation that was not cancelled");

    let record = world.record("where");
    let cwd = PathBuf::from(record["cwd"].as_str().expect("a recorded directory"));
    assert_eq!(
        cwd.canonicalize().expect("the child's directory"),
        world
            .workspace
            .root()
            .canonicalize()
            .expect("the worktree's directory")
    );
    let entries: Vec<String> = record["entries"]
        .as_array()
        .expect("a record lists what the child could see")
        .iter()
        .map(|entry| entry.as_str().expect("an entry is a string").to_string())
        .collect();
    assert!(
        entries.contains(&"src".to_string()) && entries.contains(&".gitignore".to_string()),
        "the child saw the fixture tree, not somewhere else: {entries:?}"
    );

    let mut names: Vec<String> = record["env"]
        .as_array()
        .expect("a record carries the environment its child received")
        .iter()
        .map(|entry| {
            let entry = entry.as_str().expect("an environment entry is a string");
            entry
                .split_once('=')
                .unwrap_or_else(|| panic!("{entry} is not a NAME=VALUE entry"))
                .0
                .to_string()
        })
        .collect();
    names.sort();
    let allowed = ["HOME", "LANG", "PATH", "RUSTUP_HOME"];
    assert!(
        names.iter().all(|name| allowed.contains(&name.as_str())),
        "a check child saw a name outside the workspace allowlist: {names:?}"
    );
}

#[tokio::test]
async fn the_rescan_starts_the_program_the_check_declared() {
    let world = World::new(CancellationToken::new());
    let wrapped = world.copy_of(&scanner().display().to_string(), "opt/acme/bin/scan-images");
    let contract = Contract::of(vec![world.rescan(&wrapped, "ok")]);

    let r = evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(
        !r.rejected(),
        "the scripted scanner's `ok` arm writes a report"
    );
    let argv = argv(&world.scanner_record());
    assert_eq!(
        argv.first().map(String::as_str),
        Some(wrapped.display().to_string().as_str()),
        "the scanner started was not the one the check declared: {argv:?}"
    );
    assert_eq!(
        argv.get(1).map(String::as_str),
        Some("ok"),
        "the check's own arguments reach the scanner ahead of the adapter's: {argv:?}"
    );
    assert!(argv.iter().any(|arg| arg == "--json-output-file"));
    assert!(argv.last().map(String::as_str) == Some(IMAGE));
}

#[tokio::test]
async fn a_check_that_is_not_installed_is_not_run_rather_than_failed() {
    let world = World::new(CancellationToken::new());
    let absent = PathBuf::from(format!("{}-which-is-not-installed", stub().display()));
    assert!(!absent.exists(), "{} exists", absent.display());
    let contract = Contract::of(vec![
        Check {
            program: absent.display().to_string(),
            args: Vec::new(),
            success: Success::ExitZero,
        },
        world.scripted(&stub(), "after", &[], Success::ExitZero),
    ]);

    let r = evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(r.rejected(), "an unanswered check is not an answered one");
    assert!(
        matches!(&r.checks()[0].outcome, Outcome::NotRun(why) if why.contains("no such program")),
        "recorded as unanswered and saying why: {:?}",
        r.checks()[0].outcome
    );
    assert!(r.checks()[1].passed, "the check after it still ran");
    world.record("after");
}

#[tokio::test]
async fn a_scanner_that_is_not_installed_is_not_run_rather_than_failed() {
    let world = World::new(CancellationToken::new());
    let absent = PathBuf::from(format!("{}-which-is-not-installed", scanner().display()));
    assert!(!absent.exists(), "{} exists", absent.display());
    let contract = Contract::of(vec![world.rescan(&absent, "ok")]);

    let r = evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(r.rejected());
    assert!(
        matches!(&r.checks()[0].outcome, Outcome::NotRun(why) if why.contains("could not be started")),
        "an absent scanner is the operator's machine, not the tree's verdict: {:?}",
        r.checks()[0].outcome
    );
}

#[tokio::test]
async fn a_check_that_overruns_its_deadline_is_unanswered() {
    let world = World::new(CancellationToken::new());
    let contract = Contract::of(vec![
        world.scripted(&stub(), "hangs", &["--hang", "yes"], Success::ExitZero),
        world.scripted(&stub(), "after-the-deadline", &[], Success::ExitZero),
    ]);

    let r = evaluate(&contract, &world.tree(Duration::from_secs(1)))
        .await
        .expect("an evaluation that was not cancelled");

    world.record("hangs");
    assert!(
        matches!(&r.checks()[0].outcome, Outcome::NotRun(why) if why.contains("did not finish")),
        "a killed check is unanswered, not failed: {:?}",
        r.checks()[0].outcome
    );
    assert!(!r.checks()[0].passed);
    assert!(
        r.checks()[1].passed,
        "the contract is not shortened by a deadline either"
    );
}

#[tokio::test]
async fn a_cancelled_attempt_is_not_a_rejected_tree() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let world = World::new(cancel);
    let contract = Contract::of(vec![world.scripted(
        &stub(),
        "never",
        &[],
        Success::ExitZero,
    )]);

    evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect_err("a cancelled attempt yields no evaluation");

    assert!(
        !world.record_path("never").exists(),
        "a cancelled attempt must not start a check"
    );
}

#[tokio::test]
async fn a_cancelled_rescan_starts_no_scanner_but_cannot_say_cancelled() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let world = World::new(cancel);
    let contract = Contract::of(vec![world.rescan(&scanner(), "ok")]);

    let r = evaluate(&contract, &world.tree(AMPLE))
        .await
        .expect("the gap: a cancelled rescan yields an evaluation rather than a refusal");

    assert!(r.rejected());
    assert!(
        matches!(&r.checks()[0].outcome, Outcome::NoArtefact(why) if why.contains("cancelled")),
        "the reason survives even though the kind does not: {:?}",
        r.checks()[0].outcome
    );
    assert!(
        !world.a_scanner_ran(),
        "a cancelled attempt must not start a scanner"
    );
}
