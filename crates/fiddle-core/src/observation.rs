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

/// Everything a run observed about one invocation, in one value.
///
/// The two observations are carried side by side rather than merged, so a
/// readable work item paired with an unreadable change set stays visibly
/// half-known instead of collapsing into a single verdict.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WorkStateView {
    pub work_item: Observation<WorkItemState>,
    pub changes: Observation<ChangeSetState>,
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
