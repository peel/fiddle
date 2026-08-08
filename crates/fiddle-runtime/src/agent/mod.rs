//! The bounded rig: everything a model can see, say, or do.
//!
//! [`tools`] is the whole of that surface. Four tools, each one a typed Rig
//! [`Tool`](rig_agent::tool::Tool), and nothing else reaches the outside world
//! on the model's behalf.
//!
//! The property the module exists to hold is a separation of two channels that
//! look alike from inside a tool and are not alike at all. A tool's *arguments*
//! are authored by the model; a tool's *context* is authored by the host. Which
//! workspace is being repaired, whether the attempt is still live, and which
//! program the check runs are all host facts, so they travel through
//! [`ToolHost`](tools::ToolHost) in Rig's
//! [`ToolContext`] — never as a field of an
//! `Args` struct, and never in an advertised JSON schema. A schema is a menu:
//! anything named on it is something the model may fill in, and a workspace root
//! the model may fill in is not a workspace root at all.
//!
//! [`audit`] is the other half of the same idea, applied to what is written
//! down rather than to what is granted. The tools record themselves; the Rig
//! hook in [`audit`] only watches. Which of the two an operator ends up
//! trusting is decided here, not later.
//!
//! [`attempt`] assembles the three into one bounded run and is the only thing
//! outside this module anyone needs to call.

pub mod audit;
pub mod tools;

pub use audit::AuditHook;
pub use tools::{
    CheckOutcome, ListFiles, NoArgs, ReadFile, ReadFileArgs, RunCheck, ToolError, ToolHost,
    WriteFile, WriteFileArgs, WriteReceipt,
};

use rig_agent::completion::{PromptError, StructuredOutputError, TypedPrompt};
use rig_agent::tool::ToolContext;
use rig_agent::AgentBuilder;
use std::future::IntoFuture;
use std::time::Duration;

/// What the model is told about the situation it is in.
///
/// It names the tools rather than describing the host, for the same reason the
/// schemas do: everything here is sent to the provider, so a preamble that
/// mentioned the workspace root would leak it just as surely as a tool argument
/// would.
const PREAMBLE: &str = "\
You are repairing one small Rust project. You can read its files, list them, \
replace a file's contents, and run the project's check. You cannot do anything \
else, and there is nothing outside the project you can reach.\n\
\n\
Work in small steps: read before you write, and run the check after you write. \
Change as few files as you can. When you are done — or when you are certain you \
cannot finish — reply with the structured report and nothing else. Report what \
you actually changed, whether or not it worked.";

/// The instruction that opens the run.
const TASK: &str = "Repair this project so that its check passes, then report what you did.";

/// The bounds one attempt runs inside, all of them the host's to choose.
///
/// Five independent bounds rather than one composite, because they fail for
/// different reasons and a capability reacts differently to each: a run that
/// exhausted its turns was probably looping, one that outran the wall clock was
/// probably waiting on something, and one that touched too many files did
/// something nobody asked for. Collapsing them would throw that away.
///
/// [`AgentBudget::tool_timeout`] is a ceiling on one tool call, not on the
/// attempt. It only ever *tightens* the bound the host already put on its check
/// command — see [`attempt`] — so that neither the budget nor the command can
/// be used to loosen the other.
#[derive(Clone, Debug)]
pub struct AgentBudget {
    /// Total model calls, including the first.
    pub max_turns: usize,
    /// Per-completion token ceiling handed to the provider.
    pub max_tokens: u64,
    /// Wall-clock ceiling on the whole attempt.
    pub deadline: Duration,
    /// How many files git may report as changed before the attempt is refused.
    pub max_changed_files: usize,
    /// Ceiling on any single tool call that runs a program.
    pub tool_timeout: Duration,
}

/// What the model says it did.
///
/// Every field is a claim. `changed_files` is the model's own list and is kept
/// because a disagreement with git is itself a finding, but nothing in this
/// module reads it — the cap in [`attempt`] is checked against
/// [`Workspace::changed_files`](crate::workspace::Workspace::changed_files),
/// which the model does not author.
///
/// `claimed_complete` is evidence and only evidence. It is recorded, published,
/// and never branched on: a model that says it finished has said something
/// about itself, not about the project, and the check is what settles that.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct RepairReport {
    pub changed_files: Vec<String>,
    pub summary: String,
    pub claimed_complete: bool,
}

/// Why an attempt did not produce a report.
///
/// The four variants exist so that a capability can answer one question without
/// parsing a string: *whose fault was that?* A [`Bounded`](AgentError::Bounded)
/// attempt hit a limit we set and retrying under the same limit will do the
/// same thing. A [`Protocol`](AgentError::Protocol) failure is the model not
/// holding up its end, which a different model or a clearer prompt might fix. A
/// [`Provider`](AgentError::Provider) failure is the gateway, which is worth
/// retrying and is nobody's judgement of the model.
///
/// These are operator-facing and are never returned to the model, so unlike
/// [`ToolError`] they may carry diagnostics freely — the direction that matters
/// for a `ToolError` is towards the provider, and nothing here goes that way.
///
/// `reason` never *quotes back* something the model chose to name: not a tool
/// name, not a path. Those are unbounded, they are already in the receipts under
/// their own outcome class, and repeating them buys nothing. The one exception
/// is deliberate and is the serde diagnostic on a schema failure, which is the
/// only thing that distinguishes "the model omitted `summary`" from "the model
/// wrote prose" — and which may quote the offending fragment. Anything that
/// later copies an `AgentError` into a *published* surface has to treat that one
/// reason as model-authored text.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// A bound the host set was reached.
    #[error("the attempt was stopped by a bound: {reason}")]
    Bounded { reason: String },

    /// The attempt was cancelled from outside.
    #[error("the attempt was cancelled")]
    Cancelled,

    /// The model did not produce what it was asked to produce.
    #[error("the model did not hold up its end: {reason}")]
    Protocol { reason: String },

    /// Something between us and the model failed.
    #[error("the provider did not hold up its end: {reason}")]
    Provider { reason: String },
}

/// Run one bounded attempt and return the model's report.
///
/// Generic over Rig's own [`CompletionModel`](rig_core::completion::CompletionModel)
/// rather than over a trait of ours. A wrapper would buy a seam nobody needs and
/// cost the thing that matters here: with the trait exposed, a test substitutes
/// `MockCompletionModel` and drives the *real* tools over a *real* worktree
/// without a credential, a socket, or a second implementation of the tool loop
/// to keep in step with the first.
///
/// # The three bounds, and why they are raced
///
/// Cancellation and the deadline are `select!` arms rather than something the
/// caller applies by dropping the returned future. Dropping a future stops
/// *polling* it; it does not reliably stop what it started. A `select!` here
/// still only stops the waiting — but it stops it at a point where the effects
/// underneath have owners that clean up: losing the arm drops the run future,
/// which drops the tool call inside it, which drops the child process handle,
/// and [`Workspace::run`](crate::workspace::Workspace) sets `kill_on_drop`, so a
/// check that was still running is killed rather than orphaned. The one thing
/// nothing here can stop is a completion already in flight at the gateway: the
/// connection is dropped and the tokens are still spent. That is why
/// cancellation is *also* a token the tools check for themselves, rather than
/// only a race — a tool that has not started must not start.
///
/// The arms are `biased` and cancellation is first, which decides the one case
/// that is otherwise a coin toss: a run that completed in the same poll as its
/// cancellation. An attempt whose token was cancelled must not be reported as a
/// success, so the tie is broken deliberately rather than by `select!`'s
/// randomness.
///
/// The changed-file cap is not raced, because it cannot be: what an attempt
/// changed is only knowable once it has stopped changing things. It is checked
/// against git afterwards, and against git rather than against
/// [`RepairReport::changed_files`], which is a claim.
pub async fn attempt<M>(
    model: M,
    host: ToolHost,
    budget: AgentBudget,
) -> Result<RepairReport, AgentError>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    let agent = AgentBuilder::new(model)
        .preamble(PREAMBLE)
        .max_tokens(budget.max_tokens)
        // The agent-wide default, so a caller who forgets the per-request bound
        // still gets one. The per-request `max_turns` below is what this
        // function actually relies on; they are different settings and the
        // duplication is deliberate.
        .default_max_turns(budget.max_turns)
        // Redundant with `prompt_typed`, which derives the schema from `T` and
        // pins native structured output regardless. Stated anyway so the
        // agent's contract does not depend on which prompting method is used.
        .output_schema::<RepairReport>()
        .tool(ReadFile)
        .tool(WriteFile)
        .tool(ListFiles)
        .tool(RunCheck)
        .add_hook(AuditHook::for_host(&host))
        .build();

    // The tools see a host whose check cannot outlive the budget. `min` rather
    // than assignment: a bound may tighten another bound and may never loosen
    // one, so a host that asked for ten seconds does not get sixty because the
    // budget said so.
    let mut bounded = host.clone();
    bounded.check.timeout = bounded.check.timeout.min(budget.tool_timeout);
    let mut ctx = ToolContext::new();
    ctx.insert(bounded);

    let run = agent
        .prompt_typed::<RepairReport>(TASK)
        .tool_context(ctx)
        .max_turns(budget.max_turns)
        .into_future();

    let report = tokio::select! {
        biased;
        _ = host.cancel.cancelled() => return Err(AgentError::Cancelled),
        _ = tokio::time::sleep(budget.deadline) => return Err(AgentError::Bounded {
            reason: format!("the deadline of {:?} elapsed", budget.deadline),
        }),
        result = run => result.map_err(classify)?,
    };

    let changed = host
        .workspace
        .changed_files()
        // Not being able to ask git what changed is a failure of ours, not of
        // the model's; `Provider` is the arm for "something we depend on
        // broke".
        .map_err(|source| AgentError::Provider {
            reason: format!("the changed-file set could not be derived: {source}"),
        })?;
    if changed.len() > budget.max_changed_files {
        return Err(AgentError::Bounded {
            reason: format!(
                "{} files changed, and the cap is {}",
                changed.len(),
                budget.max_changed_files
            ),
        });
    }
    Ok(report)
}

/// Sort a Rig failure into whose fault it was.
///
/// Matched on Rig's own typed variants rather than on the text of its messages.
/// Both error enums are `#[non_exhaustive]`, so the wildcard arms are required
/// by the compiler rather than chosen — and they fall to `Provider`, which is
/// the safe default: a variant nobody has seen yet is more likely to be a new
/// transport failure than a new way for a model to misbehave, and mislabelling
/// a gateway fault as `Protocol` would tell a capability to stop retrying
/// something that would have worked.
///
/// [`PromptError::UnknownToolCall`] is `Protocol` and not `Bounded`: the tool
/// set is a bound, but naming a tool outside it is the model saying something
/// false rather than the model reaching a limit. Its `tool_name` is deliberately
/// not quoted into the reason — it is a string the model authored, and
/// [`AuditHook`] has already recorded the call under `unknown_tool`.
fn classify(error: StructuredOutputError) -> AgentError {
    match error {
        StructuredOutputError::DeserializationError(source) => AgentError::Protocol {
            reason: format!("the report did not match the schema: {source}"),
        },
        StructuredOutputError::EmptyResponse => AgentError::Protocol {
            reason: "the model returned no final content at all".to_string(),
        },
        StructuredOutputError::PromptError(prompt) => match *prompt {
            PromptError::MaxTurnsError { max_turns, .. } => AgentError::Bounded {
                reason: format!("the turn budget of {max_turns} was exhausted"),
            },
            PromptError::PromptCancelled { .. } => AgentError::Cancelled,
            PromptError::UnknownToolCall { .. } => AgentError::Protocol {
                reason: "the model called a tool that does not exist".to_string(),
            },
            other => AgentError::Provider {
                reason: other.to_string(),
            },
        },
        other => AgentError::Provider {
            reason: other.to_string(),
        },
    }
}

/// What the runtime observed for itself over one attempt.
///
/// Recorded by the tools rather than by a Rig hook, so that evidence never
/// depends on hook behaviour: Rig's own documentation calls hooks controls
/// rather than authorization, and a control that stops firing must not be able
/// to silently empty the record of what happened.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ToolReceipts {
    pub calls: Vec<ToolReceipt>,
}

/// One tool call, as the runtime saw it.
///
/// Three fields, and the shortness is a decision rather than a placeholder.
/// Receipts are published in the evidence bundle, so every field has to be safe
/// to publish without anybody re-reading it first — which rules out the
/// requested path (model-authored, and unbounded), the file contents, and the
/// resolved path (the operator's filesystem layout). What is left answers the
/// questions a bundle is actually asked: which tools ran, how each went, and
/// where the time went.
///
/// `outcome` is one of six classes, and which writer produced it is part of
/// reading it correctly:
///
/// | outcome | written by | means |
/// |---|---|---|
/// | `ok` | the tool body | it did the thing |
/// | `refused` | the tool body | we declined, before the filesystem was touched |
/// | `cancelled` | the tool body | the attempt was stopped from outside |
/// | `failed` | the tool body | we acted and the world did not cooperate |
/// | `malformed` | [`AuditHook`] | the model's arguments did not decode, so no body ran |
/// | `unknown_tool` | [`AuditHook`] | the model named a tool that does not exist |
///
/// The first four are the record proper and do not depend on a hook being
/// installed. The last two describe calls that never reach a tool body at all,
/// which is why nothing but a hook could ever have seen them; see [`audit`] for
/// why that does not make the evidence hook-contingent.
///
/// `duration_ms` is zero for the two hook-written classes, honestly: there was
/// no body to time.
///
/// A `&'static str` rather than an enum because the set is closed at the points
/// that write it and the only consumers are a serializer and a human reading
/// JSON.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ToolReceipt {
    pub tool: String,
    pub outcome: &'static str,
    pub duration_ms: u64,
}
