//! What an effect leaves behind: what was observed, and what was refused.
//!
//! A receipt is deliberately a record of an *observation* rather than of a
//! response. Its `postcondition` and `external_ref` are read back out of the
//! world after the mutation, because the case this whole milestone exists for is
//! the one where the response never arrived — and a receipt assembled from a
//! response would have nothing to say about it.
//!
//! The failures live here beside it for the same reason the outcome does: the
//! interesting thing about an effect that did not happen is *which kind* of not
//! happening it was, and that is a single taxonomy whether the answer is a
//! receipt or an error.

use crate::github::GhError;
use fiddle_core::{EffectId, EffectKind, PayloadHash};

/// Something the world was observed to contain.
///
/// Three methods rather than one, because a receipt needs three different
/// things from an observation and they are not interchangeable: a sentence a
/// person reads, an external reference a later run can look the object up by,
/// and the typed value the calling capability actually wanted.
///
/// `into_value` takes `self` so the typed value is moved out rather than
/// cloned, which is what lets an observation carry something a capability
/// consumes.
pub trait ObservedState {
    /// The typed result the proposing capability asked for.
    type Value;

    /// What is true out there, in terms a person reading a bundle understands.
    fn describe(&self) -> String;

    /// The external revision or reference: a commit sha, a PR number, a run id.
    /// `None` when the object has no stable external name.
    fn reference(&self) -> Option<String>;

    /// The typed value, consuming the observation.
    fn into_value(self) -> Self::Value;
}

/// The verified result of one effect.
///
/// **A receipt is not serialized, and `Serialize` is inert here.** It reaches a
/// published bundle as a *rendered* [`EvidenceRef`](fiddle_core::EvidenceRef) —
/// see `receipt_evidence` in `crate::capability::publish`, which states the
/// case: the bundle's evidence is a list of strings and giving a
/// structured receipt a home in the report schema would widen a published
/// contract. This comment used to claim the opposite, and the two could not both
/// be right.
///
/// The derive is unexercised rather than merely unused, and by more than
/// omission: none of the three `T`s a *run* instantiates it with —
/// `PublishedBranch`, `PullRequest`, `WorkflowRun` — is itself `Serialize`, so
/// the derived bound is unsatisfiable for every receipt production can produce.
/// It stays because the epic's `## Contracts` pins the derive;
/// `docs/BACKLOG.md` records removing it.
///
/// There is no `Deserialize`, and that part was always true: for the same reason
/// [`fiddle_core::PolicyDecision`] has none, nothing reads a receipt back in as
/// an authority. A later run re-derives the identity from canonical inputs and
/// looks at the world again.
#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectReceipt<T> {
    pub effect_id: EffectId,
    pub payload_hash: PayloadHash,
    pub target: String,
    /// Always [`EffectOutcome::Committed`](super::EffectOutcome::Committed) on
    /// every path that builds one: a receipt is the *observed* result, and both
    /// construction sites in [`super`] reach it from a postcondition that was
    /// read back. The other two outcomes leave as an [`EffectError`] instead, and
    /// the field is still the three-valued type because that vocabulary is what a
    /// bundle consumer matches on.
    pub outcome: super::EffectOutcome,
    /// What was observed to be true after the operation, read back rather than
    /// assumed from the response.
    pub postcondition: String,
    /// The external revision or reference: a commit sha, a PR number, a run id.
    pub external_ref: Option<String>,
    pub value: T,
}

/// What repeating the *same* invocation against the *same* deployment document
/// would do to a failure.
///
/// The question [`fiddle_core::RunOutcome::Retryable`] documents as its own
/// test — *would repeating this invocation, once someone has fixed what the
/// reason names, succeed?* — asked of one failure rather than left for a caller
/// to guess from a message. It is the discriminator between exit **11** and exit
/// **20**, and it is a two-valued question because the exit table has exactly
/// two rows for a run that executed and did not complete.
///
/// **"Fixed" is narrower than "somebody could do something about it",** and the
/// codebase already draws the line where this type draws it. A change set
/// carrying a foreign correlation marker is fixable — somebody settles whose
/// change set it is — and `crate::orchestration::concluded` maps it to
/// [`fiddle_core::RunOutcome::Failed`] anyway, because repeating *re-derives the
/// same verdict from the same observation*. That is the test: not whether a
/// human could intervene, but whether the failure is an **obstacle in front of**
/// the request or a **conclusion about** it. Read the loose way, every failure
/// is correctable and exit 20 becomes unreachable — which is the reading that
/// had automation looping on a denied effect forever.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recurrence {
    /// Something incidental got in the way, and the same invocation succeeds
    /// once it is gone: a network comes back, a rate limit lifts, a lost answer
    /// is settled by a read that works. Nothing here contradicts what the
    /// invocation asked for. [`fiddle_core::RunOutcome::Retryable`].
    Correctable,

    /// The same invocation, against the same document, reaches the same answer
    /// — because the answer *is* the conclusion, drawn from inputs that repeat
    /// unchanged. [`fiddle_core::RunOutcome::Failed`], whose promise is exactly
    /// *this will not succeed by being repeated as invoked*.
    Permanent,
}

/// Every way an effect can fail to produce a receipt.
///
/// Each variant carries the [`EffectKind`] because a refusal reaches a person
/// long after the run, and "denied" with no antecedent sends its reader back to
/// the configuration to guess — the same argument [`fiddle_core::PolicyDecision`]
/// makes for carrying a reason.
///
/// [`EffectError::Unresolved`] is the variant that is worth the type. It is not
/// "it failed"; it is "nobody knows, and the read that was supposed to settle it
/// did not". Collapsing it into [`EffectError::Adapter`] would tell a caller a
/// write failed when it may well have landed, and the retry would perform it
/// twice.
///
/// The variants split two ways, and the split is [`EffectError::recurrence`]
/// rather than an ordering of the enum: four of them are permanent under
/// repetition and reach exit 20, two are correctable and reach exit 11.
#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("policy denied {kind:?}: {reason}")]
    PolicyDenied { kind: EffectKind, reason: String },
    /// M2 has no decision channel. Fails closed and names what would satisfy it.
    #[error("{kind:?} requires a human decision, which M3 introduces: {reason}")]
    HumanDecisionRequired { kind: EffectKind, reason: String },
    /// The result was unknown and the postcondition read did not settle it.
    #[error("{kind:?} left an unresolved outcome: {reason}")]
    Unresolved { kind: EffectKind, reason: String },
    /// The envelope was minted for one payload and the operation would have
    /// applied another.
    ///
    /// Its own variant rather than a [`EffectError::PolicyDenied`], because no
    /// policy refused this: the proposal and the operation disagree about what
    /// the request *is*, which is a defect in the caller rather than a rule about
    /// what may be done. The identity is unchanged in this case — that is the
    /// whole reason the payload is hashed separately — so without the digest the
    /// mismatch would arrive looking like ordinary work.
    ///
    /// Both digests are carried because either one alone sends its reader to
    /// recompute the other, and a diagnostic that named only what was refused
    /// could not say what it was refused *against*.
    #[error(
        "{kind:?} was authorized for payload {} and would apply {}; nothing was performed",
        approved.0,
        applying.0
    )]
    PayloadDiverged {
        kind: EffectKind,
        approved: PayloadHash,
        applying: PayloadHash,
    },
    /// More than one object matched where exactly one was the postcondition.
    #[error("{kind:?} found {count} matching objects, expected at most one")]
    DuplicateState { kind: EffectKind, count: usize },
    #[error("adapter failure for {kind:?}: {source}")]
    Adapter { kind: EffectKind, source: GhError },
}

impl EffectError {
    /// Which exit row this failure belongs in, decided per variant and in one
    /// visible table.
    ///
    /// Exhaustive by construction — no wildcard arm — so a seventh variant
    /// cannot be added without its author being made to answer this question.
    /// That is the point: the three permanent refusals below were added by M2
    /// *without* the question being asked, every one of them inherited exit 11
    /// from the arm that catches a capability `Err`, and automation retrying on
    /// 11 looped on a denied effect indefinitely.
    ///
    /// See `docs/technical/decisions/016-a-permanent-refusal-is-not-retryable.md`.
    pub fn recurrence(&self) -> Recurrence {
        match self {
            // A `[github.policy]` rule is a property of the document, and the
            // document is an input to the invocation rather than a thing in its
            // way. Repeating hands `policy::combine` the same pair and gets the
            // same `Deny` back, forever. An operator who edits `fiddle.toml` is
            // not repeating this invocation; they are describing a different
            // deployment and running against that.
            EffectError::PolicyDenied { .. } => Recurrence::Permanent,

            // The same, one step weaker in the document and one step stronger in
            // consequence: `RequireHuman` is a rule that resolves through a
            // decision channel, and M2 has none. Nothing a repeat can reach will
            // answer it, so a repeat re-derives the same requirement.
            //
            // **Not `Suspended`,** which is the shape it will take in M3 and is
            // the wrong word here. `Suspended` says a run is *waiting* — it
            // promises something can arrive and resume it. In M2 nothing can,
            // and a run that exited 10 on a decision no channel exists to make
            // would be telling an operator to wait for something that is never
            // coming. M2's epic contract reserves that row for M3 for exactly
            // this reason; when the channel exists, this arm moves there and the
            // move is a behaviour change with a decision behind it rather than a
            // code quietly meaning something new.
            EffectError::HumanDecisionRequired { .. } => Recurrence::Permanent,

            // A defect in the caller, not a condition in the world: the
            // proposal and the operation disagree about what the request *is*,
            // and they are both this build's own code. Every repeat of this
            // build disagrees identically. Fixing it means shipping a different
            // fiddle.
            EffectError::PayloadDiverged { .. } => Recurrence::Permanent,

            // **Considered separately from the two refusals above, and it lands
            // here by a different argument.** Nothing refused this: two objects
            // matched where the postcondition allows one, so the world holds an
            // ambiguity fiddle is not entitled to resolve — picking the first is
            // precisely what `GhError::Duplicate` exists to have refused.
            //
            // That is `Blocked`'s family rather than a refusal's, and `Blocked ⇒
            // Failed` is already this codebase's rule, argued at length in
            // `crate::orchestration::concluded` for the closest available
            // precedent: a change set carrying a *different* invocation's
            // correlation marker. That case is fixable too — somebody settles
            // whose change set it is — and it is `Failed` anyway, because
            // repeating re-derives the same verdict from the same observation
            // and keeps doing so until a human intervenes in the world. A second
            // pull request open on one head is the same shape, so it gets the
            // same row for the same stated reason.
            EffectError::DuplicateState { .. } => Recurrence::Permanent,

            // The variant the whole milestone is about, and the one place
            // `Retryable` is the *only* honest answer. Nobody knows whether the
            // write landed, and `Unknown` is resolved by reading the world — the
            // first thing a repeat does, at the executor's step 3, before it
            // proposes anything. `fiddle-cli`'s `read_retry` documentation
            // already states this route: exhausting the budget reaches
            // `Unresolved` → `Retryable` → exit 11.
            EffectError::Unresolved { .. } => Recurrence::Correctable,

            // A forge that would not answer, a credential that was refused, a
            // rate limit, a wrapper that printed something unreadable. Every one
            // of them is an obstacle in front of the request rather than an
            // answer to it, and every one satisfies `Retryable`'s test directly:
            // fix what the reason names — the host, the token, the wait — and
            // the same invocation succeeds. Pinned by
            // `exactly_once::an_unreachable_github_publishes_nothing_and_reports_an_unread_forge`.
            EffectError::Adapter { .. } => Recurrence::Correctable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: EffectKind = EffectKind::EnsurePullRequest;

    fn reason() -> String {
        "because".to_string()
    }

    /// **The table, asserted rather than described.** Every variant, both
    /// families, in one place — so a change to any arm shows up as a failing
    /// assertion naming the variant rather than as a silently different exit
    /// code three crates away.
    #[test]
    fn every_effect_failure_declares_which_exit_row_it_belongs_in() {
        let cases: [(&str, EffectError, Recurrence); 6] = [
            (
                "a deployment rule denies the kind",
                EffectError::PolicyDenied {
                    kind: KIND,
                    reason: reason(),
                },
                Recurrence::Permanent,
            ),
            (
                "a decision channel M2 does not have",
                EffectError::HumanDecisionRequired {
                    kind: KIND,
                    reason: reason(),
                },
                Recurrence::Permanent,
            ),
            (
                "the caller's own two halves disagree",
                EffectError::PayloadDiverged {
                    kind: KIND,
                    approved: PayloadHash("a".into()),
                    applying: PayloadHash("b".into()),
                },
                Recurrence::Permanent,
            ),
            (
                "the world holds an ambiguity fiddle may not resolve",
                EffectError::DuplicateState {
                    kind: KIND,
                    count: 2,
                },
                Recurrence::Permanent,
            ),
            (
                "nobody knows, and a read settles it",
                EffectError::Unresolved {
                    kind: KIND,
                    reason: reason(),
                },
                Recurrence::Correctable,
            ),
            (
                "the forge would not answer",
                EffectError::Adapter {
                    kind: KIND,
                    source: GhError::Auth,
                },
                Recurrence::Correctable,
            ),
        ];

        for (what, error, expected) in cases {
            assert_eq!(
                error.recurrence(),
                expected,
                "{what}: {error} was classified {:?}",
                error.recurrence()
            );
        }
    }

    /// The discriminating half. A classification with every variant on one side
    /// would satisfy the table above by accident, and the whole finding is that
    /// exactly that had happened: all six were exit 11.
    #[test]
    fn the_two_families_are_both_inhabited() {
        assert_ne!(
            EffectError::PolicyDenied {
                kind: KIND,
                reason: reason(),
            }
            .recurrence(),
            EffectError::Unresolved {
                kind: KIND,
                reason: reason(),
            }
            .recurrence(),
            "a refused effect and an unsettled one must not share an exit row"
        );
    }
}
