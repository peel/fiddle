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
//! Loading lives here rather than in `fiddle-core` because it reads the
//! filesystem, and `fiddle-core` is mechanically held pure.

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
    /// `max_capability_attempts = 5` gets one attempt, and this and
    /// `decisions/013-one-attempt-bound-not-two.md` are the only two places a
    /// reader learns it — `config check` reports such a document valid, because
    /// it is.
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
    #[serde(default = "default_max_capability_attempts")]
    #[allow(dead_code, reason = "recorded as ADR 013; see the note above")]
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
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    /// Where per-attempt worktrees are created.
    #[serde(default = "default_workspace_root")]
    pub root: PathBuf,

    /// The repository an attempt branches its worktree from and never writes
    /// to.
    #[serde(default)]
    pub fixture: Option<PathBuf>,

    /// The command that decides whether a repair earned the correlation
    /// marker.
    #[serde(default)]
    pub check: Option<Check>,

    /// How an attempt is isolated from the repository under repair.
    #[serde(default)]
    pub isolation: Isolation,

    /// Ceiling on any single command run inside the workspace, the check
    /// included. 15m is the reference configuration's value — long enough for a
    /// cold `cargo test`, short enough that a hung one is noticed within a
    /// coffee break.
    #[serde(default = "default_command_timeout")]
    pub command_timeout: HumanDuration,

    /// What happens to the worktree when the attempt ends.
    #[serde(default)]
    pub cleanup: Cleanup,
}

/// A program the workspace runs to judge an attempt.
///
/// A table of `program` and `args` rather than one shell string, because a
/// shell string has to be split by somebody and every splitter is wrong about
/// quoting somewhere. `fiddle_runtime::workspace::WorkspaceCommand` takes the
/// program and its arguments already separated; this is the same shape, so the
/// document says exactly what will be executed.
///
/// There is no `timeout` key: the bound on the check is
/// [`Workspace::command_timeout`], which is documented as the ceiling on any
/// single command run inside the workspace, *the check included*. A second
/// place to write it down is a second place for the two to disagree.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// The program to run, resolved against the workspace command runner's
    /// `PATH`.
    pub program: String,

    /// Its arguments, already separated. Defaulted empty so a check that takes
    /// none can be written as `{ program = "…" }`.
    #[serde(default)]
    pub args: Vec<String>,
}

/// How an attempt is isolated from the repository it repairs.
///
/// One variant, because M1 supports one mechanism. The enum still earns its
/// place: it turns `isolation = "none"` into a refusal at the line it was
/// written on, where a plain `String` would have accepted it and left the
/// operator believing they had turned isolation off.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
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
}
