use fiddle_core::{DeploymentRule, EffectKind, Severities};
use fiddle_runtime::effect::DeploymentPolicy;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    #[serde(default)]
    pub github: Option<GitHub>,
    #[serde(default)]
    pub scanner: Option<Scanner>,
    #[serde(default)]
    pub orchestration: Option<Orchestration>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scanner {
    #[serde(default = "default_wizcli")]
    pub cli: ProgramRef,

    #[serde(default = "default_scan_timeout")]
    pub timeout: HumanDuration,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orchestration {
    #[serde(default)]
    pub cve: Option<OrchestrationCve>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrchestrationCve {
    pub image: String,

    #[serde(default)]
    pub severities: Severities,

    #[serde(default = "default_max_findings")]
    pub max_findings: usize,

    #[serde(default = "default_cve_title")]
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stub {
    pub root: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub dir: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub model: String,

    pub base_url: WrittenOrNamed,

    pub api_key: EnvRef,

    #[serde(default = "default_max_turns")]
    pub max_turns: usize,

    #[serde(default = "default_max_capability_attempts")]
    pub max_capability_attempts: usize,

    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,

    #[serde(default = "default_max_changed_files")]
    pub max_changed_files: usize,

    #[serde(default = "default_deadline")]
    pub deadline: HumanDuration,

    #[serde(default = "default_tool_timeout")]
    pub tool_timeout: HumanDuration,
}

#[derive(Debug)]
pub struct EnvRef {
    pub env: String,
}

const CREDENTIAL_MUST_BE_NAMED: &str = "a credential must be named here, never written here: use \
     `{ env = \"VARIABLE_NAME\" }` and export the value instead";

impl<'de> Deserialize<'de> for EnvRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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

    fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<EnvRef, A::Error> {
        Ok(EnvRef {
            env: named_variable(map)?,
        })
    }
}

fn named_variable<'de, A: serde::de::MapAccess<'de>>(mut map: A) -> Result<String, A::Error> {
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
    Ok(env)
}

#[derive(Debug)]
pub enum WrittenOrNamed {
    Written(String),
    Named(String),
}

impl<'de> Deserialize<'de> for WrittenOrNamed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(WrittenOrNamedVisitor)
    }
}

struct WrittenOrNamedVisitor;

impl<'de> serde::de::Visitor<'de> for WrittenOrNamedVisitor {
    type Value = WrittenOrNamed;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(
            "a value written as a string, or a table naming an environment \
             variable, as `{ env = \"NAME\" }`",
        )
    }

    fn visit_str<E: serde::de::Error>(self, written: &str) -> Result<WrittenOrNamed, E> {
        Ok(WrittenOrNamed::Written(written.to_string()))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<WrittenOrNamed, A::Error> {
        Ok(WrittenOrNamed::Named(named_variable(map)?))
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "WorkspaceDocument")]
pub struct Workspace {
    pub root: PathBuf,

    pub fixture: Option<PathBuf>,

    pub check: Option<ProgramRef>,

    pub checks: Vec<CheckRef>,

    pub commands: Vec<CommandRef>,

    pub isolation: Isolation,

    pub command_timeout: HumanDuration,

    pub cleanup: Cleanup,
}

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
    commands: Vec<CommandRef>,
    #[serde(default)]
    isolation: Isolation,
    #[serde(default = "default_command_timeout")]
    command_timeout: HumanDuration,
    #[serde(default)]
    cleanup: Cleanup,
}

impl TryFrom<WorkspaceDocument> for Workspace {
    type Error = String;

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
        if let Some(twice) = declared_twice(&document.commands) {
            return Err(format!(
                "`[[workspace.commands]]` declares `{twice}` twice. A declaration \
                 is what an attempt may run and how much of it the attempt may \
                 vary, and two entries spelling the same program and the same \
                 arguments answer that twice. Keep the one that says what this \
                 deployment means"
            ));
        }
        Ok(Self {
            root: document.root,
            fixture: document.fixture,
            check: document.check,
            checks: document.checks,
            commands: document.commands,
            isolation: document.isolation,
            command_timeout: document.command_timeout,
            cleanup: document.cleanup,
        })
    }
}

fn declared_twice(commands: &[CommandRef]) -> Option<String> {
    let mut seen: Vec<(&str, &[String])> = Vec::new();
    for command in commands {
        let spelled = (command.program.as_str(), command.args.as_slice());
        if seen.contains(&spelled) {
            return Some(spelled_command(command));
        }
        seen.push(spelled);
    }
    None
}

fn spelled_command(command: &CommandRef) -> String {
    let mut spelled = command.program.clone();
    for argument in &command.args {
        spelled.push(' ');
        spelled.push_str(argument);
    }
    spelled
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRef {
    pub program: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default)]
    pub extend: Extend,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Extend {
    #[default]
    None,

    Arguments,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRef {
    pub program: String,

    #[serde(default)]
    pub args: Vec<String>,

    pub success: Success,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Success {
    ExitZero,

    ExitZeroAndNoOutput,

    ArtefactWritten,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRef {
    pub program: String,

    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHub {
    pub repo: Repo,

    pub base: String,

    pub token: EnvRef,

    #[serde(default = "default_gh")]
    pub cli: ProgramRef,

    #[serde(default = "default_git")]
    pub git: PathBuf,

    #[serde(default)]
    pub work: Option<PathBuf>,

    #[serde(default)]
    pub workflow: Option<String>,

    #[serde(default)]
    pub required_checks: Vec<String>,

    #[serde(default = "default_gh_config_dir")]
    pub config_dir: PathBuf,

    #[serde(default = "default_effect_timeout")]
    pub timeout: HumanDuration,

    #[serde(default)]
    pub read_retry: ReadRetryTable,

    #[serde(default)]
    pub policy: PolicyTable,

    #[serde(default)]
    pub decision: Option<Decision>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "ReadRetryDocument")]
pub struct ReadRetryTable {
    pub attempts: u32,
    pub initial: HumanDuration,
    pub max: HumanDuration,
}

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
    pub fn as_read_retry(&self) -> fiddle_runtime::effect::ReadRetry {
        fiddle_runtime::effect::ReadRetry::bounded(
            self.attempts,
            self.initial.as_duration(),
            self.max.as_duration(),
        )
    }
}

const DEFAULT_READ_ATTEMPTS: u32 = 4;

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

#[derive(Debug, Deserialize)]
#[serde(try_from = "DecisionDocument")]
pub struct Decision {
    pub authorized: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionDocument {
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Isolation {
    #[default]
    GitWorktree,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Cleanup {
    #[default]
    Always,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HumanDuration(Duration);

impl HumanDuration {
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    const fn secs(secs: u64) -> Self {
        Self(Duration::from_secs(secs))
    }
}

impl std::fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let seconds = self.0.as_secs();
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

fn default_wizcli() -> ProgramRef {
    ProgramRef {
        program: "wizcli".to_string(),
        args: Vec::new(),
    }
}

fn default_scan_timeout() -> HumanDuration {
    HumanDuration::secs(20 * 60)
}

fn default_max_findings() -> usize {
    5
}

fn default_cve_title() -> String {
    DEFAULT_CVE_TITLE.to_string()
}

pub const DEFAULT_CVE_TITLE: &str = "{project}: dependency advisories";

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

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("invalid configuration in {path}")]
#[diagnostic(code(fiddle::config::invalid))]
pub struct InvalidConfig {
    pub path: PathBuf,
    #[source_code]
    src: miette::NamedSource<String>,
    #[label("{message}")]
    span: Option<miette::SourceSpan>,
    message: String,
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ConfigError {
    #[error("configuration file not found: {0}")]
    #[diagnostic(code(fiddle::config::not_found))]
    NotFound(PathBuf),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Invalid(#[from] Box<InvalidConfig>),
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text =
        std::fs::read_to_string(path).map_err(|_| ConfigError::NotFound(path.to_path_buf()))?;
    toml::from_str(&text).map_err(|e| {
        let message = e.message().to_string();
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

const REDACTED: &str = "\"<redacted>\"";

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

    #[test]
    fn the_repository_under_repair_and_its_check_are_configured() {
        let workspace = toml::from_str::<Config>(VALID).unwrap().workspace.unwrap();
        assert_eq!(workspace.fixture, Some(PathBuf::from("fixtures/m1-demo")));
        let check = workspace.check.expect("the check is part of the document");
        assert_eq!(check.program, "cargo");
        assert_eq!(check.args, ["test", "--offline"]);
    }

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

    const WORKSPACE_ONLY: &str = "[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n\
                                  [report]\ndir=\"r\"\n[workspace]\n";

    #[test]
    fn a_deployment_declares_the_programs_an_attempt_may_run() {
        let workspace = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}\
             [[workspace.commands]]\nprogram = \"go\"\nargs = [\"mod\", \"tidy\"]\n\n\
             [[workspace.commands]]\nprogram = \"go\"\nargs = [\"mod\", \"edit\"]\n\
             extend = \"arguments\"\n"
        ))
        .expect("a deployment that repairs an ecosystem declares that ecosystem")
        .workspace
        .unwrap();

        assert_eq!(workspace.commands.len(), 2);
        assert_eq!(workspace.commands[0].program, "go");
        assert_eq!(workspace.commands[0].args, ["mod", "tidy"]);
        assert_eq!(
            workspace.commands[0].extend,
            Extend::None,
            "a declaration that says nothing about extension takes no argument \
             from the model"
        );
        assert_eq!(workspace.commands[1].extend, Extend::Arguments);
    }

    #[test]
    fn a_declaration_needs_no_argument_and_no_extension_key() {
        let workspace = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}[[workspace.commands]]\nprogram = \"tidy\"\n"
        ))
        .expect("a program with no argument is a whole declaration")
        .workspace
        .unwrap();
        assert!(workspace.commands[0].args.is_empty());
        assert_eq!(workspace.commands[0].extend, Extend::None);
    }

    #[test]
    fn a_declaration_naming_no_program_is_refused() {
        let error = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}[[workspace.commands]]\nargs = [\"tidy\"]\n"
        ))
        .expect_err("arguments without a program name nothing to run");
        assert!(error.message().contains("program"), "{error}");
    }

    #[test]
    fn a_declaration_carries_no_bound_of_its_own() {
        let error = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}[[workspace.commands]]\nprogram = \"tidy\"\n\
             timeout = \"5m\"\n"
        ))
        .expect_err("a declared command is bounded by workspace.command_timeout, not by itself");
        assert!(error.message().contains("timeout"), "{error}");
    }

    #[test]
    fn an_unknown_extension_value_is_refused_rather_than_read_as_permission() {
        let error = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}[[workspace.commands]]\nprogram = \"tidy\"\n\
             extend = \"anything\"\n"
        ))
        .expect_err("a value the schema does not admit must not become the permissive one");
        assert!(
            error.message().contains("none") && error.message().contains("arguments"),
            "the refusal must name the two answers this key takes: {error}"
        );
    }

    #[test]
    fn one_command_declared_twice_is_refused_because_it_answers_one_question_twice() {
        let error = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}\
             [[workspace.commands]]\nprogram = \"go\"\nargs = [\"mod\", \"edit\"]\n\n\
             [[workspace.commands]]\nprogram = \"go\"\nargs = [\"mod\", \"edit\"]\n\
             extend = \"arguments\"\n"
        ))
        .expect_err("two entries for one command say two things about what may vary");
        assert!(
            error.message().contains("go mod edit"),
            "the refusal must name the declaration it refused: {error}"
        );
    }

    #[test]
    fn two_declarations_sharing_a_program_and_differing_in_arguments_both_stand() {
        let workspace = toml::from_str::<Config>(&format!(
            "{WORKSPACE_ONLY}\
             [[workspace.commands]]\nprogram = \"go\"\nargs = [\"mod\", \"tidy\"]\n\n\
             [[workspace.commands]]\nprogram = \"go\"\nargs = [\"mod\", \"edit\"]\n\
             extend = \"arguments\"\n"
        ))
        .expect("one program with two argument lists is two declarations")
        .workspace
        .unwrap();
        assert_eq!(workspace.commands.len(), 2);
    }

    #[test]
    fn a_document_declaring_no_command_declares_none_rather_than_a_default() {
        let workspace = toml::from_str::<Config>(WORKSPACE_ONLY)
            .unwrap()
            .workspace
            .unwrap();
        assert!(
            workspace.commands.is_empty(),
            "a default program name would put an ecosystem back into Rust"
        );
    }

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

    #[test]
    fn naming_both_shapes_is_refused_rather_than_resolved_by_precedence() {
        const SINGULAR: &str = "check = { program = \"cargo\", args = [\"test\"] }\n";
        let both = format!("{WORKSPACE_ONLY}{SINGULAR}{THREE_CHECKS}");

        toml::from_str::<toml::Table>(&both)
            .expect("the document must be well-formed TOML for its refusal to mean anything");

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
        let bad = VALID.replace(
            r#"api_key = { env = "LITELLM_API_KEY" }"#,
            r#"api_key = "sk-literal-secret""#,
        );
        assert!(toml::from_str::<Config>(&bad).is_err());
    }

    #[test]
    fn a_written_base_url_is_the_value_the_document_carries() {
        let agent = toml::from_str::<Config>(VALID).unwrap().agent.unwrap();
        let WrittenOrNamed::Written(written) = agent.base_url else {
            panic!("the document writes the endpoint, so it must load as written")
        };
        assert_eq!(written, "https://litellm.firn.snplow.net/v1");
    }

    #[test]
    fn a_named_base_url_is_the_variable_the_document_names() {
        let text = VALID.replace(
            r#"base_url = "https://litellm.firn.snplow.net/v1""#,
            r#"base_url = { env = "FIDDLE_MODEL_BASE_URL" }"#,
        );
        let agent = toml::from_str::<Config>(&text).unwrap().agent.unwrap();
        let WrittenOrNamed::Named(variable) = agent.base_url else {
            panic!("the document names the endpoint, so it must load as named")
        };
        assert_eq!(variable, "FIDDLE_MODEL_BASE_URL");
    }

    #[test]
    fn a_base_url_naming_no_variable_is_refused() {
        for table in [
            r#"{ env = "" }"#,
            r#"{ env = "  " }"#,
            "{ }",
            r#"{ name = "V" }"#,
        ] {
            let text = VALID.replace(r#""https://litellm.firn.snplow.net/v1""#, table);
            assert!(
                toml::from_str::<Config>(&text).is_err(),
                "`base_url = {table}` names no variable and must be refused"
            );
        }
    }

    #[test]
    fn a_credential_is_still_refused_in_the_form_base_url_now_accepts() {
        let bad = VALID.replace(
            r#"api_key = { env = "LITELLM_API_KEY" }"#,
            r#"api_key = "sk-literal-secret""#,
        );
        let error = toml::from_str::<Config>(&bad).unwrap_err();
        assert_eq!(
            error.message(),
            CREDENTIAL_MUST_BE_NAMED,
            "a written endpoint must not make a written credential legal"
        );
    }

    #[test]
    fn an_m0_shaped_document_still_loads() {
        let cfg: Config =
            toml::from_str("[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n")
                .unwrap();
        assert!(cfg.agent.is_none() && cfg.workspace.is_none());
    }

    #[test]
    fn forty_turns_of_the_largest_tool_result_stay_inside_the_measured_context() {
        const CONTEXT_TOKENS: usize = 262_144;
        const BYTES_PER_TOKEN: usize = 4;
        const REFUSED_REQUEST_BYTES: usize = 1_527_171;

        let per_turn =
            fiddle_runtime::agent::RESULT_CAP_BYTES + fiddle_runtime::agent::NOTE_ALLOWANCE_BYTES;
        let worst = default_max_turns() * per_turn;

        assert!(
            worst < CONTEXT_TOKENS * BYTES_PER_TOKEN,
            "{} turns of {per_turn} bytes reach {worst} bytes, and the context holds {} bytes",
            default_max_turns(),
            CONTEXT_TOKENS * BYTES_PER_TOKEN
        );
        assert!(
            worst < REFUSED_REQUEST_BYTES,
            "the gateway refused a request of {REFUSED_REQUEST_BYTES} bytes, and this bound \
             permits {worst}"
        );
    }

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

    #[test]
    fn an_unknown_key_inside_the_workspace_table_is_refused() {
        let bad = VALID.replace("isolation = \"git-worktree\"", "network = \"open\"");
        let err = toml::from_str::<Config>(&bad).unwrap_err();
        assert!(err.message().contains("network"), "got {}", err.message());
    }

    #[test]
    fn an_unsupported_isolation_or_cleanup_is_refused() {
        let bad = VALID.replace("isolation = \"git-worktree\"", "isolation = \"none\"");
        assert!(toml::from_str::<Config>(&bad).is_err());
        let bad = VALID.replace("isolation = \"git-worktree\"", "cleanup = \"never\"");
        assert!(toml::from_str::<Config>(&bad).is_err());
    }

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

        assert!(with("45").is_err(), "a unit-free number must not parse");
        assert!(with("45 minutes").is_err(), "unknown unit");
        assert!(with("-5m").is_err(), "a negative bound is not a bound");
        assert!(with("0s").is_err(), "a zero bound is a typo, not a policy");
    }

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

        assert_eq!(redacted("short".into(), Some(2..99)), (String::new(), None));
    }

    fn line_of(text: &str, offset: usize) -> usize {
        text[..offset].chars().filter(|c| *c == '\n').count() + 1
    }

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

    #[test]
    fn the_forge_defaults_are_the_ones_documented() {
        let github = github(FORGE);
        assert_eq!(github.cli.program, "gh");
        assert!(github.cli.args.is_empty());
        assert_eq!(github.git, PathBuf::from("git"));
        assert_eq!(github.config_dir, PathBuf::from(".fiddle/gh-config"));
        assert_eq!(github.timeout.as_duration(), Duration::from_secs(5 * 60));
        assert!(github.required_checks.is_empty());
        assert_eq!(github.work, None);
        assert_eq!(github.workflow, None);
    }

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

    #[test]
    fn a_written_read_retry_reaches_the_runtime_type() {
        let github = github(&format!(
            "{FORGE}read_retry = {{ attempts = 3, initial = \"2s\", max = \"1m\" }}\n"
        ));
        assert_eq!(github.read_retry.attempts, 3);

        let retry = github.read_retry.as_read_retry();
        assert_eq!(retry.attempts(), 3);
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

    #[test]
    fn an_unknown_key_inside_the_read_retry_table_is_refused() {
        let bad = format!("{FORGE}read_retry = {{ attempt = 8 }}\n");
        assert!(toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .contains("attempt"));
    }

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

        let fine = format!("{FORGE}read_retry = {{ attempts = 1 }}\n");
        assert_eq!(github(&fine).read_retry.attempts, 1);
    }

    #[test]
    fn a_first_wait_above_the_ceiling_is_refused() {
        let bad = format!("{FORGE}read_retry = {{ initial = \"30s\", max = \"4s\" }}\n");
        let message = toml::from_str::<Config>(&bad)
            .unwrap_err()
            .message()
            .to_string();
        assert!(message.contains("longer than the ceiling"), "got {message}");
    }

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
        assert_eq!(
            "peel/fiddle-effects.acceptance"
                .parse::<Repo>()
                .unwrap()
                .name,
            "fiddle-effects.acceptance"
        );
        assert_eq!(
            "peel/fiddle".parse::<Repo>().unwrap().to_string(),
            "peel/fiddle"
        );
    }

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
        for (key, kind) in RULE_KEYS {
            assert_eq!(key, kind.as_str());
        }
    }

    #[test]
    fn every_rule_key_governs_the_effect_kind_it_is_named_after() {
        let kinds = RULE_KEYS;
        for (key, _) in kinds {
            let table = github(&format!("{FORGE}\n[github.policy]\n{key} = \"deny\"\n")).policy;
            for (other_key, other_kind) in kinds {
                let expected = match other_key == key {
                    true => DeploymentRule::Deny,
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

    fn with_decision(body: &str) -> String {
        format!("{FORGE}\n[github.decision]\n{body}\n")
    }

    fn decision(body: &str) -> Result<Decision, toml::de::Error> {
        toml::from_str::<Config>(&with_decision(body)).map(|config| {
            config
                .github
                .expect("the forge table is there")
                .decision
                .expect("the decision table is there")
        })
    }

    #[test]
    fn the_decision_table_names_who_may_decide() {
        let decision = decision("authorized = [505401, 42]").unwrap();
        assert_eq!(decision.authorized, [505401, 42]);
    }

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

    #[test]
    fn an_absent_authorized_list_is_refused() {
        let error = toml::from_str::<Config>(&with_decision("")).unwrap_err();
        assert!(
            error.message().contains("authorized"),
            "the refusal must name the missing key, got {}",
            error.message()
        );
        assert!(
            !error.message().contains("nobody"),
            "a forgotten key and an emptied one must not read alike: {}",
            error.message()
        );
    }

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
        assert_eq!(
            decision("authorized = [505401]").unwrap().authorized,
            [505401],
            "the same document without the misspelling has to load"
        );
    }

    #[test]
    fn the_authorized_list_takes_ids_and_refuses_logins() {
        assert!(decision(r#"authorized = ["peel"]"#).is_err());
        assert!(
            decision(r#"authorized = [505401, "peel"]"#).is_err(),
            "one login among the ids is still a login"
        );
    }

    #[test]
    fn a_document_naming_no_decision_channel_still_loads() {
        assert!(github(FORGE).decision.is_none());
    }

    #[test]
    fn a_document_naming_no_forge_still_loads() {
        let cfg: Config =
            toml::from_str("[project]\nname=\"p\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n")
                .unwrap();
        assert!(cfg.github.is_none());
    }
}
