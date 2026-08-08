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
//! makes the two-invocation stability proof checkable.

pub mod stub;

pub use stub::StubMark;

use fiddle_core::{CapabilityId, EvidenceRef, NextAction};
use std::path::PathBuf;

/// Every capability this build can execute.
///
/// The single source of the known-id list: the CLI validates `--capability`
/// against it, so a build that gains a capability offers it and names it in a
/// diagnostic without anyone remembering to update a second list.
pub const CAPABILITIES: [CapabilityId; 2] = [fiddle_core::STUB_MARK, fiddle_core::FIXTURE_REPAIR];

/// Proof that a derivation authorised an execution.
///
/// The field is private and the only constructor is [`ExecutionGrant::authorise`],
/// so a value of this type cannot exist unless some [`NextAction`] was
/// `Execute`. That is the whole point: "the capability is never executed from a
/// blocked derivation" stops being a property of the orchestration's control
/// flow and becomes a property of the types, checkable by the compiler at every
/// call site that will ever exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionGrant {
    capability_id: CapabilityId,
}

impl ExecutionGrant {
    /// A grant for `action`, if and only if it authorises an execution.
    ///
    /// `Complete` and `Blocked` yield `None`, and there is no other way in.
    pub fn authorise(action: &NextAction) -> Option<Self> {
        match action {
            NextAction::Execute { capability_id } => Some(ExecutionGrant {
                capability_id: *capability_id,
            }),
            NextAction::Complete | NextAction::Blocked { .. } => None,
        }
    }

    /// The capability the derivation named.
    pub fn capability_id(self) -> CapabilityId {
        self.capability_id
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::STUB_MARK;

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

    fn grant() -> ExecutionGrant {
        ExecutionGrant::authorise(&NextAction::Execute {
            capability_id: STUB_MARK,
        })
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
        assert_eq!(grant().capability_id(), STUB_MARK);
        assert_eq!(ExecutionGrant::authorise(&NextAction::Complete), None);
        assert_eq!(
            ExecutionGrant::authorise(&NextAction::Blocked {
                reason: "unobservable".into()
            }),
            None
        );
    }
}
