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
//! # Why the unsuccessful arms are six variants and not one
//!
//! [`ScanError`] has one variant per way a scan can fail to produce a report,
//! because those are six different situations for whoever is looking: a scanner
//! that is not installed, one that ran and gave up, one that wrote nothing, one
//! that wrote something unreadable, an image that does not exist, and a
//! container daemon that is not listening. A single variant carrying a reason
//! string would make all six one value, and a test could then only assert that
//! *some* string came back — it could not assert that a broken scanner and a
//! mistyped image tag are told apart, which is the property that matters,
//! because only one of them is worth retrying.
//!
//! The fields those variants carry are diagnostics and nothing else. What
//! discriminates an arm is always the variant.
//!
//! # Which of them a repeat gets past
//!
//! [`ScanError::recurrence`] is that question, answered per variant in one
//! visible table and exhaustively, so a seventh arm cannot be added without its
//! author being asked it. It is the same three-valued vocabulary the effect
//! layer uses — [`crate::effect::Recurrence`] — reached across rather than
//! copied, because a scan failure and an effect failure end the same run and a
//! second enum meaning the same three things is a second table to keep in step.
//!
//! Two arms are correctable and four are permanent, and the split is not about
//! which of them a person could do something about — all six have a remedy. It
//! is ADR 016's test: whether the failure is an **obstacle in front of** the
//! request or a **conclusion about** it. [`ScanError::DaemonUnreachable`] is the
//! clearest obstacle this module has, and [`ScanError::Failed`] is the other,
//! because that is where a deadline and a cancellation arrive — which is the
//! same reading as the paragraph above, where a broken scanner is the one of
//! that pair worth retrying. The four that remain are the image not existing and
//! the artefact being wrong, and both of those come back identical from an
//! identical invocation.
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
/// See the module header for why this is six variants. Each carries enough to
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

    /// The container daemon a scanner reaches its images through is not
    /// listening, so nothing was inspected.
    ///
    /// A variant of its own for [`ScanError::ImageAbsent`]'s reason and one
    /// more. Left in [`ScanError::Failed`] it is indistinguishable from a
    /// scanner that ran and gave up, and those have opposite remedies — one
    /// sends an operator to the scanner, the other to a host that is simply
    /// down. It is also the only arm here a repeat gets past with nothing
    /// changed, so collapsing it costs the run its exit row as well as its
    /// diagnostic.
    ///
    /// **No image field, deliberately.** The other absent-artefact arms name
    /// what was asked for because the reference is the thing in question; here
    /// the reference was never resolved and quoting it would suggest the tag is
    /// what to look at.
    ///
    /// # `DOCKER_HOST` is named, and nothing in this build reads it
    ///
    /// The message names it because it is the operator's remedy — the variable
    /// they set in their own shell when their daemon is not on the default
    /// socket — and a diagnostic that named only the socket would send an
    /// operator pointed at a remote daemon to look at a file that was never
    /// going to be there.
    ///
    /// It is *not* in any allowlist and this build never reads or sets it. That
    /// was measured rather than assumed on 2026-08-13: under `env_clear` plus
    /// `PATH`, `HOME` and `LANG`, the client reaches the daemon over the default
    /// Unix socket, on a CI runner as much as locally, and *setting*
    /// `DOCKER_HOST` wrongly is what breaks that. So a workspace command still
    /// runs under four names — `workspace::a_workspace_command_inherits_no_credential`
    /// pins them — and this adapter under five. A planned ADR admitting a sixth
    /// was found not to be owed and was dropped.
    #[error(
        "the container daemon could not be reached, so no image was inspected; \
         start it, or point DOCKER_HOST at the one that is listening: {stderr}"
    )]
    DaemonUnreachable { stderr: String },
}

impl ScanError {
    /// Which exit row a run that reached this failure belongs in, decided per
    /// variant and in one visible table.
    ///
    /// Exhaustive by construction — no wildcard arm — for the reason
    /// [`crate::effect::EffectError::recurrence`] is: a seventh variant cannot
    /// be added without its author being made to answer this question by the
    /// compiler. That is not hypothetical here. Every arm below except the last
    /// was written before this method existed, and a scan that could not reach
    /// its daemon was one of them.
    ///
    /// The vocabulary is the effect layer's rather than one of this module's
    /// own, because a scan failure and an effect failure end the same run
    /// through the same exit table; see the module header.
    pub fn recurrence(&self) -> crate::effect::Recurrence {
        use crate::effect::Recurrence;
        match self {
            // Nothing is at that path. The next invocation resolves the same
            // seam to the same nothing, which is `Permanent`'s test exactly —
            // and it is the same argument `EffectError::PolicyDenied` makes: an
            // operator who installs a scanner is not repeating this invocation,
            // they are describing a different deployment and running against it.
            ScanError::Missing { .. } => Recurrence::Permanent,

            // The catch-all, and it is correctable because of what is *in* it
            // rather than in spite of it. This is where a deadline and a
            // cancellation arrive — see the module header for why they have no
            // arms of their own — along with a spawn this runtime could not
            // complete and a scratch directory it could not create. None of
            // those is a conclusion about the image; every one of them is
            // something the next attempt may simply not meet. It is also the
            // reading the header has always taken of the pair *a broken scanner
            // and a mistyped tag*: this is the one worth retrying.
            ScanError::Failed { .. } => Recurrence::Correctable,

            // The artefact is wrong, which is a fact about what this scanner
            // writes for this image and not about the machine it ran on. An
            // empty report and a truncated one both come back byte for byte
            // from an identical invocation; the remedy is a scanner version or
            // a defect report, and neither is reached by running it again.
            ScanError::NoOutput { .. } | ScanError::Unparseable { .. } => Recurrence::Permanent,

            // The tag never resolved. Repeating asks the same registry the same
            // question — the conclusion, not an obstacle in front of it. Its own
            // doc comment already says the remedy is the caller's.
            ScanError::ImageAbsent { .. } => Recurrence::Permanent,

            // The one arm that is unambiguously an obstacle: nothing about the
            // image, the scanner or the credential is wrong, and the host comes
            // back. `Recurrence::Correctable`'s own definition — "a network
            // comes back, a rate limit lifts" — is this situation, and it is
            // what makes the remedy the message names worth acting on rather
            // than something to read after the run has been abandoned.
            ScanError::DaemonUnreachable { .. } => Recurrence::Correctable,
        }
    }
}
