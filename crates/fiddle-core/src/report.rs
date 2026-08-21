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
    use crate::observation::{Observation, SourceRef, WorkItemState, WorkStateView};
    use crate::outcome::{Mode, RunOutcome};
    use crate::{CapabilityId, NextAction};

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
}
