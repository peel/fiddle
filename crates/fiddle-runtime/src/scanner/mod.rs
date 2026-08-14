//! Running a container scanner over an image, and deciding what it produced.
//!
//! The port is [`Scanner`] and the contract is a **subprocess**: a program this
//! project did not write is handed an image reference, and what comes back is a
//! file, an exit status and two streams of text. Nothing here links a scanner as
//! a library, which is what lets an operator pin, wrap or replace one, and what
//! lets the whole capability be gated offline against a scripted `wizcli`.
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

use crate::process::{run_bounded, Bounded};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

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

// ---------------------------------------------------------------------------
// The `wizcli` adapter
// ---------------------------------------------------------------------------

/// What the report is called inside the scratch directory.
///
/// A fixed name rather than one per call: the directory belongs to this adapter,
/// so there is nothing to collide with, and a stable name is what makes a failed
/// scan's leftovers findable by whoever is debugging one.
const REPORT_FILE: &str = "scan.json";

/// The scanner, reached as a subprocess.
///
/// `program` and `args` are the operator seam — the same shape
/// [`crate::GhCli`] carries for `gh`, and for the same reason: an operator who
/// must pin a version or wrap the binary in a launcher has somewhere to do it,
/// and the offline gate substitutes a scripted scanner through it rather than
/// through the environment, which is pinned.
pub struct Wizcli {
    program: PathBuf,
    args: Vec<String>,
    /// Where the report is written. Supplied rather than created here so the
    /// directory's lifetime belongs to whoever owns the attempt: a scan's
    /// artefact should survive long enough to be published as evidence, and
    /// exactly as long as the attempt that produced it.
    scratch: PathBuf,
    timeout: Duration,
    /// Held rather than passed per call, because a scan takes no context: this
    /// adapter is built for one attempt, and the token that ends that attempt is
    /// the token that must end its children. It is the only channel a `^C` has
    /// to a child in a process group of its own — see [`crate::process`].
    cancel: CancellationToken,
}

impl Wizcli {
    pub fn new(
        program: PathBuf,
        args: Vec<String>,
        scratch: PathBuf,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            program,
            args,
            scratch,
            timeout,
            cancel,
        }
    }

    /// Where this scan's report will be.
    fn report_path(&self) -> PathBuf {
        self.scratch.join(REPORT_FILE)
    }
}

#[async_trait]
impl Scanner for Wizcli {
    async fn scan(&self, image: &str) -> Result<ScanReport, ScanError> {
        let report = self.report_path();
        // Removed before the scanner runs, and this is not tidiness. The rule
        // below is *the artefact decides*, so a report left behind by an earlier
        // scan would be read as this one's — and a scan of an image that does
        // not exist would come back carrying the previous image's findings,
        // which is the worst failure this module could have.
        if let Err(source) = remove_if_present(&report) {
            return Err(ScanError::Failed {
                status: "the previous report could not be cleared".to_string(),
                stderr: source.to_string(),
            });
        }

        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .arg("--json-output-file")
            .arg(&report)
            .arg(image);

        // `env_clear` then an explicit allowlist, which is what every other
        // spawn site in this runtime does and for the same reason: a credential
        // added to the runner tomorrow is excluded by default rather than by
        // somebody remembering to deny it. `std::env::remove_var` would mutate
        // this process and is wrong for a concurrent runtime.
        //
        // **One name so far, and that is the honest state of it.** A scanner
        // needs `PATH` to find whatever it shells out to — the container runtime
        // it inspects images through — and it needs nothing else this adapter
        // can argue for yet. The names that carry the tenant credential belong
        // with the authentication channel, and this adapter does not have one:
        // Wiz is testable only in CI, so guessing the channel here would mean
        // guessing twice. The allowlist grows in the same change that adds them,
        // where the assertion that pins it can be written beside them.
        //
        // No `HOME`, deliberately and permanently, for the reason `gh` has none:
        // with `HOME` gone, the child cannot reach an operator's ambient
        // scanner configuration, so "it used the credential it was given" stays
        // a fact about the process rather than a claim about it.
        command.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            command.env("PATH", path);
        }

        // No stdin: a scan is unattended, and a program that waited on a
        // terminal it does not have would hang until the deadline rather than
        // report anything. The bound below — the deadline, the process group and
        // the group kill — is `process`'s, shared with every other child this
        // runtime starts.
        let bounded = run_bounded(&mut command, None, self.timeout, &self.cancel).await;
        let output = match bounded {
            // `NotFound` is the one io error that says something about the
            // world rather than about this process, and it is the whole of
            // `Missing`: there is no scanner at that path. Everything else —
            // a permission refusal, a resource limit, a wait that failed — is
            // this runtime failing to run a scanner that may well exist.
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(ScanError::Missing {
                    program: self.program.clone(),
                    reason: source.to_string(),
                })
            }
            Err(source) => {
                return Err(ScanError::Failed {
                    status: "the scanner could not be run".to_string(),
                    stderr: source.to_string(),
                })
            }
            Ok(Bounded::TimedOut) => {
                return Err(ScanError::Failed {
                    status: format!("killed after {:?}", self.timeout),
                    stderr: String::new(),
                })
            }
            Ok(Bounded::CancelledAfterSpawn) => {
                return Err(ScanError::Failed {
                    status: "cancelled while the scanner was running".to_string(),
                    stderr: String::new(),
                })
            }
            Ok(Bounded::Finished(output)) => output,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Read before the artefact is opened, deliberately. What scanner ran and
        // what it resolved the image to are facts about the *scan*, and they are
        // the facts a clean report depends on for its meaning — so they are
        // taken from what the child said it did, before anything can fail on
        // what it wrote.
        let scanner_version = scanner_version(&stdout);
        let image_digest = image_digest(&stdout);

        // The artefact decides. See the module header: a non-zero exit is what
        // an organisation policy hit looks like, and it says nothing about
        // whether this scan produced a usable report.
        let raw = match std::fs::read_to_string(&report) {
            Ok(raw) => raw,
            // No artefact at all, so now — and only now — the exit code and the
            // child's diagnostics are consulted, for the one thing they can
            // settle: whether there was anything to scan.
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(match names_an_absent_image(&stderr) {
                    true => ScanError::ImageAbsent {
                        image: image.to_string(),
                        stderr: snippet(&stderr),
                    },
                    false => ScanError::Failed {
                        status: describe(&output.status),
                        stderr: snippet(&stderr),
                    },
                })
            }
            // The artefact is there and this build cannot read it — bytes that
            // are not UTF-8, or a file it may not open. That is the same
            // situation as a truncated document and not the same as an absent
            // one: something was written and it is not a scanner report.
            Err(source) => {
                return Err(ScanError::Unparseable {
                    path: report,
                    reason: source.to_string(),
                })
            }
        };

        if raw.trim().is_empty() {
            return Err(ScanError::NoOutput { path: report });
        }
        let document = serde_json::from_str(&raw).map_err(|source| ScanError::Unparseable {
            path: report,
            reason: source.to_string(),
        })?;

        Ok(ScanReport {
            document,
            scanner_version,
            image_digest,
        })
    }
}

/// Delete `path` if it is there, and say nothing if it was not.
///
/// `NotFound` is the ordinary case — the first scan of an attempt — and is not
/// a failure to clear anything.
fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// The version the scanner announced, from its own banner.
///
/// Read from the child's output rather than from the report, because the report
/// does not carry it: the document describes the image, and which scanner looked
/// at the image is a fact about the run. Empty when the child announced nothing,
/// which is weak evidence rather than a failed scan — a scan that worked is not
/// something to discard because its banner changed shape.
fn scanner_version(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("wizcli "))
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The digest the scanner resolved the image reference to.
///
/// Matched on the `sha256:` prefix rather than on a position, because the line
/// it sits in is prose the scanner writes for a person and its wording is not
/// something this build should depend on.
fn image_digest(stdout: &str) -> String {
    stdout
        .split_whitespace()
        .find(|word| word.starts_with("sha256:"))
        .unwrap_or_default()
        .to_string()
}

/// Whether the scanner's complaint is that the image does not exist.
///
/// The one place this module reads a child's words, and it is unavoidable: an
/// absent image and a broken scanner both exit non-zero having written nothing,
/// so the exit status cannot separate them and only the diagnostic can. Matched
/// case-insensitively over the phrasings the registry and daemon tooling
/// actually produce.
fn names_an_absent_image(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    ["no such image", "manifest unknown", "not found in registry"]
        .iter()
        .any(|phrase| stderr.contains(phrase))
}

/// How a finished child ended, for a diagnostic.
///
/// A signal death has no code at all, which is why this is not `status.code()`
/// formatted: `None` there would print as nothing and read as an exit of zero.
fn describe(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit {code}"),
        None => "killed by a signal".to_string(),
    }
}

/// A bounded quotation of a child's diagnostics, so an error can be specific
/// without pasting an unbounded stream into a log. The same bound, for the same
/// reason, as `github::cli`'s.
fn snippet(text: &str) -> String {
    const LIMIT: usize = 120;
    let text = text.trim();
    match text.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}
