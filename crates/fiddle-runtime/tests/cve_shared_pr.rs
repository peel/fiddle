mod support;

use fiddle_core::{
    content_digest, effect_id, AttemptId, EffectId, EffectName, ProposedEffect,
    ENSURE_BRANCH_PUBLISHED, ENSURE_CHECK_REQUESTED, ENSURE_PULL_REQUEST, ENSURE_PULL_REQUEST_BODY,
    ENSURE_PULL_REQUEST_READY, FIXTURE_REPAIR, PUBLISH_DECISION_REQUEST,
};
use fiddle_runtime::capability::cve::{
    check_out, plan, plan_shared_pull_request, publish_work, Approved, Checkout, PlanError,
    Publication, Refusal, SharedWork, BRANCH_STEM, CVE_LABEL, PUSHABLE_PREFIX,
};
use fiddle_runtime::capability::{land, GroupStatus, InWorktree};
use fiddle_runtime::effect::{
    EffectContext, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ReadRetry,
};
use fiddle_runtime::github::{
    find_labelled_pull_request, pull_request_body_target, EnsurePullRequest, EnsurePullRequestBody,
    PullRequest,
};
use fiddle_runtime::journal::{AttemptTrace, FileJournal, JOURNAL_DIR};
use fiddle_runtime::workspace::Workspace;
use fiddle_runtime::{GhCli, GitCli};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::cve::{
    advisories_of, ask_git, landing_world, remote_world, try_ask_git, LandingWorld, RemoteWorld,
    ONLY_ON_THE_REMOTE_BASE, ON_THE_SHARED_BRANCH,
};
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const OWNER: &str = "peel";

const BASE: &str = "main";

const SHARED_HEAD: &str = "security/cve-remediation-20260813";

const TODAY: &str = "20260817";

const SHARED_TITLE: &str = "fiddle: mitigate reported advisories";

const PR: u64 = 7;

const SEEDED_BODY: &str = "opened by fiddle, contents to follow";

const PATIENT: Duration = Duration::from_secs(60);

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

const SLUG: &str = "beans-w-1";

const LANDED_CVE: &str = "CVE-2026-4242";

const A_TIP: &str = "aaaaaaaabbbbbbbbccccccccddddddddeeeeeeee";

const RUN_SUMMARY: &str = "fiddle mitigated the advisories listed below.";

struct Forge {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
    watched: Mutex<Vec<Watched>>,
    trace: AttemptTrace,
}

impl EffectTrace for Forge {
    fn step(&self, kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
        self.watched.lock().unwrap().push(Watched {
            kind: kind.clone(),
            step,
            mutations: self.mutations().len(),
            remote_branches: self.remote_branches(),
        });
        self.trace.step(kind, step);
    }
}

struct Watched {
    kind: EffectName,
    step: ExecutionStep,
    mutations: usize,
    remote_branches: Vec<String>,
}

impl Forge {
    fn holding_the_shared_pull_request() -> Self {
        let world = Self::empty();
        let dir = &world.dir;

        let by_number = dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&by_number).unwrap();
        std::fs::write(
            by_number.join(format!("{PR}.json")),
            serde_json::json!({
                "number": PR,
                "state": "open",
                "title": "fiddle: mitigate reported advisories",
                "body": SEEDED_BODY,
                "draft": false,
                "node_id": "PR_kwDOshared",
            })
            .to_string(),
        )
        .unwrap();

        world
    }

    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        let forge = Self {
            dir,
            steps: Mutex::new(Vec::new()),
            watched: Mutex::new(Vec::new()),
            trace: AttemptTrace::new(),
        };
        ask_git(
            forge.dir.path(),
            &[
                "-c",
                "init.defaultBranch=main",
                "init",
                "--quiet",
                "--bare",
                "remote.git",
            ],
        );
        forge
    }

    fn remote(&self) -> PathBuf {
        self.dir.path().join("remote.git")
    }

    fn seed_branch(&self, branch: &str) -> String {
        let remote = self.remote();
        let named = format!("refs/heads/{branch}");
        if let Ok(existing) = try_ask_git(&remote, &["rev-parse", "--verify", "--quiet", &named]) {
            return existing;
        }
        let commit = ask_git(
            &remote,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit-tree",
                EMPTY_TREE,
                "-m",
                branch,
            ],
        );
        ask_git(&remote, &["update-ref", &named, &commit]);
        commit
    }

    fn pr(&self, number: u64) -> SeededPullRequest {
        let seed: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("pulls_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        let head = seed
            .iter()
            .find(|pr| pr["number"].as_u64() == Some(number))
            .and_then(|pr| pr["head"].as_str())
            .unwrap_or_else(|| panic!("this world holds no pull request numbered {number}"))
            .to_string();
        let branch = head.split_once(':').map(|(_, r)| r).unwrap_or(&head);
        SeededPullRequest {
            head_sha: ask_git(
                &self.remote(),
                &["rev-parse", &format!("refs/heads/{branch}")],
            ),
        }
    }

    fn remote_branches(&self) -> Vec<String> {
        ask_git(
            &self.remote(),
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    fn seed_pull_request(&self, number: u64, head: &str, labels: &[&str]) {
        self.seed_pull_request_in_state(number, head, labels, "open");
    }

    fn seed_pull_request_in_state(&self, number: u64, head: &str, labels: &[&str], state: &str) {
        self.seed_branch(head);

        let path = self.dir.path().join("pulls_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({
            "number": number,
            "state": state,
            "head": format!("{OWNER}:{head}"),
            "base": BASE,
            "title": SHARED_TITLE,
            "body": SEEDED_BODY,
            "labels": labels,
        }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    fn seed_issue(&self, number: u64, labels: &[&str]) {
        let path = self.dir.path().join("issues_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({
            "number": number,
            "title": "a person's own note about advisories",
            "labels": labels,
        }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    fn answer_the_label_search_without_filtering(&self) {
        std::fs::write(self.dir.path().join("issues_unfiltered"), "yes").unwrap();
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

    fn context(&self) -> EffectContext {
        EffectContext::new(
            self.gh(),
            unreachable_git(),
            self.dir.path().to_path_buf(),
            CancellationToken::new(),
        )
    }

    fn context_pushing_from(&self, worktree: &Path) -> EffectContext {
        EffectContext::new(
            self.gh(),
            GitCli::new(
                PathBuf::from("git"),
                "ghp_never_reaches_a_network".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                PATIENT,
            ),
            worktree.to_path_buf(),
            CancellationToken::new(),
        )
    }

    fn requests(&self) -> Vec<Vec<String>> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_else(|_| Vec::new());
        files.sort();
        files
            .iter()
            .filter_map(|file| {
                let recorded: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
                Some(
                    recorded["argv"]
                        .as_array()?
                        .iter()
                        .filter_map(|a| a.as_str().map(str::to_string))
                        .collect(),
                )
            })
            .collect()
    }

    fn body_writes(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                let method = argv
                    .iter()
                    .position(|a| a == "--method")
                    .and_then(|at| argv.get(at + 1));
                method.map(String::as_str) == Some("PATCH")
                    && argv
                        .iter()
                        .any(|a| a == &format!("/repos/{REPO}/pulls/{PR}"))
            })
            .count()
    }

    fn creation_requests(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                let method = argv
                    .iter()
                    .position(|a| a == "--method")
                    .and_then(|at| argv.get(at + 1));
                method.map(String::as_str) == Some("POST")
                    && argv.iter().any(|a| a == &format!("/repos/{REPO}/pulls"))
            })
            .count()
    }

    fn open_pull_requests(&self) -> usize {
        let seeded: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("pulls_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        let landed = std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains(&format!("POST_repos_{}_pulls", REPO.replace('/', "_"))))
            .count();
        seeded.len() + landed
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    fn mutations(&self) -> Vec<String> {
        self.requests()
            .iter()
            .filter(|argv| method_of(argv).as_deref() != Some("GET"))
            .map(|argv| argv.join(" "))
            .collect()
    }

    fn mutations_outside_an_effect(&self) -> Vec<String> {
        let mutations = self.mutations();
        let covered = self.apply_windows();
        mutations
            .iter()
            .enumerate()
            .filter(|(at, _)| !covered.iter().any(|(from, to)| (from..to).contains(&at)))
            .map(|(_, what)| what.clone())
            .collect()
    }

    fn apply_windows(&self) -> Vec<(usize, usize)> {
        let watched = self.watched.lock().unwrap();
        let mut windows = Vec::new();
        for (at, opened) in watched.iter().enumerate() {
            if opened.step != ExecutionStep::Apply {
                continue;
            }
            let closed = watched[at + 1..]
                .iter()
                .find(|later| {
                    later.kind == opened.kind && later.step == ExecutionStep::ObservePostcondition
                })
                .map(|later| later.mutations)
                .unwrap_or(usize::MAX);
            windows.push((opened.mutations, closed));
        }
        windows
    }

    fn branches_gained_during(&self, kind: EffectName) -> Vec<String> {
        let watched = self.watched.lock().unwrap();
        let before = watched
            .iter()
            .find(|it| it.kind == kind && it.step == ExecutionStep::Apply)
            .map(|it| it.remote_branches.clone())
            .unwrap_or_default();
        let after = watched
            .iter()
            .find(|it| it.kind == kind && it.step == ExecutionStep::ObservePostcondition)
            .map(|it| it.remote_branches.clone())
            .unwrap_or_default();
        after
            .into_iter()
            .filter(|branch| !before.contains(branch))
            .collect()
    }

    fn journalling(&self, report_dir: &Path, attempt: &AttemptId) -> Journal {
        self.trace.attach(Arc::new(FileJournal::new(
            report_dir,
            SLUG,
            attempt,
            INVOCATION_REF,
        )));
        Journal {
            path: report_dir
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join(format!("{}.jsonl", attempt.0)),
        }
    }
}

struct SeededPullRequest {
    head_sha: String,
}

fn method_of(argv: &[String]) -> Option<String> {
    argv.iter()
        .position(|a| a == "--method")
        .and_then(|at| argv.get(at + 1))
        .cloned()
}

struct Journal {
    path: PathBuf,
}

impl Journal {
    fn records(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn effect_steps_kinds(&self) -> Vec<EffectName> {
        self.kinds_where(|_| true)
    }

    fn kinds_that_applied(&self) -> Vec<EffectName> {
        self.kinds_where(|step| step == ExecutionStep::Apply.as_str())
    }

    fn kinds_where(&self, wanted: impl Fn(&str) -> bool) -> Vec<EffectName> {
        let mut kinds: Vec<EffectName> = self
            .records()
            .iter()
            .filter(|record| record["record"] == "effect_step")
            .filter(|record| wanted(record["step"].as_str().unwrap_or_default()))
            .filter_map(|record| EffectName::parse(record["kind"].as_str()?).ok())
            .collect();
        kinds.dedup();
        kinds
    }
}

struct BodyUpdate {
    effect_id: EffectId,
    applied: bool,
    observed: String,
}

async fn update_body(forge: &Forge, body: &str) -> BodyUpdate {
    let operation = EnsurePullRequestBody::new(REPO.to_string(), PR, body.to_string());
    let target = operation.target();
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(ENSURE_PULL_REQUEST_BODY),
        target: target.clone(),
        payload: operation.payload(),
    };

    let before = forge.steps().len();
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let ctx = forge.context();
    let receipt = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        forge,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
    .expect("a body update against a pull request the world holds");

    assert_eq!(
        receipt.outcome,
        EffectOutcome::Committed,
        "every walk in this file is expected to conclude; only *how* differs"
    );
    BodyUpdate {
        effect_id: effect_id(PROJECT, INVOCATION_REF, ENSURE_PULL_REQUEST_BODY, &target),
        applied: forge.steps()[before..].contains(&ExecutionStep::Apply.as_str()),
        observed: receipt.value.body,
    }
}

#[tokio::test]
async fn a_changed_body_is_a_new_effect_and_applies() {
    let forge = Forge::holding_the_shared_pull_request();

    let one = update_body(&forge, "covers 1 CVE").await;
    let three = update_body(&forge, "covers 3 CVEs").await;

    assert_ne!(
        one.effect_id, three.effect_id,
        "a changed body is a different effect, or run two spends run one's identity"
    );
    assert!(
        three.applied,
        "and it applies against a world run one already wrote to; steps were {:?}",
        forge.steps()
    );
    assert_eq!(
        three.observed, "covers 3 CVEs",
        "read back out of the world, so the rewrite is observed rather than reported"
    );
    assert_eq!(forge.body_writes(), 2, "two different bodies, two writes");
}

#[tokio::test]
async fn an_unchanged_body_is_idempotent() {
    let forge = Forge::holding_the_shared_pull_request();

    let first = update_body(&forge, "covers 1 CVE").await;
    let again = update_body(&forge, "covers 1 CVE").await;

    assert_eq!(
        first.effect_id, again.effect_id,
        "the same body against the same pull request is the same effect"
    );
    assert!(first.applied, "the first run had work to do");
    assert!(
        !again.applied,
        "and the second found the postcondition already satisfied; steps were {:?}",
        forge.steps()
    );
    assert_eq!(again.observed, "covers 1 CVE");
    assert_eq!(forge.body_writes(), 1, "one write, not two");
}

#[test]
fn the_inversion_of_removing_the_digest_fails_this_test() {
    assert!(digest_is_part_of_target(&EffectName::shipped(
        ENSURE_PULL_REQUEST_BODY
    )));
}

fn digest_is_part_of_target(kind: &EffectName) -> bool {
    match kind.as_str() {
        ENSURE_PULL_REQUEST_BODY => {
            let short = pull_request_body_target(REPO, PR, "covers 1 CVE");
            let other = pull_request_body_target(REPO, PR, "covers 3 CVEs");
            let long = pull_request_body_target(REPO, PR, &"covers 3 CVEs. ".repeat(500));

            short != other
                && short == pull_request_body_target(REPO, PR, "covers 1 CVE")
                && !short.contains("covers")
                && long.len() == other.len()
        }
        ENSURE_BRANCH_PUBLISHED
        | ENSURE_PULL_REQUEST
        | ENSURE_CHECK_REQUESTED
        | PUBLISH_DECISION_REQUEST
        | ENSURE_PULL_REQUEST_READY => false,
        other => panic!("{other} is not an effect this build ships"),
    }
}

#[test]
fn the_target_names_the_repository_the_number_and_the_published_digest() {
    let target = pull_request_body_target(REPO, PR, "covers 1 CVE");

    assert!(target.contains(REPO), "{target}");
    assert!(target.contains(&PR.to_string()), "{target}");
    assert!(
        target.contains(&content_digest("covers 1 CVE")),
        "the target must carry fiddle_core's digest and not a second one: {target}"
    );
}

async fn open_the_shared_pull_request(
    forge: &Forge,
    head: &str,
    labels: &[&str],
) -> EffectReceipt<PullRequest> {
    let operation = EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        head.to_string(),
        BASE.to_string(),
        SHARED_TITLE.to_string(),
        "opened by fiddle, contents to follow".to_string(),
        false,
    )
    .labelled(labels.iter().map(|it| it.to_string()).collect());
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(ENSURE_PULL_REQUEST),
        target: operation.target(),
        payload: operation.payload(),
    };
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let ctx = forge.context();
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        forge,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
    .expect("a pull request create against a world that holds none")
}

async fn discover(forge: &Forge) -> Option<fiddle_runtime::github::SharedPullRequest> {
    find_labelled_pull_request(&forge.gh(), REPO, CVE_LABEL, &CancellationToken::new())
        .await
        .expect("the label search is readable")
}

async fn decide(forge: &Forge) -> Result<Approved, PlanError> {
    plan_shared_pull_request(&forge.gh(), REPO, BASE, TODAY, &CancellationToken::new()).await
}

#[tokio::test]
async fn a_created_pull_request_carries_the_label_that_finds_it() {
    let forge = Forge::empty();
    forge.seed_branch(SHARED_HEAD);

    let receipt = open_the_shared_pull_request(&forge, SHARED_HEAD, &[CVE_LABEL]).await;

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert!(
        receipt.value.labels.contains(&CVE_LABEL.to_string()),
        "the postcondition read found the pull request carrying the label: {:?}",
        receipt.value
    );

    let found = discover(&forge)
        .await
        .expect("the run that created it must be able to find it again");
    assert_eq!(found.number, receipt.value.number);
    assert_eq!(
        found.head, SHARED_HEAD,
        "and at the branch it was opened on"
    );
    assert!(
        found.duplicates.is_empty(),
        "one pull request, so nothing to note: {:?}",
        found.duplicates
    );

    assert_eq!(
        forge
            .steps()
            .iter()
            .filter(|step| **step == ExecutionStep::Apply.as_str())
            .count(),
        1,
        "the label is not a second effect: {:?}",
        forge.steps()
    );
    let posts: Vec<String> = forge
        .requests()
        .iter()
        .filter(|argv| argv.iter().any(|a| a == "POST"))
        .filter_map(|argv| argv.iter().find(|a| a.starts_with('/')).cloned())
        .collect();
    assert_eq!(
        posts,
        [
            format!("/repos/{REPO}/pulls"),
            format!("/repos/{REPO}/issues/{}/labels", receipt.value.number),
        ],
        "the create, and then the label on the object it created — in that order, \
         because the label is addressed by a number that does not exist until the \
         create has run"
    );
}

#[tokio::test]
async fn an_unlabelled_pull_request_is_not_the_labelled_effect_having_happened() {
    let operation = EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        SHARED_HEAD.to_string(),
        BASE.to_string(),
        SHARED_TITLE.to_string(),
        "opened by fiddle, contents to follow".to_string(),
        false,
    )
    .labelled(vec![CVE_LABEL.to_string()]);

    let unlabelled = Forge::empty();
    unlabelled.seed_pull_request(41, SHARED_HEAD, &[]);
    assert_eq!(
        operation.inspect(&unlabelled.context()).await.unwrap(),
        None,
        "the pull request exists and the postcondition does not hold"
    );

    let labelled = Forge::empty();
    labelled.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    let observed = operation
        .inspect(&labelled.context())
        .await
        .unwrap()
        .expect("the same head and base, and this time it carries the label");
    assert_eq!(observed.number, 41);
    assert_eq!(observed.labels, [CVE_LABEL]);
}

#[tokio::test]
async fn a_label_a_person_added_does_not_unsatisfy_the_postcondition() {
    let operation = EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        SHARED_HEAD.to_string(),
        BASE.to_string(),
        SHARED_TITLE.to_string(),
        "opened by fiddle, contents to follow".to_string(),
        false,
    )
    .labelled(vec![CVE_LABEL.to_string()]);

    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL, "needs-triage"]);

    let observed = operation
        .inspect(&forge.context())
        .await
        .unwrap()
        .expect("ours is there, beside somebody else's");
    assert_eq!(observed.labels, [CVE_LABEL, "needs-triage"]);
}

#[tokio::test]
async fn a_pull_request_created_without_the_label_is_invisible_to_the_next_run() {
    let forge = Forge::empty();

    let receipt = open_the_shared_pull_request(&forge, SHARED_HEAD, &[]).await;

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        forge.open_pull_requests(),
        1,
        "the pull request really exists; it is only the label that is missing"
    );
    assert!(
        receipt.value.labels.is_empty(),
        "and it carries none: {:?}",
        receipt.value
    );

    assert!(
        discover(&forge).await.is_none(),
        "an unlabelled pull request is invisible to the discovery read, so the \
         next run would open a second one"
    );
}

#[tokio::test]
async fn an_existing_open_pull_request_is_reused_and_never_duplicated() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    let approved = decide(&forge)
        .await
        .expect("a head under the pushable prefix");

    assert_eq!(approved.reused(), Some(41));
    assert_eq!(
        approved.branch(),
        SHARED_HEAD,
        "the run adds to the pull request's own branch rather than cutting one"
    );
    assert!(
        !approved.branch().contains(TODAY),
        "and it is emphatically not today's fresh branch: {}",
        approved.branch()
    );
    assert!(approved.duplicates().is_empty());
    assert_eq!(forge.creation_requests(), 0, "never a second");
    assert_eq!(forge.open_pull_requests(), 1);
}

#[tokio::test]
async fn with_nothing_open_a_dated_branch_is_cut_from_an_origin_ref() {
    let forge = Forge::empty();

    let approved = decide(&forge)
        .await
        .expect("an empty world plans a fresh cut");

    assert_eq!(approved.reused(), None);
    assert_eq!(approved.branch(), format!("{BRANCH_STEM}{TODAY}"));
    assert!(
        approved.branch().starts_with(PUSHABLE_PREFIX),
        "a branch this capability cuts must satisfy its own push guard: {}",
        approved.branch()
    );
    assert_eq!(
        approved.from(),
        format!("origin/{BASE}"),
        "the remote's base, and never local HEAD or local main"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "the create is not this lane's"
    );
}

#[tokio::test]
async fn reuse_names_the_remote_tip_of_the_pull_requests_branch() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    let approved = decide(&forge)
        .await
        .expect("a head under the pushable prefix");

    assert_eq!(approved.from(), format!("origin/{SHARED_HEAD}"));
    assert!(!approved.from().contains("HEAD"), "{}", approved.from());
}

#[tokio::test]
async fn several_open_pull_requests_take_the_lowest_and_note_the_rest() {
    let forge = Forge::empty();
    for number in [57u64, 41, 63] {
        forge.seed_pull_request(
            number,
            &format!("{BRANCH_STEM}2026080{number}"),
            &[CVE_LABEL],
        );
    }

    let approved = decide(&forge).await.expect("every head is pushable here");

    assert_eq!(approved.reused(), Some(41), "the lowest, not the first");
    assert_eq!(
        approved.duplicates(),
        [57, 63],
        "ascending, and both of them"
    );

    let note = approved
        .note(CVE_LABEL)
        .expect("an anomaly a person made is an anomaly a person is told about");
    assert!(
        note.contains("57") && note.contains("63"),
        "the note must name the ones a person has to close: {note}"
    );
    assert!(
        !note.contains("41"),
        "and must not name the one being reused, which is not a duplicate: {note}"
    );
    assert_eq!(forge.creation_requests(), 0, "and never another");
}

#[tokio::test]
async fn one_open_pull_request_is_not_an_anomaly_and_is_not_noted() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    assert_eq!(decide(&forge).await.unwrap().note(CVE_LABEL), None);
}

#[tokio::test]
async fn a_label_search_answered_without_its_filter_is_narrowed_here() {
    let forge = Forge::empty();
    forge.seed_pull_request(12, "feature/somebody-elses-work", &[]);
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    forge.answer_the_label_search_without_filtering();

    let found = discover(&forge)
        .await
        .expect("one pull request carries the label");

    assert_eq!(
        found.number, 41,
        "the unlabelled #12 is lower and must still not be settled on"
    );
    assert!(found.duplicates.is_empty());
}

#[tokio::test]
async fn a_closed_pull_request_carrying_the_label_is_not_settled_on() {
    let forge = Forge::empty();
    forge.seed_pull_request_in_state(
        12,
        "security/cve-remediation-20260701",
        &[CVE_LABEL],
        "closed",
    );
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    forge.answer_the_label_search_without_filtering();

    let found = discover(&forge).await.expect("the open one is still found");

    assert_eq!(found.number, 41, "#12 is closed and merged or abandoned");
    assert!(
        found.duplicates.is_empty(),
        "and a closed pull request is not an anomaly to report: {:?}",
        found.duplicates
    );
}

#[tokio::test]
async fn a_plain_issue_carrying_the_label_is_not_the_shared_pull_request() {
    let forge = Forge::empty();
    forge.seed_issue(9, &[CVE_LABEL]);
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    let found = discover(&forge)
        .await
        .expect("the pull request is still found");

    assert_eq!(found.number, 41, "#9 is an issue, not a pull request");
    assert!(
        found.duplicates.is_empty(),
        "and it is not an anomaly either: {:?}",
        found.duplicates
    );
}

struct Published {
    approved: Approved,
    checkout: Checkout,
    worktree_head: String,
    work: SharedWork,
    history_before_landing: String,
    history_after_landing: String,
    journal_grew_across_the_landing: usize,
    journal: Journal,
    _reports: TempDir,
}

impl Published {
    fn bundle(&self) -> serde_json::Value {
        serde_json::json!({ "observations": self.checkout.observed() })
    }

    fn local_commits_are_not_effects(&self) -> bool {
        self.history_after_landing != self.history_before_landing
            && self.journal_grew_across_the_landing == 0
    }
}

async fn publish(forge: &Forge, world: &RemoteWorld) -> Published {
    let cancel = CancellationToken::new();
    let attempt = AttemptId("01JCVEPUBLISH0000000000000".to_string());
    let reports = TempDir::new().expect("a temporary directory for the attempt journal");
    let journal = forge.journalling(reports.path(), &attempt);

    let approved = plan_shared_pull_request(&forge.gh(), REPO, BASE, TODAY, &cancel)
        .await
        .expect("a head under the pushable prefix");

    let checkout = check_out(&world.tree, &approved)
        .await
        .expect("the remote holds the refs this run named");

    let root = TempDir::new().expect("a temporary directory for the worktree");
    let workspace = Workspace::create_at(
        world.tree.path(),
        root.path(),
        &attempt,
        checkout.revision(),
        cancel.clone(),
    )
    .expect("a worktree at the revision the checkout named");
    let worktree_head = ask_git(workspace.root(), &["rev-parse", "HEAD"]);
    let history_before_landing = ask_git(workspace.root(), &["log", "--format=%B"]);

    let changed = world.bump_into(workspace.root());
    let before = journal.records().len();
    land(
        &InWorktree::new(&workspace, PATIENT, &unreachable_git()),
        &advisories_of(&world.findings),
        &GroupStatus::Clean,
        &changed,
        None,
    )
    .await
    .expect("a clean group over a tree that really changed");
    let journal_grew_across_the_landing = journal.records().len() - before;
    let history_after_landing = ask_git(workspace.root(), &["log", "--format=%B"]);
    let landed = ask_git(workspace.root(), &["rev-parse", "HEAD"]);

    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let ctx = forge.context_pushing_from(workspace.root());
    let executor = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        forge,
        ReadRetry::none(),
    );
    let work = publish_work(
        &executor,
        FIXTURE_REPAIR,
        &approved,
        &Publication {
            repo: REPO.to_string(),
            head_owner: OWNER.to_string(),
            title: SHARED_TITLE.to_string(),
            summary: RUN_SUMMARY.to_string(),
            head_sha: landed,
            attempts: 1,
            label: CVE_LABEL,
            draft: false,
        },
    )
    .await
    .expect("a branch this capability may push to, and one pull request");

    Published {
        approved,
        checkout,
        worktree_head,
        work,
        history_before_landing,
        history_after_landing,
        journal_grew_across_the_landing,
        journal,
        _reports: reports,
    }
}

#[tokio::test]
async fn reusing_a_pull_request_checks_out_its_remote_tip_and_records_both_revisions() {
    let forge = Forge::empty();
    let world = remote_world(&forge.remote(), Some(SHARED_HEAD), &[LANDED_CVE]);
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    let tip = forge.pr(41).head_sha;

    let out = publish(&forge, &world).await;

    assert_eq!(out.approved.reused(), Some(41));
    assert_eq!(
        out.worktree_head, tip,
        "the remote tip, never a local branch left by an earlier run"
    );
    assert_ne!(
        out.worktree_head,
        world
            .stale_head
            .clone()
            .expect("this world has a stale local branch of the same name"),
        "checking the branch out by name would land here"
    );
    assert_ne!(
        out.worktree_head, world.stale_main,
        "and branching from local HEAD would land here"
    );
    assert!(
        workspace_holds(&world, &out.worktree_head, ON_THE_SHARED_BRANCH),
        "the tree the attempt ran in has to be the shared branch's tree"
    );

    let obs = out.bundle()["observations"].clone();
    assert!(
        obs["base_revision"].is_string() && obs["pr_head"].is_string(),
        "both are observed; the bundle says which the attempt ran against: {obs}"
    );
    assert_eq!(obs["attempt_tree"], "pr_head");
    assert_eq!(obs["pr_head"], out.worktree_head, "{obs}");
    assert_eq!(
        obs["base_revision"], world.base_revision,
        "the base is observed on this arm too, and it is the remote's: {obs}"
    );
    assert_ne!(
        obs["base_revision"], obs["pr_head"],
        "a world in which the two coincided could not tell them apart: {obs}"
    );

    assert_eq!(out.work.pull_request, 41);
    assert_eq!(forge.creation_requests(), 0, "never a second");
    assert_eq!(forge.open_pull_requests(), 1);
    assert_eq!(
        out.work.head_sha,
        ask_git(
            &forge.remote(),
            &["rev-parse", &format!("refs/heads/{SHARED_HEAD}")]
        ),
        "the receipt carries what the remote was observed to hold, and the branch \
         has moved on from {tip} to the commit the landing added"
    );
}

#[tokio::test]
async fn every_external_mutation_passes_the_effect_executor() {
    let forge = Forge::empty();
    let world = remote_world(&forge.remote(), None, &[LANDED_CVE]);

    let out = publish(&forge, &world).await;

    let kinds = out.journal.effect_steps_kinds();
    assert!(
        kinds.contains(&EffectName::shipped(ENSURE_BRANCH_PUBLISHED)),
        "the journal must name the branch effect: {kinds:?}"
    );
    assert!(
        kinds.contains(&EffectName::shipped(ENSURE_PULL_REQUEST)),
        "and the pull request effect: {kinds:?}"
    );
    let applied = out.journal.kinds_that_applied();
    assert!(
        applied.contains(&EffectName::shipped(ENSURE_BRANCH_PUBLISHED))
            && applied.contains(&EffectName::shipped(ENSURE_PULL_REQUEST)),
        "both effects had work to do in an empty world: {applied:?}"
    );

    let mutations = forge.mutations();
    assert!(
        !mutations.is_empty(),
        "no request that could change the forge was dispatched at all, so this \
         lane is measuring nothing"
    );
    assert_eq!(
        forge.mutations_outside_an_effect(),
        Vec::<String>::new(),
        "these reached the forge outside any effect's apply window; every one of \
         the {} dispatched must fall inside one",
        mutations.len()
    );

    assert_eq!(
        forge.branches_gained_during(EffectName::shipped(ENSURE_BRANCH_PUBLISHED)),
        [out.approved.branch().to_string()],
        "the push must have happened inside the branch effect's apply window; the \
         remote's branches are now {:?}",
        forge.remote_branches()
    );

    assert!(
        out.local_commits_are_not_effects(),
        "a local commit is not an external mutation and is deliberately not \
         journaled as one — the landing committed ({} record(s) added)",
        out.journal_grew_across_the_landing
    );
    assert!(
        out.history_after_landing.contains(LANDED_CVE),
        "and it is *this group's* commit rather than any commit at all: {}",
        out.history_after_landing
    );
    assert_eq!(out.work.branch, out.approved.branch());
    assert_eq!(forge.creation_requests(), 1, "one create, and only one");
    assert_eq!(forge.open_pull_requests(), 1);
}

#[tokio::test]
async fn a_fresh_branch_is_cut_from_the_remote_and_is_dated() {
    let forge = Forge::empty();
    let world = remote_world(&forge.remote(), None, &[LANDED_CVE]);

    let out = publish(&forge, &world).await;

    assert!(
        out.approved.branch().starts_with(BRANCH_STEM),
        "{}",
        out.approved.branch()
    );
    assert!(out.approved.branch().ends_with(TODAY), "and dated");

    let calls = world.tree.git_calls();
    assert!(
        calls.iter().any(|call| call.contains("origin/main")),
        "never local HEAD or local main; a stale local main contaminated a prior \
         run. The subject ran: {calls:?}"
    );
    assert_eq!(out.worktree_head, world.base_revision);
    assert_ne!(
        out.worktree_head, world.stale_main,
        "branching from local main would land here"
    );
    assert!(
        workspace_holds(&world, &out.worktree_head, ONLY_ON_THE_REMOTE_BASE),
        "the base moved on after the clone was taken, and the attempt has to be \
         standing on the commit that moved it"
    );

    let obs = out.bundle()["observations"].clone();
    assert_eq!(obs["attempt_tree"], "base_revision");
    assert_eq!(obs["base_revision"], world.base_revision);
    assert!(
        obs.get("pr_head").is_some_and(|it| it.is_null()),
        "no pull request was open, and the bundle has to say so rather than \
         leaving a reader to read a missing key as an old build: {obs}"
    );
}

fn workspace_holds(world: &RemoteWorld, revision: &str, path: &str) -> bool {
    ask_git(
        world.tree.path(),
        &["ls-tree", "--name-only", revision, path],
    )
    .lines()
    .any(|line| line == path)
}

#[tokio::test]
async fn a_head_branch_outside_the_pushable_prefix_stops_before_committing() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, "feature/not-security", &[CVE_LABEL]);

    let refused = decide(&forge)
        .await
        .expect_err("a head outside the pushable prefix is not something to work around");

    assert!(
        matches!(
            &refused,
            PlanError::Refused(Refusal::HeadOutsideThePushablePrefix { number: 41, .. })
        ),
        "{refused}"
    );
    let said = refused.to_string();
    assert!(said.contains("feature/not-security"), "{said}");
    assert!(said.contains(PUSHABLE_PREFIX), "{said}");
    assert!(said.contains("41"), "{said}");
}

#[tokio::test]
async fn the_prefix_refusal_reaches_the_run_before_any_commit() {
    let refused_world = landing_world(&["CVE-2026-4242"]);
    let refused_forge = Forge::empty();
    refused_forge.seed_pull_request(41, "feature/not-security", &[CVE_LABEL]);
    let before = refused_world.tree.all_commit_bodies();

    let outcome = discover_then_land(&refused_forge, &refused_world).await;

    assert!(matches!(outcome, Err(PlanError::Refused(_))), "{outcome:?}");
    assert_eq!(
        refused_world.tree.all_commit_bodies(),
        before,
        "the history must be exactly what it was, so nothing was committed"
    );
    assert!(
        refused_world.tree.git_calls().is_empty(),
        "and no git command ran at all: {:?}",
        refused_world.tree.git_calls()
    );

    let allowed_world = landing_world(&["CVE-2026-4242"]);
    let allowed_forge = Forge::empty();
    allowed_forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    let before = allowed_world.tree.all_commit_bodies();

    discover_then_land(&allowed_forge, &allowed_world)
        .await
        .expect("a pushable head lets the landing run");

    assert_ne!(
        allowed_world.tree.all_commit_bodies(),
        before,
        "so the driver really does commit when the guard lets it"
    );
    assert!(
        allowed_world
            .tree
            .head_commit_body()
            .contains("CVE-2026-4242"),
        "and it is this group's commit: {}",
        allowed_world.tree.head_commit_body()
    );
}

async fn discover_then_land(forge: &Forge, world: &LandingWorld) -> Result<Approved, PlanError> {
    let approved =
        plan_shared_pull_request(&forge.gh(), REPO, BASE, TODAY, &CancellationToken::new()).await?;

    land(
        &world.tree,
        &advisories_of(&world.findings),
        &fiddle_runtime::capability::GroupStatus::Clean,
        &world.changed,
        None,
    )
    .await
    .expect("a clean group over a tree that really changed");

    Ok(approved)
}

#[test]
fn the_branch_this_capability_cuts_satisfies_its_own_push_guard() {
    assert!(
        BRANCH_STEM.starts_with(PUSHABLE_PREFIX),
        "{BRANCH_STEM} is not under {PUSHABLE_PREFIX}"
    );
    assert_eq!(PUSHABLE_PREFIX, "security/");
    assert_eq!(BRANCH_STEM, "security/cve-remediation-");
    assert_eq!(CVE_LABEL, "security/cve");
}

#[test]
fn the_decision_is_taken_over_the_observation_alone() {
    use fiddle_runtime::github::SharedPullRequest;

    let outside = SharedPullRequest {
        number: 41,
        head: "feature/not-security".to_string(),
        head_sha: A_TIP.to_string(),
        base: BASE.to_string(),
        title: SHARED_TITLE.to_string(),
        duplicates: Vec::new(),
    };
    assert!(matches!(
        plan(Some(outside), BASE, TODAY),
        Err(Refusal::HeadOutsideThePushablePrefix { .. })
    ));

    let inside = SharedPullRequest {
        number: 41,
        head: SHARED_HEAD.to_string(),
        head_sha: A_TIP.to_string(),
        base: BASE.to_string(),
        title: SHARED_TITLE.to_string(),
        duplicates: vec![57, 63],
    };
    let approved = plan(Some(inside), BASE, TODAY).expect("a pushable head");
    assert_eq!(approved.reused(), Some(41));
    assert_eq!(approved.duplicates(), [57, 63]);
    assert_eq!(
        approved.pr_head(),
        Some(A_TIP),
        "the tip the observation named is carried through, because it is what the \
         attempt's tree is made at"
    );

    let fresh = plan(None, BASE, TODAY).expect("nothing open is not a refusal");
    assert_eq!(fresh.reused(), None);
    assert_eq!(fresh.branch(), format!("{BRANCH_STEM}{TODAY}"));
    assert_eq!(
        fresh.pr_head(),
        None,
        "there is no pull request, so there is no head for one to have"
    );
}

const SHIPPED: [&str; 6] = [
    ENSURE_BRANCH_PUBLISHED,
    ENSURE_PULL_REQUEST,
    ENSURE_CHECK_REQUESTED,
    PUBLISH_DECISION_REQUEST,
    ENSURE_PULL_REQUEST_READY,
    ENSURE_PULL_REQUEST_BODY,
];

#[test]
fn no_comment_edit_path_exists() {
    assert!(
        SHIPPED.iter().all(|name| !name.contains("comment")),
        "an effect name names a comment: {SHIPPED:?}"
    );

    let scan = scan_for_comment_dispatches();

    assert!(
        scan.dispatches > 0,
        "no `.api(` dispatch was found under any crate's src, so this lane is \
         looking in the wrong place"
    );
    assert!(
        !scan.reaching.is_empty(),
        "no dispatch was resolved to a comment path, so the resolution this lane \
         depends on has stopped working"
    );
    assert!(
        scan.graphql_mutations > 0,
        "no `.graphql(` call resolved to a query naming a mutation, so the rule \
         that would catch `updateIssueComment` is matching against nothing"
    );
    assert!(
        !scan.allowed.is_empty(),
        "the allowlist matched nothing, so it was never tested: {:#?}",
        scan.reaching
    );

    assert!(
        scan.edits.is_empty(),
        "these reach a comment that already exists with something other than a \
         read, and `DecisionError::RequestEdited` depends on none of them \
         existing:\n{}",
        scan.edits.join("\n")
    );
}

struct CommentScan {
    dispatches: usize,
    graphql_mutations: usize,
    reaching: Vec<String>,
    allowed: Vec<String>,
    edits: Vec<String>,
}

fn scan_for_comment_dispatches() -> CommentScan {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate lives under the workspace's crates directory");

    let mut scan = CommentScan {
        dispatches: 0,
        graphql_mutations: 0,
        reaching: Vec::new(),
        allowed: Vec::new(),
        edits: Vec::new(),
    };

    for file in rust_sources(crates) {
        let text = std::fs::read_to_string(&file).expect("a source file of this workspace");
        let flat = collapse(&text);
        let defined = definitions(&flat);
        let at = file.strip_prefix(crates).unwrap_or(&file).display();

        for (call, args) in calls(&flat, ".api(") {
            scan.dispatches += 1;
            let verb = literal(args.first().map(String::as_str).unwrap_or_default());
            let path = expanded(
                &defined,
                args.get(1).map(String::as_str).unwrap_or_default(),
            );
            if !path.contains("/comments") {
                continue;
            }
            let where_ = format!("{at}: {verb} {call}");
            scan.reaching.push(where_.clone());
            match permitted(&verb, &path) {
                true => scan.allowed.push(where_),
                false => scan.edits.push(where_),
            }
        }

        for (call, args) in calls(&flat, ".graphql(") {
            scan.dispatches += 1;
            let query = expanded(
                &defined,
                args.first().map(String::as_str).unwrap_or_default(),
            );
            if query.contains("mutation") {
                scan.graphql_mutations += 1;
            }
            if query.contains("Comment") {
                let where_ = format!("{at}: graphql {call}");
                scan.reaching.push(where_.clone());
                scan.edits.push(where_);
            }
        }
    }

    scan
}

fn permitted(verb: &str, path: &str) -> bool {
    let collection = path.contains("/comments\"") || path.contains("/comments?");
    let member = path.contains("/comments/");
    match verb {
        "GET" => true,
        "POST" => collection && !member,
        _ => false,
    }
}

fn rust_sources(crates: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending: Vec<PathBuf> = std::fs::read_dir(crates)
        .expect("the workspace's crates directory is readable")
        .flatten()
        .map(|entry| entry.path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    assert!(
        !pending.is_empty(),
        "no crate under {} has a src directory",
        crates.display()
    );
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|why| panic!("{} is readable: {why}", dir.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

fn collapse(text: &str) -> String {
    let mut flat = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        flat.push_str(trimmed);
        flat.push(' ');
    }
    flat
}

fn calls(flat: &str, marker: &str) -> Vec<(String, Vec<String>)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = flat[from..].find(marker) {
        let open = from + at + marker.len() - 1;
        from = open + 1;
        let Some(close) = matching_paren(flat, open) else {
            continue;
        };
        let inside = &flat[open + 1..close];
        found.push((
            format!("{marker}{inside})"),
            split_top_level(inside)
                .into_iter()
                .map(str::to_string)
                .collect(),
        ));
    }
    found
}

fn matching_paren(flat: &str, open: usize) -> Option<usize> {
    let bytes = flat.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, byte) in bytes.iter().enumerate().skip(open) {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b'(') => depth += 1,
            (false, _, b')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(inside: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut in_string, mut escaped) = (0i32, 0usize, false, false);
    for (i, byte) in inside.bytes().enumerate() {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b'(' | b'[' | b'{') => depth += 1,
            (false, _, b')' | b']' | b'}') => depth -= 1,
            (false, _, b',') if depth == 0 => {
                parts.push(inside[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inside[start..].trim());
    parts
}

fn literal(expr: &str) -> String {
    match (expr.find('"'), expr.rfind('"')) {
        (Some(open), Some(close)) if close > open => expr[open + 1..close].to_string(),
        _ => expr.to_string(),
    }
}

fn expanded(defined: &Definitions, expr: &str) -> String {
    let mut text = expr.to_string();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for _ in 0..4 {
        let mut grew = false;
        for name in identifiers(&text) {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(body) = defined.get(&name) {
                text.push(' ');
                text.push_str(body);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    text
}

type Definitions = std::collections::BTreeMap<String, String>;

fn definitions(flat: &str) -> Definitions {
    let mut defined = Definitions::new();
    let mut bind = |name: &str, body: &str| {
        defined
            .entry(name.to_string())
            .or_default()
            .push_str(&format!(" {body}"));
    };

    for (at, _) in flat.match_indices("fn ") {
        let rest = &flat[at + 3..];
        let Some(open) = rest.find('(') else { continue };
        let name = rest[..open].trim();
        if !is_identifier(name) {
            continue;
        }
        if let Some(body) = rest
            .find('{')
            .and_then(|brace| matching_brace(rest, brace).map(|close| &rest[brace..=close]))
        {
            bind(name, body);
        }
    }

    for keyword in ["let ", "const ", "static "] {
        for (at, _) in flat.match_indices(keyword) {
            let rest = &flat[at + keyword.len()..];
            let Some(end) = semicolon(rest) else { continue };
            let statement = &rest[..end];
            let Some(equals) = statement.find(" = ") else {
                continue;
            };
            let (pattern, body) = statement.split_at(equals);
            for name in identifiers(pattern) {
                bind(&name, &body[3..]);
            }
        }
    }

    defined
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !text.starts_with(|c: char| c.is_numeric())
}

fn semicolon(text: &str) -> Option<usize> {
    let (mut in_string, mut escaped) = (false, false);
    for (i, byte) in text.bytes().enumerate() {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b';') => return Some(i),
            _ => {}
        }
    }
    None
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (i, byte) in text.bytes().enumerate().skip(open) {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b'{') => depth += 1,
            (false, _, b'}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn identifiers(expr: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in expr.chars() {
        match (in_string, escaped, ch) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (true, false, '"') => in_string = false,
            (true, false, _) => {}
            (false, _, '"') => {
                in_string = true;
                flush(&mut current, &mut names);
            }
            (false, _, c) if c.is_alphanumeric() || c == '_' => current.push(c),
            _ => flush(&mut current, &mut names),
        }
    }
    flush(&mut current, &mut names);
    names
}

fn flush(current: &mut String, names: &mut Vec<String>) {
    if !current.is_empty() && !current.chars().next().is_some_and(|c| c.is_numeric()) {
        names.push(std::mem::take(current));
    }
    current.clear();
}
