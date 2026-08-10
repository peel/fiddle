//! Telemetry over an agent run — deliberately not the record of it.
//!
//! Rig describes its hooks as controls for *steering* a run, and a control is
//! not an authorization boundary: `on_tool_call` runs before the tool body, in
//! a stack whose earlier members may already have stopped the run, and a hook
//! that is never registered simply does not fire. Anything derived only from a
//! hook is therefore contingent on the hook, which is a fine property for a
//! progress indicator and a disqualifying one for evidence.
//!
//! So the split is deliberate and it is the whole content of this module.
//! [`ToolReceipts`](super::ToolReceipts) is written by the tool bodies in
//! [`tools`](super::tools), on both the path where the tool acted and the path
//! where it refused; that is the record. [`AuditHook`] watches the same calls go
//! past, names them, and always answers [`ToolCallAction::Run`]; that is the
//! telemetry. If the hook were removed from the builder tomorrow the receipts
//! would be byte-for-byte what they are today, and a test asserts exactly that.
//!
//! The hook is not the place to enforce anything, and it is written so that it
//! could not be mistaken for one. It has no reason to refuse, because every rule
//! worth enforcing — which tree may be touched, whether the attempt is still
//! live, which program the check runs — is already enforced inside the tool
//! where it cannot be bypassed by a hook that did not run.
//!
//! # The two calls a tool body cannot record
//!
//! Self-recording covers exactly the calls that *reach* a tool body. Two kinds
//! never do, and for those the hook is not a redundant second observer — it is
//! the only observer there is. Both were found by driving a real agent run and
//! reading which hooks fired, because the two look identical from the outside
//! and are nothing alike underneath:
//!
//! - **The model malformed the arguments of a real tool.** Rig decodes
//!   `Tool::Args` *before* it enters `Tool::call` (`tool/mod.rs:462`), so the
//!   body is never entered. `on_tool_call` fires, then `on_tool_result` fires
//!   carrying a framework-built `invalid_args` error, **and the run continues
//!   normally** — the model is simply told its arguments were bad. This is the
//!   dangerous one, because it is silent: without a receipt the bundle says the
//!   runtime did nothing, when the model may have flailed for twenty turns.
//! - **The model named a tool that does not exist.** Rig builds an
//!   [`InvalidToolCallContext`] only for a name outside the allowed set
//!   (`agent/run/mod.rs:597`), fires `on_invalid_tool_call`, and — with every
//!   hook returning `None` — fails the whole run with `UnknownToolCall`. Loud
//!   rather than silent, but still absent from the record.
//!
//! `on_invalid_tool_call` does **not** fire for the first case, and
//! `on_tool_call` does not fire for the second. They are separate seams and
//! they get separate outcome classes: `malformed` and `unknown_tool`.
//!
//! Recording these here does not make the evidence hook-dependent, because
//! there is no version of this fact a tool body could ever have written down.
//! Every call that *does* reach a body is still recorded by the body, and
//! `a_run_with_no_hook_at_all_produces_the_same_evidence` still holds.
//!
//! The discipline that keeps the two writers from colliding is
//! [`AuditHook::rejected_before_any_body_ran`]: `on_tool_result` fires for
//! *every* call, including the ones the body already recorded, so it records
//! only when the failure did not come from a tool body at all.

use super::{ToolError, ToolHost, ToolReceipt, ToolReceipts};
use rig_agent::agent::hook::{
    AgentHook, HookContext, InvalidToolCallAction, InvalidToolCallContext, ToolCall,
    ToolCallAction, ToolResultAction, ToolResultEvent,
};
use std::sync::{Arc, Mutex};

/// Names the tools a run asked for, lets every one of them through, and writes
/// down the two kinds of call that never reach a tool body.
///
/// Cheap to clone, and clones share both the observation log and the receipts
/// sink: the builder takes the hook by value, so the caller keeps a clone if it
/// wants to read what was seen.
///
/// Only the tool *name* is ever kept. Both events also carry `args`, which is
/// the model's own JSON and may be a whole file's contents; recording it would
/// make the telemetry unboundedly large and would put model-authored strings
/// into a surface the operator reads.
#[derive(Clone)]
pub struct AuditHook {
    observed: Arc<Mutex<Vec<String>>>,
    /// The same sink the [`ToolHost`] holds, not a second one.
    receipts: Arc<Mutex<ToolReceipts>>,
}

impl AuditHook {
    /// A hook writing into the given receipts sink.
    ///
    /// There is deliberately no `Default` and no nullary `new`. A hook holding a
    /// private sink nobody reads would compile, run, observe everything and
    /// contribute nothing to the evidence — which is precisely the failure this
    /// type now exists to prevent, so it is not constructible.
    pub fn new(receipts: Arc<Mutex<ToolReceipts>>) -> Self {
        Self {
            observed: Arc::new(Mutex::new(Vec::new())),
            receipts,
        }
    }

    /// A hook writing into the same record as `host`'s tools.
    ///
    /// The preferred constructor: one argument, and the sharing it exists to
    /// guarantee cannot be got wrong at the call site.
    pub fn for_host(host: &ToolHost) -> Self {
        Self::new(Arc::clone(&host.receipts))
    }

    /// The tool names this hook saw, in the order it saw them.
    ///
    /// A snapshot rather than a borrow: the lock is released before the caller
    /// looks at the result, so reading telemetry cannot stall a run.
    pub fn observed(&self) -> Vec<String> {
        self.lock().clone()
    }

    /// Append a receipt for a call that no tool body could have recorded.
    ///
    /// `duration_ms` is zero and that is the honest value: nothing ran. The
    /// field measures a tool body's work, and there was no tool body.
    fn record(&self, tool: &str, outcome: &'static str) {
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .calls
            .push(ToolReceipt {
                tool: tool.to_string(),
                outcome,
                duration_ms: 0,
            });
    }

    /// The observation log.
    ///
    /// A poisoned lock is recovered from rather than propagated. The alternative
    /// is that one panicking hook invocation turns every later one into a panic
    /// of its own, which would take down a run over a telemetry fault — exactly
    /// the coupling this module exists to avoid.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.observed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Whether the framework rejected this call before any tool body saw it.
    ///
    /// Stated as a positive test for the thing being recorded rather than as
    /// `!is_success()`, and the difference is not stylistic. Rig has four
    /// dispositions, not two: success, error, refusal, and *skipped*. A skipped
    /// call carries no error at all, so a negated success test would record it
    /// as `malformed` — labelling a call some future hook declined as one the
    /// model got wrong. Requiring an error to be present makes that impossible.
    ///
    /// Given an error, the discriminator is its *source*, not its kind. Kind
    /// cannot do the job: our own [`ToolError::Rejected`] is classified
    /// `invalid_args` exactly like the framework's decode failure, so an outcome
    /// keyed on kind would double-count every refused path traversal. But a tool
    /// body's error always arrives with the concrete [`ToolError`] attached as a
    /// downcastable source, because `ToolError::into_execution_error` puts it
    /// there — and a framework decode failure carries a `serde_json::Error`.
    ///
    /// Both dispositions are consulted. Rig files an intentional refusal under
    /// `refusal()` and everything else under `error()`, and our `NoHostContext`
    /// takes the first path while the rest take the second.
    fn rejected_before_any_body_ran(result: &rig_agent::tool::ToolResult) -> bool {
        result
            .error()
            .or_else(|| result.refusal())
            .is_some_and(|error| error.downcast_ref::<ToolError>().is_none())
    }
}

impl AgentHook for AuditHook {
    /// Observe a tool call and let it proceed, always.
    ///
    /// The return is unconditionally [`ToolCallAction::Run`]. The other three
    /// actions — rewrite the arguments, skip the call, stop the run — are all
    /// ways of making the hook decide something, and a decision taken here is a
    /// decision that vanishes when the hook is not installed.
    async fn on_tool_call(&self, _ctx: &HookContext, event: ToolCall<'_>) -> ToolCallAction {
        self.lock().push(event.tool_name.to_string());
        ToolCallAction::Run
    }

    /// Record a call the model malformed, and change nothing else.
    ///
    /// This fires for every resolved call — including the ones a tool body has
    /// already written down — so the guard is the whole of the logic. A success
    /// carries no error, a failure raised by a body carries that body's
    /// [`ToolError`], and a skipped call carries nothing at all. What is left is
    /// a call the framework rejected before the body was entered.
    ///
    /// [`ToolResultAction::Keep`] preserves the model-visible presentation
    /// exactly. Rewriting it here would make what the model reads depend on
    /// whether telemetry was installed.
    async fn on_tool_result(
        &self,
        _ctx: &HookContext,
        event: ToolResultEvent<'_>,
    ) -> ToolResultAction {
        if Self::rejected_before_any_body_ran(event.raw_result) {
            self.record(event.tool_name, "malformed");
        }
        ToolResultAction::Keep
    }

    /// Record a call naming a tool that does not exist, and resolve nothing.
    ///
    /// `None` leaves the decision to any later hook and, if there is none,
    /// preserves Rig's fail-fast default — so the run still ends in
    /// `UnknownToolCall` exactly as it did before this hook existed. The
    /// recording is the only effect; `Fail`, `Retry`, `Repair`, `Skip` and
    /// `Stop` are all decisions, and a decision taken in a hook is one that
    /// disappears when the hook is not registered.
    async fn on_invalid_tool_call(
        &self,
        _ctx: &HookContext,
        event: &InvalidToolCallContext,
    ) -> Option<InvalidToolCallAction> {
        self.lock().push(event.tool_name.clone());
        self.record(&event.tool_name, "unknown_tool");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tests::test_host;
    use crate::agent::ReadFile;
    use rig_agent::completion::Prompt;
    use rig_agent::tool::ToolContext;
    use rig_agent::AgentBuilder;
    use rig_core::test_utils::{MockCompletionModel, MockTurn};

    #[tokio::test]
    async fn the_hook_observes_a_tool_call_and_lets_it_run() {
        let (host, _g) = test_host();
        let hook = AuditHook::for_host(&host);
        let model = MockCompletionModel::new([
            MockTurn::tool_call(
                "call_1",
                "read_file",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
            MockTurn::text("done"),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(hook.clone())
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let answer = agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        assert_eq!(answer, "done");
        assert_eq!(hook.observed(), vec!["read_file".to_string()]);
        // `Run` is not asserted directly — the enum is not returned to anyone the
        // test can see. It is asserted by consequence: had the hook answered
        // `Skip` or `Stop`, the tool body would not have executed and there would
        // be no receipt for it.
        assert_eq!(
            host.receipts().calls.len(),
            1,
            "the call was let through, and the tool body recorded itself"
        );
    }

    #[tokio::test]
    async fn a_run_with_no_hook_at_all_produces_the_same_evidence() {
        // The same script, the same tool, no hook. If the receipts came from the
        // hook this run would have none.
        let (host, _g) = test_host();
        let model = MockCompletionModel::new([
            MockTurn::tool_call(
                "call_1",
                "read_file",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
            MockTurn::text("done"),
        ]);
        let agent = AgentBuilder::new(model).tool(ReadFile).build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        let receipts = host.receipts();
        assert_eq!(
            receipts.calls.len(),
            1,
            "evidence must not be contingent on a hook being installed"
        );
        assert_eq!(receipts.calls[0].tool, "read_file");
        assert_eq!(receipts.calls[0].outcome, "ok");
    }

    #[tokio::test]
    async fn the_hook_keeps_the_models_arguments_out_of_the_telemetry() {
        // The tool refuses this path, which is the point: the refusal is the
        // moment an implementation would be tempted to log what was asked for.
        let (host, _g) = test_host();
        let hook = AuditHook::for_host(&host);
        let model = MockCompletionModel::new([
            MockTurn::tool_call(
                "call_1",
                "read_file",
                serde_json::json!({"path":"../../etc/passwd"}),
            ),
            MockTurn::text("done"),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(hook.clone())
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        assert_eq!(hook.observed(), vec!["read_file".to_string()]);
        let receipts = host.receipts();
        assert_eq!(receipts.calls[0].outcome, "refused");
        // Both writers saw this call: the body refused it, and `on_tool_result`
        // fired afterwards carrying that same refusal. Exactly one receipt means
        // the hook recognised the failure as one a body had already recorded.
        // Two would mean every refused path traversal is double-counted, and
        // `invalid_args` is the kind our own refusal shares with a decode
        // failure — so this is the assertion that keeps the two apart.
        assert_eq!(
            receipts.calls.len(),
            1,
            "a refusal the tool body recorded must not be recorded again: {receipts:?}"
        );
    }

    /// The silent gap, now closed.
    ///
    /// Rig decodes `Tool::Args` before it enters `Tool::call`, so a call whose
    /// arguments do not match the schema never reaches a body that could record
    /// itself — and, unlike an unknown tool name, **the run carries on
    /// normally**. Before this was recorded, a model that malformed every call
    /// it made produced a bundle indistinguishable from one where it made no
    /// calls at all.
    ///
    /// `malformed` is its own class for the same reason `refused` and `failed`
    /// are separate: it says the model got the call wrong, which is neither us
    /// declining nor the world failing.
    #[tokio::test]
    async fn a_call_the_model_malformed_is_recorded_even_though_no_body_ran() {
        let (host, _g) = test_host();
        let hook = AuditHook::for_host(&host);
        let model = MockCompletionModel::new([
            MockTurn::tool_call("call_1", "read_file", serde_json::json!({"nope": 1})),
            MockTurn::text("done"),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(hook.clone())
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let answer = agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        assert_eq!(
            hook.observed(),
            vec!["read_file".to_string()],
            "the hook fires on the raw arguments, before they are decoded"
        );
        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1, "{receipts:?}");
        assert_eq!(receipts.calls[0].tool, "read_file");
        assert_eq!(
            receipts.calls[0].outcome, "malformed",
            "the model got the call wrong, which is neither a refusal nor a fault"
        );
        // Worth stating outright, because it is the reason this gap was the
        // dangerous one: nothing about the run's own outcome betrays it. A
        // transcript of nothing but malformed calls still ends `Ok`.
        assert_eq!(answer, "done");
    }

    /// The other call a body never sees, and the one that is at least loud.
    ///
    /// A name outside the allowed set is the only thing Rig builds an
    /// `InvalidToolCallContext` for. Every hook returning `None` preserves the
    /// fail-fast default, so the run still ends in `UnknownToolCall` — the
    /// recording changes what is written down and nothing about control flow.
    #[tokio::test]
    async fn a_call_naming_a_tool_that_does_not_exist_is_recorded_and_still_fails_the_run() {
        let (host, _g) = test_host();
        let hook = AuditHook::for_host(&host);
        let model = MockCompletionModel::new([
            MockTurn::tool_call("call_1", "delete_everything", serde_json::json!({})),
            MockTurn::text("unreachable"),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(hook.clone())
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let failure = agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect_err("an unknown tool still fails the run");

        assert!(
            matches!(
                failure,
                rig_agent::completion::PromptError::UnknownToolCall { .. }
            ),
            "returning None must leave Rig's fail-fast default alone: {failure:?}"
        );
        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1, "{receipts:?}");
        assert_eq!(receipts.calls[0].tool, "delete_everything");
        assert_eq!(receipts.calls[0].outcome, "unknown_tool");
    }

    /// A call somebody else declined is not a call the model got wrong.
    ///
    /// `on_tool_result` also fires for a call skipped by another hook, and a
    /// skipped result is not a success — so a naive `!is_success()` test would
    /// file it under `malformed` and blame the model for a decision the host
    /// made. Nothing in this crate skips today; this is here so that the day
    /// something does, the bundle does not quietly start lying.
    #[tokio::test]
    async fn a_call_another_hook_skipped_is_not_blamed_on_the_model() {
        struct SkipEverything;
        impl AgentHook for SkipEverything {
            async fn on_tool_call(
                &self,
                _ctx: &HookContext,
                _event: ToolCall<'_>,
            ) -> ToolCallAction {
                ToolCallAction::Skip("declined by policy".to_string())
            }
        }

        let (host, _g) = test_host();
        let hook = AuditHook::for_host(&host);
        let model = MockCompletionModel::new([
            MockTurn::tool_call(
                "call_1",
                "read_file",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
            MockTurn::text("done"),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(SkipEverything)
            .add_hook(hook.clone())
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        let receipts = host.receipts();
        assert!(
            !receipts
                .calls
                .iter()
                .any(|call| call.outcome == "malformed"),
            "a skipped call was blamed on the model: {receipts:?}"
        );
    }

    /// The name the model invented is evidence; nothing else about it is.
    ///
    /// A receipt naming a tool that does not exist still must not become a way
    /// for the model to write arbitrary text into the operator's bundle beyond
    /// the name itself — and in particular the arguments never travel.
    #[tokio::test]
    async fn an_invented_call_contributes_its_name_and_none_of_its_arguments() {
        let (host, _g) = test_host();
        let hook = AuditHook::for_host(&host);
        let model = MockCompletionModel::new([MockTurn::tool_call(
            "call_1",
            "exfiltrate",
            serde_json::json!({"secret": "swordfish", "path": "/etc/passwd"}),
        )]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(hook.clone())
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let _ = agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await;

        let published = serde_json::to_string(&host.receipts()).expect("receipts serialize");
        assert!(published.contains("exfiltrate"), "{published}");
        for leaked in ["swordfish", "/etc/passwd"] {
            assert!(
                !published.contains(leaked),
                "a model-authored argument reached the evidence bundle: {published}"
            );
        }
    }
}
