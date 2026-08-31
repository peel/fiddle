#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct SourceRef(pub String);

impl std::fmt::Display for SourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation<T> {
    Available {
        value: T,
        source: SourceRef,
        revision: Option<String>,
    },

    Unavailable {
        source: SourceRef,
        reason: String,
    },

    NotApplicable {
        reason: String,
    },
}

impl<T> Observation<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Observation::Available { value, .. } => Some(value),
            Observation::Unavailable { .. } | Observation::NotApplicable { .. } => None,
        }
    }

    pub fn source(&self) -> Option<&SourceRef> {
        match self {
            Observation::Available { source, .. } | Observation::Unavailable { source, .. } => {
                Some(source)
            }
            Observation::NotApplicable { .. } => None,
        }
    }

    pub fn is_unavailable(&self) -> bool {
        matches!(self, Observation::Unavailable { .. })
    }
}

#[derive(
    Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, fiddle_macros::VariantCount,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Ready,
    InProgress,
    InReview,
    Blocked,
    Done,
    Unknown,
}

impl WorkState {
    pub const ALL: [WorkState; WorkState::VARIANT_COUNT] = [
        WorkState::Ready,
        WorkState::InProgress,
        WorkState::InReview,
        WorkState::Blocked,
        WorkState::Done,
        WorkState::Unknown,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectedStatus {
    pub state: WorkState,
    pub jira_status_id: String,
    pub jira_status_name: String,
    pub jira_status_category: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkItemState {
    pub id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_status: Option<ProjectedStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeSetState {
    pub marker: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReviewState {
    pub branch: Option<String>,
    pub pull_request: Option<u64>,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VerificationState {
    pub head_sha: String,
    pub required_missing: Vec<String>,
    pub failed: Vec<String>,
    pub pending: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Publication {
    pub review: Observation<ReviewState>,
    pub verification: Observation<VerificationState>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TreeObservation {
    pub base_revision: String,
    pub pr_head: Option<String>,
    pub attempt_tree: String,
    pub scanned_image_digest: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct WorkStateView {
    pub work_item: Observation<WorkItemState>,
    pub changes: Observation<ChangeSetState>,
    pub review: Observation<ReviewState>,
    pub verification: Observation<VerificationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeObservation>,
}

impl WorkStateView {
    pub fn without_publication(
        work_item: Observation<WorkItemState>,
        changes: Observation<ChangeSetState>,
    ) -> Self {
        WorkStateView {
            work_item,
            changes,
            review: Observation::NotApplicable {
                reason: "no forge was consulted, so no pull request is expected".to_string(),
            },
            verification: Observation::NotApplicable {
                reason: "no forge was consulted, so no checks are expected".to_string(),
            },
            tree: None,
        }
    }

    pub fn with_publication(
        work_item: Observation<WorkItemState>,
        changes: Observation<ChangeSetState>,
        publication: Publication,
    ) -> Self {
        WorkStateView {
            work_item,
            changes,
            review: publication.review,
            verification: publication.verification,
            tree: None,
        }
    }

    pub fn at_revision(mut self, tree: Option<TreeObservation>) -> Self {
        self.tree = tree;
        self
    }

    pub fn has_completion_state(&self) -> bool {
        !matches!(self.work_item, Observation::NotApplicable { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_item() -> WorkItemState {
        WorkItemState {
            id: "fiddle-m0-demo".to_string(),
            status: "open".to_string(),
            projected_status: None,
            revision: None,
        }
    }

    #[test]
    fn an_available_observation_serializes_under_its_variant_name() {
        let observed = Observation::Available {
            value: work_item(),
            source: SourceRef("stub:work/fiddle-m0-demo.json".to_string()),
            revision: None,
        };
        assert_eq!(
            serde_json::to_string(&observed).unwrap(),
            r#"{"available":{"value":{"id":"fiddle-m0-demo","status":"open"},"source":"stub:work/fiddle-m0-demo.json","revision":null}}"#,
            "these are M0's bytes: a field nothing projected onto contributes none of them"
        );
    }

    #[test]
    fn a_projected_status_appears_only_when_one_was_projected() {
        let unprojected = serde_json::to_value(work_item()).unwrap();
        assert!(
            unprojected.get("projected_status").is_none(),
            "a work item nobody projected onto must not carry the key: {unprojected}"
        );

        let mut value = work_item();
        value.projected_status = Some(ProjectedStatus {
            state: WorkState::InReview,
            jira_status_id: "10001".into(),
            jira_status_name: "Awaiting Security Review".into(),
            jira_status_category: "In Progress".into(),
        });
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(
            json["projected_status"]["state"],
            serde_json::json!("in_review")
        );
        assert_eq!(
            json["projected_status"]["jira_status_id"],
            serde_json::json!("10001")
        );
        assert_eq!(
            json["projected_status"]["jira_status_name"],
            serde_json::json!("Awaiting Security Review")
        );
        assert_eq!(
            json["projected_status"]["jira_status_category"],
            serde_json::json!("In Progress")
        );
    }

    #[test]
    fn a_projected_status_survives_the_document_it_was_written_to() {
        let mut value = work_item();
        value.projected_status = Some(ProjectedStatus {
            state: WorkState::Blocked,
            jira_status_id: "10004".into(),
            jira_status_name: "Waiting on Vendor".into(),
            jira_status_category: "In Progress".into(),
        });
        let written = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<WorkItemState>(&written).unwrap(),
            value
        );
    }

    #[test]
    fn a_work_item_written_before_a_tracker_was_read_still_reads() {
        let before_jira = r#"{"id":"fiddle-m0-demo","status":"open"}"#;
        assert_eq!(
            serde_json::from_str::<WorkItemState>(before_jira).unwrap(),
            work_item()
        );
    }

    #[test]
    fn an_unavailable_observation_carries_a_reason_and_no_value() {
        let observed: Observation<WorkItemState> = Observation::Unavailable {
            source: SourceRef("stub:work/fiddle-m0-demo.json".to_string()),
            reason: "stub source unreadable".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&observed).unwrap();
        assert!(json.get("available").is_none());
        assert_eq!(json["unavailable"]["reason"], "stub source unreadable");
        assert_eq!(observed.value(), None);
        assert!(observed.is_unavailable());
    }

    #[test]
    fn a_not_applicable_observation_names_no_source() {
        let observed: Observation<ChangeSetState> = Observation::NotApplicable {
            reason: "no change set is expected for this invocation".to_string(),
        };
        assert_eq!(observed.source(), None);
        assert!(!observed.is_unavailable());
    }

    #[test]
    fn the_view_gains_two_observations_without_moving_the_existing_two() {
        let view = WorkStateView {
            work_item: Observation::Available {
                value: work_item(),
                source: SourceRef("stub:work/fiddle-m0-demo.json".to_string()),
                revision: None,
            },
            changes: Observation::Available {
                value: ChangeSetState { marker: None },
                source: SourceRef("stub:changes/fiddle-m0-demo.json".to_string()),
                revision: None,
            },
            review: Observation::NotApplicable {
                reason: "no pull request is expected".to_string(),
            },
            verification: Observation::NotApplicable {
                reason: "no checks are expected".to_string(),
            },
            tree: None,
        };

        let json: serde_json::Value = serde_json::to_value(&view).unwrap();
        for key in ["work_item", "changes", "review", "verification"] {
            assert!(json.get(key).is_some(), "{key} must be present");
        }
        assert!(
            json.get("tree").is_none(),
            "a view with no worktree behind it must not carry the key at all: {json}"
        );
        assert_eq!(json["work_item"]["available"]["value"]["status"], "open");
        assert!(json["changes"]["available"].is_object());
        assert!(json["changes"]["available"]["value"]["marker"].is_null());
    }

    #[test]
    fn only_a_reference_that_names_no_work_item_has_no_completion_state() {
        let over = |work_item| {
            WorkStateView::without_publication(
                work_item,
                Observation::Available {
                    value: ChangeSetState { marker: None },
                    source: SourceRef("stub:changes/x.json".to_string()),
                    revision: None,
                },
            )
            .has_completion_state()
        };

        assert!(
            over(Observation::Available {
                value: work_item(),
                source: SourceRef("stub:work/fiddle-m0-demo.json".to_string()),
                revision: None,
            }),
            "a reference that names a work item is accounted for by its change set"
        );
        assert!(
            over(Observation::Unavailable {
                source: SourceRef("stub:work/fiddle-m0-demo.json".to_string()),
                reason: "unreadable".to_string(),
            }),
            "and it still names one when the tracker could not be read"
        );
        assert!(
            !over(Observation::NotApplicable {
                reason: "this invocation names no work item".to_string(),
            }),
            "a reference that names none has nothing whose completion could be recorded"
        );
    }

    #[test]
    fn a_capability_that_publishes_nothing_reports_not_applicable() {
        let view = WorkStateView::without_publication(
            Observation::NotApplicable {
                reason: "n/a".to_string(),
            },
            Observation::NotApplicable {
                reason: "n/a".to_string(),
            },
        );

        assert!(
            matches!(view.review, Observation::NotApplicable { .. }),
            "an unpublished review is not applicable, not empty: {:?}",
            view.review
        );
        assert!(
            matches!(view.verification, Observation::NotApplicable { .. }),
            "an unrequested verification is not applicable, not empty: {:?}",
            view.verification
        );
        let json: serde_json::Value = serde_json::to_value(&view).unwrap();
        for key in ["review", "verification"] {
            assert!(
                json[key]["available"].is_null() && json[key]["unavailable"].is_null(),
                "{key} must publish neither a value nor a failure"
            );
            assert!(
                json[key]["not_applicable"]["reason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()),
                "{key} must say why the question does not apply"
            );
        }
    }

    #[test]
    fn a_repository_with_nothing_published_is_an_available_review() {
        let observed = Observation::Available {
            value: ReviewState {
                branch: None,
                pull_request: None,
                state: None,
            },
            source: SourceRef("github:peel/fiddle/pulls".to_string()),
            revision: None,
        };
        let json: serde_json::Value = serde_json::to_value(&observed).unwrap();
        assert!(json["available"]["value"]["branch"].is_null());
        assert!(json["available"]["value"]["pull_request"].is_null());
        assert!(json["available"]["value"]["state"].is_null());
        assert_eq!(observed.value().and_then(|r| r.pull_request), None);
    }

    #[test]
    fn an_unmarked_change_set_is_an_available_value() {
        let observed = Observation::Available {
            value: ChangeSetState { marker: None },
            source: SourceRef("stub:changes/fiddle-m0-demo.json".to_string()),
            revision: None,
        };
        let json: serde_json::Value = serde_json::to_value(&observed).unwrap();
        assert!(json["available"]["value"]["marker"].is_null());
        assert_eq!(observed.value(), Some(&ChangeSetState { marker: None }));
    }
}
