//! The strict `fiddle.toml` schema and its loader.
//!
//! Configuration is a typed, strict document: every struct carries
//! `#[serde(deny_unknown_fields)]`, so an unrecognised key is a hard error
//! rather than a silently ignored one.
//!
//! The schema admits no secret-*valued* field, and that is a property of the
//! types rather than a rule reviewers are asked to remember. M1 is the
//! milestone a credential arrives in, and it arrives as [`EnvRef`]: `api_key`
//! deserializes only from `{ env = "NAME" }`, so a document carrying
//! `api_key = "sk-…"` does not load at all. "No secret in configuration" is
//! therefore something the parser decides, not something a reader has to
//! notice.
//!
//! M2 adds the second credential — `[github] token` — and adds it as the same
//! type rather than as a second field that happens to follow the same rule.
//! That is the point of [`EnvRef`] being a type: the property is proved once and
//! inherited, so a third credential cannot arrive carrying a `String` variant
//! nobody noticed.
//!
//! Loading lives here rather than in `fiddle-core` because it reads the
//! filesystem, and `fiddle-core` is mechanically held pure.

use fiddle_core::{DeploymentRule, EffectKind};
use fiddle_runtime::effect::DeploymentPolicy;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The whole configuration document.
///
/// `agent` and `workspace` are optional because M0's document has neither and
/// must keep loading unchanged — a deployment that runs only the deterministic
/// capability names no model and needs no worktree. Optional here means
/// "absent is a legal document", never "absent is filled in silently": a
/// capability that needs a model asks for [`Config::agent`] and fails loudly
/// when the table is missing, rather than inventing an endpoint.
///
/// Both tables landed one task before anything read them, behind a blanket
/// `#[allow(dead_code)]`. That allowance is gone: `main.rs` builds the gateway
/// client, `fiddle_runtime::agent::AgentBudget` and
/// `fiddle_runtime::RepairConfig` out of these fields, so the compiler now
/// proves the schema is wired rather than being asked to overlook that it is
/// not. One field still carries a narrow allowance of its own —
/// [`Agent::max_capability_attempts`] — with the reason written at the field
/// and the decision recorded in `decisions/013-one-attempt-bound-not-two.md`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub project: Project,
    pub stub: Stub,
    pub report: Report,
    #[serde(default)]
    pub agent: Option<Agent>,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    /// The forge a publication reaches. Optional for the same reason the two
    /// above are: M0's and M1's documents have none and must keep loading
    /// unchanged, and a deployment that never publishes has not left this blank
    /// — it has described a deployment that does not publish.
    #[serde(default)]
    pub github: Option<GitHub>,
}

/// Identity of the project a fiddle run acts on.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
}

/// Where the fixture-backed stub ports read and write their state.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stub {
    pub root: PathBuf,
}

/// Where a run publishes its evidence bundles.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub dir: PathBuf,
}

/// The model an agentic capability talks to, and the bounds it runs inside.
///
/// Three keys have no default and must be written down — `model`, `base_url`
/// and `api_key`. Each names a deployment decision that cannot be guessed
/// without being wrong somewhere: a default model silently changes what a run
/// costs and produces, a default endpoint sends prompts to whoever owns it, and
/// a default variable name would let a document look complete while pointing at
/// a credential nobody meant to lend it. Everything else is a *bound*, and a
/// bound has a defensible conservative value, so it is defaulted here and
/// pinned by test.
///
/// This is the shape `fiddle_runtime::agent::AgentBudget` is built from; the
/// field names are its field names, so the mapping can be read rather than
/// deciphered.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    /// The model identifier as the gateway spells it.
    pub model: String,

    /// The OpenAI-compatible endpoint the gateway serves.
    pub base_url: String,

    /// The NAME of an environment variable, never a value. See [`EnvRef`].
    pub api_key: EnvRef,

    /// The **inner** bound: total model calls within one attempt, enforced by
    /// Rig itself rather than by a counter of ours.
    ///
    /// 40 is the reference configuration's value. A read-edit-check loop over a
    /// small fixture spends two to three turns per iteration, so 40 leaves room
    /// for roughly a dozen genuine iterations before the run is judged to be
    /// looping rather than working.
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,

    /// The **outer** bound: how many times the runtime may start a fresh
    /// attempt at the same work.
    ///
    /// Independent of [`Agent::max_turns`] on purpose (design §6.4). They fail
    /// for different reasons and are owned by different layers — Rig stops a
    /// looping conversation, the runtime stops a capability that keeps losing —
    /// and only their *product* bounds what a deployment pays, which is why
    /// neither may be derived from the other. 3 is the reference
    /// configuration's value: enough for a transient gateway failure and one
    /// genuine retry, few enough that a systematically broken fixture is not
    /// paid for three times over.
    ///
    /// The reference configuration eventually files this under `[execution]`
    /// alongside `run_timeout` and `max_parallel`. That table is deferred to
    /// the milestone that has a durable lifecycle to bound (design §6.6), so
    /// the key lives beside the bound it is the counterpart to until then.
    ///
    /// **This bound does not fire.** A document that writes
    /// `max_capability_attempts = 5` gets one attempt.
    ///
    /// Nothing in the runtime starts a second attempt at the same work:
    /// `fiddle_runtime::attempt` runs one, and a capability that failed
    /// surfaces as `RunOutcome::Retryable` for the *caller* to repeat. Reading
    /// this value means writing that retry loop, and `Retryable` is produced by
    /// four sites of which only one is "the capability tried and lost", so the
    /// loop needs a distinction the outcome type does not carry — see the ADR,
    /// which prices the whole change rather than asserting it is small. The key
    /// stays because a document written against the reference configuration
    /// must load, and because the pair of bounds is only meaningful written
    /// down together.
    ///
    /// It carried a `#[allow(dead_code)]` until `config check` learned to say
    /// so at runtime. The value is now read by
    /// [`crate::render::config_check_json`], which reports it as *accepted and
    /// not enforced* beside the number of attempts that will actually be made —
    /// so the sharp edge is discoverable by a machine reading a payload rather
    /// than only by a human reading this comment and the ADR. Design §6.6
    /// promises a deferred key is loud rather than silent; a key that is
    /// *known* rather than unknown escapes `deny_unknown_fields` entirely, and
    /// this is where that route is closed instead.
    #[serde(default = "default_max_capability_attempts")]
    pub max_capability_attempts: usize,

    /// Per-completion token ceiling handed to the provider. 8192 is the
    /// reference configuration's value, and comfortably holds a patch plus the
    /// account of it that the structured output asks for.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,

    /// How many files git may report as changed before an attempt is refused.
    ///
    /// 16 is what the runtime's own repair path already uses. A fixture repair
    /// that rewrote more than a handful of files did something nobody asked
    /// for; the headroom above "a handful" is for the lockfiles and generated
    /// files a legitimate fix drags along with it.
    #[serde(default = "default_max_changed_files")]
    pub max_changed_files: usize,

    /// Wall-clock ceiling on one whole attempt. 45m is the reference
    /// configuration's value.
    #[serde(default = "default_deadline")]
    pub deadline: HumanDuration,

    /// Ceiling on any single tool call that runs a program.
    ///
    /// Defaulted equal to [`Workspace::command_timeout`] deliberately. The
    /// runtime takes the *tighter* of the two, so equal defaults mean neither
    /// silently overrides the other and an operator who lowers one gets exactly
    /// the bound they asked for.
    #[serde(default = "default_tool_timeout")]
    pub tool_timeout: HumanDuration,
}

/// The name of an environment variable holding a credential.
///
/// The type exists to make one thing impossible rather than merely
/// discouraged: there is no `String` variant, no `Option<String>` fallback and
/// no untagged alternative, so the only document that produces an `EnvRef` is
/// one that names a variable. A resolved secret cannot be represented, which is
/// why "configuration holds no credential" can be checked by the parser instead
/// of by a reviewer.
///
/// Hand-written rather than derived for the sake of the diagnostic. Serde's
/// derived error for a wrongly-typed value is `invalid type: string
/// "sk-live-…", expected struct EnvRef` — it quotes the value, which for this
/// one field means printing the very secret the type exists to keep out of
/// reach. [`EnvRefVisitor::visit_str`] answers the same mistake without
/// repeating what it was given.
#[derive(Debug)]
pub struct EnvRef {
    pub env: String,
}

/// What a reader is told when they wrote a credential where a name belongs.
///
/// It does not echo what it was given, and it is worded as the fix rather than
/// as the violation, because the reader is holding a document they now have to
/// clean out of their history.
const CREDENTIAL_MUST_BE_NAMED: &str = "a credential must be named here, never written here: use \
     `{ env = \"VARIABLE_NAME\" }` and export the value instead";

impl<'de> Deserialize<'de> for EnvRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // `deserialize_any` rather than `deserialize_map`: TOML is
        // self-describing, so a string value reaches `visit_str` and is
        // answered by the message above. Asking for a map instead would have
        // the *deserializer* raise the type error, quoting the value, before
        // the visitor ever sees it.
        deserializer.deserialize_any(EnvRefVisitor)
    }
}

struct EnvRefVisitor;

impl<'de> serde::de::Visitor<'de> for EnvRefVisitor {
    type Value = EnvRef;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a table naming an environment variable, as `{ env = \"NAME\" }`")
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<EnvRef, E> {
        Err(E::custom(CREDENTIAL_MUST_BE_NAMED))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<EnvRef, A::Error> {
        use serde::de::Error;
        let mut env: Option<String> = None;
        while let Some(key) = map.next_key::<String>()? {
            if key != "env" {
                return Err(A::Error::unknown_field(&key, &["env"]));
            }
            if env.is_some() {
                return Err(A::Error::duplicate_field("env"));
            }
            env = Some(map.next_value()?);
        }
        let env = env.ok_or_else(|| A::Error::missing_field("env"))?;
        if env.trim().is_empty() {
            return Err(A::Error::custom(
                "the name of an environment variable cannot be empty",
            ));
        }
        Ok(EnvRef { env })
    }
}

/// How an attempt gets a checkout of its own, what it is a checkout *of*, what
/// judges it, and what happens to it after.
///
/// `root` is defaulted to the reference configuration's location and the two
/// enums to their only supported value, so `[workspace]` may be written empty
/// and still mean something exact.
///
/// # Two keys the design's enumeration does not have
///
/// Design §6.6 lists this table as `root`, `isolation`, `command_timeout` and
/// `cleanup`. `fiddle_runtime::RepairConfig` needs two things that enumeration
/// does not name — the repository under repair, and the check that decides
/// whether a repair earned anything — and with `deny_unknown_fields` there is
/// no other way for an operator to supply either. They are added here rather
/// than guessed at the call site, because both are the kind of value that
/// cannot be defaulted without being wrong somewhere:
///
/// - a guessed `fixture` would branch a worktree of whichever repository the
///   process happened to be standing in;
/// - a guessed `check` would be the thing that *decides the milestone's central
///   property*. "The outcome is decided by the configured check" is the whole
///   claim; a check nobody configured deciding it is not a smaller version of
///   that claim, it is a different one.
///
/// Both are therefore `Option` with no default. `[workspace]` written empty
/// still loads — an M0-shaped deployment configures neither — and a capability
/// that needs one refuses by name when it is absent, at the moment it is
/// needed.
/// The strictness lives on [`WorkspaceDocument`], which is what serde actually
/// deserializes, for [`ReadRetryTable`]'s reason: this table now carries a
/// constraint *between* two of its keys, and a constraint between keys can only
/// be applied once all of them have been read. This type is reachable only
/// through the conversion below, so a `Workspace` naming both check shapes
/// cannot be parsed into existence.
#[derive(Debug, Deserialize)]
#[serde(try_from = "WorkspaceDocument")]
pub struct Workspace {
    /// Where per-attempt worktrees are created.
    pub root: PathBuf,

    /// The repository an attempt branches its worktree from and never writes
    /// to.
    pub fixture: Option<PathBuf>,

    /// The command that decides whether a repair earned the correlation
    /// marker.
    ///
    /// There is no `timeout` key inside it: the bound on the check is
    /// [`Workspace::command_timeout`], which is documented as the ceiling on any
    /// single command run inside the workspace, *the check included*. A second
    /// place to write it down is a second place for the two to disagree.
    ///
    /// **This stays for M1's capability, and stays a `ProgramRef`.** M1's one
    /// check has one meaning of success — the process exited zero — and every
    /// document already written against it keeps loading unchanged. A document
    /// that wants several, or one that wants a criterion other than the exit
    /// status, writes [`Workspace::checks`] instead. A document that writes both
    /// is refused; see the conversion below.
    pub check: Option<ProgramRef>,

    /// The checks a repair is judged by, in the order they run.
    ///
    /// Empty when the document names none, which is the same document as one
    /// naming `checks = []`; a deployment configuring no list is the M1 shape
    /// and reaches [`Workspace::check`] instead.
    pub checks: Vec<CheckRef>,

    /// How an attempt is isolated from the repository under repair.
    pub isolation: Isolation,

    /// Ceiling on any single command run inside the workspace, the check
    /// included. 15m is the reference configuration's value — long enough for a
    /// cold `cargo test`, short enough that a hung one is noticed within a
    /// coffee break.
    pub command_timeout: HumanDuration,

    /// What happens to the worktree when the attempt ends.
    pub cleanup: Cleanup,
}

/// The same table before its own constraint has been applied.
///
/// The defaults live here rather than on [`Workspace`], because this is the type
/// serde fills in: a `#[serde(default)]` on a struct reached only through
/// `TryFrom` would never fire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceDocument {
    #[serde(default = "default_workspace_root")]
    root: PathBuf,
    #[serde(default)]
    fixture: Option<PathBuf>,
    #[serde(default)]
    check: Option<ProgramRef>,
    #[serde(default)]
    checks: Vec<CheckRef>,
    #[serde(default)]
    isolation: Isolation,
    #[serde(default = "default_command_timeout")]
    command_timeout: HumanDuration,
    #[serde(default)]
    cleanup: Cleanup,
}

impl TryFrom<WorkspaceDocument> for Workspace {
    type Error = String;

    /// **A contradiction is refused rather than ranked.**
    ///
    /// A document naming both shapes has said two things about what judges a
    /// repair, and none of the three ways to reconcile them is safely the
    /// operator's intent: running the singular one ignores a list somebody
    /// wrote, running the list ignores the check M1 documents, and running both
    /// invents an ordering nobody asked for. A precedence rule would pick one of
    /// those silently and be wrong for the other two deployments, so there is no
    /// precedence rule — the operator picks, in the file, where the mistake is.
    fn try_from(document: WorkspaceDocument) -> Result<Self, String> {
        if document.check.is_some() && !document.checks.is_empty() {
            return Err(
                "`[workspace] check` and `[[workspace.checks]]` are two answers \
                 to one question — what judges a repair — and this document \
                 gives both. There is no precedence between them, because \
                 quietly running one and ignoring the other would be wrong for \
                 whichever deployment meant the other. Keep `check` for a single \
                 check whose success is exit zero, or move it into the list as \
                 an entry declaring its own `success`, but not both"
                    .to_string(),
            );
        }
        Ok(Self {
            root: document.root,
            fixture: document.fixture,
            check: document.check,
            checks: document.checks,
            isolation: document.isolation,
            command_timeout: document.command_timeout,
            cleanup: document.cleanup,
        })
    }
}

/// One check in [`Workspace::checks`]: a program, its arguments, and **what
/// success means for this check**.
///
/// The three fields are written out rather than flattening a [`ProgramRef`] in,
/// because `#[serde(flatten)]` and `deny_unknown_fields` cannot both hold — a
/// flattened struct makes every unknown key look like one the other half might
/// claim, so serde stops refusing them. `check = { program = "make", timeout =
/// "5m" }` being refused at its line is a property [`ProgramRef`] already has a
/// test for, and this list must not be the shape where a mistyped key goes
/// quiet.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRef {
    /// The program to run, resolved against the runner's `PATH`.
    pub program: String,

    /// Its arguments, already separated. Defaulted empty for
    /// [`ProgramRef::args`]' reason.
    #[serde(default)]
    pub args: Vec<String>,

    /// **No default, deliberately.** Defaulting to `exit-zero` would let a
    /// document look complete while the scanner in it was judged by the one
    /// criterion it is known not to answer — `wizcli` exits non-zero on the
    /// findings it was run to produce. A criterion that has to be written down
    /// is a criterion an operator has decided.
    pub success: Success,
}

/// What it means for a check to have succeeded.
///
/// **A closed set, and each check names its own member.** The three came from
/// three real programs that disagree: a build succeeds by exiting zero, a
/// formatter succeeds by exiting zero *and printing nothing* — it reports the
/// files it would rewrite on stdout and still exits zero — and a scanner
/// succeeds by *writing its artefact*, whatever it exits, because a non-zero
/// exit is how it reports findings rather than how it reports failure.
///
/// The alternative was to recognise the program: `if program == "wizcli"`, or a
/// table mapping known commands to the meaning each is known to have. That is
/// rejected outright, and it is the property this type exists to make
/// unavailable. Such a rule would mean an operator renaming a check, pinning it
/// to an absolute path, or putting a wrapper script in front of it had silently
/// changed what the check *decides* — a rename is the last edit anybody expects
/// to alter behaviour, and the failure would show up as a green run that should
/// have been red. With the criterion declared, a rename is a rename. **No code
/// anywhere may derive a `Success` from a program name.**
///
/// Closed rather than a string for [`Isolation`]'s reason: `success =
/// "no-output"` is refused at the line it was written on, where a `String` would
/// have accepted it and left the operator believing they had configured
/// something.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Success {
    /// The process exited zero. M1's only meaning, and a build's.
    ExitZero,

    /// The process exited zero and wrote nothing to stdout or stderr — the
    /// formatter shape, where the output *is* the complaint.
    ExitZeroAndNoOutput,

    /// The artefact was written, whatever the process exited. The scanner shape;
    /// `fiddle_runtime`'s `wizcli` adapter is the implementation of it.
    ArtefactWritten,
}

/// A program this deployment runs, and the arguments it runs it with.
///
/// A table of `program` and `args` rather than one shell string, because a
/// shell string has to be split by somebody and every splitter is wrong about
/// quoting somewhere. `fiddle_runtime::workspace::WorkspaceCommand` and
/// `fiddle_runtime::github::GhCli` both take the program and its arguments
/// already separated; this is the same shape, so the document says exactly what
/// will be executed.
///
/// **One type for both seams, deliberately.** `[workspace] check` and
/// `[github] cli` exist for the same reason — an operator may have to pin a
/// version or put a wrapper in front of a program — and they are the seam the
/// deterministic suites substitute a scripted program through. Two identical
/// structs would be two chances for the two seams to drift into different
/// spellings of one idea; the bound on each is written where the bound lives,
/// which is the one thing that genuinely differs between them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRef {
    /// The program to run, resolved against the runner's `PATH`.
    pub program: String,

    /// Its arguments, already separated. Defaulted empty so a program that takes
    /// none can be written as `{ program = "…" }`.
    #[serde(default)]
    pub args: Vec<String>,
}

/// The forge this deployment publishes to, and the rules it publishes under.
///
/// Three keys have no default and must be written down — `repo`, `base` and
/// `token` — for the reason `[agent]`'s three have none: each names a
/// deployment decision that cannot be guessed without being wrong somewhere. A
/// defaulted repository would publish somebody's work into whatever repository
/// this build shipped with, a defaulted base would open a pull request against a
/// branch nobody nominated, and a defaulted variable name would let a document
/// look complete while pointing at a credential nobody meant to lend it.
///
/// Two more are `Option` with no default — `work` and `workflow` — following
/// [`Workspace::fixture`]'s precedent exactly and for its exact reason: a
/// guessed `work` would publish the commit of whichever repository the process
/// happened to be standing in, and a guessed `workflow` would dispatch a
/// workflow nobody nominated to verify the change. A document that names neither
/// still loads, and a publication refuses by name at the moment it needs one.
///
/// Everything else is a *bound* or a *seam*, and both have a defensible value,
/// so they are defaulted here and pinned by test.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHub {
    /// The repository, as `owner/name` — the spelling the API path uses.
    pub repo: Repo,

    /// The branch a pull request is opened against.
    pub base: String,

    /// The NAME of an environment variable, never a value. See [`EnvRef`].
    ///
    /// One credential rather than two: `gh` and `git push` are different
    /// programs with different environments, and they authenticate to the same
    /// forge as the same principal. A second key would be a second thing to
    /// rotate and a second way for the two to disagree about who fiddle is.
    pub token: EnvRef,

    /// The `gh` this deployment runs.
    ///
    /// The operator seam, and the same one `[workspace] check` already offers:
    /// someone may have to pin a `gh` version or put a wrapper in front of it,
    /// and the deterministic suite substitutes a scripted `gh` here rather than
    /// through a test-only capability. Nothing fake enters the product to make
    /// that possible.
    #[serde(default = "default_gh")]
    pub cli: ProgramRef,

    /// The `git` this deployment pushes with.
    ///
    /// A bare program and not a [`ProgramRef`], because `git push`'s argument
    /// vector is not a seam: it is asserted exactly, by
    /// `git_publish::the_push_is_the_argument_vector_it_claims_to_be`, so that
    /// no credential can reach `argv`. What an operator may still need is a
    /// different `git` on the far end of that vector, and that is this key.
    #[serde(default = "default_git")]
    pub git: PathBuf,

    /// The worktree whose `HEAD` is published, **for a publication only**.
    ///
    /// No default, for [`Workspace::fixture`]'s reason: the commit being
    /// published is the thing `publish_change` is *about*, and a guessed value
    /// would publish whichever repository the process was standing in.
    ///
    /// # A proposal does not read this key, and must not
    ///
    /// `propose_change` publishes the tree its own attempt worked in, and that
    /// path is *derived* rather than configured:
    /// `fiddle_runtime::capability::attempt_worktree` builds it from
    /// `[workspace] root`, the project name and the invocation reference — the
    /// canonical inputs the branch name itself is derived from — so the tree the
    /// attempt writes and the tree the push publishes are one value that cannot
    /// come to disagree. A proposal that took the worktree from this key instead
    /// would publish a commit from whichever tree an operator happened to name,
    /// with the payload hash still matching because no payload mentions a path;
    /// `ProposeChange::execute` refuses exactly that mismatch rather than trusting
    /// its caller, which is why the derivation is the one input and this key is
    /// not a second one.
    ///
    /// So a document describing a proposing deployment leaves `work` out, and
    /// `config check` reports it as absent rather than as something a proposal is
    /// waiting for.
    #[serde(default)]
    pub work: Option<PathBuf>,

    /// The workflow a check is requested from, spelled as the API path spells it
    /// — a file name or a numeric id. No default, as above.
    #[serde(default)]
    pub workflow: Option<String>,

    /// The check names a reader of the verification cares about, matched by
    /// name.
    ///
    /// Defaulted empty, and that is not a permissive default: a check nobody
    /// required is not consulted, so an empty list is a deployment that requires
    /// nothing of CI rather than one that has been let off.
    ///
    /// **Observed, and not enforced — the key is reported as such.** The names
    /// reach `Executor::observe_checks`, which looks each of them up against the
    /// published head and splits what it finds into
    /// [`fiddle_core::VerificationState`]'s three lists, and that value reaches
    /// the bundle as `observations.verification`. Nothing then branches on it:
    /// `fiddle_core::assess` matches on the work item and the change set alone,
    /// so a required check that is missing, failed or still pending leaves the
    /// run's outcome exactly where an all-green one does. `config check` says so
    /// in both renderings rather than leaving an operator to infer it from the
    /// word "required"; see
    /// `decisions/017-required-checks-are-observed-not-enforced.md` for why the
    /// disclosure was the move and enforcement was not.
    #[serde(default)]
    pub required_checks: Vec<String>,

    /// Where `gh` is pointed for its own configuration.
    ///
    /// It exists so that `gh` cannot reach the operator's keyring or their
    /// logged-in account: with `GH_CONFIG_DIR` pointed at an empty directory and
    /// no `HOME` at all, the credential `gh` uses is provably the one this
    /// document named. Defaulted beside `[workspace] root`, and under the same
    /// directory, because it is scratch space of the same kind.
    #[serde(default = "default_gh_config_dir")]
    pub config_dir: PathBuf,

    /// Ceiling on any single external call — one `gh api` round trip, one
    /// `git push`, one `git rev-parse`.
    ///
    /// 5m is generous for an API call and adequate for pushing a change of the
    /// size a fiddle attempt produces, and short enough that a hung one is
    /// noticed. `gh` has no timeout flag of its own, so this bound is the only
    /// one there is.
    #[serde(default = "default_effect_timeout")]
    pub timeout: HumanDuration,

    /// How long a postcondition read may spend waiting for GitHub to agree with
    /// itself.
    ///
    /// A bound, like `timeout`, so it is defaulted like one. What it bounds is
    /// the **read** and only the read: there is no key here for retrying a
    /// mutation, and that absence is deliberate rather than an omission — see
    /// [`fiddle_runtime::effect::ReadRetry`], which is where the reason is
    /// written down.
    #[serde(default)]
    pub read_retry: ReadRetryTable,

    /// The deployment's rule per effect kind.
    ///
    /// Absent means `allow`, because a document that says nothing must not be
    /// stricter than one that says `allow` — and it can never be *weaker* than
    /// the capability's own minimum whatever it says, which is
    /// [`fiddle_core::combine`]'s job rather than this table's.
    #[serde(default)]
    pub policy: PolicyTable,

    /// Who this deployment nominated to decide.
    ///
    /// `Option` with no default, following `work` and `workflow` rather than
    /// `policy`, and the difference between the two is the whole reason: an absent
    /// `policy` is a document adding no *gate*, which is a coherent thing to say
    /// nothing about, whereas an absent `decision` would be a document adding no
    /// *approver* — and a default there is either nobody, which can never continue
    /// a run, or somebody nobody nominated. So a deployment that never puts a
    /// change to a person leaves the table out, and one that names it names an
    /// approver or is refused. See [`Decision`].
    #[serde(default)]
    pub decision: Option<Decision>,
}

/// `[github] read_retry = { attempts, initial, max }`.
///
/// Real GitHub does not answer its own writes immediately: a dispatched
/// workflow run is reliably missing from the runs listing for a moment, and a
/// ref read has answered 404 straight after the push that created the branch.
/// This table is how long a deployment is willing to wait for that to resolve
/// before the run reports an unresolved outcome and hands the decision back.
///
/// It is a strict table of its own, because `deny_unknown_fields` on the parent
/// does not reach into a child — and a mistyped `attempt = 8` that parsed as
/// nothing would be a bound an operator believes they set. The strictness lives
/// on [`ReadRetryDocument`], which is what serde actually deserializes; this
/// type is only ever reached through the conversion below, so a value of it
/// that has not been checked cannot be parsed into existence.
#[derive(Debug, Deserialize)]
#[serde(try_from = "ReadRetryDocument")]
pub struct ReadRetryTable {
    /// Total reads, not waits. One means "look once and take the answer",
    /// which is the behaviour that existed before this key did.
    pub attempts: u32,
    /// The first wait. Doubles after each unsettled read, up to `max`.
    pub initial: HumanDuration,
    /// The ceiling on any single wait, including one GitHub asked for with a
    /// `Retry-After`.
    pub max: HumanDuration,
}

/// An absent table is the same document as an empty one.
///
/// Written through the same three functions the per-key defaults use, so a
/// document with no `read_retry` and a document with `read_retry = {}` cannot
/// drift into meaning different things.
impl Default for ReadRetryTable {
    fn default() -> Self {
        Self {
            attempts: default_read_attempts(),
            initial: default_read_initial(),
            max: default_read_max(),
        }
    }
}

impl ReadRetryTable {
    /// The bound as the runtime's own type, on the deployment's own clock.
    ///
    /// This is the whole path from the document to the executor, and it is one
    /// function so that there is nowhere for the document's numbers to be
    /// replaced by somebody's constants on the way.
    pub fn as_read_retry(&self) -> fiddle_runtime::effect::ReadRetry {
        fiddle_runtime::effect::ReadRetry::bounded(
            self.attempts,
            self.initial.as_duration(),
            self.max.as_duration(),
        )
    }
}

/// Four reads, so three waits: `1s`, `2s` and `4s`, for **7 seconds at the
/// outside** and about half that once the jitter is applied.
///
/// The number it has to cover is the one that was measured, not the one that
/// would be safest in the abstract: real GitHub took roughly two seconds to
/// list a dispatched run and to answer for a just-pushed ref, and
/// `scripts/live-github.sh` re-runs on a four-second cadence. Seven seconds
/// covers both with room over.
///
/// The cost of a *larger* budget is paid by the runs that are never going to
/// settle, and they are the ones that most need to hand the decision back:
/// exhausting reaches `Unresolved` → `RunOutcome::Retryable` → exit 11, and a
/// caller sitting behind a long budget is a caller that cannot act on it.
const DEFAULT_READ_ATTEMPTS: u32 = 4;

/// The same table before its own constraints have been applied.
///
/// The constraints are applied here, at the parse boundary, rather than where
/// the value is used: `attempts = 0` is a document that says every
/// postcondition goes unobserved, and `initial` above `max` is a document whose
/// first wait would be silently shortened to a number nobody wrote. Both are
/// mistakes an operator can only fix in the file, so the file is where they are
/// refused.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRetryDocument {
    #[serde(default = "default_read_attempts")]
    attempts: u32,
    #[serde(default = "default_read_initial")]
    initial: HumanDuration,
    #[serde(default = "default_read_max")]
    max: HumanDuration,
}

impl TryFrom<ReadRetryDocument> for ReadRetryTable {
    type Error = String;

    fn try_from(document: ReadRetryDocument) -> Result<Self, String> {
        if document.attempts == 0 {
            return Err(
                "a postcondition read must happen at least once; `attempts = 0` \
                 would leave every effect unobserved, and `attempts = 1` is how \
                 a document asks for no waiting"
                    .to_string(),
            );
        }
        if document.initial.as_duration() > document.max.as_duration() {
            return Err(format!(
                "the first wait ({}) is longer than the ceiling on any wait ({}), \
                 so `initial` could never be honoured — raise `max` or lower \
                 `initial`",
                document.initial, document.max
            ));
        }
        Ok(Self {
            attempts: document.attempts,
            initial: document.initial,
            max: document.max,
        })
    }
}

/// A repository, as `owner/name`.
///
/// Split at the parse boundary rather than carried as a `String` and split at
/// the point of use, because the owner half is *load-bearing*: the head a pull
/// request is opened from is labelled `owner:branch`, and the lookup that
/// decides whether a pull request already exists matches on that label. A
/// `String` that turned out not to have an owner would be discovered after the
/// branch had already been pushed — a run that failed having already changed the
/// world, which is the shape of failure this milestone exists to avoid.
///
/// Both halves must be non-empty and there must be exactly one separator. That
/// is stricter than GitHub's own naming rules and deliberately so: the value is
/// pasted into an API path, and this is the one place it can be refused with a
/// line number.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl std::str::FromStr for Repo {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let malformed =
            || format!("expected a repository as owner/name, as in \"peel/fiddle\" — got {text:?}");
        let (owner, name) = text.split_once('/').ok_or_else(malformed)?;
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return Err(malformed());
        }
        Ok(Repo {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }
}

impl<'de> Deserialize<'de> for Repo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// What the document says about each effect kind, one key per kind.
///
/// Exhaustive rather than a map, and that is the point of the type: a map would
/// admit a key for an effect kind this build has never heard of and accept it
/// silently, which is precisely how a rule an operator believed they had written
/// comes to apply to nothing. One field per kind, and adding a kind without a
/// field is a compile error in [`PolicyTable::rule_for`] — which is how the two
/// kinds M3 introduced arrived here rather than being forgotten.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyTable {
    #[serde(default = "allow")]
    pub ensure_branch_published: DeploymentRule,
    #[serde(default = "allow")]
    pub ensure_pull_request: DeploymentRule,
    #[serde(default = "allow")]
    pub ensure_check_requested: DeploymentRule,
    #[serde(default = "allow")]
    pub publish_decision_request: DeploymentRule,
    #[serde(default = "allow")]
    pub ensure_pull_request_ready: DeploymentRule,
    #[serde(default = "allow")]
    pub ensure_pull_request_body: DeploymentRule,
}

/// What an absent rule means.
///
/// `Allow` is not "permitted": it is *this document adds no gate*, and it can
/// never remove one — `combine` takes the stricter of the deployment's rule and
/// the capability's own minimum. A default of anything stricter would make a
/// document that says nothing stricter than one that says `allow`, which is a
/// difference no reader could have predicted from the file.
fn allow() -> DeploymentRule {
    DeploymentRule::Allow
}

impl Default for PolicyTable {
    fn default() -> Self {
        PolicyTable {
            ensure_branch_published: allow(),
            ensure_pull_request: allow(),
            ensure_check_requested: allow(),
            publish_decision_request: allow(),
            ensure_pull_request_ready: allow(),
            ensure_pull_request_body: allow(),
        }
    }
}

/// **The key is consumed here, and this is the only place it is read.**
///
/// The executor asks this question once per effect, at its step 4, and acts on
/// the answer `combine` gives. A `match` rather than a lookup so that the keys
/// cannot be cross-wired without the mapping being visible in as many lines as
/// there are kinds.
impl DeploymentPolicy for PolicyTable {
    fn rule_for(&self, kind: EffectKind) -> DeploymentRule {
        match kind {
            EffectKind::EnsureBranchPublished => self.ensure_branch_published,
            EffectKind::EnsurePullRequest => self.ensure_pull_request,
            EffectKind::EnsureCheckRequested => self.ensure_check_requested,
            EffectKind::PublishDecisionRequest => self.publish_decision_request,
            EffectKind::EnsurePullRequestReady => self.ensure_pull_request_ready,
            EffectKind::EnsurePullRequestBody => self.ensure_pull_request_body,
        }
    }
}

/// `[github.decision]` — who may decide.
///
/// A strict table of its own, for [`ReadRetryTable`]'s reason: `deny_unknown_fields`
/// on `[github]` does not reach into a child, so a misspelled `authorised = […]`
/// would otherwise parse as nothing and leave an operator believing they had
/// nominated somebody. The strictness lives on [`DecisionDocument`], which is what
/// serde actually deserializes; this type is only ever reached through the
/// conversion below, so a value of it that has not been checked cannot be parsed
/// into existence.
///
/// **One key, and it stays one.** A `max_pages` bound was specified for this table
/// and then removed, because `human::CONVERSATION_PAGES`
/// already argues the case against it: nobody asked to configure how much of a
/// conversation fiddle reads, and a document value that could disagree with the
/// operation's own bound would be worse than a constant that cannot. Two reads of
/// one conversation — one to find this run's question, one to find the reply below
/// it — have to agree, and the way to guarantee that is a constant.
#[derive(Debug, Deserialize)]
#[serde(try_from = "DecisionDocument")]
pub struct Decision {
    /// The immutable numeric user ids of the people this deployment nominated to
    /// decide.
    ///
    /// **Ids, and the type is what refuses a login.** A login can be changed and
    /// the vacated name reclaimed, so an allowlist matching one would let a
    /// renamed-and-reclaimed account inherit an approver's authority; and not
    /// `author_association` either, which says what somebody's relationship to the
    /// repository is rather than whether *this deployment* nominated them. A
    /// `Vec<u64>` means a document cannot express either, so there is no path on
    /// which the code has to match an authorization loosely — the refusal belongs
    /// to the schema rather than to a check somebody could forget to write.
    ///
    /// No default, and an empty list refused: see the conversion below.
    pub authorized: Vec<u64>,
}

/// The same table before its own constraint has been applied.
///
/// The constraint is applied at the parse boundary rather than where the value is
/// used, for [`ReadRetryDocument`]'s reason: an empty approver list is a mistake an
/// operator can only fix in the file, so the file is where it is refused.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionDocument {
    /// No `default`, so a document naming the table and forgetting the key is
    /// refused by serde naming the key — which is a different refusal from the
    /// empty list below, because a forgotten key and an emptied one are different
    /// mistakes.
    authorized: Vec<u64>,
}

impl TryFrom<DecisionDocument> for Decision {
    type Error = String;

    fn try_from(document: DecisionDocument) -> Result<Self, String> {
        if document.authorized.is_empty() {
            return Err(
                "`authorized = []` names nobody, and nobody is not the permissive \
                 reading: a deployment that can publish a question and can never \
                 accept an answer suspends every run for ever. Name the numeric \
                 user ids that may decide, or leave `[github.decision]` out of the \
                 document altogether"
                    .to_string(),
            );
        }
        Ok(Self {
            authorized: document.authorized,
        })
    }
}

/// How an attempt is isolated from the repository it repairs.
///
/// One variant, because M1 supports one mechanism. The enum still earns its
/// place: it turns `isolation = "none"` into a refusal at the line it was
/// written on, where a plain `String` would have accepted it and left the
/// operator believing they had turned isolation off.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Isolation {
    /// A detached git worktree per attempt, branched from the fixture.
    #[default]
    GitWorktree,
}

/// What becomes of a workspace when its attempt ends.
///
/// One variant, for the same reason [`Isolation`] has one, and with a sharper
/// edge: `fiddle_runtime::workspace::Workspace` removes its worktree on `Drop`
/// as well as on an explicit teardown, so there is no code path that keeps one.
/// Admitting `"never"` here would be a promise the runtime cannot keep — the
/// directory an operator went looking for would already be gone. The key exists
/// so the axis is visible and so a document written against the reference
/// configuration loads; the missing variant arrives with the code that can
/// honour it.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Cleanup {
    /// The worktree is removed however the attempt ends.
    #[default]
    Always,
}

/// A wall-clock bound, written the way the reference configuration writes one:
/// a whole number and a unit, as `"45m"`.
///
/// The alternative was a plain integer under a field named for its unit —
/// `deadline_seconds = 2700`. Both are unambiguous; the string was chosen
/// because the unit lives in the *value* rather than in the *name*, and a name
/// is the part that cannot change later without breaking every document that
/// already exists. `deadline_seconds = 2700` becoming `deadline = "45m"` is a
/// migration; `"45m"` becoming `"2700s"` is a preference. It also keeps the
/// documents this schema accepts the same shape as the reference configuration
/// in the PRD, which is what operators will copy from.
///
/// The parser is deliberately small: one integer, one unit of `s`, `m` or `h`.
/// A bare number is refused because it is exactly the ambiguity the string form
/// exists to remove, and zero is refused because a zero bound cannot be
/// satisfied by any attempt — a mistyped `"0s"` would look like an agent that
/// always fails rather than a document that always fails it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HumanDuration(Duration);

impl HumanDuration {
    /// The bound as the runtime's own type.
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    const fn secs(secs: u64) -> Self {
        Self(Duration::from_secs(secs))
    }
}

/// Written back the way it was written down: the largest whole unit that
/// divides it, so a document saying `"45m"` is echoed as `"45m"` rather than as
/// an amount of seconds an operator has to divide before recognising.
///
/// Round-tripping is the actual contract, not the cosmetics:
/// [`HumanDuration::from_str`] accepts everything this produces, so a caller
/// reading a duration out of a `config check` payload can put it straight back
/// into a document. `a_rendered_duration_parses_back_to_itself` is what keeps
/// the two halves in step.
impl std::fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let seconds = self.0.as_secs();
        // Ordered widest-first, and every arm divides exactly: a bound that is
        // not a whole number of hours or minutes is reported in seconds rather
        // than rounded, because a rounded bound is not the bound that will fire.
        if seconds.is_multiple_of(3600) {
            write!(f, "{}h", seconds / 3600)
        } else if seconds.is_multiple_of(60) {
            write!(f, "{}m", seconds / 60)
        } else {
            write!(f, "{seconds}s")
        }
    }
}

impl std::str::FromStr for HumanDuration {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let malformed = || {
            format!(
                "expected a duration with a unit — a whole number followed by \
                 `s`, `m` or `h`, as in \"45m\" — got {text:?}"
            )
        };
        let split = text
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(malformed)?;
        let (digits, unit) = text.split_at(split);
        let count: u64 = digits.parse().map_err(|_| malformed())?;
        let seconds = match unit {
            "s" => Some(count),
            "m" => count.checked_mul(60),
            "h" => count.checked_mul(3600),
            _ => return Err(malformed()),
        }
        .ok_or_else(|| format!("the duration {text:?} is longer than this program can express"))?;
        if seconds == 0 {
            return Err(format!(
                "a bound of {text:?} can never be satisfied; leave the key out \
                 to take the default"
            ));
        }
        Ok(Self::secs(seconds))
    }
}

impl<'de> Deserialize<'de> for HumanDuration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

fn default_max_turns() -> usize {
    40
}

fn default_max_capability_attempts() -> usize {
    3
}

fn default_max_tokens() -> u64 {
    8192
}

fn default_max_changed_files() -> usize {
    16
}

fn default_deadline() -> HumanDuration {
    HumanDuration::secs(45 * 60)
}

fn default_tool_timeout() -> HumanDuration {
    HumanDuration::secs(15 * 60)
}

fn default_command_timeout() -> HumanDuration {
    HumanDuration::secs(15 * 60)
}

fn default_workspace_root() -> PathBuf {
    PathBuf::from(".fiddle/workspaces")
}

fn default_gh() -> ProgramRef {
    ProgramRef {
        program: "gh".to_string(),
        args: Vec::new(),
    }
}

fn default_git() -> PathBuf {
    PathBuf::from("git")
}

fn default_gh_config_dir() -> PathBuf {
    PathBuf::from(".fiddle/gh-config")
}

fn default_effect_timeout() -> HumanDuration {
    HumanDuration::secs(5 * 60)
}

fn default_read_attempts() -> u32 {
    DEFAULT_READ_ATTEMPTS
}

fn default_read_initial() -> HumanDuration {
    HumanDuration::secs(1)
}

fn default_read_max() -> HumanDuration {
    HumanDuration::secs(4)
}

/// A document that failed the strict schema, carrying enough context to point
/// at the offending bytes.
///
/// Design §4.6 requires the diagnostic to name the offending key *and its
/// line*, rendered through `miette`. `toml::de::Error::span` gives the byte
/// range; miette turns it into a source snippet with a caret. `thiserror`
/// alone would render the message and lose the location.
///
/// The whole source text lives in here, which is why [`ConfigError`] holds it
/// behind a `Box`: a configuration error is returned once, at the process
/// boundary, and there is no reason for every `Result` in the call chain to
/// reserve room for the file.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("invalid configuration in {path}")]
#[diagnostic(code(fiddle::config::invalid))]
pub struct InvalidConfig {
    /// The document that was rejected, as the caller named it.
    pub path: PathBuf,
    #[source_code]
    src: miette::NamedSource<String>,
    #[label("{message}")]
    span: Option<miette::SourceSpan>,
    message: String,
}

/// Everything that can go wrong loading a configuration document.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ConfigError {
    #[error("configuration file not found: {0}")]
    #[diagnostic(code(fiddle::config::not_found))]
    NotFound(PathBuf),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Invalid(#[from] Box<InvalidConfig>),
}

/// Read and strictly deserialize the configuration document at `path`.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text =
        std::fs::read_to_string(path).map_err(|_| ConfigError::NotFound(path.to_path_buf()))?;
    toml::from_str(&text).map_err(|e| {
        let message = e.message().to_string();
        // Every other diagnostic quotes the line it is about, which is the
        // whole value of a line-aware error. This one must not: the line a
        // literal credential is written on *is* the credential, so quoting it
        // would print the secret to a terminal and, worse, to a CI log — the
        // schema refusing to hold a credential is not worth much if the refusal
        // publishes it. The location survives the redaction; only the bytes go.
        let (source, span) = if message == CREDENTIAL_MUST_BE_NAMED {
            redacted(text, e.span())
        } else {
            (text, e.span())
        };
        ConfigError::Invalid(Box::new(InvalidConfig {
            path: path.to_path_buf(),
            src: miette::NamedSource::new(path.display().to_string(), source),
            span: span.map(|r| (r.start, r.end - r.start).into()),
            message,
        }))
    })
}

/// The placeholder a redacted value is rendered as.
const REDACTED: &str = "\"<redacted>\"";

/// `text` with the bytes `span` covers replaced by [`REDACTED`], and `span`
/// moved onto the placeholder so the caret still lands where the value was.
///
/// Only the bytes at or after the span shift, and nothing else is labelled, so
/// the line and column miette reports are the ones the operator has to go and
/// edit. A span that is out of bounds or off a character boundary — which a
/// well-formed document cannot produce — drops the snippet entirely rather than
/// risking a partial one: the message alone is a worse diagnostic, and a
/// diagnostic is a worse thing to be wrong about than it is to be terse about.
fn redacted(
    text: String,
    span: Option<std::ops::Range<usize>>,
) -> (String, Option<std::ops::Range<usize>>) {
    let Some(range) = span else {
        return (text, None);
    };
    let (Some(before), Some(after)) = (text.get(..range.start), text.get(range.end..)) else {
        return (String::new(), None);
    };
    let redacted = format!("{before}{REDACTED}{after}");
    (redacted, Some(range.start..range.start + REDACTED.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document exercising every table the schema admits, written the way an
    /// operator would write it: the credential is *named*, never carried.
    const VALID: &str = r#"
[project]
name = "p"

[stub]
root = "s"

[report]
dir = "r"

[agent]
model = "claude-sonnet-5"
base_url = "https://litellm.firn.snplow.net/v1"
api_key = { env = "LITELLM_API_KEY" }
max_turns = 12

[workspace]
root = ".fiddle/workspaces"
isolation = "git-worktree"
fixture = "fixtures/m1-demo"
check = { program = "cargo", args = ["test", "--offline"] }
"#;

    #[test]
    fn the_agent_and_workspace_tables_load() {
        let cfg: Config = toml::from_str(VALID).unwrap();
        let agent = cfg.agent.unwrap();
        assert_eq!(agent.model, "claude-sonnet-5");
        assert_eq!(agent.api_key.env, "LITELLM_API_KEY");
        assert_eq!(agent.max_turns, 12);
        assert_eq!(cfg.workspace.unwrap().isolation, Isolation::GitWorktree);
    }

    /// The repository under repair and the command that judges it are both
    /// written down, in the shape `WorkspaceCommand` takes: program and
    /// arguments already separated, so nothing has to guess where one argument
    /// ends and the next begins.
    #[test]
    fn the_repository_under_repair_and_its_check_are_configured() {
        let workspace = toml::from_str::<Config>(VALID).unwrap().workspace.unwrap();
        assert_eq!(workspace.fixture, Some(PathBuf::from("fixtures/m1-demo")));
        let check = workspace.check.expect("the check is part of the document");
        assert_eq!(check.program, "cargo");
        assert_eq!(check.args, ["test", "--offline"]);
    }

    /// Neither key may be invented when it is absent, so both are `None` rather
    /// than defaulted — including for a `[workspace]` written empty, which is
    /// how a deployment that runs only the deterministic capability writes it.
    #[test]
    fn an_unconfigured_fixture_and_check_are_absent_rather_than_guessed() {
        let cfg: Config = toml::from_str(
            "[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n[workspace]\n",
        )
        .unwrap();
        let workspace = cfg.workspace.unwrap();
        assert_eq!(workspace.fixture, None);
        assert!(workspace.check.is_none());
    }

    /// A check with no arguments is written as the program alone; the strict
    /// schema still reaches inside the table.
    #[test]
    fn a_check_may_take_no_arguments_and_is_still_strict() {
        let with = |check: &str| {
            toml::from_str::<Config>(&VALID.replace(
                r#"check = { program = "cargo", args = ["test", "--offline"] }"#,
                check,
            ))
        };
        let workspace = with(r#"check = { program = "make" }"#)
            .unwrap()
            .workspace
            .unwrap();
        assert!(workspace.check.unwrap().args.is_empty());

        assert!(
            with(r#"check = { program = "make", timeout = "5m" }"#).is_err(),
            "the check is bounded by workspace.command_timeout; a second place \
             to write that down is a second place for the two to disagree"
        );
        assert!(
            with(r#"check = { args = ["test"] }"#).is_err(),
            "a check with no program names nothing to run"
        );
    }

    // -----------------------------------------------------------------------
    // The ordered check list. See [`Success`] for why the criterion is written
    // in the document rather than recognised from the program.
    // -----------------------------------------------------------------------

    /// An M0-shaped document with a `[workspace]` for the list to hang off.
    /// Deliberately without a singular `check`, so each scenario below adds
    /// exactly the lines it is about.
    const WORKSPACE_ONLY: &str = "[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n\
                                  [report]\ndir=\"r\"\n[workspace]\n";

    /// The three programs the milestone was specified against, which disagree
    /// about what success is.
    const THREE_CHECKS: &str = r#"
[[workspace.checks]]
program = "go"
args = ["build", "./..."]
success = "exit-zero"

[[workspace.checks]]
program = "go"
args = ["fmt", "./..."]
success = "exit-zero-and-no-output"

[[workspace.checks]]
program = "wizcli"
args = ["scan"]
success = "artefact-written"
"#;

    /// `[[workspace.checks]]` is a table array, so the order the document writes
    /// is the order the checks run in — and each entry's criterion is the one
    /// that entry declared.
    #[test]
    fn checks_declare_their_own_success_criterion() {
        let workspace = toml::from_str::<Config>(&format!("{WORKSPACE_ONLY}{THREE_CHECKS}"))
            .unwrap()
            .workspace
            .unwrap();
        assert_eq!(workspace.checks.len(), 3);
        assert_eq!(workspace.checks[0].success, Success::ExitZero);
        assert_eq!(workspace.checks[1].success, Success::ExitZeroAndNoOutput);
        assert_eq!(workspace.checks[2].success, Success::ArtefactWritten);
        // The order is part of the claim, so the programs are pinned too: a
        // schema that sorted or deduplicated would still satisfy the three
        // assertions above by accident.
        assert_eq!(
            workspace
                .checks
                .iter()
                .map(|c| (c.program.as_str(), c.args.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("go", vec!["build".to_string(), "./...".to_string()]),
                ("go", vec!["fmt".to_string(), "./...".to_string()]),
                ("wizcli", vec!["scan".to_string()]),
            ]
        );
    }

    /// **The criterion is read from the document, never inferred from the
    /// program.**
    ///
    /// The same command twice, declaring two different criteria, and both
    /// survive. No rule keyed on a program name could produce two answers for
    /// one command, so this is what fails the day such a rule is written — and
    /// it is the property that makes an operator's rename safe: if `go fmt`
    /// does not *mean* anything to this code, then neither does `./scripts/fmt`
    /// or `/opt/go/bin/go`.
    #[test]
    fn one_command_declared_two_ways_keeps_both_meanings() {
        let checks = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}\n\
             [[workspace.checks]]\nprogram = \"go\"\nargs = [\"fmt\", \"./...\"]\n\
             success = \"exit-zero\"\n\n\
             [[workspace.checks]]\nprogram = \"go\"\nargs = [\"fmt\", \"./...\"]\n\
             success = \"exit-zero-and-no-output\"\n"
        ))
        .unwrap()
        .workspace
        .unwrap()
        .checks;
        assert_eq!(checks[0].success, Success::ExitZero);
        assert_eq!(
            checks[1].success,
            Success::ExitZeroAndNoOutput,
            "one command, two declarations, two meanings"
        );
    }

    /// A criterion is required of every check, including the two whose meaning
    /// a reader would cheerfully guess — which is exactly why guessing is not
    /// on offer.
    #[test]
    fn every_check_must_declare_a_criterion_however_familiar_its_program() {
        for entry in [
            "program = \"go\"\nargs = [\"fmt\", \"./...\"]\n",
            "program = \"wizcli\"\nargs = [\"scan\"]\n",
        ] {
            let error =
                toml::from_str::<Config>(&format!("{WORKSPACE_ONLY}[[workspace.checks]]\n{entry}"))
                    .expect_err("a check that declares nothing must be refused");
            assert!(
                error.message().contains("success"),
                "the diagnostic must name the missing key, got: {}",
                error.message()
            );
        }
    }

    /// The set is closed, so a criterion nobody implemented is refused at the
    /// line it was written on rather than accepted and never honoured.
    #[test]
    fn a_criterion_outside_the_closed_set_is_refused_with_the_set() {
        let error = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}[[workspace.checks]]\nprogram = \"go\"\nsuccess = \"no-output\"\n"
        ))
        .expect_err("`no-output` is not a criterion this deployment can honour");
        let message = error.message();
        assert!(
            message.contains("no-output") && message.contains("artefact-written"),
            "the diagnostic must name what was written and what was available, \
             got: {message}"
        );
    }

    /// A mistyped key inside a check is refused, which is what the three fields
    /// are written out longhand for — see [`CheckRef`].
    #[test]
    fn an_unknown_key_inside_a_check_is_refused() {
        let error = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}[[workspace.checks]]\nprogram = \"go\"\n\
             success = \"exit-zero\"\ntimeout = \"5m\"\n"
        ))
        .expect_err("a check is bounded by workspace.command_timeout, not by itself");
        assert!(
            error.message().contains("timeout"),
            "got: {}",
            error.message()
        );
    }

    /// M1's shape, unchanged: one check, no list, and no criterion to write
    /// because the singular check has always meant exit zero.
    #[test]
    fn the_singular_check_still_loads_for_the_m1_capability() {
        let workspace = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}check = {{ program = \"cargo\", args = [\"test\"] }}\n"
        ))
        .expect("every document written against M1 keeps loading")
        .workspace
        .unwrap();
        assert_eq!(workspace.check.expect("the M1 check").program, "cargo");
        assert!(
            workspace.checks.is_empty(),
            "an unwritten list is empty, not invented from the singular check"
        );
    }

    /// **Refused because both shapes are named — and provably not for any other
    /// reason.**
    ///
    /// A malformed document is also an `Err`, so `is_err()` alone would pass
    /// against a schema that resolved the contradiction by precedence and a
    /// document with a typo in it. Three things separate those cases here: the
    /// bytes parse as TOML before any schema sees them; each half loads on its
    /// own, which between them covers every line of the whole; and the
    /// diagnostic names the two keys in conflict and lands on the table they
    /// were written in.
    #[test]
    fn naming_both_shapes_is_refused_rather_than_resolved_by_precedence() {
        const SINGULAR: &str = "check = { program = \"cargo\", args = [\"test\"] }\n";
        let both = format!("{WORKSPACE_ONLY}{SINGULAR}{THREE_CHECKS}");

        // Well-formed TOML. Not "the schema accepted it" — a generic parse, so
        // the refusal below cannot be a syntax complaint wearing a schema's
        // clothes.
        toml::from_str::<toml::Table>(&both)
            .expect("the document must be well-formed TOML for its refusal to mean anything");

        // And each half is a document this schema accepts, which is what rules
        // out any line of `both` being individually at fault.
        assert!(toml::from_str::<Config>(&both.replace(SINGULAR, "")).is_ok());
        assert!(toml::from_str::<Config>(&both.replace(THREE_CHECKS, "")).is_ok());

        let error = toml::from_str::<Config>(&both)
            .expect_err("a contradiction is refused, never silently ranked");
        let message = error.message().to_string();
        assert!(
            message.contains("`[workspace] check`") && message.contains("`[[workspace.checks]]`"),
            "the diagnostic must name both shapes so the operator knows which \
             two lines conflict: {message}"
        );
        assert!(
            message.contains("no precedence"),
            "and must say that neither wins, since a reader's first guess is \
             that one of them quietly does: {message}"
        );

        // Refused somewhere an operator can go and edit, the way an empty
        // approver list is.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(&path, &both).unwrap();
        let ConfigError::Invalid(invalid) = load(&path).unwrap_err() else {
            panic!("a document naming both shapes is an invalid document");
        };
        let offset = invalid
            .span
            .expect("miette needs somewhere to point")
            .offset();
        assert_eq!(
            line_of(&both, offset),
            both.lines()
                .position(|line| line.starts_with("[workspace]"))
                .unwrap()
                + 1,
            "the caret belongs on the table holding the contradiction"
        );
    }

    #[test]
    fn a_resolved_secret_in_configuration_is_refused() {
        // The schema admits a variable NAME. A literal value must not parse,
        // which makes "no secret in configuration" a parse-time property.
        let bad = VALID.replace(
            r#"api_key = { env = "LITELLM_API_KEY" }"#,
            r#"api_key = "sk-literal-secret""#,
        );
        assert!(toml::from_str::<Config>(&bad).is_err());
    }

    #[test]
    fn an_m0_shaped_document_still_loads() {
        // m0_skeleton writes a document with no [agent] or [workspace]; it must
        // keep passing config check unchanged.
        let cfg: Config =
            toml::from_str("[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n")
                .unwrap();
        assert!(cfg.agent.is_none() && cfg.workspace.is_none());
    }

    /// The defaults are a claim about what a run costs when nobody says
    /// otherwise, so they are pinned rather than left to whatever the last edit
    /// of a `default_*` function happened to return.
    #[test]
    fn the_defaults_are_the_ones_documented() {
        let cfg: Config = toml::from_str(
            "[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [agent]\nmodel=\"m\"\nbase_url=\"u\"\napi_key={env=\"K\"}\n[workspace]\n",
        )
        .unwrap();

        let agent = cfg.agent.unwrap();
        assert_eq!(agent.max_turns, 40, "the inner bound Rig enforces");
        assert_eq!(
            agent.max_capability_attempts, 3,
            "the outer bound, which is parsed and not consumed — ADR 013"
        );
        assert_eq!(agent.max_tokens, 8192);
        assert_eq!(agent.max_changed_files, 16);
        assert_eq!(agent.deadline.as_duration(), Duration::from_secs(45 * 60));
        assert_eq!(
            agent.tool_timeout.as_duration(),
            Duration::from_secs(15 * 60),
            "equal to the workspace command timeout, so neither tightens the \
             other until an operator asks for it"
        );

        let workspace = cfg.workspace.unwrap();
        assert_eq!(workspace.root, PathBuf::from(".fiddle/workspaces"));
        assert_eq!(workspace.isolation, Isolation::GitWorktree);
        assert_eq!(
            workspace.command_timeout.as_duration(),
            Duration::from_secs(15 * 60)
        );
        assert_eq!(workspace.cleanup, Cleanup::Always);
    }

    /// The two bounds are independent knobs, not one value read twice — a
    /// document must be able to move either without moving the other.
    ///
    /// A claim about the *schema*, and only that. The product of them would be
    /// the worst case a deployment pays for if both were consumed; only
    /// `max_turns` is, so today the worst case is one attempt's. See ADR 013,
    /// and [`Agent::max_capability_attempts`] for why the key is here at all.
    #[test]
    fn the_two_attempt_bounds_are_set_separately() {
        let cfg: Config = toml::from_str(
            "[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [agent]\nmodel=\"m\"\nbase_url=\"u\"\napi_key={env=\"K\"}\n\
             max_turns=7\nmax_capability_attempts=2\n",
        )
        .unwrap();
        let agent = cfg.agent.unwrap();
        assert_eq!(agent.max_turns, 7);
        assert_eq!(agent.max_capability_attempts, 2);
    }

    /// Strictness has to reach *into* the new tables, not merely admit them.
    #[test]
    fn an_unknown_key_inside_the_agent_table_is_refused_at_its_line() {
        let text = VALID.replace("max_turns = 12", "temperature = 0.7");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(&path, &text).unwrap();

        let err = load(&path).unwrap_err();
        let ConfigError::Invalid(invalid) = err else {
            panic!("an unknown key must be an invalid document");
        };
        assert!(
            invalid.message.contains("temperature"),
            "the diagnostic must name the offending key, got {}",
            invalid.message
        );
        let offset = invalid
            .span
            .expect("miette needs somewhere to point")
            .offset();
        assert_eq!(
            line_of(&text, offset),
            text.lines()
                .position(|l| l.starts_with("temperature"))
                .unwrap()
                + 1,
            "the span must land on the offending line"
        );
    }

    /// The same, one table over: two `deny_unknown_fields` attributes are two
    /// separate chances to forget one.
    #[test]
    fn an_unknown_key_inside_the_workspace_table_is_refused() {
        let bad = VALID.replace("isolation = \"git-worktree\"", "network = \"open\"");
        let err = toml::from_str::<Config>(&bad).unwrap_err();
        assert!(err.message().contains("network"), "got {}", err.message());
    }

    /// `isolation` and `cleanup` each have exactly one value M1 can honour.
    /// Admitting a second spelling would be a promise the runtime does not
    /// keep, so the schema refuses it loudly instead of ignoring it silently.
    #[test]
    fn an_unsupported_isolation_or_cleanup_is_refused() {
        let bad = VALID.replace("isolation = \"git-worktree\"", "isolation = \"none\"");
        assert!(toml::from_str::<Config>(&bad).is_err());
        let bad = VALID.replace("isolation = \"git-worktree\"", "cleanup = \"never\"");
        assert!(toml::from_str::<Config>(&bad).is_err());
    }

    /// A duration carries its own unit, so a field name cannot disagree with
    /// the value under it.
    #[test]
    fn a_duration_is_written_with_its_unit() {
        let with = |text: &str| {
            toml::from_str::<Config>(&VALID.replace(
                "max_turns = 12",
                &format!("max_turns = 12\ndeadline = \"{text}\""),
            ))
            .map(|c| c.agent.unwrap().deadline.as_duration())
        };
        assert_eq!(with("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(with("45m").unwrap(), Duration::from_secs(2700));
        assert_eq!(with("2h").unwrap(), Duration::from_secs(7200));

        // A bare number is exactly the ambiguity the string form removes.
        assert!(with("45").is_err(), "a unit-free number must not parse");
        assert!(with("45 minutes").is_err(), "unknown unit");
        assert!(with("-5m").is_err(), "a negative bound is not a bound");
        // A zero bound can never be satisfied: every attempt under it is born
        // already out of time, so a typo would look like a systematic failure
        // of the agent rather than of the document.
        assert!(with("0s").is_err(), "a zero bound is a typo, not a policy");
    }

    /// A duration `config check` reports can be written straight back into a
    /// document.
    ///
    /// The round trip is the contract, not the spelling: a caller reading
    /// `"45m"` out of a payload must be able to put `"45m"` into a file. The
    /// rendering is asserted too, because "45m" and "2700s" are the same bound
    /// and only one of them is the one the operator wrote.
    #[test]
    fn a_rendered_duration_parses_back_to_itself() {
        for (seconds, expected) in [
            (45 * 60, "45m"),
            (15 * 60, "15m"),
            (2 * 3600, "2h"),
            (90, "90s"),
            (1, "1s"),
        ] {
            let rendered = HumanDuration::secs(seconds).to_string();
            assert_eq!(rendered, expected);
            assert_eq!(
                rendered.parse::<HumanDuration>().unwrap().as_duration(),
                Duration::from_secs(seconds),
                "a reported duration must be one this schema accepts: {rendered}"
            );
        }
    }

    /// Refusing a credential must not publish it.
    ///
    /// The diagnostic is line-aware, so by default it quotes the source line —
    /// and for this one mistake the source line is the secret. What a reader
    /// needs is the location and the fix, neither of which requires the bytes.
    #[test]
    fn the_refusal_of_a_credential_does_not_repeat_it() {
        let text = VALID.replace(
            r#"api_key = { env = "LITELLM_API_KEY" }"#,
            r#"api_key = "sk-literal-secret""#,
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(&path, &text).unwrap();

        let ConfigError::Invalid(invalid) = load(&path).unwrap_err() else {
            panic!("a literal credential must be an invalid document");
        };
        let rendered = format!("{:?}", miette::Report::new(*invalid));
        assert!(
            !rendered.contains("sk-literal-secret"),
            "the rendered diagnostic quoted the credential: {rendered}"
        );
        assert!(
            rendered.contains(REDACTED),
            "the value must still be shown as redacted, so the caret has \
             something to point at: {rendered}"
        );
        assert!(
            rendered.contains("fiddle.toml:14"),
            "redacting must not cost the location: {rendered}"
        );
    }

    /// Redaction moves the span with the bytes, or gives up on the snippet
    /// altogether. It must never leave a caret pointing into shifted text.
    #[test]
    fn redaction_keeps_the_span_over_the_placeholder() {
        let (text, span) = redacted("api_key = \"sk-x\"\nnext = 1\n".into(), Some(10..16));
        assert_eq!(text, format!("api_key = {REDACTED}\nnext = 1\n"));
        assert_eq!(&text[span.clone().unwrap()], REDACTED);
        assert_eq!(
            span.unwrap().start,
            10,
            "the value still starts where it did"
        );

        // A range no document can produce, checked anyway: the snippet goes
        // rather than being rendered against text it no longer describes.
        assert_eq!(redacted("short".into(), Some(2..99)), (String::new(), None));
    }

    /// The 1-based line `offset` falls on.
    fn line_of(text: &str, offset: usize) -> usize {
        text[..offset].chars().filter(|c| *c == '\n').count() + 1
    }

    /// The three keys with no defensible default, and nothing else.
    const FORGE: &str = r#"
[project]
name = "p"

[stub]
root = "s"

[report]
dir = "r"

[github]
repo = "peel/fiddle"
base = "main"
token = { env = "FIDDLE_GITHUB_TOKEN" }
"#;

    fn github(text: &str) -> GitHub {
        toml::from_str::<Config>(text).unwrap().github.unwrap()
    }

    #[test]
    fn the_github_table_loads_and_names_its_credential() {
        let github = github(FORGE);
        assert_eq!(github.repo.to_string(), "peel/fiddle");
        assert_eq!(github.repo.owner, "peel");
        assert_eq!(github.repo.name, "fiddle");
        assert_eq!(github.base, "main");
        assert_eq!(github.token.env, "FIDDLE_GITHUB_TOKEN");
    }

    /// The defaults are a claim about what a publication does when nobody says
    /// otherwise, so they are pinned rather than left to whatever a `default_*`
    /// function happened to return.
    #[test]
    fn the_forge_defaults_are_the_ones_documented() {
        let github = github(FORGE);
        assert_eq!(github.cli.program, "gh");
        assert!(github.cli.args.is_empty());
        assert_eq!(github.git, PathBuf::from("git"));
        assert_eq!(github.config_dir, PathBuf::from(".fiddle/gh-config"));
        assert_eq!(github.timeout.as_duration(), Duration::from_secs(5 * 60));
        assert!(github.required_checks.is_empty());
        // Neither may be invented when absent; both are refused by name at the
        // moment they are needed.
        assert_eq!(github.work, None);
        assert_eq!(github.workflow, None);
    }

    // -----------------------------------------------------------------------
    // `[github] read_retry`
    // -----------------------------------------------------------------------

    /// Absent means the documented budget, and it is pinned here rather than
    /// left to whatever three `default_*` functions happened to return.
    #[test]
    fn the_read_retry_defaults_are_the_ones_documented() {
        let github = github(FORGE);
        assert_eq!(github.read_retry.attempts, 4);
        assert_eq!(
            github.read_retry.initial.as_duration(),
            Duration::from_secs(1)
        );
        assert_eq!(github.read_retry.max.as_duration(), Duration::from_secs(4));
    }

    /// The document's numbers, and the same numbers on the runtime's own type.
    #[test]
    fn a_written_read_retry_reaches_the_runtime_type() {
        let github = github(&format!(
            "{FORGE}read_retry = {{ attempts = 3, initial = \"2s\", max = \"1m\" }}\n"
        ));
        assert_eq!(github.read_retry.attempts, 3);

        let retry = github.read_retry.as_read_retry();
        assert_eq!(retry.attempts(), 3);
        // The ceiling, asserted through what it *does* rather than through an
        // accessor: a `Retry-After` longer than `max` is capped at `max`, so a
        // wait of exactly a minute is the document's `"1m"` and nothing else.
        assert_eq!(
            retry.delay(
                1,
                fiddle_runtime::RetryAdvice {
                    retry_after: Some(Duration::from_secs(3600)),
                    rate_limit_remaining: None,
                },
                &fiddle_core::EffectId("0".repeat(16)),
            ),
            Duration::from_secs(60)
        );
    }

    /// Strictness reaches one table deeper. `deny_unknown_fields` on `[github]`
    /// does not reach into a child, so a mistyped `attempt = 8` would otherwise
    /// parse as nothing and leave an operator believing they had set a bound.
    #[test]
    fn an_unknown_key_inside_the_read_retry_table_is_refused() {
        let bad = format!("{FORGE}read_retry = {{ attempt = 8 }}\n");
        assert!(toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .contains("attempt"));
    }

    /// A budget of zero reads is not a stricter policy — it is a postcondition
    /// that is never observed, so every effect would be unresolved. Refused in
    /// the file, which is the only place an operator can fix it.
    #[test]
    fn a_budget_of_no_reads_at_all_is_refused() {
        let bad = format!("{FORGE}read_retry = {{ attempts = 0 }}\n");
        let message = toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .to_string();
        assert!(
            message.contains("at least once"),
            "the refusal must say what is wrong, got {message}"
        );

        // And one read is the way to ask for no waiting, so it must be legal.
        let fine = format!("{FORGE}read_retry = {{ attempts = 1 }}\n");
        assert_eq!(github(&fine).read_retry.attempts, 1);
    }

    /// A first wait longer than the ceiling on any wait is a document whose
    /// `initial` could never be honoured. Refused rather than silently
    /// shortened to a number nobody wrote.
    #[test]
    fn a_first_wait_above_the_ceiling_is_refused() {
        let bad = format!("{FORGE}read_retry = {{ initial = \"30s\", max = \"4s\" }}\n");
        let message = toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .to_string();
        assert!(message.contains("longer than the ceiling"), "got {message}");
    }

    /// The whole point of the key: the document changes what a run *does*.
    ///
    /// Two documents, two read counts, one executor built the way `main` builds
    /// it. A test that only asserted the numbers survived deserialization would
    /// pass on a build where nothing downstream ever looked at them — which is
    /// exactly the defect this key was added to stop repeating.
    #[tokio::test]
    async fn the_document_decides_how_many_times_the_postcondition_is_read() {
        async fn reads_under(document: &str) -> usize {
            let github = github(&format!("{FORGE}{document}"));
            let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let ctx = unreachable_effect_context();
            let deployment = AllowEverything;
            let executor = fiddle_runtime::effect::Executor::new(
                fiddle_core::PUBLISH_CHANGE,
                "p".to_string(),
                "beans:w-1".to_string(),
                &deployment,
                &ctx,
                &DiscardTheWalk,
                github.read_retry.as_read_retry(),
            );
            let proposed = fiddle_core::ProposedEffect {
                capability: fiddle_core::PUBLISH_CHANGE,
                kind: fiddle_core::EffectKind::EnsureBranchPublished,
                target: "refs/heads/fiddle/abc".to_string(),
                payload: "{}".to_string(),
            };
            executor
                .execute(proposed, NeverSettles(counter.clone()))
                .await
                .expect_err("a postcondition that never appears is never a success");
            counter.load(std::sync::atomic::Ordering::SeqCst)
        }

        // One look before the mutation, then the budget after it.
        assert_eq!(
            reads_under("read_retry = { attempts = 1 }\n").await,
            2,
            "one attempt is one post-mutation read"
        );
        assert_eq!(
            reads_under("read_retry = { attempts = 3, initial = \"1s\", max = \"1s\" }\n").await,
            4,
            "and three is three — the document, not a constant in the executor"
        );
    }

    /// An operation whose postcondition never appears, counting its looks.
    ///
    /// Deliberately not the runtime suite's scripted world: this test is about
    /// the path from *this file's* document to the executor, so it holds its own
    /// minimal world rather than reaching into another crate's fixtures.
    struct NeverSettles(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    struct NeverObserved;

    impl fiddle_runtime::effect::ObservedState for NeverObserved {
        type Value = ();
        fn describe(&self) -> String {
            unreachable!("this postcondition is never observed")
        }
        fn reference(&self) -> Option<String> {
            None
        }
        fn into_value(self) {}
    }

    #[async_trait::async_trait]
    impl fiddle_runtime::effect::IntegrationOperation for NeverSettles {
        type State = NeverObserved;

        fn minimum(&self) -> fiddle_core::HumanDecisionRequirement {
            fiddle_core::HumanDecisionRequirement::Automatic
        }

        /// The same payload the proposal above names. This test's subject is the
        /// read budget, so it has to get past the executor's step 6 — and it does
        /// so by agreeing with the proposal rather than by being exempt from it.
        fn payload(&self) -> String {
            "{}".to_string()
        }

        async fn inspect(
            &self,
            _ctx: &fiddle_runtime::effect::EffectContext,
        ) -> Result<Option<NeverObserved>, fiddle_runtime::GhError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }

        async fn apply(
            &self,
            _ctx: &fiddle_runtime::effect::EffectContext,
            _authorized: &fiddle_runtime::effect::AuthorizedEffect<Self>,
        ) -> Result<(), fiddle_runtime::GhError> {
            Ok(())
        }
    }

    struct AllowEverything;

    impl fiddle_runtime::effect::DeploymentPolicy for AllowEverything {
        fn rule_for(&self, _kind: fiddle_core::EffectKind) -> fiddle_core::DeploymentRule {
            fiddle_core::DeploymentRule::Allow
        }
    }

    struct DiscardTheWalk;

    impl fiddle_runtime::effect::EffectTrace for DiscardTheWalk {
        fn step(
            &self,
            _kind: fiddle_core::EffectKind,
            _step: fiddle_runtime::effect::ExecutionStep,
        ) {
        }
    }

    /// A context nothing above reaches: the operation ignores it, and both
    /// program paths are ones that do not exist, so a change that made the
    /// executor talk to GitHub behind the operation's back would fail loudly
    /// rather than quietly acquire a network.
    fn unreachable_effect_context() -> fiddle_runtime::effect::EffectContext {
        fiddle_runtime::effect::EffectContext::new(
            fiddle_runtime::GhCli::new(
                PathBuf::from("/nonexistent/gh"),
                Vec::new(),
                String::new(),
                "GH_TOKEN",
                PathBuf::from("/nonexistent"),
                Duration::from_secs(1),
            ),
            fiddle_runtime::git::GitCli::new(
                PathBuf::from("/nonexistent/git"),
                String::new(),
                "FIDDLE_GITHUB_TOKEN",
                Duration::from_secs(1),
            ),
            PathBuf::from("/nonexistent"),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    /// A resolved forge credential must not parse, for the reason a resolved
    /// model credential must not: a document that can hold one gets committed
    /// holding one.
    #[test]
    fn a_literal_forge_token_is_refused() {
        let bad = FORGE.replace(
            r#"token = { env = "FIDDLE_GITHUB_TOKEN" }"#,
            r#"token = "ghp_a_literal_secret""#,
        );
        let error = toml::from_str::<Config>(&bad).unwrap_err();
        assert_eq!(
            error.message(),
            CREDENTIAL_MUST_BE_NAMED,
            "the refusal must be the one `load` redacts the source line for"
        );
    }

    /// Strictness reaches into both new tables, not merely admits them.
    #[test]
    fn an_unknown_key_inside_the_github_or_policy_table_is_refused() {
        let bad = FORGE.replace("base = \"main\"", "reviewers = [\"someone\"]");
        assert!(toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .contains("reviewers"));

        let bad = format!("{FORGE}\n[github.policy]\nensure_everything = \"deny\"\n");
        assert!(toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .contains("ensure_everything"));
    }

    /// A repository is `owner/name`, and the owner half is load-bearing: it is
    /// the label a pull request's head is matched on. A value it cannot be
    /// derived from is refused here rather than after a branch has been pushed.
    #[test]
    fn a_repository_is_an_owner_and_a_name() {
        assert_eq!(
            "peel/fiddle".parse::<Repo>().unwrap(),
            Repo {
                owner: "peel".to_string(),
                name: "fiddle".to_string()
            }
        );
        for bad in ["fiddle", "peel/", "/fiddle", "peel/fiddle/extra", ""] {
            assert!(
                bad.parse::<Repo>().is_err(),
                "{bad:?} is not owner/name and must not parse"
            );
        }
        // A dot and a dash are ordinary in a repository name and must survive.
        assert_eq!(
            "peel/fiddle-effects.acceptance"
                .parse::<Repo>()
                .unwrap()
                .name,
            "fiddle-effects.acceptance"
        );
        // Rendered back the way it was written, because it is pasted into an
        // API path in exactly that form.
        assert_eq!(
            "peel/fiddle".parse::<Repo>().unwrap().to_string(),
            "peel/fiddle"
        );
    }

    /// Every kind a document may write a rule for, with the key it is written as.
    ///
    /// One list rather than two, because the two scenarios below assert the same
    /// mapping from opposite sides and a list that was extended in one of them
    /// would leave the other silently sampling. Adding a kind to
    /// [`PolicyTable::rule_for`] without adding it here leaves an arm no test
    /// reaches — which is the state `publish_decision_request` and
    /// `ensure_pull_request_ready` were in between the task that added them and
    /// this one.
    const RULE_KEYS: [(&str, EffectKind); EffectKind::ALL.len()] = [
        ("ensure_branch_published", EffectKind::EnsureBranchPublished),
        ("ensure_pull_request", EffectKind::EnsurePullRequest),
        ("ensure_check_requested", EffectKind::EnsureCheckRequested),
        (
            "publish_decision_request",
            EffectKind::PublishDecisionRequest,
        ),
        (
            "ensure_pull_request_ready",
            EffectKind::EnsurePullRequestReady,
        ),
        (
            "ensure_pull_request_body",
            EffectKind::EnsurePullRequestBody,
        ),
    ];

    /// The list above holds every kind this build has, rather than however many
    /// somebody remembered.
    ///
    /// Its length is [`EffectKind::ALL`]'s, so a kind added without a line here
    /// is a compile error rather than a lane that quietly stops covering it. The
    /// length alone would still admit a list that named one kind twice and
    /// another not at all, which is what this asserts away.
    #[test]
    fn the_rule_keys_cover_every_effect_kind() {
        let named: std::collections::BTreeSet<_> =
            RULE_KEYS.iter().map(|(_, kind)| kind.as_str()).collect();
        assert_eq!(named.len(), RULE_KEYS.len(), "a kind is listed twice");
        for kind in EffectKind::ALL {
            assert!(
                named.contains(kind.as_str()),
                "{} has no rule key, so no document can gate it",
                kind.as_str()
            );
        }
        // And each key really is the kind's own wire spelling, which is what
        // makes the document's vocabulary and the identity's one word.
        for (key, kind) in RULE_KEYS {
            assert_eq!(key, kind.as_str());
        }
    }

    /// **Each key answers for its own effect kind, and for no other.**
    ///
    /// Written out over the whole product rather than sampled: the failure this
    /// guards against is a cross-wiring, where a rule an operator wrote for one
    /// kind silently governs another — and a sampled test would pass on a
    /// `rule_for` that returned one field for everything.
    #[test]
    fn every_rule_key_governs_the_effect_kind_it_is_named_after() {
        let kinds = RULE_KEYS;
        for (key, _) in kinds {
            let table = github(&format!("{FORGE}\n[github.policy]\n{key} = \"deny\"\n")).policy;
            for (other_key, other_kind) in kinds {
                let expected = match other_key == key {
                    true => DeploymentRule::Deny,
                    // Every kind the document said nothing about adds no gate.
                    false => DeploymentRule::Allow,
                };
                assert_eq!(
                    table.rule_for(other_kind),
                    expected,
                    "with only {key} denied, {other_key} must be {expected:?}"
                );
            }
        }
    }

    /// All three rules are spellings the document may write, and a fourth is
    /// not: a rule this build cannot honour is refused rather than read as the
    /// permissive one.
    #[test]
    fn the_rules_a_document_may_write_are_the_three_that_exist() {
        for (written, expected) in [
            ("allow", DeploymentRule::Allow),
            ("require_human", DeploymentRule::RequireHuman),
            ("deny", DeploymentRule::Deny),
        ] {
            let policy = github(&format!(
                "{FORGE}\n[github.policy]\nensure_pull_request = \"{written}\"\n"
            ))
            .policy;
            assert_eq!(policy.rule_for(EffectKind::EnsurePullRequest), expected);
        }
        assert!(
            toml::from_str::<Config>(&format!(
                "{FORGE}\n[github.policy]\nensure_pull_request = \"probably\"\n"
            ))
            .is_err(),
            "a rule nothing can honour must not load"
        );
    }

    /// An absent `[github.policy]` is the same table as one written with every
    /// key set to `allow` — a document that says nothing must not be stricter
    /// than one that says so out loud.
    ///
    /// The `allow` document is built from [`RULE_KEYS`] rather than written out,
    /// so a kind added to the enumeration is compared here too instead of being
    /// asserted about a table that never mentioned it.
    #[test]
    fn an_absent_policy_table_adds_no_gate() {
        let absent = github(FORGE).policy;
        let spelled = github(&format!(
            "{FORGE}\n[github.policy]\n{}",
            RULE_KEYS
                .iter()
                .map(|(key, _)| format!("{key} = \"allow\"\n"))
                .collect::<String>()
        ))
        .policy;
        for (key, kind) in RULE_KEYS {
            assert_eq!(absent.rule_for(kind), DeploymentRule::Allow, "{key}");
            assert_eq!(absent.rule_for(kind), spelled.rule_for(kind), "{key}");
        }
    }

    /// **A silent document does not disarm the human gate.**
    ///
    /// This is the cell [`fiddle_core::combine`] exists for, reached through the
    /// deployment table rather than through a unit test of the policy module, and
    /// it is what makes defaulting the two new rows safe rather than lax: `Allow`
    /// is *this document adds no gate*, and the capability's own minimum survives
    /// it.
    ///
    /// The second half is the one a reader would not think to check. The question
    /// itself must never be gated — an effect that published a question and then
    /// needed a decision to publish it could not terminate — so
    /// `publish_decision_request` defaulting to `allow` is load-bearing in the
    /// opposite direction from its sibling.
    #[test]
    fn a_silent_policy_table_still_requires_a_human_for_the_ready_transition() {
        let table = github(FORGE).policy;
        assert_eq!(
            table.rule_for(EffectKind::EnsurePullRequestReady),
            DeploymentRule::Allow,
            "a document that says nothing must not be stricter than one saying so"
        );
        assert!(
            matches!(
                fiddle_core::combine(
                    fiddle_core::HumanDecisionRequirement::Human,
                    table.rule_for(EffectKind::EnsurePullRequestReady)
                ),
                fiddle_core::PolicyDecision::RequireHumanDecision { .. }
            ),
            "the human gate must survive a document that names neither new kind"
        );
        assert_eq!(
            fiddle_core::combine(
                fiddle_core::HumanDecisionRequirement::Automatic,
                table.rule_for(EffectKind::PublishDecisionRequest)
            ),
            fiddle_core::PolicyDecision::Allow,
            "publishing a question cannot itself require a question"
        );
    }

    /// **A document may still deny the gated transition outright**, and then
    /// there is nothing left to ask about.
    ///
    /// [`fiddle_core::combine`]'s first arm, reached from a document rather than
    /// from a literal: the deny is written in `[github.policy]`, read back through
    /// `rule_for`, and handed to `combine` beside the capability's own `Human`
    /// minimum — the pair that would be a question if the document had said
    /// nothing. What the process does with that decision is
    /// `EffectError::PolicyDenied`'s `Recurrence::Permanent`, which is exit 20;
    /// this asserts the half that starts in the file.
    #[test]
    fn a_denied_ready_transition_is_a_denial_and_not_a_question() {
        let table = github(&format!(
            "{FORGE}\n[github.policy]\nensure_pull_request_ready = \"deny\"\n"
        ))
        .policy;
        assert_eq!(
            table.rule_for(EffectKind::EnsurePullRequestReady),
            DeploymentRule::Deny
        );
        assert!(
            matches!(
                fiddle_core::combine(
                    fiddle_core::HumanDecisionRequirement::Human,
                    table.rule_for(EffectKind::EnsurePullRequestReady)
                ),
                fiddle_core::PolicyDecision::Deny { .. }
            ),
            "a settled refusal is not a wait, even against a Human minimum"
        );
    }

    // -----------------------------------------------------------------------
    // `[github.decision]`
    // -----------------------------------------------------------------------

    /// [`FORGE`] with a `[github.decision]` table carrying `body`.
    ///
    /// A mutation of a document already known to load, for the reason every other
    /// scenario in this file uses one: a refusal is then attributable to the lines
    /// under test and to nothing else in the file.
    fn with_decision(body: &str) -> String {
        format!("{FORGE}\n[github.decision]\n{body}\n")
    }

    /// The table [`with_decision`] builds, or the refusal it earned.
    fn decision(body: &str) -> Result<Decision, toml::de::Error> {
        toml::from_str::<Config>(&with_decision(body)).map(|config| {
            config
                .github
                .expect("the forge table is there")
                .decision
                .expect("the decision table is there")
        })
    }

    /// The table loads, and the ids arrive in the order the document wrote them.
    ///
    /// The order is asserted rather than the set, because the walk's rule for which
    /// reply decides is about ordering too, and a list quietly sorted or reversed on
    /// the way in is the kind of difference no other test here would see.
    #[test]
    fn the_decision_table_names_who_may_decide() {
        let decision = decision("authorized = [505401, 42]").unwrap();
        assert_eq!(decision.authorized, [505401, 42]);
    }

    /// **Empty is not permissive.**
    ///
    /// A deployment naming nobody can publish a question and can never accept an
    /// answer, so every run under it would suspend for ever — a document to refuse
    /// rather than a run that accepts anybody. The message has to say *why*,
    /// because "authorized is empty" is a restatement of the line the operator is
    /// already looking at.
    ///
    /// **The caret lands on the table, not on the key**, and the name says so
    /// rather than claiming the key's own line: the constraint is applied by
    /// [`Decision`]'s conversion, which serde reaches once the whole table has
    /// been read, so the span it can attribute the refusal to is the table's. The
    /// alternative — a newtype on the field, the way [`Repo`] refuses a
    /// repository at `repo`'s own line — would buy one line of precision for a
    /// second type and a second place to look, over a table whose every key is on
    /// the two lines the caret already covers.
    #[test]
    fn an_empty_authorized_list_is_refused_at_the_table_it_was_written_in() {
        let text = with_decision("authorized = []");
        let error = toml::from_str::<Config>(&text).unwrap_err();
        let message = error.message().to_string();
        assert!(message.contains("authorized"), "got {message}");
        assert!(
            message.contains("nobody"),
            "the reason must be stated: {message}"
        );

        // And it is refused *somewhere* an operator can go and edit, which is the
        // whole value of a line-aware diagnostic.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(&path, &text).unwrap();
        let ConfigError::Invalid(invalid) = load(&path).unwrap_err() else {
            panic!("a document naming nobody must be an invalid document");
        };
        let offset = invalid
            .span
            .expect("miette needs somewhere to point")
            .offset();
        assert_eq!(
            line_of(&text, offset),
            text.lines()
                .position(|line| line.starts_with("[github.decision]"))
                .unwrap()
                + 1,
            "the span must land on the table that named nobody"
        );
    }

    /// Absent is refused too, and differently: one document forgot the key and
    /// one emptied it. There is no default, because a guessed approver is a
    /// guessed authorization.
    #[test]
    fn an_absent_authorized_list_is_refused() {
        // A table with nothing in it, which is what "forgot the key" looks like now
        // that the table has exactly one key to forget.
        let error = toml::from_str::<Config>(&with_decision("")).unwrap_err();
        assert!(
            error.message().contains("authorized"),
            "the refusal must name the missing key, got {}",
            error.message()
        );
        // Distinct from the empty case: that one explains itself, and this one is
        // serde saying a required field is not there.
        assert!(
            !error.message().contains("nobody"),
            "a forgotten key and an emptied one must not read alike: {}",
            error.message()
        );
    }

    /// Strictness reaches one table deeper, for [`ReadRetryTable`]'s reason:
    /// `deny_unknown_fields` on `[github]` does not reach into a child, and a
    /// misspelled key that parsed as nothing would be an authorization an operator
    /// believes they wrote.
    ///
    /// **The document is otherwise complete, and that is what makes this a test
    /// about a mistyped key rather than about an unknown one.** The stray key is a
    /// misspelling of the table's only real key, beside a correct spelling of it —
    /// so the same document with the stray line removed loads, which is asserted
    /// here, and the refusal is therefore attributable to strictness and to nothing
    /// else. Without that half the scenario would keep passing over a table that had
    /// stopped being strict, on the strength of `authorized` being absent instead.
    #[test]
    fn a_mistyped_key_in_the_decision_table_is_refused() {
        let error =
            toml::from_str::<Config>(&with_decision("authorized = [505401]\nauthorised = [42]"))
                .unwrap_err();
        assert!(
            error.message().contains("authorised"),
            "the diagnostic must name the key that will do nothing, got {}",
            error.message()
        );
        // The half that keeps the scenario honest: strictness is the only reason the
        // document above was refused.
        assert_eq!(
            decision("authorized = [505401]").unwrap().authorized,
            [505401],
            "the same document without the misspelling has to load"
        );
    }

    /// **The type refuses a login, so no document can express one.**
    ///
    /// Not a check somebody could forget: `Vec<u64>` means a deployment cannot
    /// write an authorization that the code would then have to match loosely
    /// against a name that can be changed and reclaimed.
    #[test]
    fn the_authorized_list_takes_ids_and_refuses_logins() {
        assert!(decision(r#"authorized = ["peel"]"#).is_err());
        assert!(
            decision(r#"authorized = [505401, "peel"]"#).is_err(),
            "one login among the ids is still a login"
        );
    }

    /// A deployment that never puts a change to a person names no approvers, and
    /// the table is absent rather than invented — `work` and `workflow`'s rule,
    /// not `policy`'s.
    #[test]
    fn a_document_naming_no_decision_channel_still_loads() {
        assert!(github(FORGE).decision.is_none());
    }

    /// An M0-shaped document still loads, with the third table absent rather
    /// than invented.
    #[test]
    fn a_document_naming_no_forge_still_loads() {
        let cfg: Config =
            toml::from_str("[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n")
                .unwrap();
        assert!(cfg.github.is_none());
    }
}
