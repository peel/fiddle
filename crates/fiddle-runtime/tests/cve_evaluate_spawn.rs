//! The same contract as `cve_evaluate`, over real children in a real worktree.
//!
//! `cve_evaluate` drives a scripted [`Tree`]: it can build a world where
//! `docker build` fails and `go vet` passes, which is not a world anybody can
//! build offline, and it can ask that world afterwards what it was told to
//! start. What it cannot do is show that *starting* a check is anything at all.
//! Every claim there about five separate commands is a claim about a recorder
//! written in this repository to agree with the runner in this repository, and
//! a port with no production implementation would let the whole of it stay true
//! while nothing in the product ever spawned.
//!
//! So this file is the other half. The subject is
//! [`InWorkspace`] — the production [`Tree`] — over a
//! [`Workspace`] branched from a throwaway git repository, and every check in it
//! becomes a process the operating system really ran. What is asserted is what
//! those processes *were*: their `argv`, their working directory, and the
//! criterion each of them was judged by afterwards.
//!
//! # What stands in for the five programs, and why anything has to
//!
//! Design §2.6's checks are `go build`, `go fmt`, `go vet`, `docker build` and a
//! `wizcli` rescan. This project's dev shell declares
//! `[rustToolchain, alejandra, gh, jq]`: there is no Go toolchain here, no
//! container daemon and no scanner, and the gate is offline. A suite that
//! required any of them would be a suite that never ran.
//!
//! It does not need them. What the criterion says is that each check runs *as
//! its own command*, judged by *its own declaration* — and neither half is about
//! Go. `check_stub` is a program that can be asked to exit zero silently, to
//! exit zero while naming a file, to exit non-zero, or to hang until it is
//! killed; those are the four shapes the contract distinguishes, and it writes
//! down what it was started with so the shape can be checked against the
//! declaration that produced it. The rescan is the scripted `wizcli` the rest of
//! this crate's scanner suites already drive, reached the way an operator would
//! reach a wrapper: through the check's own `program`.
//!
//! # The one lane that matters most
//!
//! [`the_rescan_starts_the_program_the_check_declared`]. A [`Wizcli`] needs a
//! scratch directory, a credential, a deadline and a token that no [`Check`]
//! carries, so the obvious adapter builds one scanner and holds it — and then
//! runs *its* scanner for every artefact check, whatever the check said. That
//! adapter passes every other lane here. This one points the rescan at a copy of
//! the scanner under a path of the test's choosing and reads the child's own
//! `argv` back, so an adapter that ignored the declaration is caught by the
//! operating system rather than by an intention.
//!
//! [`Tree`]: fiddle_runtime::evaluate::Tree
//! [`Wizcli`]: fiddle_runtime::Wizcli

mod fixture;

use fiddle_core::AttemptId;
use fiddle_runtime::evaluate::{evaluate, Check, Contract, InWorkspace, Outcome, Rescan, Success};
use fiddle_runtime::scanner::WizCredential;
use fiddle_runtime::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Longer than any lane here needs, so a failure is a failure of the lane
/// rather than of a loaded machine. [`a_check_that_overruns_its_deadline_is_unanswered`]
/// is the one lane that sets its own.
const AMPLE: Duration = Duration::from_secs(60);

/// The image every rescan here is pointed at. Not an image anybody can pull,
/// which is the point: nothing offline resolves it and the scripted scanner is
/// the only thing that ever answers for it.
const IMAGE: &str = "ghcr.io/acme/widget:fiddle-fixture";

/// The tenant identifier the rescans authenticate as.
const CLIENT_ID: &str = "fiddle-client-1c93f0a5";

/// What the scripted scanner writes its own record into, beside its report.
const CHILD_RECORD: &str = "child.json";

/// A worktree to judge, the scratch a rescan writes into, and the programs the
/// checks are pointed at.
///
/// The field order is load bearing: [`Workspace`] removes its worktree on
/// [`Drop`], and the [`tempfile::TempDir`] below it owns the repository that
/// worktree was branched from. Declared the other way round, the repository
/// would be gone before the teardown that needs it.
struct World {
    workspace: Workspace,
    dir: tempfile::TempDir,
}

impl World {
    /// A world over [`fixture::trivial_repo`], with the given token.
    ///
    /// `trivial_repo` and not `broken_crate`, and the difference matters here in
    /// the opposite direction to everywhere else it is discussed: nothing in
    /// this file runs a build, because the subject is *that a check became a
    /// child*, not what a compiler thought of a tree. A cargo package would add
    /// a minute of compilation to every lane and change no assertion.
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

    /// The adapter under test, with `timeout` on every check.
    fn tree(&self, timeout: Duration) -> InWorkspace<'_> {
        InWorkspace::new(
            &self.workspace,
            timeout,
            Rescan {
                scratch: self.dir.path().join("scan"),
                credential: WizCredential {
                    client_id: CLIENT_ID.to_string(),
                    client_secret: "fiddle-secret-3b8e51d0".to_string(),
                },
                image: IMAGE.to_string(),
            },
        )
    }

    /// A check that runs `program`, recording what it was started with under
    /// `label`, and declaring `success`.
    ///
    /// The record path is absolute and outside the worktree on purpose: a check
    /// runs *in* the tree under judgement, so a record written relative to it
    /// would be a file the attempt's own changed-file derivation then reports as
    /// work somebody did.
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

    /// The rescan, run through `program` on the scripted scanner's `arm` arm.
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

    /// What the child that ran under `label` wrote down about itself.
    ///
    /// Panics when there is none, and that is the assertion rather than a
    /// convenience: a check the adapter never started leaves no record, so an
    /// adapter that ran something of its own choosing fails here by name.
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

    /// What the scripted scanner wrote down about itself, beside its report.
    fn scanner_record(&self) -> serde_json::Value {
        let path = self.dir.path().join("scan").join(CHILD_RECORD);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|source| panic!("no scanner recorded itself: {source}"));
        serde_json::from_str(&raw).expect("the scripted scanner writes a JSON record")
    }

    /// Whether any scanner started at all.
    fn a_scanner_ran(&self) -> bool {
        self.dir.path().join("scan").join(CHILD_RECORD).exists()
    }

    /// A copy of `program` at `at`, under this world's temporary directory.
    ///
    /// How an operator pins or wraps a tool, and the only way a lane can put two
    /// *different* program paths in front of one fixture: an adapter that ran a
    /// program of its own choosing would still produce the right `argv` if every
    /// check named the same binary.
    fn copy_of(&self, program: &str, at: &str) -> PathBuf {
        let path = self.dir.path().join(at);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory to copy to");
        std::fs::copy(program, &path).expect("a copy of the fixture program");
        path
    }
}

/// The scripted check, as cargo promises to name it.
fn stub() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_check_stub"))
}

/// The scripted scanner, the same one every other scanner suite here drives.
fn scanner() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_wiz_stub"))
}

/// The `argv` a recorded child was started with.
fn argv(record: &serde_json::Value) -> Vec<String> {
    record["argv"]
        .as_array()
        .expect("a record carries an argv array")
        .iter()
        .map(|arg| arg.as_str().expect("an argument is a string").to_string())
        .collect()
}

/// What a check declared, as `argv` would spell it.
fn declared(check: &Check) -> Vec<String> {
    std::iter::once(check.program.clone())
        .chain(check.args.iter().cloned())
        .collect()
}

/// Five real children, each started from its own declaration.
///
/// The list of results is the weak half — five results could be five copies of
/// one status, which is why `cve_evaluate` asks its recorder — and here the
/// strong half is on disk: four records, written by four processes, each
/// carrying the `argv` the operating system saw. A runner chaining the commands
/// with `&&` leaves two of them absent; a runner that built its own command line
/// leaves four that do not match their declarations.
///
/// The formatter is the one that fails, and it fails through a wrapper whose
/// path says neither `go` nor `fmt` — so the criterion cannot have come from the
/// program name, over a program that really ran.
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

    // In declared order, and all five: the two after the failure ran too.
    assert_eq!(
        r.checks()
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>(),
        contract.checks.iter().map(Check::name).collect::<Vec<_>>()
    );
    // Four processes, because a record is written once per invocation and there
    // are four of them. This is the assertion the result list cannot make.
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
    // And the wrapper really is a path with nothing to recognise in it, so the
    // verdict below cannot have been derived from a program name.
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

/// A check runs *in the tree under judgement*, which is what makes an evaluation
/// an evaluation of anything.
///
/// The record carries the child's own working directory and what it could see
/// from there, so a runner that spawned in the runtime's own directory — where
/// `go build ./...` would compile this repository — is caught by both.
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
    // Canonical on both sides: a macOS temporary directory lives under `/var`,
    // which is a symlink to `/private/var`, so the string the workspace was
    // created with is not the string a child resolves its own directory to.
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

    // And it saw nothing else. `workspace::a_workspace_command_inherits_no_credential`
    // is where the allowlist is *pinned* — both shapes of it, exactly, against a
    // planted credential — and this deliberately does not restate that: it
    // asserts the weaker containment, from the evaluation path, so that an
    // adapter which reached around `Workspace::run` to build a spawn of its own
    // is caught here rather than only by a test of a function it stopped
    // calling.
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

/// The rescan runs the program *its check* named.
///
/// The lane the whole adapter is at risk in. A scanner needs a scratch
/// directory, a credential, a deadline and a cancellation token, none of which a
/// [`Check`] carries — so the adapter that suggests itself holds one scanner,
/// built once, and runs it for every artefact check. That adapter passes every
/// other lane in this file: the report is written, the criterion is met, the
/// verdict is right. What it has quietly done is disconnect the operator seam,
/// so an operator who pinned or wrapped their scanner would find the wrapper
/// ignored and never be told.
///
/// So the scanner here is a *copy*, at a path this test chose, and the assertion
/// is the child's own `argv`. The arm arrives through `check.args` in the same
/// breath, which is the other half of the same seam.
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
    // The adapter's own flags are still there, after the check's — this lane is
    // about where the program came from, not about replacing what `Wizcli` adds.
    assert!(argv.iter().any(|arg| arg == "--json-output-file"));
    assert!(argv.last().map(String::as_str) == Some(IMAGE));
}

/// A program that is not on the machine is *unanswered*, not failed — over a
/// real spawn refusal rather than a scripted one.
///
/// `cve_evaluate` asserts the same distinction against a fixture that hands back
/// `ErrorKind::NotFound` because it was told to. This is the operating system
/// saying it, which is the only version of the claim that could be wrong.
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

/// A scanner that is not on the machine reaches the same place by a different
/// route: [`ScanError::Missing`] is the one scan failure that is the operator's
/// machine rather than the tree, and it is mapped to *not run*.
///
/// [`ScanError::Missing`]: fiddle_runtime::ScanError::Missing
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

/// A check killed at its deadline produced no observation, and is recorded as
/// one that did not run rather than as one the tree failed.
///
/// The case only exists over a real child: a scripted tree answers or it does
/// not, and there is nothing in it for a deadline to interrupt. It is why
/// [`Unanswered::TimedOut`] is a third kind — a killed child has no exit status,
/// so reporting it as a failing check would make a `docker build` that hung on a
/// loaded machine indistinguishable from one the repair broke.
///
/// [`Unanswered::TimedOut`]: fiddle_runtime::evaluate::Unanswered::TimedOut
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

    // It started — the record proves the child existed — and then it was killed.
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

/// A cancelled attempt is not a rejected tree.
///
/// Nothing went wrong with the tree, so no evaluation is produced at all — the
/// runner's [`Cancelled`] rather than five failing results, which an outcome
/// derived from would read as a repair that tried and lost.
///
/// [`Cancelled`]: fiddle_runtime::evaluate::Cancelled
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

/// The known gap, pinned rather than described: a cancelled *rescan* is recorded
/// as an artefact check that produced nothing.
///
/// [`Tree::scan`] returns [`ScanError`], which has nowhere to say *cancelled* —
/// a scan changes nothing outside the process, so `produced no report` is the
/// whole of what a caller can act on, and the port declines to invent a
/// distinction its adapter could not make. The consequence is that a contract of
/// nothing but a rescan comes back as a rejection rather than as an abandonment.
///
/// What the adapter *can* do it does, and that is the second assertion: no
/// scanner is started at all once the token is cancelled. The gap is in what can
/// be reported, not in what is spent.
///
/// [`Tree::scan`]: fiddle_runtime::evaluate::Tree::scan
/// [`ScanError`]: fiddle_runtime::ScanError
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
