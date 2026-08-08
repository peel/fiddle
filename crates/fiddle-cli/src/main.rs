mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use fiddle_core::{
    CapabilityId, FiddleBuild, InvocationRef, InvocationRefError, RunOutcome, WorkStateView,
};
use fiddle_runtime::{
    AttemptContext, Capability, StubChangePort, StubMark, StubWorkItemPort, CAPABILITIES,
};
use std::process::ExitCode;

/// Usage error or invalid input — row `2` of the exit-code table. Clap already
/// exits with this code for usage errors, so the constant exists to keep every
/// half of the row visibly the same number.
const EXIT_INVALID_INPUT: u8 = 2;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let termination = match dispatch(&cli) {
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

/// The capability `--capability` names, or a rejection listing what exists.
///
/// The known-id list comes from [`CAPABILITIES`], so the diagnostic cannot fall
/// out of step with what the binary can actually run.
fn resolve_capability(requested: &str) -> Result<CapabilityId, UnknownCapability> {
    CAPABILITIES
        .into_iter()
        .find(|candidate| candidate.0 == requested)
        .ok_or_else(|| UnknownCapability {
            requested: requested.to_string(),
            known: CAPABILITIES
                .iter()
                .map(|capability| capability.0)
                .collect::<Vec<_>>()
                .join(", "),
        })
}

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
            | CliError::UnknownCapability(_),
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

fn dispatch(cli: &cli::Cli) -> Result<RunOutcome, CliError> {
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
            let config = config::load(&cli.config)?;
            let observed = observe(&config, &reference);
            // The CLI owns the configuration, so the CLI computes the marker
            // this invocation expects and hands it to the core. `assess` and
            // `derive_next` never reach for it themselves — that is what keeps
            // them pure functions of their arguments.
            let expected_marker =
                fiddle_core::correlation_key(&config.project.name, &reference.as_str());
            let assessment = fiddle_core::assess(&observed, &expected_marker);
            // `inspect` takes no `--capability`, so it reports what the M0 plan
            // would run: `stub_mark`. Naming it here rather than in the core is
            // the point of the argument — the caller that knows which
            // capability is under consideration is the one that says so, and a
            // later selection reaches this line rather than the derivation.
            let next_action =
                fiddle_core::derive_next(&observed, &expected_marker, fiddle_core::STUB_MARK);
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
            // a document they never mentioned. `--capability` is validated here
            // too, before anything is observed and long before anything could
            // be executed, so a rejected invocation provably did nothing.
            let reference: InvocationRef =
                invocation_ref.parse().map_err(InvalidInvocationRef::from)?;
            if let Some(requested) = capability {
                // M0 knows exactly one capability, so a valid selection can only
                // ever name the capability the derivation would choose anyway.
                // The flag's job here is to reject an unknown id loudly rather
                // than to narrow a plan that cannot be narrowed.
                resolve_capability(requested)?;
            }
            let config = config::load(&cli.config)?;

            let (work_items, changes) = ports(&config);
            let marking = StubMark::new(&config.stub.root, &config.project.name);
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
                capability: &marking as &dyn Capability,
            });

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
    use fiddle_core::{ChangeSetState, NextAction, Observation};
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
                    reason: "awaiting a decision".into(),
                },
                10,
            ),
            (
                RunOutcome::Retryable {
                    reason: "try again".into(),
                },
                11,
            ),
            (
                RunOutcome::Failed {
                    error: "will not succeed".into(),
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
    #[test]
    fn a_race_after_executing_still_exits_on_the_table() {
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
        });

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
