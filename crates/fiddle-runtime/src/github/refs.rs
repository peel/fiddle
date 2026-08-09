//! One branch, published under a name a fresh process recomputes.
//!
//! This is the first [`IntegrationOperation`], and it is the one the milestone's
//! reduction rests on. §5.5 of the design cut a `Fiddle-Effect-Id` commit
//! trailer, an ownership verdict table and a pinned committer timestamp on a
//! single argument: **`git push` to a named ref is already idempotent**. The
//! same commit twice is a no-op, and a different commit is refused as a
//! non-fast-forward. What is left for this module to get right is not machinery
//! but *interpretation*, and there are three pieces of it.
//!
//! # The name is derived, not remembered
//!
//! [`branch_name`] runs the effect identity's own derivation over the run's
//! canonical inputs. Nothing local is consulted, so the process that has to
//! recognise a branch a previous process may have pushed computes the same name
//! before it can look anything up — which is the whole of the recovery. A second
//! hash is deliberately not invented: an identity and the ref it names must not
//! be able to drift apart.
//!
//! # A 404 is knowledge
//!
//! [`Observation::Unavailable`](fiddle_core::Observation::Unavailable) is never
//! equivalent to empty — M0's fail-closed rule — and this module is where that
//! rule meets the GitHub boundary. "I asked for the ref and GitHub said it is
//! not there" is a *read that succeeded*, and it is the only state that licenses
//! a push. A source that could not be read is the other thing entirely, and it
//! stays an error so the executor never mistakes an outage for an empty remote
//! and pushes over the top of it.
//!
//! # A divergent ref is git's judgment, not this module's
//!
//! [`EnsureBranchPublished::inspect`] answers `Ok(None)` for a ref that exists
//! at some *other* commit, which sends the executor on to the push. That looks
//! like the module declining to notice a conflict; it is the opposite. Ancestry
//! is git's to decide and not ours — we do not have the remote's history in
//! hand — so the question is put to the one participant that can answer it, and
//! the non-fast-forward that comes back is reported and never forced.
//! [`GitCli::publish`](crate::git::GitCli::publish) has no parameter that could
//! ask for a force, which is what makes "reported and never forced" a fact about
//! the command line rather than about this branch of this match.

use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::git::PublishedBranch;
use crate::github::GhError;
use fiddle_core::{effect_id, EffectKind, HumanDecisionRequirement};

/// The namespace every branch fiddle publishes lives under.
///
/// A prefix rather than a bare digest, so a person looking at a repository's
/// branch list can see whose it is without consulting anything. It is also what
/// keeps the deterministic name out of the space a human might have used: a
/// branch under `fiddle/` is fiddle's by construction.
const NAMESPACE: &str = "fiddle";

/// The deterministic remote locator for one run's published work.
///
/// [`EffectId`](fiddle_core::EffectId)'s own derivation is reused rather than a
/// second hash invented, so the name and the identity cannot drift apart under a
/// later change to either. The digest is `blake3` over a length-prefixed
/// encoding of the canonical inputs, rendered as 16 hex characters — see
/// [`effect_id`] for why the framing is length-prefixed and not separated.
///
/// **Why the kind's `target` slot carries the project.** A branch's name is
/// per-run, not per-target: the design's §6.6 says the name comes from
/// `(project, invocation_ref)`, and it has to, because the branch *is* the
/// target and a name derived from its own target would be circular. Filling the
/// slot with the project keeps the derivation total and injective — the encoding
/// is injective for every tuple, including one with a repeated field — while
/// leaving the *effect's* identity, which is derived over the real target
/// `refs/heads/<branch>`, distinct from the name it produces.
///
/// # Git ref syntax
///
/// The result is `fiddle/` followed by 16 lowercase hex characters, and that is
/// a valid ref name for every possible input rather than for the well-behaved
/// ones: a hex digest cannot contain `..`, cannot end in `.lock`, cannot begin
/// with `-` or `.`, and cannot carry a `+` or a `:` that would change what a
/// refspec means. The property is structural, not conventional, which is why
/// this function needs no rejection path and returns a `String` rather than a
/// `Result`. [`crate::git::publish`]'s own boundary check still refuses names it
/// is handed by anyone else.
pub fn branch_name(project: &str, invocation_ref: &str) -> String {
    let id = effect_id(
        project,
        invocation_ref,
        EffectKind::EnsureBranchPublished,
        project,
    );
    format!("{NAMESPACE}/{}", id.0)
}

/// The canonical target identity for a branch effect.
///
/// Written here rather than spelled at each call site, because it is hashed into
/// the effect identity: two spellings of the same target would be two effects,
/// and a fresh process would fail to recognise work it had really done.
pub fn branch_target(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

/// A branch that exists on the remote, pointing where this run intends.
///
/// The `sha` is what the remote was *observed* to hold, never what the push
/// reported: the case this milestone exists for is the one where the push's
/// answer never arrived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRef {
    pub branch: String,
    pub sha: String,
}

impl ObservedState for BranchRef {
    type Value = PublishedBranch;

    fn describe(&self) -> String {
        format!("refs/heads/{} points at {}", self.branch, self.sha)
    }

    fn reference(&self) -> Option<String> {
        Some(self.sha.clone())
    }

    fn into_value(self) -> PublishedBranch {
        PublishedBranch {
            branch: self.branch,
            sha: self.sha,
        }
    }
}

/// Publish this run's work to its deterministic branch.
///
/// `intended_sha` is supplied rather than read here, and that is deliberate:
/// the commit being published is the attempt's business, and an operation that
/// resolved `HEAD` for itself could publish a different commit from the one the
/// caller proposed — with the payload hash still matching, because the payload
/// would never have named it.
pub struct EnsureBranchPublished {
    /// `owner/name`, as the API path spells it.
    repo: String,
    /// The branch, without `refs/heads/`.
    branch: String,
    /// The commit the branch must point at for the postcondition to hold.
    intended_sha: String,
}

impl EnsureBranchPublished {
    pub fn new(repo: String, branch: String, intended_sha: String) -> Self {
        Self {
            repo,
            branch,
            intended_sha,
        }
    }

    /// The branch this operation publishes.
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// The canonical target identity to propose this effect under.
    pub fn target(&self) -> String {
        branch_target(&self.branch)
    }

    /// The single-ref read. `/git/ref/heads/<branch>` and not `/git/refs/...`:
    /// the plural form answers a *prefix* match with an array, which would make
    /// "is this ref there?" a question about how many things came back.
    fn ref_path(&self) -> String {
        format!("/repos/{}/git/ref/heads/{}", self.repo, self.branch)
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsureBranchPublished {
    type State = BranchRef;

    /// Unattended.
    ///
    /// The name is namespaced and derived from this run's own inputs, so the
    /// only ref this operation can touch is one this run owns, and a push to it
    /// cannot fast-forward over anyone else's work. Deployment may still
    /// strengthen this to a human decision and can never weaken it — that is
    /// [`combine`](fiddle_core::combine)'s rule, not this method's.
    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    /// Does the remote already hold this branch, at this commit?
    ///
    /// Called twice by the executor — before the push to find out whether it is
    /// needed, and after it to find out whether it happened — and the three
    /// answers it can give are what the whole operation turns on.
    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<BranchRef>, GhError> {
        let response = match ctx.gh.api("GET", &self.ref_path(), None, &ctx.cancel).await {
            Ok(response) => response,
            // The ref is not there. A read that succeeded and returned an
            // absence, which is knowledge and not a failure to look — this is
            // the only state that licenses a push, and reporting it as an error
            // would fail closed on the ordinary first run.
            Err(GhError::Http { status: 404, .. }) => return Ok(None),
            // Everything else is the source being unreadable. It stays an error
            // all the way up, because an outage that arrived looking like an
            // empty remote is how a second branch gets pushed.
            Err(error) => return Err(error),
        };

        // Checked rather than defaulted. A 200 whose body carries no object sha
        // is not an absent ref; it is a `gh` that answered something this client
        // cannot read, and defaulting it to an empty string would turn that into
        // "present but divergent" and send us to push against a remote whose
        // state we do not actually know.
        let sha = response.body["object"]["sha"]
            .as_str()
            .ok_or_else(|| {
                GhError::Malformed(format!(
                    "{} answered {} with no object sha",
                    self.ref_path(),
                    response.status
                ))
            })?
            .to_string();

        match sha == self.intended_sha {
            true => Ok(Some(BranchRef {
                branch: self.branch.clone(),
                sha,
            })),
            // Present, but pointing at work this run did not do. Not this
            // postcondition, so the executor goes on to the push — where git,
            // which owns ancestry, refuses the non-fast-forward. See the module
            // documentation for why that judgment is not made here.
            false => Ok(None),
        }
    }

    /// One `git push`, and the only line in this operation that changes anything.
    ///
    /// The published sha is deliberately discarded: it is what `git` said, and
    /// the executor's next act is to read the remote back. A receipt assembled
    /// from this value would be a receipt for a response rather than for an
    /// observation, which is the thing step 8 exists to prevent.
    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        ctx.git
            .publish(&ctx.work, &self.branch, &ctx.cancel)
            .await
            .map(|_published| ())
            .map_err(GhError::Push)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name is `fiddle/` plus the identity's own digest, pinned to a value
    /// computed outside this crate —
    ///
    /// ```text
    /// printf '11:acme/widget9:beans:w-123:ensure_branch_published11:acme/widget' | b3sum
    /// ```
    ///
    /// — so this pins the *definition* rather than whatever the implementation
    /// happens to compute today. That is what the recovery actually rests on: a
    /// later process is a later build too, and a name that moved between builds
    /// would fail to find a branch that had really been pushed, and push a
    /// second one.
    #[test]
    fn the_branch_name_is_pinned_to_the_identity_derivation() {
        assert_eq!(
            branch_name("acme/widget", "beans:w-1"),
            "fiddle/6d5aa806964432bc"
        );
    }

    /// The target is what the identity is derived over, so its spelling is a
    /// contract and not a formatting choice.
    #[test]
    fn the_target_is_the_full_ref() {
        assert_eq!(branch_target("fiddle/abc"), "refs/heads/fiddle/abc");
    }
}
