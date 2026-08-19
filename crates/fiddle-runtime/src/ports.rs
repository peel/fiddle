use fiddle_core::{ChangeSetState, Observation, WorkItemState};

pub trait WorkItemPort: Send + Sync {
    fn observe(&self, work_id: &str) -> Observation<WorkItemState>;
}

pub trait ChangePort: Send + Sync {
    fn observe(&self, work_id: &str) -> Observation<ChangeSetState>;
}

#[cfg(any(test, feature = "contract-harness"))]
pub mod contract {
    use super::{ChangePort, WorkItemPort};
    use fiddle_core::Observation;

    pub trait WorkItemWorlds {
        type Port: WorkItemPort;

        fn work_id(&self) -> &str;

        fn origin(&self) -> &str;

        fn source_absent(&self) -> Self::Port;

        fn source_malformed(&self) -> Self::Port;

        fn source_open(&self) -> Self::Port;
    }

    pub trait ChangeWorlds {
        type Port: ChangePort;

        fn work_id(&self) -> &str;

        fn origin(&self) -> &str;

        fn source_absent(&self) -> Self::Port;

        fn source_malformed(&self) -> Self::Port;

        fn source_unmarked(&self) -> Self::Port;

        fn source_marked(&self, marker: &str) -> Self::Port;
    }

    pub fn work_item_port_contract<W: WorkItemWorlds>(worlds: &W) {
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
