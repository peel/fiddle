//! The ports through which fiddle observes the world, and the contract every
//! implementation of them must satisfy.
//!
//! A port is the whole of what the pure core is allowed to learn: an
//! implementation reaches outside the process, and what comes back is an
//! [`Observation`], never a bare value and never an error the caller has to
//! interpret. That is what keeps the fail-closed rule enforceable — a port
//! cannot report "nothing was there" when it means "I could not look".
//!
//! M0 ships only fixture-backed implementations ([`crate::stub`]). The
//! [`contract`] module below states the observable contract once, against the
//! *traits*, so a later real adapter proves itself by running the same harness
//! rather than by writing its own tests and hoping they mean the same thing.

use fiddle_core::{ChangeSetState, Observation, WorkItemState};

/// Observes the work item an invocation addresses.
///
/// `Send + Sync` because orchestration is free to hold a port behind a shared
/// reference; an implementation that needs interior mutability owns that
/// problem rather than pushing it onto every caller.
pub trait WorkItemPort: Send + Sync {
    /// Observe the work item identified by `work_id` in this port's source.
    ///
    /// Total by construction: a source that cannot be read yields
    /// [`Observation::Unavailable`] with a reason, not an `Err` and not a
    /// defaulted value.
    fn observe(&self, work_id: &str) -> Observation<WorkItemState>;
}

/// Observes the change set an invocation has produced.
pub trait ChangePort: Send + Sync {
    /// Observe the change set recorded for `work_id` in this port's source.
    ///
    /// A source that is readable but records no change set yields
    /// [`Observation::Available`] with `marker: None` — that is a real
    /// observation. Only an unreadable source yields
    /// [`Observation::Unavailable`].
    fn observe(&self, work_id: &str) -> Observation<ChangeSetState>;
}

/// The reusable, adapter-agnostic port contract.
///
/// The every-milestone gate asks for *adapter contract tests*, not for each
/// adapter's own private tests. So the contract is written once here, in terms
/// of the traits and of worlds an implementation must be able to put itself
/// into — never in terms of files, HTTP, or any particular source. An adapter
/// opts in by implementing [`WorkItemWorlds`] / [`ChangeWorlds`] and calling
/// [`work_item_port_contract`] / [`change_port_contract`] from one `#[test]`.
///
/// Available to other crates behind the `contract-harness` feature, so M1's
/// real adapters — which live outside this crate — run this same harness
/// instead of reinventing it.
#[cfg(any(test, feature = "contract-harness"))]
pub mod contract {
    use super::{ChangePort, WorkItemPort};
    use fiddle_core::Observation;

    /// The worlds a [`WorkItemPort`] implementation must be able to be placed
    /// in for the contract to be checkable against it.
    ///
    /// Each method hands back a port already pointed at that world. How the
    /// world is built is the adapter's business: a fixture directory here, a
    /// stubbed HTTP server or a scratch repository for a real adapter.
    pub trait WorkItemWorlds {
        type Port: WorkItemPort;

        /// The work item every world in this fixture is about.
        fn work_id(&self) -> &str;

        /// The prefix this adapter's source references are namespaced by — the
        /// origin half of `<origin>:<locator>`, such as `stub`.
        fn origin(&self) -> &str;

        /// A world in which the source cannot be reached at all.
        fn source_absent(&self) -> Self::Port;

        /// A world in which the source answers, but with something that is not
        /// a work item.
        fn source_malformed(&self) -> Self::Port;

        /// A world in which the source describes the work item as `open`.
        fn source_open(&self) -> Self::Port;
    }

    /// The worlds a [`ChangePort`] implementation must be able to be placed in.
    pub trait ChangeWorlds {
        type Port: ChangePort;

        /// The work item every world in this fixture is about.
        fn work_id(&self) -> &str;

        /// The prefix this adapter's source references are namespaced by.
        fn origin(&self) -> &str;

        /// A world in which the source cannot be reached at all.
        fn source_absent(&self) -> Self::Port;

        /// A world in which the source answers, but with something that is not
        /// a change set.
        fn source_malformed(&self) -> Self::Port;

        /// A world the source can read and in which no change set is recorded.
        fn source_unmarked(&self) -> Self::Port;

        /// A world in which the recorded change set carries `marker`.
        fn source_marked(&self, marker: &str) -> Self::Port;
    }

    /// Assert the contract every [`WorkItemPort`] implementation must satisfy.
    ///
    /// Panics with the offending observation when it does not, so a caller only
    /// has to wire the worlds up and call this from a single test.
    pub fn work_item_port_contract<W: WorkItemWorlds>(worlds: &W) {
        let id = worlds.work_id();

        // 1. An unreachable source is unobservable — never `Available` with a
        //    defaulted value, and never a silent absence.
        assert_unavailable_with_reason(
            &worlds.source_absent().observe(id),
            worlds.origin(),
            "an unreachable source",
        );

        // 2. A source that answers with nonsense is unobservable for the same
        //    reason: a parse failure is not evidence about the work item.
        assert_unavailable_with_reason(
            &worlds.source_malformed().observe(id),
            worlds.origin(),
            "a malformed source",
        );

        // 3. A readable source yields the value *and* says where it came from.
        match worlds.source_open().observe(id) {
            Observation::Available { value, source, .. } => {
                assert_eq!(
                    value.status, "open",
                    "the port must report the source's own status verbatim"
                );
                assert_origin(&source.0, worlds.origin(), "a readable source");
            }
            other => panic!("a readable source must be Available, got {other:?}"),
        }
    }

    /// Assert the contract every [`ChangePort`] implementation must satisfy.
    pub fn change_port_contract<W: ChangeWorlds>(worlds: &W) {
        let id = worlds.work_id();

        assert_unavailable_with_reason(
            &worlds.source_absent().observe(id),
            worlds.origin(),
            "an unreachable source",
        );
        assert_unavailable_with_reason(
            &worlds.source_malformed().observe(id),
            worlds.origin(),
            "a malformed source",
        );

        // A readable source with nothing recorded is the load-bearing case: it
        // must be `Available` with an empty marker, because "I looked and there
        // is no change set" is knowledge, whereas the two cases above are not.
        match worlds.source_unmarked().observe(id) {
            Observation::Available { value, source, .. } => {
                assert_eq!(
                    value.marker, None,
                    "an unrecorded change set must observe as an absent marker"
                );
                assert_origin(&source.0, worlds.origin(), "a readable source");
            }
            other => panic!("a readable but unmarked source must be Available, got {other:?}"),
        }

        match worlds.source_marked("m0-marker").observe(id) {
            Observation::Available { value, source, .. } => {
                assert_eq!(
                    value.marker.as_deref(),
                    Some("m0-marker"),
                    "the port must report the recorded marker verbatim"
                );
                assert_origin(&source.0, worlds.origin(), "a readable source");
            }
            other => panic!("a recorded change set must be Available, got {other:?}"),
        }
    }

    /// An unobservable source must say so, say why, and still name what it
    /// failed to read.
    fn assert_unavailable_with_reason<T: std::fmt::Debug>(
        observed: &Observation<T>,
        origin: &str,
        world: &str,
    ) {
        match observed {
            Observation::Unavailable { source, reason } => {
                assert!(
                    !reason.trim().is_empty(),
                    "{world} must explain why it could not be observed"
                );
                assert_origin(&source.0, origin, world);
            }
            other => panic!("{world} must be Unavailable, got {other:?}"),
        }
    }

    /// A source reference has to identify its origin, or a payload carrying
    /// several observations cannot say which of them came from where.
    fn assert_origin(source: &str, origin: &str, world: &str) {
        let expected = format!("{origin}:");
        assert!(
            source.starts_with(&expected),
            "{world} must name its origin as `{expected}…`, got `{source}`"
        );
        assert!(
            source.len() > expected.len(),
            "{world} must locate itself within its origin, got `{source}`"
        );
    }
}
