//! The scanner documents, as bytes, and the advisory sentinel they carry.
//!
//! # Why this is a file of its own rather than more of `cve.rs`
//!
//! [`cve`](super) states the rule these builders exist to serve: *the stub is
//! where a document meets the disk, and that stub's arms should print these
//! bytes rather than embed a second copy of them.* Honouring it means the
//! scripted `wizcli` — a `[[bin]]` target — has to be able to see the builders,
//! and a `[[bin]]` is compiled against `[dependencies]` alone. `cve.rs` reaches
//! `tempfile` for its Go trees, which is a `[dev-dependency]`, so including that
//! file into the stub does not compile.
//!
//! Splitting the document half out is what makes the shared-bytes rule
//! satisfiable rather than aspirational. The alternative — the stub embedding a
//! document of its own — is the second copy the rule forbids, and it would drift
//! from the one the projection lanes assert against, which is exactly the
//! failure that would present as a projection bug.
//!
//! Nothing here reaches anything but `serde_json`, and that is the constraint
//! this file has to keep: a helper added below that needs a dev-dependency puts
//! the stub back where it started.
//!
//! Every name is re-exported from `cve`, so callers still write
//! `support::cve::report_with(..)` and the split is invisible to them.

// ---------------------------------------------------------------------------
// The advisory sentinel
// ---------------------------------------------------------------------------

/// Advisory prose, planted where a scanner document carries a description.
///
/// The projection is meant to carry six fields and no free text, and a report
/// whose description is something innocuous cannot tell *dropped the prose* apart
/// from *there was no prose*.
///
/// Here rather than beside the other three sentinels in `cve.rs` because it is
/// the only one a *document* carries, and the documents had to come to the stub.
/// `cve::ALL_SENTINELS` still lists it, so "no two sentinels can be confused" is
/// asserted over all four.
pub const SENTINEL_PROSE: &str = "fiddle-prose-c47a06f9";

// ---------------------------------------------------------------------------
// Scanner documents
// ---------------------------------------------------------------------------

/// Library packages a scanner document can report, cycled by position so that two
/// advisory ids produce two different packages.
const LIBRARY_PACKAGES: [(&str, &str, &str); 3] = [
    ("golang.org/x/crypto", "v0.31.0", "v0.35.0"),
    ("golang.org/x/net", "v0.24.0", "v0.28.0"),
    ("github.com/docker/docker", "v24.0.7", "v24.0.9"),
];

/// The same for OS packages, whose versions are a distribution's and not a
/// module's — which is why the two arrays cannot share a projection rule.
const OS_PACKAGES: [(&str, &str, &str); 3] = [
    ("libssl3", "3.0.11-r0", "3.0.12-r0"),
    ("busybox", "1.36.1-r5", "1.36.1-r7"),
    ("zlib", "1.3-r0", "1.3.1-r0"),
];

/// The advisory description a document carries unless a test asked for prose.
///
/// Innocuous, and it has to be: a default of [`SENTINEL_PROSE`] would put the
/// sentinel in every world, and "the prose did not cross the boundary" would then
/// be untestable because no document lacks it.
const BENIGN_DESCRIPTION: &str = "a benign advisory summary";

/// What the library array holds when a variant does not say.
pub const DEFAULT_LIBRARY_CVES: [&str; 1] = ["CVE-2026-0001"];

/// What the OS array holds when a variant does not say.
pub const DEFAULT_OS_CVES: [&str; 1] = ["CVE-2026-0002"];

/// A second advisory against the base layer, for the one document that needs
/// two of them.
///
/// A constant of its own rather than a second entry in [`DEFAULT_OS_CVES`], and
/// the doc comment above says why: that array is what a document holds *unless a
/// variant says otherwise*, so a second id in it would add a finding to every
/// document every other caller builds. Lanes that assert a verdict count or a
/// budget's arithmetic would change meaning with nobody having touched them.
pub const SECOND_OS_CVE: &str = "CVE-2026-0005";

/// A second advisory against a *library*, in the second package of
/// [`LIBRARY_PACKAGES`] rather than the first.
///
/// The one document that needs it is the two-group world: a run forms one group
/// per bump target, so two library findings against two different modules are
/// the only shape from which a second *attemptable* group comes — every other
/// document here has one library finding and an OS one, and the OS half is
/// always a base image nothing can select a tag for. Without a second module
/// there is no second group, and with no second group `cve::fold` has no earlier
/// rescan to be consulted over.
///
/// A constant of its own for [`SECOND_OS_CVE`]'s reason, which applies here
/// twice over: an extra entry in [`DEFAULT_LIBRARY_CVES`] would add a second
/// *fixable, attributable* finding to every document, and every lane that counts
/// attempts or bounds findings would change meaning with nobody having touched
/// it.
pub const SECOND_LIBRARY_CVE: &str = "CVE-2026-0003";

/// A vulnerable package, as a scanner reports one.
#[derive(Debug, Clone)]
struct Package {
    name: String,
    version: String,
    vulnerabilities: Vec<serde_json::Value>,
}

/// The `libraries` half of a scanner document.
///
/// A type of its own, and so is [`OsPackages`], for one reason:
/// `report_with(libraries(..), os_packages(..))` takes two arrays of the same
/// shape, and two `Vec`s could be handed over the wrong way round with nothing to
/// notice. A projection bug and a transposed fixture look identical in the
/// result, so the transposition is made a compile error instead.
#[derive(Debug, Clone)]
pub struct Libraries(Vec<Package>);

/// The `osPackages` half. See [`Libraries`].
#[derive(Debug, Clone)]
pub struct OsPackages(Vec<Package>);

/// Library packages, one per advisory id.
pub fn libraries(cves: &[&str]) -> Libraries {
    Libraries(packages(cves, &LIBRARY_PACKAGES))
}

/// Library packages whose advisories name **no published fix**.
///
/// Added by Task 16, which is the first lane that needs an *upstream-blocked*
/// finding: Design §3's second row is the fixable set being empty while there is
/// still something to report, and every other builder here writes a
/// `fixedVersion` so no document produced by them can reach it.
///
/// The field is **absent** rather than `null` or `""`, which is what
/// [`vulnerability`] does with a `None` and what the reference pipeline
/// produces. That matters here more than anywhere else: `fiddle_core::selected`
/// treats all three alike, so a fixture that wrote one of the other two would
/// still be selected and would still be blocked, and the lane would pass without
/// ever having produced the document a real scanner writes.
pub fn unfixed_libraries(cves: &[&str]) -> Libraries {
    Libraries(unfixed_packages(cves, &LIBRARY_PACKAGES))
}

/// OS packages, one per advisory id.
pub fn os_packages(cves: &[&str]) -> OsPackages {
    OsPackages(packages(cves, &OS_PACKAGES))
}

fn packages(cves: &[&str], table: &[(&str, &str, &str); 3]) -> Vec<Package> {
    cves.iter()
        .enumerate()
        .map(|(at, cve)| {
            let (name, current, fixed) = table[at % table.len()];
            Package {
                name: name.to_string(),
                version: current.to_string(),
                vulnerabilities: vec![vulnerability(cve, Some(fixed), BENIGN_DESCRIPTION)],
            }
        })
        .collect()
}

/// [`packages`] with the fix withheld. The package's own name and version still
/// come from the table, so a blocked finding and a fixable one against the same
/// position differ in exactly one key.
fn unfixed_packages(cves: &[&str], table: &[(&str, &str, &str); 3]) -> Vec<Package> {
    cves.iter()
        .enumerate()
        .map(|(at, cve)| {
            let (name, current, _fixed) = table[at % table.len()];
            Package {
                name: name.to_string(),
                version: current.to_string(),
                vulnerabilities: vec![vulnerability(cve, None, BENIGN_DESCRIPTION)],
            }
        })
        .collect()
}

/// One reported vulnerability.
///
/// `HIGH` because that is the severity the selection rule admits on its own: a
/// fixture at a lower severity would be selected only through `hasExploit`, and
/// then every test about selection would be about the other arm.
///
/// # Why the record carries more than the projection admits
///
/// `ProjectedFinding` declares `deny_unknown_fields` over exactly six names, so
/// **nothing can deserialize one of these into it** — the shape is nested, the
/// keys are the scanner's, and `packageType` is not in the document at all but is
/// a fact about *which array* the record was in. That refusal is the injection
/// boundary working, and a fixture emitting only the six fields would make the
/// projection step look like ceremony and leave the boundary assertion with
/// nothing to strip. So the record stays the scanner's: nested under a package,
/// carrying `hasExploit` and a free-form `description` no typed value admits.
fn vulnerability(cve: &str, fixed: Option<&str>, description: &str) -> serde_json::Value {
    let mut value = serde_json::json!({
        "name": cve,
        "severity": "HIGH",
        "hasExploit": false,
        "description": description,
    });
    // Absent rather than null where there is no fix, because absent is what the
    // reference pipeline produces and the two are not the same document.
    if let Some(fixed) = fixed {
        value["fixedVersion"] = serde_json::Value::String(fixed.to_string());
    }
    value
}

fn as_json(packages: &[Package]) -> serde_json::Value {
    serde_json::Value::Array(
        packages
            .iter()
            .map(|package| {
                serde_json::json!({
                    "name": package.name,
                    "version": package.version,
                    "vulnerabilities": package.vulnerabilities,
                })
            })
            .collect(),
    )
}

/// Which scanner document a world holds.
#[derive(Debug, Clone)]
pub enum ReportVariant {
    /// The ordinary document: whatever the two arrays were given.
    Plain(Libraries, OsPackages),
    /// No `osPackages` key at all.
    OsAbsent,
    /// An `osPackages` key holding an empty array.
    OsEmpty,
    /// No `libraries` key at all.
    ///
    /// The mirror of [`ReportVariant::OsAbsent`], and it is here because the two
    /// halves are the same claim: a document missing either array is a scanner
    /// that reported on half the image, and a reader deciding anything from the
    /// silence is deciding it from nothing. Only the OS half had a world until
    /// the rescan judgement needed both.
    LibrariesAbsent,
    /// One advisory reported twice, once with a fix and once without.
    DuplicateCve(String),
    /// A document carrying advisory prose.
    AdvisoryDescription(String),
}

/// How many document variants there are, pinning [`canonical_reports`]'s length.
/// The count is written down rather than inferred for the reason `cve.rs`'s
/// `SHAPES` records: a guard that computes its expectation from the list it is
/// checking is a list compared to itself, and it stayed green when an entry was
/// deleted.
const REPORT_VARIANTS: usize = 6;

impl ReportVariant {
    /// This variant's position in [`canonical_reports`]. The match is exhaustive,
    /// so a new variant cannot be added without being given a position, and the
    /// highest position here has to agree with [`REPORT_VARIANTS`].
    pub fn index(&self) -> usize {
        match self {
            ReportVariant::Plain(_, _) => 0,
            ReportVariant::OsAbsent => 1,
            ReportVariant::OsEmpty => 2,
            ReportVariant::DuplicateCve(_) => 3,
            ReportVariant::AdvisoryDescription(_) => 4,
            ReportVariant::LibrariesAbsent => 5,
        }
    }

    /// A short name for a failure message. Derived from the variant rather than
    /// written beside each construction, so it cannot label the wrong document.
    pub fn label(&self) -> String {
        match self {
            ReportVariant::Plain(Libraries(l), OsPackages(o)) => {
                format!("plain({} libraries, {} os packages)", l.len(), o.len())
            }
            ReportVariant::OsAbsent => "os-absent".to_string(),
            ReportVariant::OsEmpty => "os-empty".to_string(),
            ReportVariant::DuplicateCve(cve) => format!("duplicate({cve})"),
            ReportVariant::AdvisoryDescription(_) => "advisory-description".to_string(),
            ReportVariant::LibrariesAbsent => "libraries-absent".to_string(),
        }
    }

    fn render(&self) -> Report {
        let mut result = serde_json::Map::new();
        match self {
            ReportVariant::Plain(Libraries(l), OsPackages(o)) => {
                result.insert("libraries".to_string(), as_json(l));
                result.insert("osPackages".to_string(), as_json(o));
            }
            // The key is left out entirely, which is the whole of this world: a
            // reader that treats absent as empty and one that refuses cannot be
            // told apart by a document that has the key.
            ReportVariant::OsAbsent => {
                result.insert(
                    "libraries".to_string(),
                    as_json(&packages(&DEFAULT_LIBRARY_CVES, &LIBRARY_PACKAGES)),
                );
            }
            ReportVariant::OsEmpty => {
                result.insert(
                    "libraries".to_string(),
                    as_json(&packages(&DEFAULT_LIBRARY_CVES, &LIBRARY_PACKAGES)),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
            // The same omission on the other side. The library half it leaves
            // out is the half the two arms above hold constant, so the three
            // documents differ in exactly which array is missing.
            ReportVariant::LibrariesAbsent => {
                result.insert(
                    "osPackages".to_string(),
                    as_json(&packages(&DEFAULT_OS_CVES, &OS_PACKAGES)),
                );
            }
            // Two packages, one advisory, one fix between them. The rule this is
            // for splits fixable from upstream-blocked by subtraction, and a
            // document where the id appears once cannot show a filter putting it
            // in both sets.
            ReportVariant::DuplicateCve(cve) => {
                let (fixable_name, fixable_version, fixed) = LIBRARY_PACKAGES[0];
                let (blocked_name, blocked_version, _) = LIBRARY_PACKAGES[1];
                result.insert(
                    "libraries".to_string(),
                    as_json(&[
                        Package {
                            name: fixable_name.to_string(),
                            version: fixable_version.to_string(),
                            vulnerabilities: vec![vulnerability(
                                cve,
                                Some(fixed),
                                BENIGN_DESCRIPTION,
                            )],
                        },
                        Package {
                            name: blocked_name.to_string(),
                            version: blocked_version.to_string(),
                            vulnerabilities: vec![vulnerability(cve, None, BENIGN_DESCRIPTION)],
                        },
                    ]),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
            ReportVariant::AdvisoryDescription(prose) => {
                let (name, version, fixed) = LIBRARY_PACKAGES[0];
                result.insert(
                    "libraries".to_string(),
                    as_json(&[Package {
                        name: name.to_string(),
                        version: version.to_string(),
                        vulnerabilities: vec![vulnerability(
                            DEFAULT_LIBRARY_CVES[0],
                            Some(fixed),
                            prose,
                        )],
                    }]),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
        }
        Report {
            raw: serde_json::to_string_pretty(&serde_json::json!({ "result": result }))
                .expect("a document built from json! values serializes"),
        }
    }
}

/// A scanner document, as bytes.
///
/// The scanner version and the image digest are not in here. Those are what the
/// *scan* recorded rather than what the document said, and the adapter resolves
/// them from the child's own diagnostics. If it turns out `wizcli` puts them in
/// the file after all, the fields belong here and this note is what should
/// change.
#[derive(Debug, Clone)]
pub struct Report {
    raw: String,
}

impl Report {
    /// The bytes a scanner would have written.
    ///
    /// Pretty-printed, which no parser cares about and a failing `assert_ne!`
    /// does: the two documents a lane could not tell apart are readable side by
    /// side.
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// A document holding exactly these two arrays.
pub fn report_with(libraries: Libraries, os_packages: OsPackages) -> Report {
    ReportVariant::Plain(libraries, os_packages).render()
}

/// A document with no `osPackages` key.
pub fn report_with_os_absent() -> Report {
    ReportVariant::OsAbsent.render()
}

/// A document whose `osPackages` key holds an empty array.
pub fn report_with_os_empty() -> Report {
    ReportVariant::OsEmpty.render()
}

/// A document with no `libraries` key. See [`ReportVariant::LibrariesAbsent`].
pub fn report_with_libraries_absent() -> Report {
    ReportVariant::LibrariesAbsent.render()
}

/// A document reporting `cve` twice, once with a fix and once without.
pub fn report_with_duplicate_cve_one_fixed_one_not(cve: &str) -> Report {
    ReportVariant::DuplicateCve(cve.to_string()).render()
}

/// A document whose advisory carries `text` as its description.
pub fn report_with_advisory_description(text: &str) -> Report {
    ReportVariant::AdvisoryDescription(text.to_string()).render()
}

/// One document per variant, so completeness can be checked. See
/// [`ReportVariant::index`], and `cve::all_shapes` for why this is an array.
pub fn canonical_reports() -> [ReportVariant; REPORT_VARIANTS] {
    [
        ReportVariant::Plain(
            libraries(&DEFAULT_LIBRARY_CVES),
            os_packages(&DEFAULT_OS_CVES),
        ),
        ReportVariant::OsAbsent,
        ReportVariant::OsEmpty,
        ReportVariant::DuplicateCve("CVE-2026-0777".to_string()),
        ReportVariant::AdvisoryDescription(SENTINEL_PROSE.to_string()),
        ReportVariant::LibrariesAbsent,
    ]
}

/// Every document a lane needs to tell from every other, labelled.
///
/// Built on top of [`canonical_reports`] rather than beside it, so a variant added
/// there is compared here without anybody remembering to; the two extra entries
/// are the one-sided arrays the projection has to read both of.
pub fn distinct_reports() -> Vec<(String, Report)> {
    let mut variants: Vec<ReportVariant> = canonical_reports().into_iter().collect();
    variants.push(ReportVariant::Plain(
        libraries(&["CVE-1"]),
        os_packages(&[]),
    ));
    variants.push(ReportVariant::Plain(
        libraries(&[]),
        os_packages(&["CVE-1"]),
    ));
    variants
        .into_iter()
        .map(|variant| (variant.label(), variant.render()))
        .collect()
}
