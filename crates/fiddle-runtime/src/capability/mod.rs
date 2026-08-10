//! What fiddle can do to the world, and the proof it is allowed to.
//!
//! A capability is the only thing in fiddle that *changes* anything, so the
//! interesting design question is not what it does but what it takes to reach
//! it. Design §4.4 states the rule: `execute` is reached only via
//! [`NextAction::Execute`]. That rule is made structural here rather than
//! enforced by a well-placed `if` — [`Capability::execute`] demands an
//! [`ExecutionGrant`], and the only way to obtain one is to hand
//! [`ExecutionGrant::authorise`] a derivation that said `Execute`. A caller who
//! forgets the check cannot compile, which is a stronger guarantee than a
//! caller who remembers it today.
//!
//! This module holds the contract — the trait, the grant, the failures, and the
//! list of ids this build answers to. Each capability lives in its own child
//! module beside it: [`stub`] holds [`StubMark`], which writes this invocation's
//! correlation key into the fixture change set. It makes no network call, no
//! model call, and no `git` invocation, so the same fixture and the same
//! invocation reference always produce byte-identical output — which is what
//! makes the two-invocation stability proof checkable. [`repair`] holds
//! [`FixtureRepair`], which does the opposite of all three — it calls a model,
//! spawns processes, and branches a git worktree — and is therefore where the
//! question of what may be *believed* becomes sharp. Its answer is stated in
//! that module: the check decides, and the model's account of itself is carried
//! as evidence and consulted nowhere.

pub mod repair;
pub mod stub;

pub use repair::{FixtureRepair, RepairConfig};
pub use stub::StubMark;

use fiddle_core::{AttemptId, CapabilityId, EvidenceRef, NextAction};
use std::path::PathBuf;

/// Every capability this build can execute.
///
/// The single source of the known-id list: the CLI validates `--capability`
/// against it, so a build that gains a capability offers it and names it in a
/// diagnostic without anyone remembering to update a second list.
pub const CAPABILITIES: [CapabilityId; 2] = [fiddle_core::STUB_MARK, fiddle_core::FIXTURE_REPAIR];

/// Proof that a derivation authorised an execution, as part of a named attempt.
///
/// The fields are private and the only constructor is
/// [`ExecutionGrant::authorise`], so a value of this type cannot exist unless
/// some [`NextAction`] was `Execute`. That is the whole point: "the capability
/// is never executed from a blocked derivation" stops being a property of the
/// orchestration's control flow and becomes a property of the types, checkable
/// by the compiler at every call site that will ever exist.
///
/// # Why the attempt id is here
///
/// Because a grant is not "you may do this"; it is "**this attempt** authorises
/// you to do this", and a capability that needs to say which attempt it was has
/// nowhere else to get an honest answer. The alternative — the one this
/// replaced — was for the caller assembling a capability to mint an id of its
/// own and hand it over in the capability's configuration. That produced two
/// real, unique ids that did not name each other:
/// [`crate::orchestration::attempt`] minted the one the journal and the bundle
/// are filed under, `main.rs` minted the one
/// [`FixtureRepair`](repair::FixtureRepair) named its worktree and its evidence
/// after, and `repair:<changed>:<attempt>` therefore pointed at a bundle that
/// did not exist. A reference whose *format* implies a cross-reference that
/// does not hold is worse than one carrying no identifier at all.
///
/// Minting stays where it was — once, in `attempt`, so no caller can hand in a
/// duplicate and collide two bundles on one path. What changed is that the id
/// now *travels* to the capability along the one channel that already means
/// "you are authorised, as part of this run", instead of being minted a second
/// time at the edge.
///
/// No longer `Copy`, because [`AttemptId`] owns a `String`. It is passed by
/// value into [`Capability::execute`] exactly once per execution, so the clone
/// is per-attempt rather than per-call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionGrant {
    capability_id: CapabilityId,
    attempt: AttemptId,
}

impl ExecutionGrant {
    /// A grant for `action` as part of `attempt`, if and only if `action`
    /// authorises an execution.
    ///
    /// `Complete` and `Blocked` yield `None`, and there is no other way in.
    pub fn authorise(action: &NextAction, attempt: &AttemptId) -> Option<Self> {
        match action {
            NextAction::Execute { capability_id } => Some(ExecutionGrant {
                capability_id: *capability_id,
                attempt: attempt.clone(),
            }),
            NextAction::Complete | NextAction::Blocked { .. } => None,
        }
    }

    /// The capability the derivation named.
    pub fn capability_id(&self) -> CapabilityId {
        self.capability_id
    }

    /// The attempt this execution is part of — the same id the journal record
    /// and the published bundle are filed under, so a capability quoting it in
    /// its evidence names a document a reader can go and open.
    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt
    }
}

/// Something fiddle can do that changes the world.
///
/// `async`, because the capabilities this crate is growing towards spend their
/// time waiting: a model turn, a subprocess, a `git` invocation. The one M0
/// capability writes a single file and never yields, so it simply returns
/// immediately — the cost of the signature is paid by the caller's executor,
/// not by the work.
///
/// Boxed by `#[async_trait]` rather than written as a bare `async fn` in the
/// trait, and that is not a stylistic choice. A bare `async fn` in a trait is
/// not object-safe — its return type is per-implementation and unnameable — and
/// [`crate::RunContext`] reaches a capability through a `&dyn Capability`
/// precisely so the orchestration depends on this seam rather than on whichever
/// capability the build happens to ship. `#[async_trait]` erases the future into
/// a `Pin<Box<dyn Future + Send>>`, which keeps the trait object. One allocation
/// per execution, against a call that is about to spawn a process or wait on a
/// model, is not a trade worth losing the seam over.
#[async_trait::async_trait]
pub trait Capability: Send + Sync {
    /// The identity this capability is derived and reported under.
    fn id(&self) -> CapabilityId;

    /// The observable stage a [`ProgressEntry`](fiddle_core::ProgressEntry) for
    /// this capability is filed under.
    ///
    /// # Why the capability names it, and why there is no default
    ///
    /// A published bundle's `stage` is the vocabulary a reader uses to say
    /// *which part of the work this line is about*, so it belongs to whoever
    /// knows the parts. The orchestration does not: it holds a
    /// `&dyn Capability` precisely so it need not know which one it is holding,
    /// and the one thing it must not do is invent a name on the capability's
    /// behalf. It did exactly that until this method existed — a single
    /// `const STAGE: &str = "mark"` in [`crate::orchestration`], which is
    /// [`StubMark`]'s one step — and so a `fixture_repair` run published
    /// `{"capability_id":"fixture_repair","stage":"mark", …}`.
    ///
    /// **Deliberately not defaulted**, unlike [`Capability::receipts`]. That
    /// method defaults to the empty list, which is the neutral value: a
    /// capability with nothing to say about itself says nothing, and no reader
    /// is misled. There is no neutral stage name. Any default would be some
    /// capability's real vocabulary applied to every other one, which is
    /// verbatim the defect above — so the third capability this build gains has
    /// to name its own stage or fail to compile, rather than silently inheriting
    /// the first one's.
    ///
    /// `&'static str` rather than `String`: a stage is a fixed name from a
    /// closed set the implementation knows at compile time, not something
    /// computed per execution.
    fn stage(&self) -> &'static str;

    /// Do the thing, and hand back what a reader can go and check.
    ///
    /// The `grant` argument is not consulted for permission by convention; it
    /// *is* the permission, and an implementation must reject a grant naming a
    /// different capability rather than doing that capability's work.
    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError>;

    /// What this capability observed of its own execution, whether or not that
    /// execution succeeded.
    ///
    /// # Why this is a second method rather than part of `execute`'s return
    ///
    /// Because the interesting case is the failing one. [`Capability::execute`]
    /// returns `Result<EvidenceRef, _>`, so everything it can say about *how* it
    /// ran travels on the `Ok` arm — and an execution that failed is precisely
    /// when an operator most needs to know what it did before it failed. That
    /// gap is not hypothetical: it is what let a repair capability call no tools
    /// at all, for every model, and surface as an ordinary failed check that
    /// nothing outside the process could distinguish from a model that tried and
    /// lost. Widening the return type would close it too, at the cost of
    /// changing every implementation and every call site of the seam the
    /// orchestration is built on. A separate accessor the orchestration consults
    /// on **both** arms closes it without moving anything.
    ///
    /// Defaulted to empty, so a capability with nothing to observe about itself
    /// — [`StubMark`], which writes one file and never yields — is unaffected,
    /// and M0's bundles keep the bytes they have always had.
    ///
    /// Read *after* the execution, which is why it takes `&self` and why an
    /// implementation with something to report needs interior mutability.
    fn receipts(&self) -> Vec<EvidenceRef> {
        Vec::new()
    }
}

/// Why an execution did not produce evidence.
///
/// Every variant names the path or the identity involved, because a capability
/// failure surfaces to an operator as a run outcome's `reason` and a bare
/// "write failed" would leave them nothing to act on.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    /// The grant authorised a different capability than the one asked to run.
    ///
    /// Unreachable through the M0 orchestration, which only ever asks the
    /// capability the derivation named — but the check belongs to the
    /// capability, so that adding a second one cannot make the mismatch
    /// possible without also making it an error.
    #[error("capability `{requested}` was asked to run under a grant for `{granted}`")]
    NotAuthorised {
        granted: CapabilityId,
        requested: CapabilityId,
    },

    /// The change set could not be recorded.
    #[error("could not record the change set at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The capability finished, and the check it is answerable to did not pass.
    ///
    /// The variant that carries this milestone's central rule. `exit_code` is
    /// what decided the outcome; `claimed` is what the model said about itself
    /// and is here *because* it is not consulted — recording a claim beside the
    /// verdict that overruled it is how a reader can see that the two were
    /// different things. Nothing in this crate branches on it, and a future
    /// caller that did would be reintroducing exactly the trust this variant
    /// exists to have removed.
    #[error(
        "the check exited {exit_code}, so nothing was earned \
         (the model claimed completion: {claimed}): {stderr}"
    )]
    CheckFailed {
        claimed: bool,
        exit_code: i32,
        stderr: String,
    },

    /// The workspace the capability needed could not be prepared, used, or
    /// interrogated.
    #[error("the workspace could not be used: {0}")]
    Workspace(#[from] crate::workspace::WorkspaceError),

    /// The bounded attempt produced no report, so there is nothing to verify.
    #[error("the attempt produced no report: {0}")]
    Agent(#[from] crate::agent::AgentError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::STUB_MARK;

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
    const ATTEMPT: &str = "01JQZX0000000000000000000";

    fn grant() -> ExecutionGrant {
        ExecutionGrant::authorise(
            &NextAction::Execute {
                capability_id: STUB_MARK,
            },
            &AttemptId(ATTEMPT.to_string()),
        )
        .expect("an Execute derivation authorises")
    }

    /// **The seam survives becoming async.**
    ///
    /// The regression this guards: a bare `async fn` in a trait is not
    /// object-safe, and [`crate::RunContext`] holds a `&dyn Capability`. The
    /// binding below is spelled out with its type rather than inferred, so a
    /// signature that stopped being object-safe fails to compile here rather
    /// than at the orchestration's call site — which is the assertion.
    #[tokio::test]
    async fn a_capability_is_still_usable_as_a_trait_object() {
        let dir = tempfile::tempdir().unwrap();
        let marking = StubMark::new(dir.path(), "icecube");
        let capability: &dyn Capability = &marking;
        assert_eq!(capability.id(), STUB_MARK);
        assert!(capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .is_ok());
    }

    /// The known-id list is the one source the CLI validates `--capability`
    /// against, so a build that can run a capability has to name it here.
    #[test]
    fn both_capabilities_are_registered() {
        assert_eq!(CAPABILITIES, [STUB_MARK, fiddle_core::FIXTURE_REPAIR]);
    }

    /// The fail-closed rule, stated against the type rather than against a
    /// branch: the two non-executing derivations yield no grant at all, so no
    /// call to `execute` can be written from them.
    #[test]
    fn only_an_execute_derivation_yields_a_grant() {
        let attempt = AttemptId(ATTEMPT.to_string());
        assert_eq!(grant().capability_id(), STUB_MARK);
        assert_eq!(
            ExecutionGrant::authorise(&NextAction::Complete, &attempt),
            None
        );
        assert_eq!(
            ExecutionGrant::authorise(
                &NextAction::Blocked {
                    reason: "unobservable".into()
                },
                &attempt
            ),
            None
        );
    }

    /// A grant carries the attempt it was issued under, so a capability quoting
    /// an attempt id in its evidence quotes the one its bundle is filed under
    /// rather than one it minted for itself.
    #[test]
    fn a_grant_names_the_attempt_it_was_issued_under() {
        assert_eq!(grant().attempt_id(), &AttemptId(ATTEMPT.to_string()));
    }
}
