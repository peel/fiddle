use fiddle_core::{ChangeSetState, Observation, WorkItemState};
use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
pub trait WorkItemPort: Send + Sync {
    async fn observe(
        &self,
        work_id: &str,
        cancel: &CancellationToken,
    ) -> Observation<WorkItemState>;
}

#[async_trait::async_trait]
pub trait ChangePort: Send + Sync {
    async fn observe(
        &self,
        work_id: &str,
        cancel: &CancellationToken,
    ) -> Observation<ChangeSetState>;
}

#[cfg(any(test, feature = "contract-harness"))]
pub mod contract {
    use super::{ChangePort, WorkItemPort};
    use fiddle_core::Observation;
    use tokio_util::sync::CancellationToken;

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

    pub async fn work_item_port_contract<W: WorkItemWorlds>(worlds: &W) {
        let id = worlds.work_id();
        let running = CancellationToken::new();

        assert_unavailable_with_reason(
            &worlds.source_absent().observe(id, &running).await,
            worlds.origin(),
            "an unreachable source",
        );

        assert_unavailable_with_reason(
            &worlds.source_malformed().observe(id, &running).await,
            worlds.origin(),
            "a malformed source",
        );

        assert_unavailable_with_reason(
            &worlds.source_open().observe(id, &cancelled()).await,
            worlds.origin(),
            "a readable source a cancelled run reads",
        );

        match worlds.source_open().observe(id, &running).await {
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

    pub async fn change_port_contract<W: ChangeWorlds>(worlds: &W) {
        let id = worlds.work_id();
        let running = CancellationToken::new();

        assert_unavailable_with_reason(
            &worlds.source_absent().observe(id, &running).await,
            worlds.origin(),
            "an unreachable source",
        );
        assert_unavailable_with_reason(
            &worlds.source_malformed().observe(id, &running).await,
            worlds.origin(),
            "a malformed source",
        );

        assert_unavailable_with_reason(
            &worlds
                .source_marked("m0-marker")
                .observe(id, &cancelled())
                .await,
            worlds.origin(),
            "a readable source a cancelled run reads",
        );

        assert_unavailable_with_reason(
            &worlds.source_unmarked().observe(id, &cancelled()).await,
            worlds.origin(),
            "an unmarked source a cancelled run reads",
        );

        match worlds.source_unmarked().observe(id, &running).await {
            Observation::Available { value, source, .. } => {
                assert_eq!(
                    value.marker, None,
                    "an unrecorded change set must observe as an absent marker"
                );
                assert_origin(&source.0, worlds.origin(), "a readable source");
            }
            other => panic!("a readable but unmarked source must be Available, got {other:?}"),
        }

        match worlds
            .source_marked("m0-marker")
            .observe(id, &running)
            .await
        {
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

    fn cancelled() -> CancellationToken {
        let token = CancellationToken::new();
        token.cancel();
        token
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

#[cfg(test)]
mod tests {
    use super::WorkItemPort;
    use fiddle_core::{Observation, SourceRef, WorkItemState};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    #[derive(Default)]
    struct AnswersDifferentlyEachTime {
        reads: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl WorkItemPort for AnswersDifferentlyEachTime {
        async fn observe(
            &self,
            work_id: &str,
            _cancel: &CancellationToken,
        ) -> Observation<WorkItemState> {
            let read = self.reads.fetch_add(1, Ordering::SeqCst);
            Observation::Available {
                value: WorkItemState {
                    id: work_id.to_string(),
                    status: format!("read-{read}"),
                    projected_status: None,
                },
                source: SourceRef(format!("test:{work_id}")),
                revision: None,
            }
        }
    }

    #[derive(Default)]
    struct RepeatsItsFirstAnswer {
        world: AnswersDifferentlyEachTime,
        remembered: Mutex<Option<WorkItemState>>,
    }

    #[async_trait::async_trait]
    impl WorkItemPort for RepeatsItsFirstAnswer {
        async fn observe(
            &self,
            work_id: &str,
            cancel: &CancellationToken,
        ) -> Observation<WorkItemState> {
            let remembered = self.remembered.lock().unwrap().clone();
            let value = match remembered {
                Some(value) => value,
                None => {
                    let observed = self.world.observe(work_id, cancel).await;
                    let value = observed
                        .value()
                        .expect("the memoised world always answers")
                        .clone();
                    *self.remembered.lock().unwrap() = Some(value.clone());
                    value
                }
            };
            Observation::Available {
                value,
                source: SourceRef(format!("test:{work_id}")),
                revision: None,
            }
        }
    }

    #[tokio::test]
    async fn a_second_observation_reads_the_world_again() {
        let port = AnswersDifferentlyEachTime::default();
        let running = CancellationToken::new();
        let first = port.observe("IDENT-1", &running).await;
        let second = port.observe("IDENT-1", &running).await;
        assert_ne!(
            first.value().map(|v| v.status.as_str()),
            second.value().map(|v| v.status.as_str()),
            "re-derivation must read the source again, never a cached first answer"
        );
        assert_eq!(
            port.reads.load(Ordering::SeqCst),
            2,
            "two observations must be two reads of the world"
        );
    }

    #[tokio::test]
    async fn a_port_that_answers_from_its_first_read_fails_that_comparison() {
        let port = RepeatsItsFirstAnswer::default();
        let running = CancellationToken::new();
        let first = port.observe("IDENT-1", &running).await;
        let second = port.observe("IDENT-1", &running).await;
        assert_eq!(
            first.value().map(|v| v.status.as_str()),
            second.value().map(|v| v.status.as_str()),
            "a memoising port answers the same twice, so the comparison above is not vacuous"
        );
        assert_eq!(
            port.world.reads.load(Ordering::SeqCst),
            1,
            "a memoising port reads the world once"
        );
    }
}
