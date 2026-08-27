use crate::identity::CapabilityId;
use crate::observation::{ChangeSetState, Observation, WorkStateView};
use crate::report::EvidenceRef;

pub const STUB_MARK: CapabilityId = CapabilityId("stub_mark");

pub const FIXTURE_REPAIR: CapabilityId = CapabilityId("fixture_repair");

pub const PUBLISH_CHANGE: CapabilityId = CapabilityId("publish_change");

pub const PROPOSE_CHANGE: CapabilityId = CapabilityId("propose_change");

pub const CVE_MITIGATE: CapabilityId = CapabilityId("cve_mitigate");

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAssessment {
    NotStarted {
        evidence: Vec<EvidenceRef>,
    },

    Satisfied {
        evidence: Vec<EvidenceRef>,
    },

    Blocked {
        reason: String,
        evidence: Vec<EvidenceRef>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    Execute { capability_id: CapabilityId },

    Complete,

    Blocked { reason: String },
}

pub fn correlation_key(project: &str, invocation_ref: &str) -> String {
    blake3::hash(format!("{project}\0{invocation_ref}").as_bytes()).to_hex()[..16].to_string()
}

pub fn assess(work: &WorkStateView, expected_marker: &str) -> CapabilityAssessment {
    match (&work.work_item, &work.changes) {
        (Observation::Unavailable { source, reason }, _)
        | (_, Observation::Unavailable { source, reason }) => CapabilityAssessment::Blocked {
            reason: format!("source {source} unavailable: {reason}"),
            evidence: vec![EvidenceRef(source.0.clone())],
        },

        (
            _,
            Observation::Available {
                value,
                source: change_source,
                ..
            },
        ) => {
            let mut evidence: Vec<EvidenceRef> = work
                .work_item
                .source()
                .map(|source| EvidenceRef(source.0.clone()))
                .into_iter()
                .collect();
            evidence.push(EvidenceRef(change_source.0.clone()));

            if work.has_completion_state() {
                decide_on_marker(value, expected_marker, evidence)
            } else {
                CapabilityAssessment::NotStarted { evidence }
            }
        }

        _ => CapabilityAssessment::Blocked {
            reason: "source not applicable to the M0 orchestration".to_string(),
            evidence: vec![],
        },
    }
}

fn decide_on_marker(
    changes: &ChangeSetState,
    expected_marker: &str,
    evidence: Vec<EvidenceRef>,
) -> CapabilityAssessment {
    match &changes.marker {
        None => CapabilityAssessment::NotStarted { evidence },

        Some(marker) if marker == expected_marker => CapabilityAssessment::Satisfied { evidence },

        Some(marker) => CapabilityAssessment::Blocked {
            reason: format!(
                "change set carries marker {marker}, expected {expected_marker}: \
                 the change set was written by a different invocation"
            ),
            evidence,
        },
    }
}

pub fn derive_next(
    work: &WorkStateView,
    expected_marker: &str,
    capability_id: CapabilityId,
) -> NextAction {
    match assess(work, expected_marker) {
        CapabilityAssessment::NotStarted { .. } => NextAction::Execute { capability_id },
        CapabilityAssessment::Satisfied { .. } => NextAction::Complete,
        CapabilityAssessment::Blocked { reason, .. } => NextAction::Blocked { reason },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::{
        ChangeSetState, ProjectedStatus, SourceRef, WorkItemState, WorkState,
    };

    fn view(
        work: Observation<WorkItemState>,
        changes: Observation<ChangeSetState>,
    ) -> WorkStateView {
        WorkStateView::without_publication(work, changes)
    }

    fn avail_work() -> Observation<WorkItemState> {
        Observation::Available {
            value: WorkItemState {
                id: "x".into(),
                status: "open".into(),
                projected_status: None,
            },
            source: SourceRef("stub:work/x.json".into()),
            revision: None,
        }
    }

    fn changes_with(marker: Option<&str>) -> Observation<ChangeSetState> {
        Observation::Available {
            value: ChangeSetState {
                marker: marker.map(str::to_string),
            },
            source: SourceRef("stub:changes/x.json".into()),
            revision: None,
        }
    }

    fn unavailable() -> Observation<ChangeSetState> {
        Observation::Unavailable {
            source: SourceRef("stub:changes/x.json".into()),
            reason: "unreadable".into(),
        }
    }

    fn trackerless_work() -> Observation<WorkItemState> {
        Observation::NotApplicable {
            reason: "a self-discovering run addresses no tracker item".into(),
        }
    }

    #[test]
    fn correlation_key_is_deterministic_and_16_hex_chars() {
        let a = correlation_key("icecube", "beans:fiddle-m0-demo");
        let b = correlation_key("icecube", "beans:fiddle-m0-demo");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(
            a,
            correlation_key("other", "beans:fiddle-m0-demo"),
            "project must vary the key"
        );
        assert_ne!(
            a,
            correlation_key("icecube", "beans:other"),
            "invocation ref must vary the key"
        );
    }

    #[test]
    fn correlation_key_is_pinned_to_a_known_value() {
        assert_eq!(
            correlation_key("icecube", "beans:fiddle-m0-demo"),
            "0e6dc49c903c5338"
        );
    }

    #[test]
    fn unmarked_change_set_is_not_started() {
        assert!(matches!(
            assess(&view(avail_work(), changes_with(None)), "aaaa"),
            CapabilityAssessment::NotStarted { .. }
        ));
    }

    #[test]
    fn matching_marker_is_satisfied() {
        assert!(matches!(
            assess(&view(avail_work(), changes_with(Some("aaaa"))), "aaaa"),
            CapabilityAssessment::Satisfied { .. }
        ));
    }

    #[test]
    fn foreign_marker_is_blocked_not_satisfied() {
        match assess(&view(avail_work(), changes_with(Some("bbbb"))), "aaaa") {
            CapabilityAssessment::Blocked { reason, .. } => {
                assert!(
                    reason.contains("bbbb") && reason.contains("aaaa"),
                    "diagnostic must name both markers: {reason}"
                );
            }
            other => panic!("a foreign marker must never be Satisfied, got {other:?}"),
        }
    }

    #[test]
    fn unavailable_source_is_blocked() {
        assert!(matches!(
            assess(&view(avail_work(), unavailable()), "aaaa"),
            CapabilityAssessment::Blocked { .. }
        ));
    }

    #[test]
    fn an_unavailable_work_item_blocks_too() {
        let unreadable_work = Observation::Unavailable {
            source: SourceRef("stub:work/x.json".into()),
            reason: "unreadable".into(),
        };
        match assess(&view(unreadable_work, changes_with(None)), "aaaa") {
            CapabilityAssessment::Blocked { reason, evidence } => {
                assert!(reason.contains("unavailable"), "got {reason}");
                assert_eq!(evidence, vec![EvidenceRef("stub:work/x.json".into())]);
            }
            other => panic!("an unobservable work item must block, got {other:?}"),
        }
    }

    #[test]
    fn a_not_applicable_work_item_does_not_block() {
        let assessed = assess(&view(trackerless_work(), changes_with(None)), "aaaa");
        let CapabilityAssessment::NotStarted { evidence } = assessed else {
            panic!(
                "a trackerless run has no work item by design; that is not an obstacle, \
                 got {assessed:?}"
            );
        };
        assert_eq!(
            evidence,
            vec![EvidenceRef("stub:changes/x.json".into())],
            "a trackerless verdict cites the change set it read and nothing else"
        );
    }

    #[test]
    fn a_marker_over_a_trackerless_reference_is_not_a_completion() {
        for marker in ["aaaa", "bbbb"] {
            let assessed = assess(
                &view(trackerless_work(), changes_with(Some(marker))),
                "aaaa",
            );
            let CapabilityAssessment::NotStarted { evidence } = assessed else {
                panic!(
                    "a reference that records no completion cannot be completed by a \
                     marker {marker}, got {assessed:?}"
                );
            };
            assert_eq!(
                evidence,
                vec![EvidenceRef("stub:changes/x.json".into())],
                "and it cites the change set it read and nothing else"
            );
        }
    }

    #[test]
    fn whether_a_marker_completes_the_work_depends_on_the_reference() {
        let marked = changes_with(Some("aaaa"));
        assert!(
            matches!(
                assess(&view(avail_work(), marked.clone()), "aaaa"),
                CapabilityAssessment::Satisfied { .. }
            ),
            "a reference that names a work item is accounted for by its marker"
        );
        assert!(
            matches!(
                assess(&view(trackerless_work(), marked), "aaaa"),
                CapabilityAssessment::NotStarted { .. }
            ),
            "and a reference that names none has nothing for a marker to account"
        );
    }

    #[test]
    fn a_trackerless_run_with_an_unreadable_change_set_still_blocks() {
        assert!(
            matches!(
                assess(&view(trackerless_work(), unavailable()), "aaaa"),
                CapabilityAssessment::Blocked { .. }
            ),
            "a trackerless run still needs a change set it can read"
        );
    }

    #[test]
    fn an_assessment_cites_the_sources_it_read() {
        let assessed = assess(&view(avail_work(), changes_with(None)), "aaaa");
        let CapabilityAssessment::NotStarted { evidence } = assessed else {
            panic!("expected NotStarted, got {assessed:?}");
        };
        assert_eq!(
            evidence,
            vec![
                EvidenceRef("stub:work/x.json".into()),
                EvidenceRef("stub:changes/x.json".into()),
            ]
        );
    }

    fn jira_names(state: &WorkState) -> (&'static str, &'static str, &'static str) {
        match state {
            WorkState::Ready => ("10000", "To Do", "To Do"),
            WorkState::InProgress => ("10001", "In Progress", "In Progress"),
            WorkState::InReview => ("10002", "In Review", "In Progress"),
            WorkState::Blocked => ("10003", "Blocked", "In Progress"),
            WorkState::Done => ("10004", "Done", "Done"),
            WorkState::Unknown => ("10005", "Awaiting Triage", "In Progress"),
        }
    }

    fn work_projecting(state: WorkState) -> Observation<WorkItemState> {
        let (id, name, category) = jira_names(&state);
        Observation::Available {
            value: WorkItemState {
                id: "x".into(),
                status: name.into(),
                projected_status: Some(ProjectedStatus {
                    state,
                    jira_status_id: id.into(),
                    jira_status_name: name.into(),
                    jira_status_category: category.into(),
                }),
            },
            source: SourceRef("stub:work/x.json".into()),
            revision: None,
        }
    }

    #[test]
    fn no_projected_work_state_moves_the_assessment_or_the_next_action() {
        for state in [
            WorkState::Ready,
            WorkState::InProgress,
            WorkState::InReview,
            WorkState::Blocked,
            WorkState::Done,
            WorkState::Unknown,
        ] {
            for marker in [None, Some("aaaa"), Some("bbbb")] {
                let projected = view(work_projecting(state.clone()), changes_with(marker));
                let unprojected = view(avail_work(), changes_with(marker));
                assert_eq!(
                    assess(&projected, "aaaa"),
                    assess(&unprojected, "aaaa"),
                    "a projection is not an input to this decision: `assess` branches on \
                     observation availability and on the change-set marker, so a projected \
                     {state:?} under marker {marker:?} must not move its verdict"
                );
                assert_eq!(
                    derive_next(&projected, "aaaa", STUB_MARK),
                    derive_next(&unprojected, "aaaa", STUB_MARK),
                    "and the action derived from that verdict must not move either"
                );
            }
        }
    }

    #[test]
    fn derive_next_maps_each_assessment() {
        assert_eq!(
            derive_next(&view(avail_work(), changes_with(None)), "aaaa", STUB_MARK),
            NextAction::Execute {
                capability_id: STUB_MARK
            }
        );
        assert_eq!(
            derive_next(
                &view(avail_work(), changes_with(Some("aaaa"))),
                "aaaa",
                STUB_MARK
            ),
            NextAction::Complete
        );
        assert!(matches!(
            derive_next(&view(avail_work(), unavailable()), "aaaa", STUB_MARK),
            NextAction::Blocked { .. }
        ));
    }

    #[test]
    fn the_derivation_names_the_capability_it_was_asked_about() {
        let unmarked = view(avail_work(), changes_with(None));
        assert_eq!(
            derive_next(&unmarked, "expected", FIXTURE_REPAIR),
            NextAction::Execute {
                capability_id: FIXTURE_REPAIR
            },
            "a derivation must name the capability under consideration, not a hardcoded one"
        );
        assert_eq!(
            derive_next(&unmarked, "expected", STUB_MARK),
            NextAction::Execute {
                capability_id: STUB_MARK
            }
        );
    }

    #[test]
    fn the_capability_does_not_change_the_other_two_verdicts() {
        let satisfied = view(avail_work(), changes_with(Some("expected")));
        assert_eq!(
            derive_next(&satisfied, "expected", FIXTURE_REPAIR),
            NextAction::Complete
        );
        assert_eq!(
            derive_next(&satisfied, "expected", STUB_MARK),
            derive_next(&satisfied, "expected", FIXTURE_REPAIR),
            "a satisfied world completes whoever asked"
        );

        let blocked = view(avail_work(), unavailable());
        assert!(matches!(
            derive_next(&blocked, "expected", FIXTURE_REPAIR),
            NextAction::Blocked { .. }
        ));
        assert_eq!(
            derive_next(&blocked, "expected", STUB_MARK),
            derive_next(&blocked, "expected", FIXTURE_REPAIR),
            "an unobservable world blocks whoever asked, with the same reason"
        );
    }

    #[test]
    fn derive_next_blocks_on_a_foreign_marker() {
        let action = derive_next(
            &view(avail_work(), changes_with(Some("bbbb"))),
            "aaaa",
            STUB_MARK,
        );
        let NextAction::Blocked { reason } = action else {
            panic!("a foreign marker must block, got {action:?}");
        };
        assert!(
            reason.contains("bbbb") && reason.contains("aaaa"),
            "{reason}"
        );
    }

    #[test]
    fn assessments_and_actions_serialize_under_their_variant_names() {
        let not_started =
            serde_json::to_value(assess(&view(avail_work(), changes_with(None)), "aaaa")).unwrap();
        assert!(not_started["not_started"]["evidence"].is_array());

        assert_eq!(
            serde_json::to_value(NextAction::Execute {
                capability_id: STUB_MARK
            })
            .unwrap(),
            serde_json::json!({ "execute": { "capability_id": "stub_mark" } })
        );
        assert_eq!(
            serde_json::to_value(NextAction::Complete).unwrap(),
            serde_json::json!("complete")
        );
        assert_eq!(
            serde_json::to_value(NextAction::Blocked {
                reason: "why".into()
            })
            .unwrap(),
            serde_json::json!({ "blocked": { "reason": "why" } })
        );
    }
}
