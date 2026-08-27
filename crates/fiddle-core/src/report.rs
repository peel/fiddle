#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct EvidenceRef(pub String);

impl std::fmt::Display for EvidenceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CapabilityExecution {
    pub capability_id: crate::identity::CapabilityId,
    pub status: String,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ProgressEntry {
    pub capability_id: crate::identity::CapabilityId,
    pub stage: String,
    pub status: String,
    pub summary: crate::published::Published,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FiddleBuild {
    pub package_version: String,
    pub source_revision: String,
}

pub const UNKNOWN_REVISION: &str = "unknown";

impl FiddleBuild {
    pub fn new(package_version: impl Into<String>, source_revision: &str) -> Self {
        let revision = source_revision.trim();
        let attributable = revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit());
        FiddleBuild {
            package_version: package_version.into(),
            source_revision: if attributable {
                revision.to_ascii_lowercase()
            } else {
                UNKNOWN_REVISION.to_string()
            },
        }
    }
}

pub const REPORT_SCHEMA: &str = "fiddle.report.v0";

pub const RUN_SCHEMA: &str = "fiddle.run.v0";

pub const INSPECT_SCHEMA: &str = "fiddle.inspect.v0";

pub const CONFIG_CHECK_SCHEMA: &str = "fiddle.config_check.v0";

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DeferredFinding {
    pub cve: crate::finding::AdvisoryId,
    pub bound: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct AttemptOutcome {
    pub cves: Vec<crate::finding::AdvisoryId>,
    pub status: String,
    pub claimed_complete: bool,
    pub dispositions: Vec<DisposedFinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DisposedFinding {
    pub cve: crate::finding::AdvisoryId,
    pub attempted: bool,
    pub note: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct AttemptBound {
    pub spent: u32,

    pub bound: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct RunDisposition {
    pub reason: String,

    pub verdicts: usize,

    pub projected: Option<usize>,

    pub already_fixed: Vec<crate::finding::AdvisoryId>,

    pub deferred: Vec<DeferredFinding>,

    pub attempts: Vec<AttemptOutcome>,

    pub branch: Option<String>,

    pub pull_request: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_bound: Option<AttemptBound>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ReportBundle {
    pub schema: &'static str,
    pub fiddle: FiddleBuild,
    pub invocation_ref: String,
    pub work_ref: Option<crate::identity::WorkRef>,
    pub attempt_id: crate::identity::AttemptId,
    pub mode: crate::outcome::Mode,
    pub outcome: crate::outcome::RunOutcome,
    pub next_action: crate::assessment::NextAction,
    pub capability_executions: Vec<CapabilityExecution>,
    pub progress: Vec<ProgressEntry>,
    pub observations: crate::observation::WorkStateView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<RunDisposition>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{AttemptId, WorkRef};
    use crate::observation::{
        ChangeSetState, Observation, ProjectedStatus, ReviewState, SourceRef, TreeObservation,
        VerificationState, WorkItemState, WorkState, WorkStateView,
    };
    use crate::outcome::{Mode, RunOutcome};
    use crate::{CapabilityId, NextAction};
    use std::collections::BTreeMap;

    type KeyTypes = BTreeMap<String, BTreeMap<&'static str, Vec<String>>>;

    #[test]
    fn a_build_admits_only_a_sha_or_the_unknown_literal() {
        let sha = "42345072552be70bf804bc8d9c337cf6bc25e99b";
        assert_eq!(FiddleBuild::new("0.1.0", sha).source_revision, sha);
        assert_eq!(
            FiddleBuild::new("0.1.0", &sha.to_ascii_uppercase()).source_revision,
            sha,
            "one sha must have one spelling in a bundle"
        );
        assert_eq!(
            FiddleBuild::new("0.1.0", &format!("  {sha}\n")).source_revision,
            sha
        );
        for fabricated in ["", "unknown", "4234507", "not-a-sha", &"z".repeat(40)] {
            assert_eq!(
                FiddleBuild::new("0.1.0", fabricated).source_revision,
                UNKNOWN_REVISION,
                "an unattributable revision must degrade, not be published as {fabricated:?}"
            );
        }
    }

    #[test]
    fn a_bundle_serializes_under_the_keys_design_4_7_specifies() {
        let bundle = ReportBundle {
            schema: REPORT_SCHEMA,
            fiddle: FiddleBuild::new("0.1.0", UNKNOWN_REVISION),
            invocation_ref: "beans:fiddle-m0-demo".to_string(),
            work_ref: Some(WorkRef("beans:fiddle-m0-demo".to_string())),
            attempt_id: AttemptId("01JATTEMPT".to_string()),
            mode: Mode::Unattended,
            outcome: RunOutcome::Completed,
            next_action: NextAction::Complete,
            capability_executions: vec![CapabilityExecution {
                capability_id: CapabilityId("stub_mark"),
                status: "completed".to_string(),
                evidence: vec![EvidenceRef("stub:changes/fiddle-m0-demo.json".to_string())],
            }],
            progress: vec![ProgressEntry {
                capability_id: CapabilityId("stub_mark"),
                stage: "mark".to_string(),
                status: "completed".to_string(),
                summary: crate::published::Published::of("wrote correlation marker"),
                evidence: Vec::new(),
            }],
            observations: WorkStateView {
                work_item: Observation::Available {
                    value: WorkItemState {
                        id: "fiddle-m0-demo".to_string(),
                        status: "open".to_string(),
                        projected_status: None,
                    },
                    source: SourceRef("stub:work/fiddle-m0-demo.json".to_string()),
                    revision: None,
                },
                changes: Observation::NotApplicable {
                    reason: "nothing yet".to_string(),
                },
                review: Observation::NotApplicable {
                    reason: "nothing published".to_string(),
                },
                verification: Observation::NotApplicable {
                    reason: "nothing to verify".to_string(),
                },
                tree: None,
            },
            disposition: None,
        };

        let value = serde_json::to_value(&bundle).unwrap();
        assert_eq!(value["schema"], "fiddle.report.v0");
        assert_eq!(value["fiddle"]["package_version"], "0.1.0");
        assert_eq!(value["fiddle"]["source_revision"], "unknown");
        assert_eq!(value["invocation_ref"], "beans:fiddle-m0-demo");
        assert_eq!(value["work_ref"], "beans:fiddle-m0-demo");
        assert_eq!(value["attempt_id"], "01JATTEMPT");
        assert_eq!(value["mode"], "unattended");
        assert_eq!(value["outcome"], "completed");
        assert_eq!(value["next_action"], "complete");
        assert_eq!(
            value["capability_executions"][0]["capability_id"],
            "stub_mark"
        );
        assert_eq!(value["progress"][0]["stage"], "mark");
        assert!(value["observations"]["work_item"]["available"].is_object());
        assert!(value["observations"]["review"]["not_applicable"].is_object());
        assert!(value["observations"]["verification"]["not_applicable"].is_object());
        assert!(
            value.get("disposition").is_none(),
            "a run with no disposition table must publish no key: {value}"
        );
    }

    #[test]
    fn a_disposition_publishes_its_row_and_the_evidence_for_it() {
        let value = serde_json::to_value(RunDisposition {
            reason: "unsafe_without_direction".to_string(),
            verdicts: 2,
            projected: Some(7),
            already_fixed: vec![crate::finding::AdvisoryId::parse("CVE-2026-0003").unwrap()],
            deferred: vec![DeferredFinding {
                cve: crate::finding::AdvisoryId::parse("CVE-2026-0004").unwrap(),
                bound: 5,
            }],
            attempts: vec![AttemptOutcome {
                cves: vec![crate::finding::AdvisoryId::parse("CVE-2026-0001").unwrap()],
                status: "needs_work".to_string(),
                claimed_complete: true,
                dispositions: vec![DisposedFinding {
                    cve: crate::finding::AdvisoryId::parse("CVE-2026-0001").unwrap(),
                    attempted: true,
                    note: "the bump moved the call sites it named".to_string(),
                }],
            }],
            branch: None,
            pull_request: Some(7),
            attempt_bound: None,
        })
        .unwrap();

        assert_eq!(value["reason"], "unsafe_without_direction");
        assert_eq!(value["verdicts"], 2);
        assert_eq!(
            value["projected"], 7,
            "two unfixed beside seven projected: the count alone would tell a \
             reader this image holds two problems: {value}"
        );
        assert_eq!(value["already_fixed"][0], "CVE-2026-0003");
        assert_eq!(value["deferred"][0]["cve"], "CVE-2026-0004");
        assert_eq!(value["deferred"][0]["bound"], 5);
        assert_eq!(value["attempts"][0]["cves"][0], "CVE-2026-0001");
        assert_eq!(value["attempts"][0]["status"], "needs_work");
        assert_eq!(value["attempts"][0]["claimed_complete"], true);
        assert_eq!(
            value["attempts"][0]["dispositions"][0]["cve"],
            "CVE-2026-0001"
        );
        assert_eq!(value["attempts"][0]["dispositions"][0]["attempted"], true);
        assert_eq!(
            value["attempts"][0]["dispositions"][0]["note"],
            "the bump moved the call sites it named"
        );
        assert!(value["branch"].is_null());
        assert_eq!(value["pull_request"], 7);
        assert!(
            value.get("attempt_bound").is_none(),
            "a run that did not stop at the bound must publish no bound: {value}"
        );
    }

    #[test]
    fn a_run_stopped_by_the_bound_publishes_the_count_and_the_bound() {
        let value = serde_json::to_value(RunDisposition {
            reason: "attempt_bound_reached".to_string(),
            verdicts: 0,
            projected: Some(0),
            already_fixed: Vec::new(),
            deferred: Vec::new(),
            attempts: Vec::new(),
            branch: None,
            pull_request: Some(9),
            attempt_bound: Some(AttemptBound { spent: 4, bound: 4 }),
        })
        .unwrap();

        assert_eq!(value["reason"], "attempt_bound_reached");
        assert_eq!(value["pull_request"], 9);
        assert_eq!(
            value["attempt_bound"],
            serde_json::json!({ "spent": 4, "bound": 4 }),
            "the row name alone cannot tell 4 of 4 from 9 of 9: {value}"
        );
    }

    #[test]
    fn a_run_that_read_no_document_publishes_no_count_of_what_it_holds() {
        let over = |projected| {
            serde_json::to_value(RunDisposition {
                reason: "scan_unusable".to_string(),
                verdicts: 0,
                projected,
                already_fixed: Vec::new(),
                deferred: Vec::new(),
                attempts: Vec::new(),
                branch: None,
                pull_request: None,
                attempt_bound: None,
            })
            .unwrap()
        };

        let never_scanned = over(None);
        let scanned_and_clean = over(Some(0));
        assert!(
            never_scanned["projected"].is_null(),
            "a run that never read a document has no count to publish: {never_scanned}"
        );
        assert_eq!(
            scanned_and_clean["projected"], 0,
            "and a scan that ran and found nothing publishes zero: {scanned_and_clean}"
        );
        assert_ne!(
            never_scanned["projected"], scanned_and_clean["projected"],
            "a feed that reads the two as one reports a clean image for a scan \
             that never ran"
        );
    }

    fn a_bundle_that_carries_every_key_at_once() -> ReportBundle {
        ReportBundle {
            schema: REPORT_SCHEMA,
            fiddle: FiddleBuild::new("0.1.0", UNKNOWN_REVISION),
            invocation_ref: "jira:FIDDLE-1".to_string(),
            work_ref: Some(WorkRef("jira:FIDDLE-1".to_string())),
            attempt_id: AttemptId("01JATTEMPT".to_string()),
            mode: Mode::Unattended,
            outcome: RunOutcome::Completed,
            next_action: NextAction::Complete,
            capability_executions: vec![CapabilityExecution {
                capability_id: CapabilityId("cve_mitigate"),
                status: "completed".to_string(),
                evidence: vec![EvidenceRef("report:findings.json".to_string())],
            }],
            progress: vec![ProgressEntry {
                capability_id: CapabilityId("cve_mitigate"),
                stage: "mitigate".to_string(),
                status: "completed".to_string(),
                summary: crate::published::Published::of("bumped one dependency"),
                evidence: vec![EvidenceRef("report:verdicts.json".to_string())],
            }],
            observations: WorkStateView {
                work_item: Observation::Available {
                    value: WorkItemState {
                        id: "FIDDLE-1".to_string(),
                        status: "Awaiting Security Review".to_string(),
                        projected_status: Some(ProjectedStatus {
                            state: WorkState::InReview,
                            jira_status_id: "10001".to_string(),
                            jira_status_name: "Awaiting Security Review".to_string(),
                            jira_status_category: "In Progress".to_string(),
                        }),
                    },
                    source: SourceRef("jira:FIDDLE-1".to_string()),
                    revision: Some("2026-08-27T09:00:00Z".to_string()),
                },
                changes: Observation::Available {
                    value: ChangeSetState {
                        marker: Some("fiddle/FIDDLE-1".to_string()),
                    },
                    source: SourceRef("git:.".to_string()),
                    revision: Some("2026-08-27T09:00:01Z".to_string()),
                },
                review: Observation::Available {
                    value: ReviewState {
                        branch: Some("fiddle/FIDDLE-1".to_string()),
                        pull_request: Some(7),
                        state: Some("open".to_string()),
                    },
                    source: SourceRef("github:peel/fiddle/pulls".to_string()),
                    revision: Some("2026-08-27T09:00:02Z".to_string()),
                },
                verification: Observation::Available {
                    value: VerificationState {
                        head_sha: "42345072552be70bf804bc8d9c337cf6bc25e99b".to_string(),
                        required_missing: vec!["gate".to_string()],
                        failed: vec!["clippy".to_string()],
                        pending: vec!["fmt".to_string()],
                    },
                    source: SourceRef("github:peel/fiddle/checks".to_string()),
                    revision: Some("2026-08-27T09:00:03Z".to_string()),
                },
                tree: Some(TreeObservation {
                    base_revision: "660a031".to_string(),
                    pr_head: Some("f00dcafe".to_string()),
                    attempt_tree: "/w/fiddle".to_string(),
                    scanned_image_digest: "sha256:deadbeef".to_string(),
                }),
            },
            disposition: Some(RunDisposition {
                reason: "unsafe_without_direction".to_string(),
                verdicts: 2,
                projected: Some(7),
                already_fixed: vec![crate::finding::AdvisoryId::parse("CVE-2026-0003").unwrap()],
                deferred: vec![DeferredFinding {
                    cve: crate::finding::AdvisoryId::parse("CVE-2026-0004").unwrap(),
                    bound: 5,
                }],
                attempts: vec![AttemptOutcome {
                    cves: vec![crate::finding::AdvisoryId::parse("CVE-2026-0001").unwrap()],
                    status: "needs_work".to_string(),
                    claimed_complete: true,
                    dispositions: vec![DisposedFinding {
                        cve: crate::finding::AdvisoryId::parse("CVE-2026-0001").unwrap(),
                        attempted: true,
                        note: "the bump moved the call sites it named".to_string(),
                    }],
                }],
                branch: Some("fiddle/FIDDLE-1".to_string()),
                pull_request: Some(7),
                attempt_bound: Some(AttemptBound { spent: 4, bound: 4 }),
            }),
        }
    }

    fn json_type(value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }

    fn types_by_key(document: &serde_json::Value) -> KeyTypes {
        fn walk(value: &serde_json::Value, at: &str, seen: &mut KeyTypes) {
            match value {
                serde_json::Value::Object(fields) => {
                    for (key, field) in fields {
                        let path = format!("{at}/{key}");
                        seen.entry(key.clone())
                            .or_default()
                            .entry(json_type(field))
                            .or_default()
                            .push(path.clone());
                        walk(field, &path, seen);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        walk(item, &format!("{at}/{index}"), seen);
                    }
                }
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_) => {}
            }
        }

        let mut seen = KeyTypes::new();
        walk(document, "", &mut seen);
        seen
    }

    #[test]
    fn the_walk_over_a_document_reports_a_key_two_producers_disagree_on() {
        let colliding = serde_json::json!({
            "disposition": { "projected": 7 },
            "observations": {
                "work_item": {
                    "available": { "value": { "projected": { "state": "in_review" } } }
                }
            }
        });

        let seen = types_by_key(&colliding);
        assert_eq!(
            seen["projected"]
                .iter()
                .map(|(kind, paths)| (*kind, paths.len()))
                .collect::<Vec<(&str, usize)>>(),
            vec![("number", 1), ("object", 1)],
            "a walk blind to this collision could not prove a bundle is free \
             of one: {colliding}"
        );
    }

    #[test]
    fn no_key_in_one_bundle_names_two_incompatible_types() {
        let document = serde_json::to_value(a_bundle_that_carries_every_key_at_once()).unwrap();

        let count = document.pointer("/disposition/projected");
        assert!(
            count.is_some_and(serde_json::Value::is_number),
            "this proves nothing unless the bundle really carries the CVE \
             count: {document}"
        );
        let status = document.pointer("/observations/work_item/available/value/projected_status");
        assert!(
            status.is_some_and(serde_json::Value::is_object),
            "and nothing unless it really carries the projected Jira status: \
             {document}"
        );

        let ambiguous: Vec<String> = types_by_key(&document)
            .into_iter()
            .filter(|(_, types)| types.len() > 1)
            .map(|(key, types)| format!("`{key}` is {types:?}"))
            .collect();
        assert!(
            ambiguous.is_empty(),
            "no reader may need a key's position to know its type: {} in \
             {document}",
            ambiguous.join(", ")
        );
    }
}
