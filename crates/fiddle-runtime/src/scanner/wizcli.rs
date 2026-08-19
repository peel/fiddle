//! The one place a Wiz credential is turned into a running process.
//!
//! This module stands to the tenant credential exactly as [`crate::github::cli`]
//! stands to the GitHub token and [`crate::gateway`] stands to the model key:
//! one construction site, so *where could this credential go?* has one answer.
//! [`Wizcli::command`] is that site, and the environment it builds is the whole
//! of what a scanner child sees.
//!
//! # The allowlist is five names, and this is the statement of it
//!
//! `PATH`, inherited from this process or [`MINIMUM_PATH`] when it has none,
//! because a scanner shells out to the container runtime it inspects images
//! through; `NO_COLOR`, because [`scanner_version`] and [`image_digest`] read the
//! child's banner and ANSI escapes in text that is going to be parsed are a
//! defect waiting to be written; and the three the credential channel owns —
//! `WIZ_CLIENT_ID`, `WIZ_CLIENT_SECRET` and `WIZ_CONFIG_DIR` — which are set in
//! [`Wizcli::authenticate`] and nowhere else.
//!
//! `scanner::the_wizcli_environment_is_exactly_its_allowlist_and_no_credential_reaches_argv`
//! asserts that set exactly, against what a child actually received, so a sixth
//! name cannot arrive without an assertion changing.
//!
//! This is a *different* set from the four names a workspace check runs under
//! (`HOME`, `LANG`, `PATH`, `RUSTUP_HOME`) and from `gh`'s five, and the three
//! are deliberately not reconciled: they are different spawn sites with
//! different needs, and the thing that would be genuinely wrong is widening one
//! of the others to make this one work. What they share is the *bound* — the
//! process group, the deadline, the cancellation — which lives in
//! [`crate::process`] and is written once.
//!
//! # `HOME` is absent, and that is the load-bearing line
//!
//! With no `HOME` and a `WIZ_CONFIG_DIR` pointing into a scratch directory this
//! adapter owns, the child has no path to an operator's ambient scanner
//! configuration. So "this adapter used the credential it was given and no
//! other" is a fact about the process rather than a promise in a comment. Adding
//! `HOME` back — even pointed at a scratch directory — would reopen whatever
//! `wizcli` keeps under a home directory, and the guarantee would quietly become
//! a guarantee about today's `wizcli`.
//!
//! # Why the credential travels by environment
//!
//! Because `/proc/<pid>/cmdline` is world-readable on Linux: a secret passed as
//! an argument is a secret every user on the box can read for as long as the
//! process lives, and for longer than that in anything that logs a command line.
//! The environment is not private either, but it is readable only by the process
//! owner, and it is the channel every other credential in this runtime uses.
//!
//! **The channel is a guess this milestone cannot settle, and it is confined to
//! one function on purpose.** Wiz is testable only in CI, where the tenant
//! credentials live, so nothing offline can ask a real `wizcli` whether it
//! accepts an environment credential. Rather than build two paths and guess
//! twice, [`Wizcli::authenticate`] is the only code that knows how a credential
//! reaches the child. If M4b's CI lane finds that `wizcli` takes it only as
//! `--secret`, the deviation is an ADR plus a change inside that one function —
//! and every assertion around it still holds, because they are about what the
//! child received rather than about which channel carried it.
//!
//! # Success is the artefact, not the status line
//!
//! See the port's header for the full argument. `wizcli` exits non-zero when an
//! organisation policy flags any finding in the tenant, so [`Wizcli::scan`] reads
//! the artefact first and consults the exit code only to disambiguate its
//! *absence*.

use super::{ScanError, ScanReport, Scanner};
use crate::process::{run_bounded, Bounded};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// What the report is called inside the scratch directory.
///
/// A fixed name rather than one per call: the directory belongs to this adapter,
/// so there is nothing to collide with, and a stable name is what makes a failed
/// scan's leftovers findable by whoever is debugging one.
const REPORT_FILE: &str = "scan.json";

/// Where `WIZ_CONFIG_DIR` points, under the scan's scratch directory.
///
/// A subdirectory rather than the scratch root, so that the configuration source
/// is a directory whose entire contents are things `wizcli` itself put there —
/// the report sitting beside it is this adapter's artefact and has no business
/// inside a directory a scanner is told to treat as its configuration.
const CONFIG_DIR: &str = "wiz-config";

/// What replaces the credential wherever a diagnostic would have quoted it.
///
/// Public because the assertion that the substitution happened is in a test
/// binary, and a test that spelled the marker itself would keep passing if this
/// value changed — which is the one thing it exists to notice. The same string
/// [`crate::github::cli`] uses, so a reader meets one marker and not two.
pub const REDACTED: &str = "[redacted]";

/// The `PATH` a child gets when this process has none.
///
/// Its own constant here rather than a shared one, matching the three spawn
/// sites that came before: each states its own environment in full, and a
/// constant reached across module boundaries would make the statement in one
/// header depend on an edit made in another.
const MINIMUM_PATH: &str = "/usr/bin:/bin";

/// The tenant credential a scan runs under.
///
/// A struct rather than two `String` parameters, for the reason
/// `tests/support/document.rs` gives for its two package types: adjacent
/// arguments of one type can be handed over the wrong way round with nothing to
/// notice, and a client id sent as a secret would authenticate against nothing
/// while the *real* secret went out as a public identifier.
#[derive(Clone, Debug)]
pub struct WizCredential {
    /// The tenant's service-account identifier. Not a secret, and it is
    /// deliberately not redacted: it is the only thing in a diagnostic that says
    /// *which* credential failed, and a run whose authentication error names
    /// nothing is a run nobody can act on.
    pub client_id: String,
    /// The service account's secret. This is the value the whole of this
    /// module's environment and redaction exist for.
    pub client_secret: String,
}

/// The scanner, reached as a subprocess.
///
/// `program` and `args` are the operator seam — the same shape [`crate::GhCli`]
/// carries for `gh`, and for the same reason: an operator who must pin a version
/// or wrap the binary in a launcher has somewhere to do it, and the offline gate
/// substitutes a scripted scanner through it rather than through the
/// environment, which is pinned.
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
    credential: WizCredential,
}

impl Wizcli {
    pub fn new(
        program: PathBuf,
        args: Vec<String>,
        scratch: PathBuf,
        timeout: Duration,
        cancel: CancellationToken,
        credential: WizCredential,
    ) -> Self {
        Self {
            program,
            args,
            scratch,
            timeout,
            cancel,
            credential,
        }
    }

    /// Where this scan's report will be.
    fn report_path(&self) -> PathBuf {
        self.scratch.join(REPORT_FILE)
    }

    /// The one `wizcli` this module builds: an empty environment, the five names
    /// it is allowed, the operator's own arguments, and this scan's flags.
    ///
    /// `env_clear` then an explicit allowlist, which is what every other spawn
    /// site in this runtime does and for the same reason: a credential added to
    /// the runner tomorrow is excluded by default rather than by somebody
    /// remembering to deny it. `std::env::remove_var` would mutate this process
    /// and is wrong for a concurrent runtime.
    fn command(&self, image: &str, report: &Path) -> Result<Command, ScanError> {
        let mut command = Command::new(&self.program);

        command.env_clear();
        // A locator may be inherited, an authority may not — M1's rule, applied
        // at a fourth spawn site. The fallback matters here rather than being
        // defensive: `PATH` is one of the five names the boundary assertion
        // counts, and a runner without one would silently make it four.
        command.env(
            "PATH",
            std::env::var_os("PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| MINIMUM_PATH.into()),
        );
        // The banner is parsed — see `scanner_version` and `image_digest` — so
        // colour in it would be escape sequences inside a version string.
        command.env("NO_COLOR", "1");
        // The credential and the configuration source, and the only place either
        // is set. Nothing else. A sixth entry in this environment is a change to
        // the security boundary and has to break
        // `the_wizcli_environment_is_exactly_its_allowlist_and_no_credential_reaches_argv`
        // before it can land.
        self.authenticate(&mut command)?;

        command
            .args(&self.args)
            // Load-bearing, and the reason is the one this whole module inverts
            // the usual exit-code rule for. **On the bean's account, `wizcli` v1
            // fails a scan on BLOCK-policy hits by default** — a claim about a
            // tool this milestone can never call, recorded here as the
            // documented assumption it is rather than as a measured fact, and
            // due its first measurement in M4b's CI lane. Those hits are the
            // tenant's policy speaking about findings that need have nothing to
            // do with this image, so the flag asks the scanner not to turn one
            // into a failed scan. It is a request and not a guarantee: an
            // operator on an older build still gets a non-zero exit, and the
            // classification below is what keeps that harmless.
            .arg("--by-policy-hits=DISABLED")
            // The artefact, at a path this adapter chose. A scanner left to its
            // own default writes wherever its configuration says, which is the
            // one thing the pinned `WIZ_CONFIG_DIR` above guarantees nothing
            // about.
            .arg("--json-output-file")
            .arg(report)
            // Last, because the image is the positional argument and because the
            // scripted scanner reads it from there.
            .arg(image);

        Ok(command)
    }

    /// Give the child the credential, and pin where it may look for another.
    ///
    /// **The whole of the authentication channel, deliberately in one function.**
    /// See this module's header: nothing offline can ask a real `wizcli` how it
    /// takes a credential, so this build makes one choice rather than two, and
    /// confines it here. A deviation found in CI is an edit to this body and to
    /// the allowlist the header states — not a search through a spawn path.
    ///
    /// `WIZ_CONFIG_DIR` belongs to this function rather than to the general
    /// environment beside it, because it is not configuration: it is the second
    /// half of the credential channel. Pointed at a directory this scan owns, it
    /// is what makes the credential above the *only* one the child can find, and
    /// moving it out of here would separate the two halves of one guarantee.
    fn authenticate(&self, command: &mut Command) -> Result<(), ScanError> {
        let config = self.scratch.join(CONFIG_DIR);
        // Created rather than merely named, so the child meets an empty
        // directory instead of an absent one. A tool that cannot open its
        // configuration directory is entitled to fall back to a default
        // location, which is exactly the ambient source this pinning exists to
        // close off.
        if let Err(source) = std::fs::create_dir_all(&config) {
            return Err(ScanError::Failed {
                status: format!(
                    "the scanner's configuration directory {} could not be created",
                    config.display()
                ),
                stderr: source.to_string(),
            });
        }

        command.env("WIZ_CLIENT_ID", &self.credential.client_id);
        command.env("WIZ_CLIENT_SECRET", &self.credential.client_secret);
        command.env("WIZ_CONFIG_DIR", &config);
        Ok(())
    }

    /// `text` with the credential taken out of it.
    ///
    /// Applied to a child's diagnostics *before* they are bounded by [`snippet`],
    /// and the order is not arbitrary: bounding first would cut a secret in half
    /// at the limit and leave the first hundred-odd characters of it in the
    /// message, which no later substitution could find.
    ///
    /// An empty secret is left alone, because replacing every empty substring
    /// would turn a diagnostic into a string of markers.
    fn redact(&self, text: &str) -> String {
        match self.credential.client_secret.is_empty() {
            true => text.to_string(),
            false => text.replace(&self.credential.client_secret, REDACTED),
        }
    }

    /// A child's diagnostics as they may appear in a [`ScanError`]: the
    /// credential removed, then bounded.
    fn diagnostic(&self, stderr: &str) -> String {
        snippet(&self.redact(stderr))
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

        let mut command = self.command(image, &report)?;

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
        // what it wrote. Nothing below can supply them: they are not in the
        // document, which `the_scan_records_what_it_scanned_before_parsing_anything`
        // asserts so that the ordering is observable rather than merely written.
        let scanner_version = scanner_version(&stdout);
        let image_digest = image_digest(&stdout);

        // The artefact decides. See the port's header: a non-zero exit is what
        // an organisation policy hit looks like, and it says nothing about
        // whether this scan produced a usable report.
        let raw = match std::fs::read_to_string(&report) {
            Ok(raw) => raw,
            // No artefact at all, so now — and only now — the exit code and the
            // child's diagnostics are consulted, for the one thing they can
            // settle: whether there was anything to scan.
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                // Three arms out of one situation, and the order is decided
                // rather than incidental. The daemon is asked about first
                // because it is the more fundamental of the two: tooling that
                // cannot reach a daemon has no way to know whether an image
                // exists, so a complaint that mentions both is a complaint about
                // the daemon — and an operator told their tag is absent when the
                // host is simply down goes and searches a registry.
                //
                // Both predicates read the child's *raw* stderr rather than the
                // quotation below it. `diagnostic` bounds the text at a hundred
                // and twenty characters, so classifying on that would let a
                // scanner with a verbose preamble push its own wording out of
                // reach and turn a daemon that is down into a generic failure.
                return Err(if names_an_unreachable_daemon(&stderr) {
                    ScanError::DaemonUnreachable {
                        stderr: self.diagnostic(&stderr),
                    }
                } else if names_an_absent_image(&stderr) {
                    ScanError::ImageAbsent {
                        image: image.to_string(),
                        stderr: self.diagnostic(&stderr),
                    }
                } else {
                    ScanError::Failed {
                        status: describe(&output.status),
                        stderr: self.diagnostic(&stderr),
                    }
                });
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
/// One of the two places this module reads a child's words, and it is
/// unavoidable: an absent image and a broken scanner both exit non-zero having
/// written nothing, so the exit status cannot separate them and only the
/// diagnostic can. Matched case-insensitively over the phrasings the registry
/// and daemon tooling actually produce.
fn names_an_absent_image(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    ["no such image", "manifest unknown", "not found in registry"]
        .iter()
        .any(|phrase| stderr.contains(phrase))
}

/// Whether the complaint is that the container daemon is not listening.
///
/// The second, for the same unavoidable reason as the first and one more: this
/// failure is not the scanner's at all. A scanner reaches the images it inspects
/// through the container runtime — which is the whole reason `PATH` is in this
/// adapter's allowlist — so the daemon being down produces a non-zero exit and
/// no artefact, exactly as a scanner that broke does, and only the wording tells
/// them apart. What it buys is in [`ScanError::DaemonUnreachable`]: a different
/// remedy and a different exit row.
///
/// Three phrasings, and none of them is `wizcli`'s. They are the container
/// client's own, quoted through by whatever ran it: the first two are what the
/// CLI prints against a socket nothing is listening on, and the third is the
/// named-pipe wording on Windows. Matching a *substring* rather than a whole
/// message is what lets them survive being wrapped in a scanner's own prose,
/// which is how they will actually arrive here.
///
/// **This is the module's one dependency on a foreign program's wording, and it
/// is stated as such rather than hidden.** A phrasing that changes upstream
/// costs this arm its classification and the failure lands back in
/// [`ScanError::Failed`] — a diagnostic that is less useful, not a scan that is
/// wrong.
fn names_an_unreachable_daemon(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "cannot connect to the docker daemon",
        "is the docker daemon running",
        "error during connect",
    ]
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
/// reason, as `github::cli`'s. Reached through [`Wizcli::diagnostic`], which
/// redacts first — see there for why that order is the only safe one.
fn snippet(text: &str) -> String {
    const LIMIT: usize = 120;
    let text = text.trim();
    match text.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}
