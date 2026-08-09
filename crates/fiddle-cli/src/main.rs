mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use fiddle_core::{
    CapabilityId, FiddleBuild, InvocationRef, InvocationRefError, RunOutcome, WorkStateView,
};
use fiddle_runtime::{
    AgentBudget, AttemptContext, Capability, FixtureRepair, GatewayError, RepairConfig,
    StubChangePort, StubMark, StubWorkItemPort, WorkspaceCommand, CAPABILITIES,
};
use std::path::Path;
use std::process::ExitCode;
use tokio_util::sync::CancellationToken;

/// Usage error or invalid input — row `2` of the exit-code table. Clap already
/// exits with this code for usage errors, so the constant exists to keep every
/// half of the row visibly the same number.
const EXIT_INVALID_INPUT: u8 = 2;

/// What a process that was interrupted twice leaves behind.
///
/// Not a row of design §4.5's table, and deliberately so: every row there is a
/// conclusion fiddle *reached* about the work, and a process killed on the spot
/// reached none. 128 + `SIGINT` is what a shell reports for a process that took
/// the signal's default disposition, which is exactly what the second interrupt
/// is asking for — see [`cancel_on_interrupt`].
const EXIT_INTERRUPTED: i32 = 130;

/// The binary drives an async runtime because [`fiddle_runtime::attempt`] is a
/// future: a capability may wait on a model turn or a subprocess. Nothing about
/// what this process *prints* or *exits with* changes — the whole command is one
/// `block_on`, and every command remains a single sequential path through it.
#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let termination = match dispatch(&cli).await {
        Ok(outcome) => Termination::Ran(outcome),
        Err(error) => {
            eprintln!("{}", render::diagnostic(&error));
            Termination::Rejected(error)
        }
    };
    ExitCode::from(exit_code_for(&termination))
}

/// How an invocation ended: as a typed run outcome, or as a rejection before
/// any plan was executed.
///
/// The two halves are joined into one type so the exit-code table has one input
/// and therefore one mapping function. Without it, "the mapping lives in
/// exactly one place" would be a claim about discipline rather than about the
/// code.
enum Termination {
    /// The command reached a conclusion about the work.
    Ran(RunOutcome),
    /// The command was refused before it could. Read-only commands that
    /// succeed report [`RunOutcome::Completed`]; this arm is only reached when
    /// fiddle declined the invocation itself.
    Rejected(CliError),
}

/// Everything a command can fail with, unified so the exit-code mapping has a
/// single input type.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
enum CliError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    InvocationRef(#[from] InvalidInvocationRef),

    #[error(transparent)]
    #[diagnostic(transparent)]
    UnknownCapability(#[from] UnknownCapability),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Unconfigured(#[from] Unconfigured),

    #[error(transparent)]
    #[diagnostic(transparent)]
    CredentialAbsent(#[from] CredentialAbsent),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Gateway(#[from] GatewayUnavailable),
}

/// A `--capability` value naming nothing this build can execute.
///
/// Rejected rather than ignored: a run asked to do something fiddle has never
/// heard of and that exited 0 having done nothing would be indistinguishable
/// from a run that did the work. The diagnostic names the value *and* what this
/// build does know, because the usual cause is a typo or a stale script.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("unknown capability `{requested}`")]
#[diagnostic(
    code(fiddle::capability::unknown),
    help("this build can execute: {known}")
)]
struct UnknownCapability {
    requested: String,
    known: String,
}

/// The capability this invocation will run.
///
/// An enum rather than a bare [`CapabilityId`], because the id is only half the
/// question: the other half is what it takes to *build* the thing, and the two
/// arms need entirely different material. Making that a type is what turns
/// `--capability` from a value the CLI validates into a value the CLI acts on —
/// the defect this closes was precisely that the flag was checked against
/// [`CAPABILITIES`] and then thrown away, leaving `stub_mark` to run whatever
/// had been asked for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Selection {
    /// The deterministic capability: no model, no worktree, no credential.
    Mark,
    /// One bounded agent attempt inside an ephemeral worktree, judged by the
    /// configured check.
    Repair,
}

impl Selection {
    /// The id this selection is derived, executed, and reported under.
    fn id(self) -> CapabilityId {
        match self {
            Selection::Mark => fiddle_core::STUB_MARK,
            Selection::Repair => fiddle_core::FIXTURE_REPAIR,
        }
    }

    /// The selection `requested` names, or a rejection listing what exists.
    ///
    /// Matched against the ids themselves rather than against
    /// [`CAPABILITIES`]'s ordering, so registering a capability this binary
    /// cannot build lands in the error arm rather than silently selecting a
    /// neighbour. `every_registered_capability_can_be_selected` is what makes
    /// that a loud failure at build time rather than a quiet one at a
    /// customer's shell.
    fn parse(requested: &str) -> Result<Self, UnknownCapability> {
        if requested == fiddle_core::STUB_MARK.0 {
            Ok(Selection::Mark)
        } else if requested == fiddle_core::FIXTURE_REPAIR.0 {
            Ok(Selection::Repair)
        } else {
            Err(UnknownCapability {
                requested: requested.to_string(),
                known: CAPABILITIES
                    .iter()
                    .map(|capability| capability.0)
                    .collect::<Vec<_>>()
                    .join(", "),
            })
        }
    }
}

/// A capability was asked for that the configuration does not describe.
///
/// The table is named, not merely reported missing, because the fix is to write
/// it: an operator reading this has a document in front of them and needs to
/// know what to add to it. It is a configuration error and exits on the same
/// row as any other, because that is what it is — the document is valid TOML
/// and satisfies the schema; it just does not describe the deployment the
/// invocation asked for.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("`{capability}` needs {missing}, and {path} does not have it")]
#[diagnostic(
    code(fiddle::config::capability_unconfigured),
    help("add {missing} to {path}, or run a capability that does not need one")
)]
struct Unconfigured {
    capability: CapabilityId,
    /// What is absent, spelled the way it is written in the document — a table
    /// as `[agent]`, a key as `workspace.fixture`.
    missing: &'static str,
    /// The document as the caller named it, already rendered: a diagnostic is
    /// text, and a `PathBuf` in one has to be displayed somewhere anyway.
    path: String,
}

/// The environment does not hold the credential the configuration names.
///
/// The **only** failure in this binary that is about a credential, because
/// [`resolve_credential`] is the only place one is read. It names the variable
/// and never its value — there is none to name, which is the point: the lookup
/// failed.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("the model credential {variable} is not set")]
#[diagnostic(
    code(fiddle::config::credential_absent),
    help("export {variable}, or set it as a repository secret, before running a capability that needs a model")
)]
struct CredentialAbsent {
    variable: String,
}

/// A model client could not be built from what the configuration named.
///
/// Wraps the runtime's [`GatewayError`] rather than restating it, and carries
/// no source chain onward: the underlying failure is about an endpoint or a
/// credential that cannot become an HTTP header, and the only one of those two
/// that is safe to render is the endpoint. `GatewayError` already keeps to
/// names — the base URL and the variable — so what reaches an operator here is
/// exactly what they can act on.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error(transparent)]
#[diagnostic(
    code(fiddle::gateway::unavailable),
    help("check the endpoint and the credential the document names")
)]
struct GatewayUnavailable(GatewayError);

/// Presentation for a rejected invocation reference.
///
/// The grammar and the defect taxonomy belong to `fiddle-core`, which stays free
/// of `miette`; what belongs to the CLI is how a defect is *shown*. This wrapper
/// supplies that and nothing else — a stable diagnostic code per defect and the
/// help text that tells the caller how to fix that specific defect, so `bogus`,
/// `mystery:x`, and `beans:` are never reported with the same words.
///
/// `Diagnostic` is implemented by hand rather than derived so the three defects
/// map to their codes and help text in one visible table instead of a mirrored
/// copy of the core enum.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
struct InvalidInvocationRef(#[from] InvocationRefError);

impl miette::Diagnostic for InvalidInvocationRef {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self.0 {
            InvocationRefError::Malformed(_) => "fiddle::invocation_ref::malformed",
            InvocationRefError::UnknownScheme(_) => "fiddle::invocation_ref::unknown_scheme",
            InvocationRefError::EmptyValue => "fiddle::invocation_ref::empty_value",
            InvocationRefError::IllegalValueCharacter { .. } => {
                "fiddle::invocation_ref::illegal_value_character"
            }
        }))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self.0 {
            InvocationRefError::Malformed(_) => {
                "name the source scheme first, separated by a colon: `fiddle inspect beans:fiddle-m0-demo`"
            }
            InvocationRefError::UnknownScheme(_) => {
                "fiddle addresses work by its source; use a scheme it knows, such as `beans:fiddle-m0-demo`"
            }
            InvocationRefError::EmptyValue => {
                "the scheme is recognised but names no work; append the identifier, as in `beans:fiddle-m0-demo`"
            }
            InvocationRefError::IllegalValueCharacter { .. } => {
                "a reference names work, never a location: fiddle derives the paths it writes from this value, so it is an identifier only — write it with ASCII letters, digits, `-`, `_` and `:`, as in `beans:fiddle-m0-demo`"
            }
        }))
    }
}

/// The single realisation of the exit-code table (design §4.5).
///
/// The whole table is one `match`, so every row is visible at once and a new
/// outcome variant cannot be added without the compiler demanding its code.
/// Nothing else in the binary decides an exit code.
///
/// | code | meaning                                                    |
/// |------|------------------------------------------------------------|
/// | 0    | completed, or `config check` valid, or `inspect` succeeded  |
/// | 2    | usage error or invalid configuration                        |
/// | 10   | suspended                                                   |
/// | 11   | retryable                                                   |
/// | 20   | failed                                                       |
fn exit_code_for(termination: &Termination) -> u8 {
    match termination {
        Termination::Ran(RunOutcome::Completed) => 0,
        Termination::Ran(RunOutcome::Suspended { .. }) => 10,
        Termination::Ran(RunOutcome::Retryable { .. }) => 11,
        Termination::Ran(RunOutcome::Failed { .. }) => 20,
        Termination::Rejected(
            CliError::Config(ConfigError::NotFound(_) | ConfigError::Invalid(_))
            | CliError::InvocationRef(_)
            | CliError::UnknownCapability(_)
            // All three of the new rejections are the same row and the same
            // kind of thing: the invocation described a deployment its
            // configuration and environment do not provide, and nothing was
            // attempted. A caller scripting fiddle needs one number for "fix
            // your setup and try again", not three.
            | CliError::Unconfigured(_)
            | CliError::CredentialAbsent(_)
            | CliError::Gateway(_),
        ) => EXIT_INVALID_INPUT,
    }
}

/// The two fixture-backed ports this configuration names.
///
/// M0 has one implementation of each; the rest of the binary depends on the
/// traits, so the only thing that changes when a real adapter arrives is this
/// one function.
fn ports(config: &config::Config) -> (StubWorkItemPort, StubChangePort) {
    (
        StubWorkItemPort::new(&config.stub.root),
        StubChangePort::new(&config.stub.root),
    )
}

/// Observe both sides of the world for one invocation.
///
/// Nothing here can fail: a port that cannot read its source returns an
/// `Unavailable` observation rather than an error, so an unobservable world is
/// *reported* to the caller instead of aborting the command. That is why
/// `inspect` still exits 0 over a missing fixture root — it succeeded at
/// looking, and what it saw was that it could not see.
fn observe(config: &config::Config, reference: &InvocationRef) -> WorkStateView {
    let (work_items, changes) = ports(config);
    fiddle_runtime::observe(&work_items, &changes, reference.value())
}

/// The build identity every bundle this binary publishes carries.
///
/// Both halves are compile-time constants: `CARGO_PKG_VERSION` from the
/// manifest and `FIDDLE_SOURCE_REVISION` from `build.rs`. Passing them through
/// [`FiddleBuild::new`] rather than into the struct fields directly is what
/// makes "never fabricated" structural — a revision that is neither a 40-hex
/// sha nor `unknown` is normalised to `unknown` there rather than trusted here.
fn build_identity() -> FiddleBuild {
    FiddleBuild::new(env!("CARGO_PKG_VERSION"), env!("FIDDLE_SOURCE_REVISION"))
}

/// **The one place a credential is read.**
///
/// It takes the *name* of a variable, because that is all the configuration can
/// hold — `config::EnvRef` has no `String` variant, so a document carrying a
/// resolved secret does not parse. The value goes from here straight into the
/// gateway client and nowhere else: it is not stored on a config type, not
/// passed to a capability, not journaled, and not published. There is exactly
/// one call to this function, and grepping for `std::env::var` in this binary
/// is how that stays true.
///
/// The absence of the variable is an error rather than an empty string, so a
/// deployment that forgot to export it is told so instead of authenticating
/// with nothing and being told by the gateway.
fn resolve_credential(variable: &str) -> Result<String, CredentialAbsent> {
    std::env::var(variable).map_err(|_| CredentialAbsent {
        variable: variable.to_string(),
    })
}

/// Cancel `token` when the operator interrupts this process.
///
/// # Why this exists at all
///
/// `Workspace::run` puts every workspace command in a process group of its own,
/// so that a timed-out `cargo test` is reaped along with the test binaries it
/// spawned. The cost of that, written down at the place it is paid, is that the
/// check no longer shares this process's group — so a terminal `^C` does not
/// reach it, and a runner that simply died on the signal would leave a build
/// running over a worktree that is about to be deleted underneath it.
/// Cancellation is the only channel left, and this is what connects the two.
///
/// # Why the second interrupt is different
///
/// Installing a handler replaces `SIGINT`'s default disposition for the whole
/// process, so an operator who presses `^C` and sees nothing happen has lost
/// the ability to stop fiddle at all. The first interrupt asks for an orderly
/// stop — the token cancels, the attempt's `select!` arms lose, the check's
/// process group is signalled, and the worktree comes down through its `Drop`
/// guard. The second says the first did not work, and takes the disposition it
/// replaced: immediate exit, on [`EXIT_INTERRUPTED`].
///
/// Exiting from a spawned task while the main one is publishing is safe for the
/// same reason a power cut is: `fiddle_runtime::evidence::publish` builds the
/// attempt directory under a temporary name and moves it into place with one
/// `rename`, so what a second interrupt can leave behind is an unnoticed
/// temporary directory, never a bundle a reader could mistake for a whole one.
///
/// # Why it is installed here rather than in `main`
///
/// It is installed only on the path that has something to cancel. The
/// deterministic capability writes one file and never yields, so a handler
/// during *its* run could only ever delay a `^C` that would otherwise have
/// worked — and M0's lane, which runs nothing else, keeps the signal behaviour
/// it has always had.
fn cancel_on_interrupt(token: &CancellationToken) {
    let token = token.clone();
    tokio::spawn(async move {
        // A platform that cannot deliver the signal leaves the token
        // uncancelled, which is the same position as having no handler at all.
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        eprintln!("interrupted; stopping the attempt (interrupt again to exit immediately)");
        token.cancel();
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(EXIT_INTERRUPTED);
        }
    });
}

/// The capability `selection` names, built from `config`.
///
/// Boxed as a trait object because the two arms are different types and the
/// orchestration takes a `&dyn Capability` — which is the seam that made this
/// function possible to write at all.
///
/// Everything a repairing capability needs is resolved here, in this order:
/// the tables, then the credential, then the client. That order is what makes
/// "the credential is resolved lazily" true in a stronger sense than "only for
/// this arm": a deployment missing its `[agent]` table is told about the table
/// rather than about a variable it would also have had to set.
fn build_capability(
    selection: Selection,
    config: &config::Config,
    config_path: &Path,
    cancel: &CancellationToken,
) -> Result<Box<dyn Capability>, CliError> {
    let missing = |missing: &'static str| Unconfigured {
        capability: selection.id(),
        missing,
        path: config_path.display().to_string(),
    };

    match selection {
        Selection::Mark => Ok(Box::new(StubMark::new(
            &config.stub.root,
            &config.project.name,
        ))),
        Selection::Repair => {
            let agent = config.agent.as_ref().ok_or_else(|| missing("[agent]"))?;
            let workspace = config
                .workspace
                .as_ref()
                .ok_or_else(|| missing("[workspace]"))?;
            let fixture = workspace
                .fixture
                .as_ref()
                .ok_or_else(|| missing("workspace.fixture"))?;
            let check = workspace
                .check
                .as_ref()
                .ok_or_else(|| missing("workspace.check"))?;

            // Only now, and only on this arm. Everything above could have
            // failed for a deployment that never intended to talk to a model.
            let credential = resolve_credential(&agent.api_key.env)?;
            let model = fiddle_runtime::completion_model(
                &agent.base_url,
                credential,
                &agent.api_key.env,
                &agent.model,
            )
            .map_err(GatewayUnavailable)?;

            // A `^C` reaches the check only through the token, so the handler
            // goes in beside the capability that holds one.
            cancel_on_interrupt(cancel);

            // Two axes the schema admits exactly one value of each on. Matched
            // rather than ignored, so that adding a variant is a compile error
            // here — at the one place that would have to honour it — instead of
            // a document that loads and quietly means something else.
            let config::Isolation::GitWorktree = workspace.isolation;
            let config::Cleanup::Always = workspace.cleanup;

            Ok(Box::new(FixtureRepair::new(
                model,
                RepairConfig {
                    fixture: fixture.clone(),
                    workspace_root: workspace.root.clone(),
                    stub_root: config.stub.root.clone(),
                    project: config.project.name.clone(),
                    // **No attempt id is minted here**, and the seam no longer
                    // has a place to put one. It used to: this binary minted an
                    // id for `RepairConfig` while `fiddle_runtime::attempt`
                    // minted the bundle's separately, so `repair:<n>:<attempt>`
                    // named an attempt that appeared in no bundle and on no
                    // disk. Both ids were real and unique, and they did not name
                    // each other.
                    //
                    // The id now travels to the capability on its
                    // `ExecutionGrant` — the value that already means "this
                    // attempt authorises this execution" — so it is still minted
                    // exactly once, in `attempt`, where no caller can hand in a
                    // duplicate and collide two bundles on one path. Everything
                    // left in this struct is a deployment decision an operator
                    // configured; the attempt belongs to the run.
                    check: WorkspaceCommand {
                        program: check.program.clone(),
                        args: check.args.clone(),
                        timeout: workspace.command_timeout.as_duration(),
                    },
                    // Handed over as configured rather than pre-tightened
                    // against the command timeout: `fiddle_runtime::agent::attempt`
                    // takes the `min` of the two itself, so doing it here as
                    // well would be a second implementation of one rule.
                    budget: AgentBudget {
                        max_turns: agent.max_turns,
                        max_tokens: agent.max_tokens,
                        deadline: agent.deadline.as_duration(),
                        max_changed_files: agent.max_changed_files,
                        tool_timeout: agent.tool_timeout.as_duration(),
                    },
                    cancel: cancel.clone(),
                },
            )))
        }
    }
}

async fn dispatch(cli: &cli::Cli) -> Result<RunOutcome, CliError> {
    match &cli.command {
        cli::Command::Config { action } => match action {
            cli::ConfigCommand::Check { json } => {
                let config = config::load(&cli.config)?;
                if *json {
                    println!("{}", render::config_check_json(&config));
                } else {
                    println!("{}", render::config_check_human(&config));
                }
                Ok(RunOutcome::Completed)
            }
        },
        cli::Command::Inspect {
            invocation_ref,
            capability,
            json,
        } => {
            // Parsed through `fiddle-core` rather than re-implemented here: the
            // CLI's only job is to turn the rejection into a diagnostic and an
            // exit code.
            //
            // The reference is validated *before* the configuration is loaded,
            // so a caller who mistyped the argument is told about the argument
            // rather than about a document they never mentioned.
            let reference: InvocationRef =
                invocation_ref.parse().map_err(InvalidInvocationRef::from)?;
            // Resolved through the very same two lines `run` uses, and that is
            // the point rather than an economy: a second spelling of "absent
            // means `stub_mark`" is exactly how the two commands would drift
            // apart again.
            let selection = match capability {
                Some(requested) => Selection::parse(requested)?,
                None => Selection::Mark,
            };
            let config = config::load(&cli.config)?;
            let observed = observe(&config, &reference);
            // The CLI owns the configuration, so the CLI computes the marker
            // this invocation expects and hands it to the core. `assess` and
            // `derive_next` never reach for it themselves — that is what keeps
            // them pure functions of their arguments.
            let expected_marker =
                fiddle_core::correlation_key(&config.project.name, &reference.as_str());
            let assessment = fiddle_core::assess(&observed, &expected_marker);
            // The capability under consideration, named by the caller. Naming it
            // here rather than in the core is the point of `derive_next`'s
            // argument — the caller that knows which capability is under
            // consideration is the one that says so — and `inspect` is a caller
            // that now knows, instead of a caller that passed a constant.
            //
            // **Selected, and not built.** `run` follows its identical
            // `Selection` into `build_capability`; this line takes the id and
            // stops, which is what keeps `inspect` read-only, offline and
            // credential-free for every value of the flag.
            let next_action = fiddle_core::derive_next(&observed, &expected_marker, selection.id());
            if *json {
                println!(
                    "{}",
                    render::inspect_json(&reference, &observed, &assessment, &next_action)
                );
            } else {
                println!(
                    "{}",
                    render::inspect_human(&reference, &observed, &assessment, &next_action)
                );
            }
            Ok(RunOutcome::Completed)
        }

        cli::Command::Run {
            invocation_ref,
            mode,
            capability,
            json,
        } => {
            // Same order as `inspect`, and for the same reason: a caller who
            // mistyped an argument is told about the argument rather than about
            // a document they never mentioned. `--capability` is resolved here
            // too, before anything is observed and long before anything could
            // be executed, so a rejected invocation provably did nothing.
            let reference: InvocationRef =
                invocation_ref.parse().map_err(InvalidInvocationRef::from)?;
            // Absent means `stub_mark`, unchanged. The default did not move when
            // a second capability was registered, which is what keeps M0's
            // acceptance lane byte-identical without being modified.
            let selection = match capability {
                Some(requested) => Selection::parse(requested)?,
                None => Selection::Mark,
            };
            let config = config::load(&cli.config)?;

            let (work_items, changes) = ports(&config);
            // The token exists on both arms so the capability builder does not
            // have to hand one back conditionally; only a capability that can
            // be interrupted installs a handler for it.
            let cancel = CancellationToken::new();
            let selected = build_capability(selection, &config, &cli.config, &cancel)?;
            // One call, because executing and recording are one transaction: the
            // runtime owns the whole attempt, including which outcome a failure
            // to record amounts to. Nothing here re-decides that — a second
            // opinion formed out here is how the outcome on stdout and the state
            // on disk came to disagree in the first place.
            let record = fiddle_runtime::attempt(&AttemptContext {
                project: &config.project.name,
                reference: &reference,
                mode: *mode,
                build: build_identity(),
                report_dir: &config.report.dir,
                work_items: &work_items,
                changes: &changes,
                capability: selected.as_ref(),
            })
            .await;

            if let Some(failure) = &record.evidence_failure {
                eprintln!("{}", render::evidence_failure(&config.report.dir, failure));
            }
            if *json {
                println!(
                    "{}",
                    render::run_json(&record.bundle, record.published.as_deref())
                );
            } else {
                println!(
                    "{}",
                    render::run_human(&record.bundle, record.published.as_deref())
                );
            }
            // The payload is printed on every path, including the failing ones:
            // a caller learns *what* fiddle concluded from stdout and *that* it
            // failed from the exit code, rather than having to choose.
            Ok(record.bundle.outcome)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{ChangeSetState, NextAction, Observation, Published};
    use fiddle_runtime::ChangePort;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Every row of design §4.5's table, in one place.
    ///
    /// The rest of the exit-code coverage is black-box, in `fiddle-acceptance`,
    /// and it can only reach the rows this build can be driven into. `Suspended`
    /// is not one of them — M0 has no decision point, so nothing outside this
    /// test can pin row 10 at all, and a variant whose code is written down but
    /// never checked is exactly how a code drifts before the milestone that
    /// starts producing it.
    #[test]
    fn every_outcome_maps_to_the_row_the_table_documents() {
        let rows: [(RunOutcome, u8); 4] = [
            (RunOutcome::Completed, 0),
            (
                RunOutcome::Suspended {
                    reason: Published::of("awaiting a decision"),
                },
                10,
            ),
            (
                RunOutcome::Retryable {
                    reason: Published::of("try again"),
                },
                11,
            ),
            (
                RunOutcome::Failed {
                    error: Published::of("will not succeed"),
                },
                20,
            ),
        ];
        for (outcome, code) in rows {
            assert_eq!(
                exit_code_for(&Termination::Ran(outcome.clone())),
                code,
                "{outcome:?} must exit {code}"
            );
        }

        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::UnknownCapability(
                UnknownCapability {
                    requested: "nonsense".into(),
                    known: "stub_mark".into(),
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::InvocationRef(
                InvalidInvocationRef(InvocationRefError::EmptyValue)
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::CredentialAbsent(
                CredentialAbsent {
                    variable: "LITELLM_API_KEY".into()
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::Unconfigured(
                Unconfigured {
                    capability: fiddle_core::FIXTURE_REPAIR,
                    missing: "[agent]",
                    path: "fiddle.toml".to_string(),
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::Gateway(
                GatewayUnavailable(fiddle_runtime::GatewayError {
                    base_url: "https://gateway.invalid/v1".into(),
                    variable: "LITELLM_API_KEY".into(),
                })
            ))),
            EXIT_INVALID_INPUT
        );
    }

    /// **Every id this build advertises can actually be selected.**
    ///
    /// [`CAPABILITIES`] is what the `--capability` diagnostic lists, so it is a
    /// promise to a caller about what they may ask for. [`Selection::parse`]
    /// matches ids one at a time, which means a capability registered in the
    /// runtime and not wired here would be advertised, requested, and then
    /// rejected as unknown. This is the assertion that turns that into a build
    /// failure — and it is the same class of mistake as the one this task
    /// exists to fix, where the flag was checked against this very list and
    /// then discarded.
    #[test]
    fn every_registered_capability_can_be_selected() {
        for registered in CAPABILITIES {
            let selection = Selection::parse(registered.0).unwrap_or_else(|error| {
                panic!(
                    "`{registered}` is advertised by CAPABILITIES and cannot be \
                     selected: {error}"
                )
            });
            assert_eq!(
                selection.id(),
                registered,
                "a selection must run the capability it was asked for"
            );
        }
    }

    /// The flag rejects what this build cannot run, and says what it can.
    #[test]
    fn an_unknown_capability_is_refused_with_the_known_list() {
        let error = Selection::parse("nope").unwrap_err();
        assert!(error.known.contains("stub_mark"), "{}", error.known);
        assert!(error.known.contains("fixture_repair"), "{}", error.known);
    }

    /// The credential is read from the variable the document names, and its
    /// absence is reported as that variable rather than as a generic failure.
    ///
    /// Deliberately not asserting on a *present* variable: this process's
    /// environment is shared by every test in the binary, and a test that set
    /// one would be reaching into the others.
    #[test]
    fn an_absent_credential_is_reported_under_the_name_that_was_asked_for() {
        let error = resolve_credential("FIDDLE_A_VARIABLE_NOTHING_EXPORTS").unwrap_err();
        assert_eq!(error.variable, "FIDDLE_A_VARIABLE_NOTHING_EXPORTS");
        let rendered = render::diagnostic(&error);
        assert!(
            rendered.contains("FIDDLE_A_VARIABLE_NOTHING_EXPORTS"),
            "an operator must learn which variable to export: {rendered}"
        );
    }

    /// The deterministic capability is built from the configuration alone, with
    /// no environment consulted, which is the property M0's credential-free
    /// acceptance lane rests on.
    #[test]
    fn the_deterministic_capability_is_built_without_a_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        // A document that names a credential nothing exports: if selecting
        // `stub_mark` resolved one, this would fail.
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [agent]\nmodel=\"m\"\nbase_url=\"http://127.0.0.1:9/v1\"\n\
             api_key={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();

        let Ok(built) =
            build_capability(Selection::Mark, &loaded, &path, &CancellationToken::new())
        else {
            panic!("the deterministic capability needs nothing but the document")
        };
        assert_eq!(built.id(), fiddle_core::STUB_MARK);
    }

    /// A repairing capability over a document that describes no deployment is
    /// refused by table, before the credential is ever reached.
    ///
    /// The order matters: an operator whose document has no `[agent]` table has
    /// a table to write, and telling them about a variable they would *also*
    /// need would be answering the second question first.
    #[test]
    fn a_repair_over_an_m0_document_names_the_missing_table_not_the_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();

        let Err(error) =
            build_capability(Selection::Repair, &loaded, &path, &CancellationToken::new())
        else {
            panic!("a repair needs a model and somewhere to work")
        };
        match error {
            CliError::Unconfigured(unconfigured) => {
                assert_eq!(unconfigured.missing, "[agent]");
                assert_eq!(unconfigured.capability, fiddle_core::FIXTURE_REPAIR);
            }
            other => panic!("expected a missing-table refusal, got {other:?}"),
        }
    }

    /// **A race must not leave a code the table does not have.**
    ///
    /// The disagreement is real rather than described: the capability writes its
    /// marker, another writer takes the change set over before the attempt looks
    /// again, and the attempt's own conclusion — not one composed here — is what
    /// gets mapped. That is what makes this a check on the whole chain, from the
    /// runtime's re-derivation to the number the process leaves behind.
    ///
    /// Before this was derived rather than asserted, this same world exited 0
    /// with a bundle reading `"outcome":"completed"` beside
    /// `"next_action":{"blocked":…}` in release, and aborted with 101 — a row of
    /// no table — in debug. Both halves are pinned shut here: one code, from the
    /// table, in either profile.
    #[tokio::test]
    async fn a_race_after_executing_still_exits_on_the_table() {
        let root = tempfile::tempdir().unwrap();
        let stub_root = root.path().join("stub-state");
        std::fs::create_dir_all(stub_root.join("work")).unwrap();
        std::fs::create_dir_all(stub_root.join("changes")).unwrap();
        std::fs::write(
            stub_root.join("work/fiddle-m0-demo.json"),
            r#"{"id":"fiddle-m0-demo","status":"open"}"#,
        )
        .unwrap();

        let reference: InvocationRef = "beans:fiddle-m0-demo".parse().unwrap();
        let work_items = StubWorkItemPort::new(&stub_root);
        let changes = OvertakenAfterTheFirstLook {
            inner: StubChangePort::new(&stub_root),
            change_set: stub_root.join("changes/fiddle-m0-demo.json"),
            looks: AtomicUsize::new(0),
        };
        let marking = StubMark::new(&stub_root, "icecube");
        let record = fiddle_runtime::attempt(&AttemptContext {
            project: "icecube",
            reference: &reference,
            mode: fiddle_core::Mode::Unattended,
            build: build_identity(),
            report_dir: &root.path().join("reports"),
            work_items: &work_items,
            changes: &changes,
            capability: &marking as &dyn Capability,
        })
        .await;

        assert!(
            matches!(record.bundle.next_action, NextAction::Blocked { .. }),
            "the race must have produced a blocked re-derivation, got {:?}",
            record.bundle.next_action
        );
        assert!(
            matches!(record.bundle.outcome, RunOutcome::Failed { .. }),
            "a blocked re-derivation is not a completed run, got {:?}",
            record.bundle.outcome
        );
        assert_eq!(
            exit_code_for(&Termination::Ran(record.bundle.outcome)),
            20,
            "the same row an unobservable world exits on, because the world it \
             leaves behind is the same one a later invocation will block on"
        );
    }

    /// Another agent rewriting the change set between an attempt's two
    /// observations.
    ///
    /// Counted rather than raced, so the window is hit on every run: the foreign
    /// write lands immediately before the second look, which is the moment a
    /// concurrent writer would have to land it. The observation itself still
    /// comes from the real stub port reading the real file.
    struct OvertakenAfterTheFirstLook {
        inner: StubChangePort,
        change_set: PathBuf,
        looks: AtomicUsize,
    }

    impl ChangePort for OvertakenAfterTheFirstLook {
        fn observe(&self, work_id: &str) -> Observation<ChangeSetState> {
            if self.looks.fetch_add(1, Ordering::Relaxed) == 1 {
                std::fs::write(&self.change_set, r#"{"marker":"0123456789abcdef"}"#).unwrap();
            }
            self.inner.observe(work_id)
        }
    }
}
