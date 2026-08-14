//! Running a container scanner over an image, and deciding what it produced.
//!
//! The port is [`Scanner`] and the contract is a **subprocess**: a program this
//! project did not write is handed an image reference, and what comes back is a
//! file, an exit status and two streams of text. Nothing here links a scanner as
//! a library, which is what lets an operator pin, wrap or replace one, and what
//! lets the whole capability be gated offline against a scripted `wizcli`.
//!
//! This file is the port and its two typed answers. The one adapter is
//! [`wizcli`], in a file of its own because it is where a tenant credential
//! becomes a running process — the same separation [`crate::github`] keeps
//! between its operations and the single `gh` construction site under them.
//!
//! # Success is the artefact, not the status line
//!
//! This module inverts the usual rule and it is the single most important thing
//! in it. `wizcli` exits non-zero when an organisation policy flags *any*
//! finding, including findings that have nothing to do with the scan that was
//! asked for, so an adapter that read the exit code first would report a
//! perfectly good report as a failed scan — and the honest handling of a failed
//! scan is to stop, which means a policy hit somewhere else in the tenant would
//! silently disable this capability. So [`Wizcli::scan`] reads the artefact
//! first and consults the exit code only to disambiguate its *absence*.
//!
//! # Why the unsuccessful arms are five variants and not one
//!
//! [`ScanError`] has one variant per way a scan can fail to produce a report,
//! because those are five different situations for whoever is looking: a scanner
//! that is not installed, one that ran and gave up, one that wrote nothing, one
//! that wrote something unreadable, and an image that does not exist. A single
//! variant carrying a reason string would make all five one value, and a test
//! could then only assert that *some* string came back — it could not assert
//! that a broken scanner and a mistyped image tag are told apart, which is the
//! property that matters, because only one of them is worth retrying.
//!
//! The fields those variants carry are diagnostics and nothing else. What
//! discriminates an arm is always the variant.
//!
//! # What a scan is not
//!
//! A scan reads and changes nothing outside this process, and that is why there
//! is no ambiguous-write vocabulary here of the kind [`crate::git::GitError`]
//! and [`crate::github::GhError`] carry. A push whose answer was lost may have
//! moved a ref; a scan whose answer was lost produced no report, and *produced no
//! report* is the whole of what a caller can act on. So a deadline and a
//! cancellation both arrive as [`ScanError::Failed`] rather than as arms of
//! their own — not because the provenance is unknown, but because no caller
//! would do anything different with it.

use async_trait::async_trait;
use std::path::PathBuf;

pub mod wizcli;

pub use wizcli::{WizCredential, Wizcli, REDACTED};

/// A container scanner, as this build is allowed to see one.
///
/// One method, because a scan is one question. The trait exists rather than a
/// bare `Wizcli` so the capability that consumes findings can be written and
/// tested against the port, and so a second scanner — or a recording one — is a
/// new implementation rather than an edit to every caller.
#[async_trait]
pub trait Scanner {
    /// Scan `image` and return what the scanner wrote, or why nothing usable
    /// came back.
    ///
    /// `image` is a reference in whatever spelling the scanner accepts — a tag,
    /// a digest, a local id. It is not parsed here: this layer's business is
    /// running the program, and a reference this build rejected but the scanner
    /// would have accepted is a scan that did not happen for no reason.
    async fn scan(&self, image: &str) -> Result<ScanReport, ScanError>;
}

/// What one scan produced.
///
/// The provenance travels *with* the document rather than beside it, because the
/// question a later rescan asks is "did the same scanner, over the same image,
/// still see this?" — and a report that cannot say which scanner or which image
/// it came from cannot answer it. A clean scan is the case that makes this
/// load-bearing: an empty finding list is only evidence if something can say
/// what produced it.
#[derive(Clone, Debug)]
pub struct ScanReport {
    /// The document the scanner wrote, parsed and otherwise untouched.
    ///
    /// Deliberately still JSON. Turning this into typed findings is a
    /// *projection* — six fields chosen out of a record that carries dozens,
    /// dropping the upstream prose — and it belongs to [`crate::cve`], which is
    /// where the boundary can be argued for and asserted. An adapter that
    /// projected on the way out would put that boundary in the one place a test
    /// of the projection cannot reach.
    pub document: serde_json::Value,
    /// Which scanner said so, as the scanner itself reported it.
    pub scanner_version: String,
    /// Which image it looked at, by digest rather than by tag: a tag is a name
    /// somebody can move, and two scans of one tag are not necessarily two scans
    /// of one image.
    pub image_digest: String,
}

impl ScanReport {
    /// Every vulnerability record the document reported, across both package
    /// arrays, exactly as the scanner wrote them.
    ///
    /// A count and not a projection. It answers one question — *did this scan
    /// report anything at all?* — and it deliberately does not distinguish an
    /// absent `osPackages` from an empty one, does not select by severity and
    /// does not reshape a record. All of that is the projection's, and a caller
    /// that wants findings fiddle will act on wants that rather than this.
    pub fn findings(&self) -> Vec<&serde_json::Value> {
        ["libraries", "osPackages"]
            .iter()
            .filter_map(|array| self.document["result"][array].as_array())
            .flatten()
            .filter_map(|package| package["vulnerabilities"].as_array())
            .flatten()
            .collect()
    }
}

/// Why a scan produced no report this build can use.
///
/// See the module header for why this is five variants. Each carries enough to
/// diagnose the situation and nothing that discriminates it.
///
/// Every field quoting a child's own words is redacted on the way in — see
/// [`wizcli`] — so the tenant secret cannot ride out of the process inside a
/// diagnostic it was never meant to reach.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// There is no scanner to run: the program is not on disk, not executable,
    /// or the spawn failed for a reason of the operating system's.
    ///
    /// The only arm the scripted scanner cannot produce, because the scripted
    /// scanner *is* the program. It is reached by pointing the seam at a path
    /// that holds nothing.
    #[error("the scanner {} could not be started: {reason}", program.display())]
    Missing { program: PathBuf, reason: String },

    /// The scanner ran, wrote no report, and did not say the image was absent.
    ///
    /// Also where a deadline and a cancellation arrive — see the module header
    /// for why a scan has no ambiguous-write arm to put them in.
    #[error("the scanner produced no report ({status}): {stderr}")]
    Failed { status: String, stderr: String },

    /// The report is where it was asked for and holds nothing.
    ///
    /// Distinct from [`ScanError::Unparseable`] because they are different
    /// defects: a scanner that created its output file and then died wrote
    /// nothing, while one that wrote a truncated or non-JSON document wrote the
    /// wrong thing. Collapsing them would report a crashed scanner as a version
    /// mismatch.
    #[error("the scanner wrote an empty report to {}", path.display())]
    NoOutput { path: PathBuf },

    /// The report is there and is not a document this build can read.
    #[error("the report at {} is not a scanner document: {reason}", path.display())]
    Unparseable { path: PathBuf, reason: String },

    /// The image named does not exist, so there was nothing to scan.
    ///
    /// A variant of its own rather than a [`ScanError::Failed`] with a
    /// suggestive message, because it is the one failure here whose remedy is
    /// the caller's: every other arm is something an operator fixes, and this
    /// one means the tag this run was given never resolved. Reporting it as a
    /// generic failure is how a mistyped tag becomes "the scanner is broken".
    #[error("no image {image}: {stderr}")]
    ImageAbsent { image: String, stderr: String },
}
