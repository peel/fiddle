//! What fiddle knows about the world, and how sure it is.
//!
//! An [`Observation`] is deliberately three-valued rather than an `Option`.
//! RFC line 796 is the rule this type exists to make unrepresentable-by-accident:
//! *`Unavailable` is not equivalent to empty or absent*. A source fiddle could
//! not read is not a source that said "nothing"; collapsing the two would let a
//! transient outage be reported as a completed world, so the two are separate
//! variants and every consumer must handle them separately.
//!
//! These types live in the pure core rather than beside the adapters that
//! produce them, for two reasons. Naming a source is a *description*, not an
//! act — building a [`SourceRef`] reaches nothing outside the process. And the
//! assessment functions that later tasks add here read observations and nothing
//! else, so the type they read has to be reachable without dragging in an
//! adapter, a runtime, or the outside world.

/// Where an observation came from, as an opaque `<origin>:<locator>` label.
///
/// The text is chosen by the adapter that produced the observation and is meant
/// for a reader, not for re-parsing: it exists so a caller looking at a payload
/// can tell *which* source spoke, and so an `Unavailable` can still say what it
/// failed to read.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct SourceRef(pub String);

impl std::fmt::Display for SourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A value fiddle tried to observe, together with how that attempt went.
///
/// Serialized externally tagged, so a payload reads
/// `{"available": {"value": …, "source": …, "revision": null}}`. The variant
/// name is therefore part of the observable contract: a consumer distinguishes
/// the three cases by which key is present, and `available` being absent is how
/// "fiddle could not see this" is expressed on the wire.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation<T> {
    /// The source was read and this is what it said. `revision` is the source's
    /// own version marker when it has one, so a later run can tell whether the
    /// world moved underneath it.
    Available {
        value: T,
        source: SourceRef,
        revision: Option<String>,
    },

    /// The source exists in principle but could not be read or understood.
    /// `reason` is required: an unobservable source that cannot say why is
    /// indistinguishable from one that was never consulted.
    Unavailable { source: SourceRef, reason: String },

    /// The question does not apply to this invocation, so no source was
    /// consulted. Distinct from [`Observation::Unavailable`]: nothing failed.
    NotApplicable { reason: String },
}

impl<T> Observation<T> {
    /// The observed value, if the source was readable.
    ///
    /// Deliberately the *only* way to reach the value, and deliberately not
    /// paired with an `unwrap_or_default`: a caller that wants to treat an
    /// unobservable source as empty has to write that collapse out in the open.
    pub fn value(&self) -> Option<&T> {
        match self {
            Observation::Available { value, .. } => Some(value),
            Observation::Unavailable { .. } | Observation::NotApplicable { .. } => None,
        }
    }

    /// The source this observation is about, when one was consulted.
    pub fn source(&self) -> Option<&SourceRef> {
        match self {
            Observation::Available { source, .. } | Observation::Unavailable { source, .. } => {
                Some(source)
            }
            Observation::NotApplicable { .. } => None,
        }
    }

    /// Whether the source failed to yield a value it was expected to have.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Observation::Unavailable { .. })
    }
}

/// The work item an invocation addresses, as its source describes it.
///
/// `status` is a free string rather than an enum because it is *the source's*
/// vocabulary — a tracker's own status name, not a fiddle concept. Normalizing
/// it here would lose the distinction between "the tracker said `wontfix`" and
/// "fiddle does not recognise that status".
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkItemState {
    pub id: String,
    pub status: String,
}

/// The change set an invocation has produced, as its source describes it.
///
/// `marker: None` means the source was read and holds no marker — a real
/// observation of an unmarked change set, which is exactly what a run that has
/// not executed yet should see. It never stands in for an unreadable source;
/// that is [`Observation::Unavailable`].
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeSetState {
    pub marker: Option<String>,
}

/// A published change, as the forge describes it.
///
/// Every field is optional because publication is *staged*, and each stage is a
/// real thing to have observed. A branch that exists with no pull request opened
/// against it yet is `pull_request: None` — a read that succeeded and found no
/// pull request, exactly as [`ChangeSetState::marker`] is `None` for a change set
/// that was read and holds no marker. A read that found nothing published at all
/// is every field `None`: still an observation, and still distinct from
/// [`Observation::NotApplicable`], which says the question was never asked
/// because it does not apply.
///
/// `state` is a free string rather than an enum for the reason
/// [`WorkItemState::status`] is: it is *the forge's* vocabulary — `open`,
/// `closed`, `merged` and whatever it adds next — and normalizing it here would
/// lose the difference between "the forge said a word fiddle does not know" and
/// "the forge said nothing".
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReviewState {
    pub branch: Option<String>,
    pub pull_request: Option<u64>,
    /// The source's own vocabulary, as [`WorkItemState::status`] is.
    pub state: Option<String>,
}

/// What CI says about one exact head, and about nothing else.
///
/// `head_sha` is a field of the value rather than context the caller is trusted
/// to remember, and that is the whole point of the type. A check suite follows a
/// *commit*, not a branch: a green result for a head the branch has since moved
/// past is a green result about something else, and a verification that could
/// not say which commit it was about would be indistinguishable from one that
/// was.
///
/// The three lists are the required checks that are not satisfied, split by
/// *why*, and they are separate for the same reason [`Observation`] has three
/// variants. A required check with no run at this head is absent — CI may not
/// have started — while one that is queued is running and one that concluded
/// anything other than a success has answered. Merging any two of them is how
/// "has not started" is read as "passed". All three empty is the only state in
/// which the required checks are satisfied.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VerificationState {
    pub head_sha: String,
    /// Required by name, with no check run at this head at all.
    pub required_missing: Vec<String>,
    /// Present at this head and concluded something that is not a success.
    pub failed: Vec<String>,
    /// Present at this head and not finished.
    pub pending: Vec<String>,
}

/// What a run that reached a forge saw there, in the pair a view needs.
///
/// The two observations travel together because they are made together and are
/// meaningless apart: a verification is *about* the head a review names, and a
/// caller free to supply one without the other could publish a green check for a
/// commit no review mentions. Pairing them also means
/// [`WorkStateView::with_publication`] has one argument to answer for rather
/// than two that must agree.
///
/// Each half is an [`Observation`] rather than a value, because the run that
/// reached the forge is exactly the run that can find it unreadable — and
/// `Unavailable` is never equivalent to empty.
#[derive(Clone, Debug)]
pub struct Publication {
    /// What the forge says has been published for this invocation.
    pub review: Observation<ReviewState>,
    /// What CI says about the head that was published.
    pub verification: Observation<VerificationState>,
}

/// Which revision a run's attempt worked at, and the other one it saw.
///
/// # Why a value here and not three keys the capability writes somewhere
///
/// Because there was nowhere else. This is Design §4's sentence —
/// *the observation carries the base revision **and** the open pull request's
/// head, and the record says which of the two the attempt actually ran against;
/// a run that recorded only one of them cannot be read afterwards* — and the
/// only durable record a run leaves is its bundle, whose `observations` is a
/// [`WorkStateView`]. That was a closed set of four ports belonging to M0's
/// assessment, so a capability with a fact of its own about *the tree* had it
/// produced and unplaced.
///
/// A named struct rather than a free-form object: the four keys are a contract
/// a reader parses, and a `serde_json::Value` here would make them whatever the
/// last capability to write one happened to emit.
///
/// `attempt_tree` is the *name* of the field holding the revision that was used,
/// so a reader finds the value beside a key of the same name rather than having
/// to be told the mapping.
///
/// # Why the scanned image's digest is in here rather than beside it
///
/// Because the pair is the point, and a sibling key could be read apart from it.
/// [`scanned_image_digest`](Self::scanned_image_digest) is not a fact about a
/// tree; it is the other half of a question only both halves answer — *which
/// image were these verdicts measured against, and which tree was remediated?*
/// A run that published one without the other would leave a reader to assume the
/// connection, which is exactly the assumption ADR 020 exists to stop being
/// silent.
///
/// This struct has no `Default` and one producer, so a field rather than a
/// second `Option` on [`WorkStateView`] makes the pairing the compiler's: there
/// is no way to record the revision without recording the digest beside it. The
/// same device the base revision already gets from being on both arms of
/// `Checkout` one layer down.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TreeObservation {
    /// What `origin/<base>` resolved to. Present on both arms, which is the
    /// half of Design §4's sentence a run is most likely to drop.
    pub base_revision: String,
    /// The reused pull request's remote tip, and `null` where none was open.
    ///
    /// `null` rather than absent, so a reader asking *was a pull request
    /// reused* gets an answer instead of a missing key that could equally mean
    /// an older build.
    pub pr_head: Option<String>,
    /// Which of the two above the attempt's worktree was made at, named as the
    /// key that holds it.
    pub attempt_tree: String,
    /// The digest the scan resolved its image reference to, as the scanner
    /// reported it.
    ///
    /// **The digest and never the configured tag.** A tag is a name somebody can
    /// move, and the whole reason this key exists is that the tag names whatever
    /// currently carries it rather than the thing that was measured.
    ///
    /// # What this asserts, and what it does not
    ///
    /// It asserts that *this run's verdicts were measured against these bytes*
    /// and *this run remediated the revision beside it*. It does **not** assert
    /// that the image was built from that revision: fiddle does not build the
    /// image it scans — the host workflow does, which is ADR 020. So the pair is
    /// a correspondence made **checkable** by whoever did build it, not one this
    /// build verified. Anything stronger would need the builder to declare the
    /// revision it built at, which nothing populates today; ADR 020's
    /// consequences say who owes it.
    pub scanned_image_digest: String,
}

/// Everything a run observed about one invocation, in one value.
///
/// The observations are carried side by side rather than merged, so a readable
/// work item paired with an unreadable change set stays visibly half-known
/// instead of collapsing into a single verdict.
///
/// [`WorkStateView::review`] and [`WorkStateView::verification`] are *appended*
/// to the two M0 published, and that is deliberate: every consumer of a bundle
/// reads `observations` by path — `observations.work_item.available.value.status`
/// — never as a key set, so two more keys are invisible to a reader that does
/// not want them and are the whole payload to one that does.
///
/// [`WorkStateView::tree`] is the third appended the same way and the first that
/// is *absent* rather than neutral when it does not apply. The other four are
/// always serialized because [`Observation::NotApplicable`] is a real answer to
/// each of them — the question was asked and does not apply — whereas "which
/// revision did the attempt run at" is not a question a capability that creates
/// no worktree can be asked at all. So it is `None` and `skip_serializing_if`,
/// and every bundle M0, M1, M2 and M3 have ever published is byte-identical.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WorkStateView {
    pub work_item: Observation<WorkItemState>,
    pub changes: Observation<ChangeSetState>,
    /// What the forge says has been published for this invocation.
    pub review: Observation<ReviewState>,
    /// What CI says about the head that was published.
    pub verification: Observation<VerificationState>,
    /// Which revision this run's attempt worked at, where the capability made a
    /// worktree and therefore had to choose one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeObservation>,
}

impl WorkStateView {
    /// The view of a run that publishes nothing outside the local world.
    ///
    /// **Not an empty review.** A capability that publishes no change has not
    /// looked for a pull request and found none — the question does not apply to
    /// it at all, which is [`Observation::NotApplicable`] and never
    /// [`Observation::Available`] holding a defaulted [`ReviewState`]. The
    /// difference is the whole of RFC line 796 applied one level up: an
    /// `Available` review with every field `None` is the positive claim *the
    /// forge was read and holds nothing*, and a run that never spoke to a forge
    /// must not be able to make it. Nor is it
    /// [`Observation::Unavailable`] — nothing failed.
    ///
    /// A constructor rather than four literals at each call site, so the two
    /// reasons are written once and a capability that *can* see a review builds
    /// the view itself — through [`WorkStateView::with_publication`] — rather
    /// than overwriting a default it inherited.
    ///
    /// # Why the reasons name the *reading* and not the capability
    ///
    /// They named the capability until a capability that publishes existed, and
    /// then they were wrong in the one place a reader would look. This
    /// constructor is reached by three callers and only one of them is about a
    /// capability that cannot publish: the read-only `inspect`, which reaches no
    /// forge whatever `--capability` names; a run's *entry* observation, taken
    /// before any capability has executed; and a run whose capability publishes
    /// nothing. What all three have in common is that no forge was consulted,
    /// which is the honest reason and the one that stays true as capabilities
    /// are added.
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
            // No worktree has been made at the moment either of this
            // constructor's three callers runs, so there is no revision to have
            // chosen. A capability that does choose one answers through
            // [`WorkStateView::at_revision`], after the fact.
            tree: None,
        }
    }

    /// The view of a run that *did* publish, and looked.
    ///
    /// The counterpart [`WorkStateView::without_publication`]'s documentation
    /// promises: "a capability that can see a review builds the view itself
    /// rather than overwriting a default it inherited". This is that
    /// constructor, and its existence is what stops the two observations from
    /// being types nobody fills.
    ///
    /// It takes the pair whole rather than four separate observations, and it
    /// takes them as [`Observation`]s rather than as values: the capability that
    /// reached the forge is the only participant that can say whether the read
    /// succeeded, and this constructor must not be able to turn "the forge could
    /// not be read" into "the forge holds nothing" on its way past.
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

    /// The same view, saying which revision the attempt worked at.
    ///
    /// Applied *after* whichever of the two constructors above built the view,
    /// rather than as a fifth argument to both, because the two facts are
    /// independent: a capability may reach a forge and make no worktree
    /// (`publish_change`), or make one and reach no forge. Folding the revision
    /// into `with_publication` would tie them together and make one of those two
    /// unsayable.
    pub fn at_revision(mut self, tree: Option<TreeObservation>) -> Self {
        self.tree = tree;
        self
    }

    /// Whether anything in this world can say the invocation's work is **done**.
    ///
    /// True for a reference that names a work item, where the change set is that
    /// state: a marker equal to the invocation's correlation key means this work
    /// has been accounted for, and design §4.3's exactly-once rests on it.
    ///
    /// False for a reference that names none. Such an invocation *discovers* its
    /// work, so there is nothing whose completion could have been recorded — and
    /// in particular the marker under its change set is not that record. The
    /// marker is derived from the project and the reference
    /// ([`correlation_key`](crate::correlation_key)); no capability and no
    /// attempt enter it, so every run over the reference computes the same value
    /// and a marker on disk says only *some run wrote one*. It cannot say which
    /// capability, and it cannot say whether the work that reference names — a
    /// container image scanned — was ever done. [ADR
    /// 023](../../../docs/technical/decisions/023-a-sweep-has-no-completion-state.md)
    /// is where the consequence is argued: such an invocation is idempotent by
    /// *rescanning* rather than by remembering.
    ///
    /// Read off [`WorkStateView::work_item`] rather than taken as an argument,
    /// so [`assess`](crate::assess) and the runtime's own conclusion about a run
    /// decide it from one place and cannot come to disagree. And read as
    /// [`Observation::NotApplicable`] specifically: a work item that *failed to
    /// read* is a world fiddle did not see, which is
    /// [`CapabilityAssessment::Blocked`](crate::CapabilityAssessment::Blocked)
    /// and never a reference without a tracker.
    ///
    /// # Two readers, on purpose
    ///
    /// This predicate has exactly two callers and they are meant to move
    /// together:
    ///
    /// - [`assess`](crate::assess) asks it to decide whether the marker under the
    ///   change set may be read as completion at all.
    /// - `fiddle_runtime::orchestration::concluded` asks it to decide what a run
    ///   whose *post*-execution action is still `Execute` concluded — `Completed`
    ///   for a reference with no completion state, `Retryable` for one that has
    ///   some, because only the second kind can be said to have lost its effect.
    ///
    /// So the two are one question asked twice about one run, not a rule and an
    /// unrelated exit-code mapping that happen to share a name. That is what the
    /// second reader is for: derive `Execute` twice from a world that records
    /// nothing about being done and you must not read the repeat as failure.
    ///
    /// The consequence is worth stating plainly, because it surprised someone
    /// once: mutating this function moves both decisions in one edit — what a
    /// marker means, and what a run that already executed concluded. That is the
    /// point, and it is also a trap for a test. A test whose *premise* is "the
    /// reference has been marked" must not establish that premise by running
    /// fiddle: such a run derives through the very rule under test, so its outcome
    /// is not independent of the mutation. While this predicate had two spellings —
    /// this one and an `Observation::NotApplicable` pattern inside `assess` —
    /// inverting it moved only the second decision, turned the setup run into an
    /// exit-11 `RunOutcome::Retryable`, and the lane that meant to catch the marker
    /// rule died on its own premise guard without ever reaching its claim. Write
    /// the marker into the world directly and let the run under test be the only
    /// run;
    /// `a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done`
    /// in `fiddle-acceptance` does, and says why there too.
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
            r#"{"available":{"value":{"id":"fiddle-m0-demo","status":"open"},"source":"stub:work/fiddle-m0-demo.json","revision":null}}"#
        );
    }

    /// The wire shape is the fail-closed rule made checkable: an unobservable
    /// source carries a reason and leaves no `available` key behind for a
    /// consumer to read as "empty".
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

    /// Additive by construction. `m0_skeleton` asserts JSON *paths*, never a key
    /// set, so two new observations cannot break it — but the two paths it reads
    /// are asserted here too, so a later restructuring is caught in core rather
    /// than three crates away in an acceptance lane.
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
        // And the fifth is *absent* rather than null, which is what keeps every
        // bundle published before it byte-identical. See [`WorkStateView::tree`].
        assert!(
            json.get("tree").is_none(),
            "a view with no worktree behind it must not carry the key at all: {json}"
        );
        // The two paths `m0_skeleton` and `inspect_observations` read, spelled
        // exactly as they spell them.
        assert_eq!(json["work_item"]["available"]["value"]["status"], "open");
        assert!(json["changes"]["available"].is_object());
        assert!(json["changes"]["available"]["value"]["marker"].is_null());
    }

    /// **The three shapes a work item comes in, and which of them has something
    /// to be done.**
    ///
    /// The predicate two decisions share — `assess`'s trackerless reading and the
    /// runtime's conclusion about a run that executed — so it is asserted over
    /// every variant rather than over the one case each of those was written for.
    /// The pairing that matters is the second and the third: a work item that
    /// *failed to read* still has a completion state, because a reference naming a
    /// tracker row does not stop naming one when the tracker is down. Collapsing
    /// the two would make an unreadable tracker look like a reference that never
    /// had one, which is the distinction `assess` keeps two arms apart for.
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

    /// A capability that publishes nothing says the question does not apply,
    /// rather than reporting an empty review as though it had looked and found
    /// none. `Available` with a defaulted value is the failure mode: it would
    /// let a run that never published anything report a checked, clean review.
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
        // The wire shape says the same thing: there is no `available` key for a
        // consumer to read a defaulted value out of, and there is no
        // `unavailable` key either — nothing failed.
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

    /// A read of GitHub that found nothing published is a *value* — the same
    /// distinction [`ChangeSetState::marker`] carries, one level up. It is what
    /// separates "looked, nothing there yet" from
    /// [`Observation::NotApplicable`]'s "the question does not apply".
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

    /// An unmarked change set is a *value*, not an absence, so it must survive
    /// the round trip as `Available` with an explicitly null marker.
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
