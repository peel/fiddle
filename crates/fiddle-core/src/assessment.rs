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
//! Three rules are encoded here rather than left to callers:
//!
//! - An unobservable source is [`CapabilityAssessment::Blocked`]. It is never
//!   read as "nothing there" and never as success, because a source that failed
//!   to answer said nothing at all about whether the work is done.
//! - A source that does not *apply* is not an unobservable one. A run that
//!   addresses no tracker item has no work item by design, so
//!   [`Observation::NotApplicable`] in that half decides on the change set alone
//!   rather than blocking. The two must stay apart in both directions: merged
//!   into the rule above, every trackerless run fails; merged the other way, a
//!   broken tracker read passes for one.
//! - A change set carrying a marker that is *not* this invocation's correlation
//!   key is `Blocked`, never [`CapabilityAssessment::Satisfied`]. A foreign
//!   marker is another writer's evidence; claiming it would report work this
//!   invocation cannot account for as its own.
//! - The marker is read as completion **only for a reference that has some
//!   completion state to read**, which is
//!   [`WorkStateView::has_completion_state`]. An invocation that names no work
//!   item discovers its own, so nothing anywhere records that it is done and a
//!   marker on its change set records only that some run wrote one — the key is
//!   derived from the project and the reference, and no capability enters it.
//!   Such a world is [`CapabilityAssessment::NotStarted`] whatever the change set
//!   carries, which is what makes such a run idempotent by *doing the work again*
//!   rather than by remembering that it once did. ADR 023 argues it.

use crate::identity::CapabilityId;
use crate::observation::{ChangeSetState, Observation, WorkStateView};
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

/// The capability M3 adds: produce a change, publish it as a draft, and ask a
/// person before it goes any further.
///
/// Here beside the other three for the reason they are here, which is worth
/// restating for this one because it is the first capability whose work is
/// *finished* by something outside the process: an id is a name, naming reaches
/// nothing, and `derive_next` has to be able to say what a run is about whether
/// that run is going to complete or suspend.
pub const PROPOSE_CHANGE: CapabilityId = CapabilityId("propose_change");

/// The capability M4 adds: clear the advisories a container scan reported, by
/// bumping the dependencies that carry them.
///
/// Here beside the other four for their reason, and it is worth restating once
/// more because this is the first capability whose invocation names *no work
/// item*: an id is a name, naming reaches nothing, and `derive_next` has to be
/// able to say what a run is about — including a run whose
/// [`InvocationRef`](crate::InvocationRef) is the bare scheme `cve` and whose
/// work item is therefore [`Observation::NotApplicable`] rather than a tracker
/// row somebody opened. That arm of [`assess`] was written before anything could
/// reach it; this is what reaches it.
pub const CVE_MITIGATE: CapabilityId = CapabilityId("cve_mitigate");

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
    ///
    /// Also the verdict over a world that *cannot* record having done it — a
    /// reference with no [completion
    /// state](crate::WorkStateView::has_completion_state), whose change set may
    /// well carry a marker. That is not a second meaning: the work such an
    /// invocation names is a fresh look at the world, and a look nobody has taken
    /// this run has not been taken. It is why a reader can meet `not_started`
    /// beside a `marked` change set and both lines be true.
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
/// about the whole one. That ordering is also what keeps the trackerless arm
/// below honest — it is reached only once every *failed* read has been taken,
/// so it can never turn a failure into a proceed.
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
        ) => decide_on_marker(
            value,
            expected_marker,
            vec![
                EvidenceRef(work_source.0.clone()),
                EvidenceRef(change_source.0.clone()),
            ],
        ),

        // A run that addresses no tracker item has no work item to read, and
        // that absence is a decision rather than a failure — every case where a
        // source *failed* was taken by the arm above. What is left is a world
        // fiddle saw in full, and **there is nothing in it that could say the
        // work is done**, so the verdict is that it is not.
        //
        // The marker is deliberately not read here, and that is the whole of this
        // arm. `expected_marker` is `correlation_key(project, invocation_ref)`:
        // no capability and no attempt enter it, so every run over this reference
        // computes the same value and a marker on disk says only *some run wrote
        // one*. For a reference that names a work item that is enough, because
        // the work item is the thing being accounted for and one accounting of it
        // is all there is. For a reference that names none there is no such
        // thing: the run discovers its work, and a marker cannot say which
        // capability wrote it or whether the work the reference names — a
        // container image scanned — was ever done.
        //
        // Reading it anyway is what made `fiddle run cve --capability stub_mark`
        // account a sweep as complete. M0's stub marks the change set and scans
        // nothing; its marker is byte-identical to the sweep's, so the next
        // `cve` invocation read `Satisfied`, `derive_next` returned `Complete`
        // before the capability was consulted, and a host running the documented
        // command nightly reported success having never looked at the image. ADR
        // 023 carries the argument in full, including what the second run of such
        // an invocation *does* conclude: it scans again, and design §4's
        // commit-log dedup — not this verdict — is what stops it opening a second
        // pull request.
        //
        // Neither half of that reasoning is about the spelling `cve`. It turns on
        // the reference naming no work item, so a later capability sharing a
        // trackerless reference inherits the rule rather than the hole.
        //
        // The change set still has to be *readable*, which the arm above already
        // requires, and it is still the one source there is to cite:
        // `NotApplicable` names none, and citing one anyway would claim a read
        // that never happened.
        //
        // Deliberately its own arm and not folded into either neighbour. Sharing
        // one with the `Unavailable` arm would fail every trackerless run;
        // widening that arm to admit `Unavailable` would let a tracker fiddle
        // could not read pass for a tracker it was never asked about.
        (
            Observation::NotApplicable { .. },
            Observation::Available {
                source: change_source,
                ..
            },
        ) => CapabilityAssessment::NotStarted {
            evidence: vec![EvidenceRef(change_source.0.clone())],
        },

        // What is left has no change set to read, so nothing here says whether
        // the work was done: an invocation this orchestration cannot act on. It
        // is not an error, but it is not a basis for executing either.
        _ => CapabilityAssessment::Blocked {
            reason: "source not applicable to the M0 orchestration".to_string(),
            evidence: vec![],
        },
    }
}

/// The verdict a *readable* change set supports, given the sources it was read
/// from.
///
/// Reached by one arm of [`assess`]: a run whose reference names a work item, and
/// which therefore has a completion state for a marker to be. It was reached by
/// two, on the reasoning that "whether the work is done is a fact about the
/// change set, and a tracker item never was the thing that settled it" — which is
/// true of the marker rule and false of what a marker over a trackerless
/// reference means. The trackerless arm now says so itself and this function is
/// no longer general over both.
///
/// Still a function rather than inlined into its one caller, and the evidence is
/// still an argument: the three-way marker rule is design §4.3's and reads as one
/// piece, while the sources a verdict may cite are the caller's business — a run
/// may never cite a source it did not read.
fn decide_on_marker(
    changes: &ChangeSetState,
    expected_marker: &str,
    evidence: Vec<EvidenceRef>,
) -> CapabilityAssessment {
    match &changes.marker {
        // A readable change set with no marker is a real observation of work
        // that has not been done, not an absence.
        None => CapabilityAssessment::NotStarted { evidence },

        Some(marker) if marker == expected_marker => CapabilityAssessment::Satisfied { evidence },

        // Design §4.3: `Satisfied` requires a *matching* marker. Both markers
        // are named so an operator can see the collision rather than only that
        // one happened.
        Some(marker) => CapabilityAssessment::Blocked {
            reason: format!(
                "change set carries marker {marker}, expected {expected_marker}: \
                 the change set was written by a different invocation"
            ),
            evidence,
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
/// second run from executing again — where there is a completion state for a
/// second run to read. Where there is none, [`assess`]'s trackerless arm answers
/// `NotStarted` every time and a second run *does* execute again, deliberately;
/// what keeps that safe is the capability's own dedup rather than this mapping,
/// and ADR 023 is where the two are told apart.
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

    /// The work item of a run that addresses no tracker item at all. No source
    /// was consulted, so there is none to name — which is exactly what
    /// distinguishes it from [`unavailable`], where a named source failed.
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

    /// A run that addresses no tracker item has no work item *by design*, and
    /// absent-by-design is not an obstacle: a trackerless orchestration reaches
    /// a verdict about its change set rather than the fallback that refuses to
    /// conclude.
    ///
    /// Its counterpart is `an_unavailable_work_item_blocks_too` directly above,
    /// where a work item fiddle *failed to read* still blocks. The two are
    /// separate match arms and must stay separate, because the distinction is
    /// the property: collapse them one way and a broken tracker read is
    /// indistinguishable from a trackerless run, collapse them the other way and
    /// every trackerless run is `Blocked`.
    #[test]
    fn a_not_applicable_work_item_does_not_block() {
        let assessed = assess(&view(trackerless_work(), changes_with(None)), "aaaa");
        let CapabilityAssessment::NotStarted { evidence } = assessed else {
            panic!(
                "a trackerless run has no work item by design; that is not an obstacle, \
                 got {assessed:?}"
            );
        };
        // It cites the one source it actually read. There is no work item source
        // to name — `NotApplicable` carries none — and naming one anyway would
        // claim a read that never happened.
        assert_eq!(
            evidence,
            vec![EvidenceRef("stub:changes/x.json".into())],
            "a trackerless verdict cites the change set it read and nothing else"
        );
    }

    /// **A marker over a trackerless reference is not a completion**, whichever
    /// value it carries.
    ///
    /// Both cases, because the *reason* they agree is the point. A marker equal to
    /// the expected key is not this run's signature: `correlation_key` is derived
    /// from the project and the reference alone, so it is the value every run over
    /// this reference computes and every capability writes — M0's `stub_mark`
    /// included, which marks a change set and scans nothing. And a marker that
    /// differs is not a rival claim on work this reference accounts for, because
    /// it accounts for none.
    ///
    /// So neither is evidence about the work, and a reference with no completion
    /// state is `NotStarted` over both worlds. This is the lane that reds if the
    /// marker is read here again: it used to answer `Satisfied` for the first and
    /// `Blocked` for the second, and the first is what let
    /// `fiddle run cve --capability stub_mark` account a sweep as done.
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

    /// The same change set, read under the two kinds of reference, reaches two
    /// different verdicts.
    ///
    /// The discriminating half of the lane above. An arm that answered
    /// `NotStarted` for *every* view would satisfy it while abandoning design
    /// §4.3, so what has to be asserted is that the trackerless answer is a
    /// consequence of the reference and not a new blanket rule: a marked change
    /// set under a work item is still `Satisfied`, and under no work item it is
    /// `NotStarted`.
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

    /// The fail-closed ordering survives the trackerless arm. A run with no work
    /// item and an *unreadable* change set is still `Blocked` — the new arm
    /// requires an observable change set and does not mean "a trackerless run
    /// always proceeds".
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
