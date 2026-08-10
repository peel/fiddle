//! What an observed world means, and what to do about it.
//!
//! [`assess`] and [`derive_next`] are the deterministic heart of fiddle: total
//! functions from a [`WorkStateView`] plus the marker this invocation expects
//! onto a verdict and an action. They take the expected marker as an argument
//! rather than deriving it, which is what keeps them pure — they consult no
//! configuration, no file, no clock, and nothing outside their arguments, so
//! the same view always yields the same verdict on any machine, in any process,
//! at any time. The caller that knows the configuration computes the marker
//! with [`correlation_key`] and passes it in.
//!
//! Two rules are encoded here rather than left to callers:
//!
//! - An unobservable source is [`CapabilityAssessment::Blocked`]. It is never
//!   read as "nothing there" and never as success, because a source that failed
//!   to answer said nothing at all about whether the work is done.
//! - A change set carrying a marker that is *not* this invocation's correlation
//!   key is `Blocked`, never [`CapabilityAssessment::Satisfied`]. A foreign
//!   marker is another writer's evidence; claiming it would report work this
//!   invocation cannot account for as its own.

use crate::identity::CapabilityId;
use crate::observation::{Observation, WorkStateView};
use crate::report::EvidenceRef;

/// The one capability M0 can execute.
pub const STUB_MARK: CapabilityId = CapabilityId("stub_mark");

/// The capability M1 adds: repair a broken fixture through a bounded agent
/// attempt.
pub const FIXTURE_REPAIR: CapabilityId = CapabilityId("fixture_repair");

/// The capability M2 adds: publish a change through authenticated effects.
///
/// Here beside the other two rather than in the runtime that executes it, for
/// the reason the other two are: an id is a *name*, and naming something reaches
/// nothing outside the process. It is what `derive_next` is told this run is
/// about, so the pure core has to be able to say it.
pub const PUBLISH_CHANGE: CapabilityId = CapabilityId("publish_change");

/// What fiddle concludes about the capability this invocation is about.
///
/// Serialized externally tagged, so the variant name is the observable
/// contract: a payload reads `{"not_started":{"evidence":[…]}}` and a consumer
/// distinguishes the three cases by which key is present. Every variant carries
/// evidence — a verdict a reader cannot go and check is not much of a verdict —
/// and `Blocked` additionally carries the reason, because "fiddle will not
/// proceed" is only actionable when it says why.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAssessment {
    /// The world was fully observed and the work has not been done yet.
    NotStarted { evidence: Vec<EvidenceRef> },

    /// The world was fully observed and carries this invocation's own marker.
    Satisfied { evidence: Vec<EvidenceRef> },

    /// Fiddle will not proceed, and says why. Reached both when a source could
    /// not be observed and when the change set belongs to someone else.
    Blocked {
        reason: String,
        evidence: Vec<EvidenceRef>,
    },
}

/// What the run should do next, derived from the assessment and nothing else.
///
/// Also externally tagged: `Complete` is the bare string `"complete"`, while
/// `Execute` and `Blocked` carry their payload under their own key.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    /// Run this capability. The only path by which a capability executes.
    Execute { capability_id: CapabilityId },

    /// There is nothing left to do; the work is already accounted for.
    Complete,

    /// Do nothing, and say why. The fail-closed arm: it is what an
    /// unobservable source and a foreign change set both lead to.
    Blocked { reason: String },
}

/// The deterministic marker a satisfied change set must carry.
///
/// `blake3(project + NUL + invocation_ref)`, rendered as the first 16 hex
/// characters (design §4.3). The separator is a NUL byte, which stops the
/// ordinary re-split collision — `("ab","c")` and `("a","bc")` hash differently.
/// It does **not** make collision impossible in general: a NUL is valid UTF-8, so
/// a caller passing one *inside* a field can still forge a pair, which
/// [`crate::effect::effect_id`]'s own test demonstrates for the identical
/// construction. That is why `effect_id` uses length-prefixed framing instead and
/// this function does not: its value is written into fixture state on disk and
/// compared by later runs, so re-basing it would break the cross-process
/// recognition it exists to provide. `invocation_ref` is already constrained to a
/// NUL-free grammar at its parse boundary; `project` is not, and BACKLOG records
/// the milestone boundary at which re-framing this becomes acceptable.
///
/// Deterministic across processes and machines, which is what makes the
/// second-invocation stability proof checkable rather than merely plausible:
/// the marker a run writes today is the marker the next run computes and
/// recognises. Hashing is arithmetic over the bytes it was handed — it reaches
/// nothing outside the process — so this belongs in the pure core beside the
/// assessment that compares against it.
pub fn correlation_key(project: &str, invocation_ref: &str) -> String {
    blake3::hash(format!("{project}\0{invocation_ref}").as_bytes()).to_hex()[..16].to_string()
}

/// Judge an observed world against the marker this invocation expects.
///
/// Total over the observation space, and ordered so the fail-closed cases win:
/// an unobservable source is decided before anything is read out of the other
/// half of the view, since a half-observed world cannot support a conclusion
/// about the whole one.
pub fn assess(work: &WorkStateView, expected_marker: &str) -> CapabilityAssessment {
    match (&work.work_item, &work.changes) {
        // Either source failing is enough: fiddle did not see the world, so it
        // has nothing to conclude about it.
        (Observation::Unavailable { source, reason }, _)
        | (_, Observation::Unavailable { source, reason }) => CapabilityAssessment::Blocked {
            reason: format!("source {source} unavailable: {reason}"),
            evidence: vec![EvidenceRef(source.0.clone())],
        },

        (
            Observation::Available {
                source: work_source,
                ..
            },
            Observation::Available {
                value,
                source: change_source,
                ..
            },
        ) => {
            let evidence = vec![
                EvidenceRef(work_source.0.clone()),
                EvidenceRef(change_source.0.clone()),
            ];
            match &value.marker {
                // A readable change set with no marker is a real observation of
                // work that has not been done, not an absence.
                None => CapabilityAssessment::NotStarted { evidence },

                Some(marker) if marker == expected_marker => {
                    CapabilityAssessment::Satisfied { evidence }
                }

                // Design §4.3: `Satisfied` requires a *matching* marker. Both
                // markers are named so an operator can see the collision rather
                // than only that one happened.
                Some(marker) => CapabilityAssessment::Blocked {
                    reason: format!(
                        "change set carries marker {marker}, expected {expected_marker}: \
                         the change set was written by a different invocation"
                    ),
                    evidence,
                },
            }
        }

        // A source that does not apply to this invocation leaves M0's
        // orchestration with no world to act on; it is not an error, but it is
        // not a basis for executing either.
        _ => CapabilityAssessment::Blocked {
            reason: "source not applicable to the M0 orchestration".to_string(),
            evidence: vec![],
        },
    }
}

/// The action the assessment implies.
///
/// A separate function from [`assess`] so that "what is true" and "what to do
/// about it" stay separable, and a one-to-one mapping so no action can be
/// reached without a verdict that justifies it — in particular
/// [`NextAction::Execute`] is reachable only from
/// [`CapabilityAssessment::NotStarted`], which is the mechanism that stops a
/// second run from executing again.
///
/// The capability under consideration is an argument, and the returned
/// [`NextAction::Execute`] names exactly it. Naming one here was
/// indistinguishable from deriving it while a single capability existed; with a
/// second it would be wrong, and the caller — which knows which capability it
/// is holding — is the only thing that can say. It changes nothing else: which
/// capability asked has no bearing on whether the world is satisfied or
/// unobservable, so the other two arms ignore it.
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
    use crate::observation::{ChangeSetState, SourceRef, WorkItemState};

    fn view(
        work: Observation<WorkItemState>,
        changes: Observation<ChangeSetState>,
    ) -> WorkStateView {
        // The review and the verification are deliberately not varied here:
        // `assess` reads the two local observations and nothing else, and a
        // helper that let a caller set the other two would suggest otherwise.
        WorkStateView::without_publication(work, changes)
    }

    fn avail_work() -> Observation<WorkItemState> {
        Observation::Available {
            value: WorkItemState {
                id: "x".into(),
                status: "open".into(),
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

    /// The key is written into fixture state by one process and compared by
    /// another, so a value pinned here is what stops a refactor from silently
    /// re-basing every marker already on disk.
    ///
    /// The expected digest was produced outside this crate — `printf
    /// 'icecube\0beans:fiddle-m0-demo' | b3sum` — so it pins the definition in
    /// design §4.3 rather than whatever this implementation happens to compute.
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

    /// An unobservable work item blocks just as an unobservable change set
    /// does — the fail-closed rule is about either half of the view, not about
    /// one privileged source.
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

    /// Every verdict must be citable: an assessment over a fully observed world
    /// names both sources it read.
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

    /// A derivation must name the capability it was asked about. While exactly
    /// one capability existed, a hardcoded id was indistinguishable from a
    /// derived one; with a second, the two answers differ and only the derived
    /// one is right.
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

    /// `Satisfied` and `Blocked` are properties of the world, not of which
    /// capability asked about it — so the two capabilities must derive the
    /// *same* action from the same view, not merely both a plausible one.
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

    /// A foreign marker must reach `Blocked` through `derive_next` too — never
    /// `Complete`, and never an execution that would overwrite another writer's
    /// change set.
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

    /// The wire spellings are part of the CLI contract: consumers match on
    /// these exact keys.
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
