//! What a run can point at as proof of what it did, and the document it
//! publishes to say so.
//!
//! [`ReportBundle`] is the whole of that document: a value, with no notion of
//! where it is written or how. Publication — the temporary directory, the
//! rename, the cleanup — belongs to the runtime, because it touches the
//! filesystem and this crate is mechanically held pure. The split is what lets
//! the bundle's shape be a compile-time contract while its durability is a
//! runtime concern.

/// A pointer to something a reader can go and check.
///
/// Deliberately the same opaque `<origin>:<locator>` shape as
/// [`crate::SourceRef`], and deliberately a distinct type: a source is where an
/// observation *came from*, while evidence is what a conclusion *rests on*.
/// They frequently coincide in M0 — an assessment cites the sources it read —
/// but the two roles diverge as soon as a capability produces an artefact that
/// was never observed.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct EvidenceRef(pub String);

impl std::fmt::Display for EvidenceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One capability a run actually executed, and how that execution ended.
///
/// The list of these is what makes "the capability was never executed"
/// checkable from outside the process: a run that derived `Blocked` publishes
/// an empty list, and no amount of reading the outcome alone could establish
/// that.
///
/// `status` is a free string rather than an enum because it describes the
/// execution, not the run: M0 records `completed` and `failed`, and a
/// capability that grows richer stages should not have to widen a core enum to
/// say so.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CapabilityExecution {
    pub capability_id: crate::identity::CapabilityId,
    pub status: String,
    pub evidence: Vec<EvidenceRef>,
}

/// One observable stage within a capability execution.
///
/// Design §4.7 requires the published bundle to carry `progress` alongside
/// `capability_executions`: the executions say *what ran*, progress says *what
/// happened while it ran*, in the words a reader can act on. M0's single
/// capability emits exactly one entry per execution, so an empty
/// `capability_executions` implies an empty `progress`.
///
/// `summary` is the one field here that is prose rather than a name from a
/// closed set, and it is therefore where a run's own failure text lands — the
/// same text [`RunOutcome`](crate::RunOutcome) carries, filed under the stage
/// it happened at. It is [`Published`](crate::Published) for the same reason
/// and with the same guarantee.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ProgressEntry {
    pub capability_id: crate::identity::CapabilityId,
    pub stage: String,
    pub status: String,
    pub summary: crate::published::Published,
    pub evidence: Vec<EvidenceRef>,
}

/// Which build of fiddle produced a bundle.
///
/// Both fields are captured when the binary is compiled, not looked up when it
/// runs, so a bundle is attributable to the exact artefact that wrote it rather
/// than to whatever happens to be checked out when someone reads it.
///
/// `source_revision` is a 40-character hexadecimal commit sha, or the literal
/// `"unknown"` when the binary was built outside a Git checkout. Those are the
/// only two admissible values: an empty string would read as "no revision" when
/// the truth is "the revision was not captured", and a plausible-but-wrong sha
/// would attribute the bundle to a commit that never produced it. A reader must
/// be able to trust that a sha here is *the* sha.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FiddleBuild {
    pub package_version: String,
    pub source_revision: String,
}

/// The literal a build takes when the revision could not be captured.
///
/// Named once, here, so the value the build script emits and the value a reader
/// checks for are the same string rather than two copies of a convention.
pub const UNKNOWN_REVISION: &str = "unknown";

impl FiddleBuild {
    /// A build identity, rejecting any revision that is neither a 40-character
    /// hexadecimal sha nor [`UNKNOWN_REVISION`].
    ///
    /// The normalisation is deliberately one-way: anything unrecognised becomes
    /// `"unknown"` rather than being passed through. A malformed or truncated
    /// revision that reached a published bundle would be a fabricated claim
    /// about provenance, which is worse than an honest absence — so the type
    /// makes it unrepresentable instead of trusting every caller to check.
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

/// The schema every bundle this build publishes declares itself to be.
///
/// A reader dispatches on this before anything else, so it is a constant rather
/// than a literal spelled at the construction site: a bundle whose shape changes
/// must change this string in the same edit.
pub const REPORT_SCHEMA: &str = "fiddle.report.v0";

/// The schema every `run --json` payload declares itself to be, as design §3.2
/// specifies it.
///
/// The bundle on disk and the payload on stdout are two documents describing one
/// attempt, and until now only the first of them was versioned: a consumer
/// reading the bundle could dispatch on [`REPORT_SCHEMA`] and survive a shape
/// change, while a consumer reading the same run's stdout had nothing to
/// dispatch on. M1 onward adds fields to these payloads, so the asymmetry grows
/// with every milestone.
pub const RUN_SCHEMA: &str = "fiddle.run.v0";

/// The schema every `inspect --json` payload declares itself to be.
///
/// Design §3.2 names only the run payload, because it is the one command the
/// milestone is built around. The discriminator is extended to the other two
/// `--json` contracts anyway: a consumer parsing `inspect` stdout has exactly
/// the versioning problem a consumer parsing `run` stdout has, and a CLI where
/// only some payloads can be dispatched on is worse than one where none can —
/// the absence of the key stops meaning anything.
pub const INSPECT_SCHEMA: &str = "fiddle.inspect.v0";

/// The schema every `config check --json` payload declares itself to be.
///
/// Same reasoning as [`INSPECT_SCHEMA`]. The value is spelled `config_check`
/// rather than `config-check` because every other identifier fiddle puts in a
/// payload — key names, capability ids, progress stages — is snake_case, and a
/// schema name is the last place to introduce a second convention.
pub const CONFIG_CHECK_SCHEMA: &str = "fiddle.config_check.v0";

/// One finding a run's budget did not reach, as the bundle records it.
///
/// The bound travels with the advisory because *this run stopped at five* is the
/// only sentence that separates a deferred finding from one fiddle assessed and
/// declined, and the two are indistinguishable in a record that names only the
/// advisory. Design §2.5.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DeferredFinding {
    /// The advisory that will have to wait for the next run.
    pub cve: crate::finding::AdvisoryId,
    /// The per-run bound that deferred it.
    pub bound: usize,
}

/// What one attempted group left behind, as the bundle records it.
///
/// This is where `claimed_complete` becomes readable from outside a run, which
/// is the whole of its stated purpose: it is *evidence beside the exit code that
/// overruled it*, and evidence nothing publishes is a field, not evidence.
///
/// `forbidden` is rendered text rather than a structured shape, and that is the
/// same decision [`TreeObservation::attempt_tree`] makes: the producing type is
/// a runtime enum whose own `Display` is what an operator reads in the verdict,
/// and a second, structured spelling here would be a second thing for that
/// wording to drift from.
///
/// [`TreeObservation::attempt_tree`]: crate::observation::TreeObservation::attempt_tree
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct AttemptOutcome {
    /// Every advisory the group covered.
    pub cves: Vec<crate::finding::AdvisoryId>,
    /// How the group ended, in the producing enum's own vocabulary.
    pub status: String,
    /// Whether the model said it had finished. **Evidence only** — nothing in
    /// the product branches on it, and publishing it is what lets a reader see
    /// the claim beside the check that overruled it.
    pub claimed_complete: bool,
    /// Every shape the scope rules forbid in the attempt's diff, in path order.
    /// All of them, not the one that decided the group: by the time anybody
    /// reads this the worktree it was computed in is gone.
    pub forbidden: Vec<String>,
}

/// What a run came to, and the evidence for that reason, in one value.
///
/// # Why this is in the bundle at all
///
/// Design §3: *every `NoChange` carries the evidence for its own reason; one
/// whose reason cannot be checked from the bundle is not evidenced.* Until this
/// key existed, five of the seven rows of that table were indistinguishable from
/// outside a run — a capability computed the pair, wrote the verdict array, and
/// returned an evidence reference carrying neither half, so *nothing to do*,
/// *already fixed in the tree* and *already covered by an open pull request*
/// published byte-identical artefacts. A table proved pairwise-distinct inside
/// the process is not a distinction anybody outside it can make.
///
/// # Why here and not beside the verdict array
///
/// The verdict array is a **contract with a different consumer** — the host
/// workflow's Jira and Slack steps read it and nothing else, as a bare array of
/// five-field rows — so a header wrapped around it would break them. It is also
/// written flat into `<report.dir>` and overwritten by each run, so it could not
/// answer *which run*. The bundle is per attempt, is schema-tagged, and is
/// already the document Design §3's sentence names.
///
/// # What it does not carry
///
/// The verdicts themselves, and the scanner's diagnostic. Both are already
/// published — the rows in the verdict report the receipt points at, the
/// diagnostic in the bundle's own `outcome` — and a second copy of either would
/// be a second place for one fact, which is the failure the whole
/// [`crate::observation`] module is arranged to avoid. What is here is the count,
/// which is what says whether the report a reader is about to open has anything
/// in it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct RunDisposition {
    /// Which row of Design §3's table this run reached, in the producing enum's
    /// own snake_case spelling.
    ///
    /// A string for [`TreeObservation`](crate::observation::TreeObservation)'s
    /// reason: the enum belongs to the runtime and this crate is the one the
    /// bundle's shape is fixed in, so what crosses is the name rather than the
    /// type. The producer's match is exhaustive, so a row added to that table
    /// has to be named here rather than defaulting to a neighbour's word.
    pub reason: String,

    /// How many advisories this run left unfixed — the length of the verdict
    /// report published beside this bundle, not a second copy of it.
    pub verdicts: usize,

    /// The advisories deduplication settled before any group was formed.
    ///
    /// Row 3's evidence, and the reason row 3 is not row 1: *the scan found
    /// nothing* and *everything the scan found was already dealt with* are two
    /// situations, and a run that published neither list presented the second as
    /// the first.
    pub already_fixed: Vec<crate::finding::AdvisoryId>,

    /// The findings the per-run budget did not reach. See [`DeferredFinding`].
    pub deferred: Vec<DeferredFinding>,

    /// What each attempted group left behind. See [`AttemptOutcome`].
    ///
    /// Non-empty on exactly the two rows where something was attempted, which is
    /// what separates *a move was made, judged and taken back* from *there was
    /// no move to make*.
    pub attempts: Vec<AttemptOutcome>,

    /// The shared branch this run's commits landed on, on the one row where any
    /// did.
    pub branch: Option<String>,

    /// The pull request this run's work is in, or the open one that already
    /// covered it.
    pub pull_request: Option<u64>,
}

/// The machine-readable record of one attempt, as design §4.7 specifies it.
///
/// Everything a reader needs to reconstruct the attempt without re-running it:
/// which build ran (`fiddle`), what it was asked to do (`invocation_ref`,
/// `work_ref`, `mode`), which attempt this was (`attempt_id`), how it ended
/// (`outcome`, `next_action`), what it actually did (`capability_executions`,
/// `progress`), and what it saw (`observations`).
///
/// `capability_executions` and `progress` are both present and both may be
/// empty: an empty pair is the positive claim that nothing was executed, which
/// is exactly what a run over already-satisfied work has to be able to say. The
/// outcome alone could not distinguish that from a run that did the work again.
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
    /// What the run came to and the evidence for that reason, where the
    /// capability that ran has a disposition table of its own.
    ///
    /// Absent rather than neutral when it does not apply, exactly as
    /// [`WorkStateView::tree`](crate::observation::WorkStateView::tree) is and
    /// for its reason: *which row did this run reach* is not a question a
    /// capability with no table can be asked, so a defaulted value here would be
    /// an answer nobody gave. Every bundle M0, M1, M2 and M3 have ever published
    /// is therefore byte-identical.
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

    /// The two admissible spellings, kept apart from everything else: a sha is
    /// carried verbatim (lowercased), and nothing else survives as one.
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

    /// The wire shape is the contract every downstream reader is written
    /// against, so the key names are asserted rather than assumed.
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
        // The bundle's schema is unchanged by two more observations, because a
        // reader dispatches on `schema` and then reads by path: `v0` promised
        // these keys and still carries them.
        assert!(value["observations"]["review"]["not_applicable"].is_object());
        assert!(value["observations"]["verification"]["not_applicable"].is_object());
        // And unchanged by the disposition key: a capability with no table
        // answers `None`, and the bundle carries no key rather than a `null` a
        // reader would have to interpret. This is the M0 bundle, byte for byte.
        assert!(
            value.get("disposition").is_none(),
            "a run with no disposition table must publish no key: {value}"
        );
    }

    /// The other half: a capability that *has* a table publishes it, under keys
    /// a reader can dispatch on.
    ///
    /// Asserted by path and by value rather than by a `Debug` rendering, because
    /// the wire shape is the contract — the whole point of this key is that a
    /// reader outside the process can tell one row from another, and a rendering
    /// nobody parses would move that distinction back inside.
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
                forbidden: vec!["a_test.go added a skipped test: t.Skip()".to_string()],
            }],
            branch: None,
            pull_request: Some(7),
        })
        .unwrap();

        assert_eq!(value["reason"], "unsafe_without_direction");
        assert_eq!(value["verdicts"], 2);
        assert_eq!(value["already_fixed"][0], "CVE-2026-0003");
        assert_eq!(value["deferred"][0]["cve"], "CVE-2026-0004");
        assert_eq!(value["deferred"][0]["bound"], 5);
        assert_eq!(value["attempts"][0]["cves"][0], "CVE-2026-0001");
        assert_eq!(value["attempts"][0]["status"], "needs_work");
        // The one field the product reads nowhere and publishes here. A record
        // that dropped it would leave a reader unable to see that the model said
        // it had finished and the check disagreed.
        assert_eq!(value["attempts"][0]["claimed_complete"], true);
        assert_eq!(
            value["attempts"][0]["forbidden"][0],
            "a_test.go added a skipped test: t.Skip()"
        );
        // `null` and not absent, both of them: *this run landed nothing* and
        // *this run's build is older than the key* must not be the same reading.
        assert!(value["branch"].is_null());
        assert_eq!(value["pull_request"], 7);
    }
}
