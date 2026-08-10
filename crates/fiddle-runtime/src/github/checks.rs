//! Observing checks, and requesting one without starting it twice.
//!
//! Only GitHub Apps may create check runs, and this credential is not one, so
//! fiddle never authors a check. What it does is *observe* checks by exact head
//! sha and, where a workflow has to be started, *dispatch* it — the one
//! operation in this milestone GitHub gives no duplicate protection for at all.
//!
//! That asymmetry is worth stating plainly, because it is what the rest of this
//! module is shaped by. [`refs`](super::refs) rests on `git push` to a named ref
//! being idempotent; [`pulls`](super::pulls) rests on GitHub refusing a second
//! pull request for the same head and base. A `workflow_dispatch` protects
//! nothing. A retried dispatch simply starts a second run, and every line below
//! is the only thing standing between a lost response and a duplicate external
//! effect.
//!
//! # The dispatch response tells you nothing
//!
//! `POST /repos/{owner}/{repo}/actions/workflows/{id}/dispatches` answers
//! **`HTTP/2.0 204 No Content`**. No body, no run id, no `Location` — there is
//! nothing in the response to correlate with. Verified against real GitHub
//! before this module was written, not assumed from the documentation.
//!
//! The obvious repair — put the identity in the dispatch inputs and filter the
//! runs listing on it — is **not available**. `GET .../actions/runs/{id}` does
//! not carry the inputs a dispatch was made with: `gh api .../actions/runs/{id}
//! --jq 'has("inputs")'` answers `false`, and no key of a run object matches
//! `/input|dispatch/i`. Also verified, and for the same reason: an identity
//! mechanism that cannot be read back is not a mechanism.
//!
//! # So the identity travels out one way and returns another
//!
//! It goes **out** as the `fiddle_effect_id` dispatch input. It comes **back**
//! through the target workflow's own `run-name`, which the runs listing *does*
//! return, as `name`:
//!
//! ```yaml
//! run-name: fiddle-${{ inputs.fiddle_effect_id }}
//! ```
//!
//! That round trip is verified rather than assumed. A probe dispatch against
//! `peel/fiddle-effects-acceptance`, whose `.github/workflows/fiddle-check.yml`
//! (commit `73b480a`) declares exactly that `run-name`, produced a run whose
//! listing entry carried `"name": "fiddle-probe0123abcd4567"` — the id this
//! process sent, returned by a field this process can filter on.
//!
//! **This is therefore a contract between this module and that workflow file,
//! and it is a fragile one.** [`run_name`] spells one half; the workflow's
//! `run-name:` spells the other, in a different repository, under a different
//! review. Change either — rename the input, drop the prefix, let the workflow
//! interpolate something else into its title — and nothing fails loudly: the
//! locator simply stops finding runs that exist, [`EnsureCheckRequested::inspect`]
//! reports an absence that is not real, and the dispatch happens again. The
//! duplicate this milestone exists to prevent is one careless edit away, in a
//! file this repository does not contain.
//!
//! The workflow also declares `concurrency: { group: fiddle-<id>,
//! cancel-in-progress: false }`, which bounds how much overlap a mistake can
//! cause. It is a mitigation and not evidence: a concurrency group says two runs
//! will not execute at once, never that only one was ever requested. The listing
//! is what decides.
//!
//! # And the listing is read fail-closed
//!
//! `GET .../workflows/{id}/runs` says "no runs" with `200` and an empty
//! `workflow_runs` array, exactly as the pull request list endpoint says "no
//! pull requests" with `200 []`. So an *error* from it is the listing being
//! unreadable, never a run being absent — the same rule
//! [`pulls`](super::pulls) applies for the same reason, and the reason is that
//! reading an outage as "nothing there" is precisely how the second dispatch
//! gets sent. [`observe_checks`] answers the same question with
//! [`Observation::Unavailable`], which is M0's rule (`Unavailable` is never
//! equivalent to empty) arriving at a third boundary.

use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::github::{encode, GhCli, GhError};
use fiddle_core::{
    effect_id, EffectId, EffectKind, HumanDecisionRequirement, Observation, SourceRef,
    VerificationState,
};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Observing checks
// ---------------------------------------------------------------------------

/// One check run's lifecycle state, with every case GitHub can report kept
/// apart from every other.
///
/// The distinctions are the point. A run that has not started, a run that is
/// still going and a run that concluded are three different pieces of news, and
/// so are the eight things a completed run can conclude. Collapsing any two of
/// them is how a report says "verified" before CI has done anything — and the
/// collapse that matters most is [`CheckState::Absent`] into
/// [`CheckState::Passed`], because a head with no checks at all looks
/// indistinguishable from a head whose checks all went green if the two share a
/// branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckState {
    /// No check run by this name at this head. Never produced by
    /// [`classify`] — a state GitHub reported is not an absence — only by
    /// [`observe_checks`] finding nothing to classify.
    Absent,
    /// Accepted and not started. GitHub's `queued`, and also its `waiting`,
    /// `requested` and `pending`, which differ only in *why* it has not started.
    Queued,
    InProgress,
    /// `completed` / `success`, and the only state that satisfies a requirement.
    Passed,
    Failed,
    Cancelled,
    TimedOut,
    ActionRequired,
    Neutral,
    Skipped,
    Stale,
    /// A status or conclusion this client does not know.
    ///
    /// Its own state rather than folded into [`CheckState::Failed`] or
    /// [`CheckState::Absent`]: "CI said something I cannot read" is a third
    /// thing, and the version of this that mapped it to `Absent` would report a
    /// check that exists as a check that does not.
    Unrecognized,
}

impl CheckState {
    /// The single state that satisfies a required check.
    ///
    /// [`CheckState::Neutral`] and [`CheckState::Skipped`] deliberately do not.
    /// GitHub's branch protection has, in some configurations, counted them as
    /// satisfying a required status check; fiddle does not, because "the job
    /// decided there was nothing to do" is not "the job verified this change",
    /// and a run that reported a skipped check as verification would be
    /// reporting a green it never received.
    pub fn is_passed(self) -> bool {
        matches!(self, CheckState::Passed)
    }

    /// Whether this state can still become another one.
    pub fn is_pending(self) -> bool {
        matches!(self, CheckState::Queued | CheckState::InProgress)
    }
}

/// Read one check run's `status` and `conclusion` into a state.
///
/// Total over every pair, including the ones GitHub has not invented yet: an
/// unknown status or conclusion becomes [`CheckState::Unrecognized`] rather than
/// anything that could be mistaken for a pass or for an absence.
pub fn classify(status: &str, conclusion: Option<&str>) -> CheckState {
    match (status, conclusion) {
        // Every pre-start status. They differ in why the run has not begun,
        // which is GitHub's scheduling detail; none of them is a start, and
        // none of them is a result.
        ("queued" | "waiting" | "requested" | "pending", _) => CheckState::Queued,
        ("in_progress", _) => CheckState::InProgress,
        ("completed", Some(conclusion)) => match conclusion {
            "success" => CheckState::Passed,
            "failure" => CheckState::Failed,
            "cancelled" => CheckState::Cancelled,
            "timed_out" => CheckState::TimedOut,
            "action_required" => CheckState::ActionRequired,
            "neutral" => CheckState::Neutral,
            "skipped" => CheckState::Skipped,
            "stale" => CheckState::Stale,
            _ => CheckState::Unrecognized,
        },
        // Completed and concluding nothing is not a completion this client can
        // read, and it is certainly not a success.
        _ => CheckState::Unrecognized,
    }
}

/// The read that locates a head's checks. Per commit, which is the whole of the
/// correctness argument.
fn check_runs_path(repo: &str, head_sha: &str) -> String {
    format!("/repos/{repo}/commits/{head_sha}/check-runs")
}

/// What CI says about one exact head.
///
/// The sha is not a convenience parameter. A check suite follows a commit, so a
/// green result for a head the branch has since moved past is a green result
/// about a different tree — and the endpoint is addressed by commit precisely so
/// that "is this change verified?" cannot accidentally become "is anything on
/// this branch verified?". Every run that comes back is checked against the sha
/// that was asked for as well, because the answer being *filtered* is GitHub's
/// job and confirming it is *this* client's, the same division
/// [`pulls`](super::pulls) makes.
///
/// Fails closed everywhere. An unreadable CI is not a CI with nothing in it, so
/// every failure — the call, an unreadable envelope, a run about another commit
/// — is [`Observation::Unavailable`] and never an empty
/// [`VerificationState`], which would read as "nothing is failing".
///
/// `required` is matched **by name**. A check nobody required is not consulted,
/// and an unrelated green one satisfies nothing: the iteration below is over the
/// required names rather than over the runs, so a requirement can only be
/// discharged by a run that carries its name.
pub async fn observe_checks(
    gh: &GhCli,
    repo: &str,
    head_sha: &str,
    required: &[String],
    cancel: &CancellationToken,
) -> Observation<VerificationState> {
    let path = check_runs_path(repo, head_sha);
    let source = SourceRef(format!("github:{repo}/commits/{head_sha}/check-runs"));
    let unavailable = |reason: String| Observation::Unavailable {
        source: SourceRef(source.0.clone()),
        reason,
    };

    let response = match gh.api("GET", &path, None, cancel).await {
        Ok(response) => response,
        // The one arm the whole fail-closed rule lives in. There is no 404-to-
        // absence branch here: a commit with no checks answers `200` with an
        // empty `check_runs`, so an error is the source being unreadable.
        Err(error) => return unavailable(format!("the check runs could not be read: {error}")),
    };

    // Checked rather than defaulted. A 200 whose body carries no `check_runs`
    // array is a `gh` answering something this client cannot read, and
    // defaulting it to empty would turn that into "no checks are failing".
    let runs = match response.body["check_runs"].as_array() {
        Some(runs) => runs,
        None => {
            return unavailable(format!(
                "{path} answered {} with no check_runs array",
                response.status
            ))
        }
    };

    let mut observed: Vec<(&str, CheckState)> = Vec::with_capacity(runs.len());
    for run in runs {
        let Some(name) = run["name"].as_str() else {
            return unavailable("a listed check run carried no name".to_string());
        };
        // The endpoint is addressed by commit, so a run about another commit is
        // an answer to a question nobody asked — a proxy, a cache, or a
        // parameter that stopped being honoured. Refused rather than filtered
        // out, because a client that silently dropped it could not tell the
        // difference between "this head has no such check" and "something
        // between here and GitHub is answering about the wrong commit".
        match run["head_sha"].as_str() {
            Some(sha) if sha == head_sha => {}
            Some(sha) => {
                return unavailable(format!(
                    "asked about {head_sha} and was answered a run for {sha}"
                ))
            }
            None => {
                return unavailable(format!("the check run {name:?} carried no head sha"));
            }
        }
        observed.push((
            name,
            classify(
                run["status"].as_str().unwrap_or_default(),
                run["conclusion"].as_str(),
            ),
        ));
    }

    // Over the *required* names, in the order they were required. A run whose
    // name nobody required cannot reach any of these lists, however green.
    let mut state = VerificationState {
        head_sha: head_sha.to_string(),
        required_missing: Vec::new(),
        failed: Vec::new(),
        pending: Vec::new(),
    };
    for name in required {
        let states: Vec<CheckState> = observed
            .iter()
            .filter(|(observed_name, _)| observed_name == name)
            .map(|(_, state)| *state)
            .collect();
        match states.as_slice() {
            [] => state.required_missing.push(name.clone()),
            // One name may be carried by more than one run — a rerun, or two
            // apps reporting under the same name — and the worst of them
            // decides. Anything settled and not a pass is a failure; anything
            // still running is pending; only every run passing is a pass.
            states => {
                if states.iter().any(|s| !s.is_passed() && !s.is_pending()) {
                    state.failed.push(name.clone());
                } else if states.iter().any(|s| s.is_pending()) {
                    state.pending.push(name.clone());
                }
            }
        }
    }

    Observation::Available {
        value: state,
        source,
        // The head the answer is about, so a consumer can tell whether the world
        // moved underneath it without trusting its own memory of what it asked.
        revision: Some(head_sha.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Requesting a check
// ---------------------------------------------------------------------------

/// The run title one effect's workflow run must carry, and the only channel by
/// which the identity comes back.
///
/// `fiddle-` rather than the `fiddle/` [`refs`](super::refs) uses: a run name is
/// free text and this is the exact spelling the live probe verified. The other
/// half of the spelling lives in the target repository's workflow file — see
/// this module's documentation for why that makes it fragile.
pub fn run_name(effect_id: &EffectId) -> String {
    format!("fiddle-{}", effect_id.0)
}

/// The canonical target identity for a check request.
///
/// The workflow and the ref it is dispatched against, because those are what
/// identify *which* run is being asked for. Written here rather than at each
/// call site because it is hashed into the effect identity — and, through it,
/// into the run's name — so two spellings of the same target would be two
/// effects looking for two differently named runs, and a fresh process would
/// dispatch a second one.
pub fn check_request_target(repo: &str, workflow: &str, git_ref: &str) -> String {
    format!("{repo}/actions/workflows/{workflow}@{git_ref}")
}

/// A workflow run that exists, as it was observed to be.
///
/// The `status` is carried because a receipt is read by a person, never because
/// anything here decides from it: the postcondition of *requesting* a check is
/// that the run exists, not that it has finished. What it concluded is
/// [`observe_checks`]'s question, asked against the head rather than against the
/// run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowRun {
    pub id: u64,
    /// The run title, which is where this effect's identity came back from.
    pub name: String,
    pub status: String,
}

impl ObservedState for WorkflowRun {
    type Value = WorkflowRun;

    fn describe(&self) -> String {
        format!(
            "workflow run {} named {:?} is {}",
            self.id, self.name, self.status
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.id.to_string())
    }

    fn into_value(self) -> WorkflowRun {
        self
    }
}

/// Start this run's workflow, or recognise the run that was already started for
/// it.
///
/// The identity is computed here, in [`EnsureCheckRequested::new`], from the
/// same canonical inputs and the same derivation
/// [`Executor`](crate::effect::Executor) uses — no clock, no counter, nothing
/// local — because the lookup happens at step 3, *before* the executor mints the
/// envelope at step 6. A fresh process therefore recomputes the name of the run
/// a previous process may have started before it can look anything up, which is
/// the whole of the recovery.
pub struct EnsureCheckRequested {
    /// `owner/name`, as the API path spells it.
    repo: String,
    /// The workflow's file name or numeric id, as the API path spells it.
    workflow: String,
    /// The ref the workflow is dispatched against.
    git_ref: String,
    /// This effect's identity, recomputed rather than remembered.
    effect_id: EffectId,
}

impl EnsureCheckRequested {
    pub fn new(
        repo: String,
        workflow: String,
        git_ref: String,
        project: &str,
        invocation_ref: &str,
    ) -> Self {
        let effect_id = effect_id(
            project,
            invocation_ref,
            EffectKind::EnsureCheckRequested,
            &check_request_target(&repo, &workflow, &git_ref),
        );
        Self {
            repo,
            workflow,
            git_ref,
            effect_id,
        }
    }

    /// The canonical target identity to propose this effect under.
    pub fn target(&self) -> String {
        check_request_target(&self.repo, &self.workflow, &self.git_ref)
    }

    /// The canonical payload: the whole dispatch request, order-stable.
    pub fn payload(&self) -> String {
        serde_json::json!({
            "inputs": { "fiddle_effect_id": self.effect_id.0 },
            "ref": self.git_ref,
            "repo": self.repo,
            "workflow": self.workflow,
        })
        .to_string()
    }

    /// The title the dispatched run will carry, and the value the listing is
    /// filtered on.
    pub fn run_name(&self) -> String {
        run_name(&self.effect_id)
    }

    /// The listing read that locates this effect's run.
    ///
    /// Narrowed to the ref and to `workflow_dispatch` so the answer is small,
    /// but **the narrowing is not the identity**: another run on the same ref
    /// from the same event is somebody else's work, and the filtering that
    /// decides is on the run's name below.
    fn runs_path(&self) -> String {
        format!(
            "/repos/{}/actions/workflows/{}/runs?branch={}&event=workflow_dispatch",
            self.repo,
            self.workflow,
            encode(&self.git_ref),
        )
    }

    /// The one path in this module that changes anything.
    fn dispatch_path(&self) -> String {
        format!(
            "/repos/{}/actions/workflows/{}/dispatches",
            self.repo, self.workflow
        )
    }

    /// Read one listed run, confirming it is the one that was asked for.
    ///
    /// The confirmation is not ceremony: this value becomes the receipt's
    /// `external_ref`, and after a lost dispatch response it is the *entire*
    /// basis for calling the effect committed — so a run that is not this
    /// effect's must never become the answer.
    fn read(&self, listed: &serde_json::Value) -> Result<WorkflowRun, GhError> {
        let id = listed["id"]
            .as_u64()
            .ok_or_else(|| GhError::Malformed("a listed workflow run carried no id".to_string()))?;
        let name = listed["name"]
            .as_str()
            .ok_or_else(|| GhError::Malformed(format!("the workflow run {id} carried no name")))?
            .to_string();
        if name != self.run_name() {
            return Err(GhError::Malformed(format!(
                "asked for {} and was answered run {id}, which is {name:?}",
                self.run_name()
            )));
        }
        Ok(WorkflowRun {
            id,
            name,
            // Payload, so its absence is not a reason to refuse the run: a run
            // whose status this client cannot read is still this effect's run,
            // and whether it has finished is not this operation's question.
            status: listed["status"].as_str().unwrap_or_default().to_string(),
        })
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsureCheckRequested {
    type State = WorkflowRun;

    /// Unattended.
    ///
    /// A dispatch runs the target repository's own workflow, on a ref this run
    /// published, under that repository's own permissions — it merges nothing
    /// and moves no branch. Deployment may still strengthen this and can never
    /// weaken it; that is [`combine`](fiddle_core::combine)'s rule, not this
    /// method's.
    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    /// Is there already a run for this effect?
    ///
    /// Called twice by the executor — before the dispatch to find out whether it
    /// is needed, and after it to find out whether it happened, which is the
    /// call that resolves a lost 204. Both are the same question, and this
    /// operation has no other way to answer it: the dispatch itself reports
    /// nothing.
    ///
    /// Filtered by **this effect's own name**, never by recency. "The most
    /// recent run on this branch" would attribute somebody else's dispatch to
    /// this effect, and the failure is silent in the worst direction: a run that
    /// is not ours read as ours means the check this run needed was never
    /// requested at all.
    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<WorkflowRun>, GhError> {
        let response = ctx
            .gh
            .api("GET", &self.runs_path(), None, &ctx.cancel)
            .await?;

        // Checked rather than defaulted, for the reason in the module
        // documentation: this listing says absence with `200` and an empty
        // array, so a body that cannot be read is not an absent run — and
        // treating it as one dispatches a second.
        let runs = response.body["workflow_runs"].as_array().ok_or_else(|| {
            GhError::Malformed(format!(
                "{} answered {} with no workflow_runs array",
                self.runs_path(),
                response.status
            ))
        })?;

        let wanted = self.run_name();
        let ours: Vec<&serde_json::Value> = runs
            .iter()
            .filter(|run| run["name"].as_str() == Some(wanted.as_str()))
            .collect();

        match ours.as_slice() {
            [] => Ok(None),
            [one] => self.read(one).map(Some),
            // Two runs for one effect is the state this task exists to prevent,
            // reported rather than resolved by picking one. Reaching it means a
            // dispatch was sent twice — by an older build, by another process,
            // or by a person — and that is something to be told about.
            many => Err(GhError::Duplicate { count: many.len() }),
        }
    }

    /// One `POST .../dispatches`, and the only line here that changes anything.
    ///
    /// The response is discarded, and there is nothing in it to discard: 204, no
    /// body, no run id. What makes the run findable afterwards is the
    /// `fiddle_effect_id` input, echoed by the workflow into its own `run-name`.
    ///
    /// The guard comes first and is load-bearing. This operation computes its
    /// identity in [`EnsureCheckRequested::new`] and the executor computes the
    /// envelope's at step 2; they agree only if both were given the same project
    /// and invocation ref. If they ever disagreed, the run would be *named* by
    /// one identity and *looked up* by the other — so the lookup would find
    /// nothing, forever, and every attempt would dispatch again. Refusing before
    /// the request is the difference between a loud failure and an unbounded
    /// supply of workflow runs.
    async fn apply(
        &self,
        ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        if authorized.effect_id() != &self.effect_id {
            // `NotSent` for its classification, which is the half that matters:
            // nothing was sent, so this is `NotCommitted` and no postcondition
            // read is owed. It was `Malformed` until that variant was corrected to
            // `Unknown` — a guard that refuses before dispatching and a `gh` whose
            // answer could not be read are opposite facts about the world, and one
            // variant cannot carry both.
            return Err(GhError::NotSent(format!(
                "the run would be named {} and looked up as {}; nothing was dispatched",
                run_name(authorized.effect_id()),
                self.run_name(),
            )));
        }
        let body = serde_json::json!({
            "ref": self.git_ref,
            "inputs": { "fiddle_effect_id": self.effect_id.0 },
        });
        ctx.gh
            .api("POST", &self.dispatch_path(), Some(&body), &ctx.cancel)
            .await
            .map(|_response| ())
    }
}
