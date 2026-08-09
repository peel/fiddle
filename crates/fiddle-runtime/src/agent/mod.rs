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

use rig_agent::agent::OutputMode;
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
/// # What a `reason` is allowed to contain
///
/// `reason` never *quotes back* something the model chose to name: not a tool
/// name, not a path. Those are unbounded, they are already in the receipts under
/// their own outcome class, and repeating them buys nothing.
///
/// There are **two** deliberate admissions of text this process did not write,
/// and they are admitted on different grounds:
///
/// - The **serde diagnostic** on a schema failure, which may quote the offending
///   fragment. It is the only thing that distinguishes "the model omitted
///   `summary`" from "the model wrote prose", and it is *model*-authored — the
///   model is a party we are already reading the output of.
/// - A **provider transport failure's own text**, on [`AgentError::Provider`],
///   and only when rig can prove there is no response body inside it. See
///   [`classify`] and [`provider_fault`] for how that is decided and why the
///   body itself is never admitted on any path.
///
/// This doc block used to say the serde diagnostic was "the one exception". It
/// was not, and the second one was the wider of the two: until `provider_fault`
/// existed, `classify`'s two wildcard arms rendered rig's error with
/// `to_string()`, and rig preserves a non-2xx **response body verbatim** in the
/// error it renders. A gateway that quotes the key it rejected — which is what
/// an OpenAI-compatible `invalid_api_key` envelope does — therefore put a
/// credential into a published bundle by the ordinary route.
///
/// Anything that later copies an `AgentError` into a *published* surface still
/// has to treat these reasons as foreign text and bound them; that is
/// [`fiddle_core::Published`]'s job and it is not optional, because
/// [`fiddle_core::RunOutcome`]'s reasons cannot be spelled any other way.
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
///
/// # Why `output_mode` is stated, and why the default was wrong here
///
/// **Without [`OutputMode::Tool`] this function calls no tools at all.** Not
/// rarely, not for weak models — never, for every model on the gateway. It was
/// found by the first Tier 1 smoke run and reproduced directly against the
/// endpoint: the same request carrying `tools` alone comes back with
/// `finish_reason: tool_calls` and a call; add `response_format: {type:
/// json_schema}` and it comes back `finish_reason: stop`, `tool_calls: null`,
/// with the report filled in from nothing.
///
/// Rig knows about this — it is issue #1928, and [`OutputMode`] is the remedy —
/// but its default, [`OutputMode::Auto`], resolves per *provider*, and it
/// resolves wrongly for us through no fault of its own. `Auto` keeps native
/// structured output whenever the provider reports
/// `composes_native_output_with_tools()`, and
/// [`crate::gateway::GatewayModel`] is `openai::completion::CompletionModel`,
/// which reports `true` — a true statement about OpenAI's own endpoint. Ours is
/// an OpenAI-*compatible* endpoint fronting Anthropic, and the composition does
/// not survive that translation. The provider type and the upstream disagree by
/// construction, so the mode is named here rather than inferred.
///
/// [`OutputMode::Tool`] registers the schema as a synthetic tool the model calls
/// to finalise, and sends no native constraint, so the four real tools stay
/// callable. It costs nothing in turns — the finalising call *is* a turn of the
/// same loop, so [`AgentBudget::max_turns`] still bounds the whole attempt and
/// there is no second request to account for. What it costs is strictness: Tool
/// mode is best-effort where Native was guaranteed, so a model may return a
/// report that does not match the schema. That is why the schema is still
/// validated afterwards and why [`classify`] maps a deserialisation failure to
/// [`AgentError::Protocol`] — under this mode, a malformed report genuinely is
/// the model failing to hold up its end, and saying so is more honest than a
/// guarantee bought by never letting it use a tool.
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
        // Stated explicitly rather than left to `prompt_typed`, which derives
        // the same schema from `T`, so the agent's contract does not depend on
        // which prompting method is used.
        .output_schema::<RepairReport>()
        // **The line that makes the tool loop happen at all.** See the section
        // in this function's documentation.
        .output_mode(OutputMode::Tool)
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
///
/// The two wildcard arms fall to [`provider_fault`] rather than to
/// `other.to_string()`, which is where the gateway's response body used to
/// enter this process's published output. See that function.
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
                reason: provider_fault(
                    other.provider_response_status(),
                    other.provider_response_body(),
                    &other,
                ),
            },
        },
        other => AgentError::Provider {
            reason: provider_fault(
                other.provider_response_status(),
                other.provider_response_body(),
                &other,
            ),
        },
    }
}

/// What may be said about a failure that came from the far end of the wire.
///
/// # The rule
///
/// **A provider's response body is never quoted, on any path.** What is said
/// instead is the HTTP status, which fiddle reads off the status line rather
/// than out of the payload — a number from a closed set, authored by the
/// protocol and not by whoever is answering on the socket.
///
/// # Why the body cannot simply be sanitised on the way out
///
/// Because it is chosen by something outside this process, and no filter over
/// content an adversary picks is a guarantee. The concrete case is not
/// hypothetical: an OpenAI-compatible gateway refusing a call answers `401` with
/// `{"error":{"message":"Incorrect API key provided: sk-…"}}` — the credential
/// fiddle just sent it, handed straight back. rig preserves that body verbatim
/// (`HttpError::InvalidStatusCodeWithMessage`, or `ProviderResponse` for a 2xx
/// error envelope) and renders it in `Display`, so every `to_string()` of such
/// an error carries it. Design §3 lists "no secret in evidence or telemetry"
/// among the invariants, and the only version of that which holds against an
/// arbitrary body is that the body does not come in.
///
/// # Why the rest of the error text still does
///
/// Because losing it would cost the diagnostic that matters most in practice and
/// buy nothing. `provider_response_body()` is rig's own answer to *does this
/// error carry a payload the provider wrote?*, and it is `Some` for exactly the
/// two variants whose rendering contains one. When it is `None` the text is
/// rig's and the transport's — "connection refused", a DNS failure, a TLS
/// error — which is what an operator with a mistyped `base_url` needs to read,
/// and which no gateway authored.
///
/// A status of `None` beside a body of `Some` is a non-HTTP transport, which
/// this deployment does not use; it is answered rather than assumed away,
/// because the safe answer costs one arm.
///
/// Generic over the status rather than naming `http::StatusCode`, so that
/// pinning rig's HTTP crate into this crate's own manifest is not the price of
/// keeping a body out of a bundle.
fn provider_fault(
    status: Option<impl std::fmt::Display>,
    body: Option<&str>,
    error: &dyn std::fmt::Display,
) -> String {
    match (status, body) {
        (Some(status), _) => format!("the gateway answered {status}"),
        (None, Some(_)) => "the gateway answered with an error payload and no status".to_string(),
        (None, None) => error.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rig_agent::completion::CompletionError;

    /// A body an unaudited gateway might send back, quoting the key it refused.
    const ECHOED: &str =
        r#"{"error":{"message":"Incorrect API key provided: sk-unit-must-not-appear-4c2f"}}"#;

    /// **A preserved provider body is never rendered into a reason.**
    ///
    /// The arm asserted here is the one the acceptance lane cannot reach: a
    /// provider error carrying a body and *no* HTTP status, which is what rig
    /// produces for a non-HTTP transport. The status-bearing arm is proven end
    /// to end by `binary_repair`'s
    /// `a_gateway_refusal_never_reaches_what_the_run_publishes`, over a real
    /// socket and the real client.
    ///
    /// Constructed through rig's own `from_provider_body`, so this stays a test
    /// about rig's error shape rather than about a shape we invented — if a
    /// later version routes the body somewhere `provider_response_body` does
    /// not report, this fails.
    #[test]
    fn a_preserved_provider_body_is_never_rendered_into_a_reason() {
        let error = StructuredOutputError::PromptError(Box::new(PromptError::CompletionError(
            CompletionError::from_provider_body(ECHOED),
        )));
        // The premise: rig really is carrying the body, and really does render
        // it. Without this the assertion below could pass over an error that
        // never held the string at all.
        assert!(
            error.to_string().contains("sk-unit-must-not-appear-4c2f"),
            "rig no longer renders a preserved body, so this test is not \
             testing anything: {error}"
        );

        match classify(error) {
            AgentError::Provider { reason } => {
                assert!(
                    !reason.contains("sk-unit-must-not-appear-4c2f"),
                    "the response body reached the reason: {reason}"
                );
                assert!(
                    reason.contains("gateway"),
                    "an operator must still learn who failed: {reason}"
                );
            }
            other => panic!("a provider failure must classify as Provider, got {other:?}"),
        }
    }

    /// The other direction, and the reason the rule is stated over
    /// `provider_response_body` rather than as "never quote rig": a transport
    /// failure carries no provider payload, so its text is rig's own and is
    /// exactly what an operator with a mistyped `base_url` needs to read.
    #[test]
    fn a_transport_failure_keeps_the_text_that_explains_it() {
        let error = StructuredOutputError::PromptError(Box::new(PromptError::CompletionError(
            CompletionError::ProviderError("connection refused".to_string()),
        )));

        match classify(error) {
            AgentError::Provider { reason } => assert!(
                reason.contains("connection refused"),
                "a failure with no provider body has nothing to withhold: {reason}"
            ),
            other => panic!("a provider failure must classify as Provider, got {other:?}"),
        }
    }
}
