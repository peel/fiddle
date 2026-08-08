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
//! # Where the two disagree
//!
//! In one case the hook knows something the receipts do not, and it is not a
//! bug in either. Rig decodes `Tool::Args` before it enters `Tool::call`, so a
//! call whose arguments do not match the advertised schema is answered by the
//! framework and never reaches a tool body that could record it; `on_tool_call`
//! fires anyway, because it runs on the raw JSON. A bundle with no receipts
//! therefore means *the runtime did nothing*, which is not the same claim as
//! *the model asked for nothing*. `a_call_the_model_malformed_is_seen_by_the
//! _hook_and_by_no_receipt` pins that, so the difference stays a known one.

use rig_agent::agent::hook::{AgentHook, HookContext, ToolCall, ToolCallAction};
use std::sync::{Arc, Mutex};

/// Names the tools a run asked for, and lets every one of them through.
///
/// Cheap to clone, and clones share one observation log: the builder takes the
/// hook by value, so the caller keeps a clone if it wants to read what was seen.
///
/// Only [`ToolCall::tool_name`] is kept. The event also carries `args`, which is
/// the model's own JSON and may be a whole file's contents; recording it would
/// make the telemetry unboundedly large and would put model-authored strings
/// into a surface the operator reads.
#[derive(Clone, Default)]
pub struct AuditHook {
    observed: Arc<Mutex<Vec<String>>>,
}

impl AuditHook {
    /// A hook that has seen nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The tool names this hook saw, in the order it saw them.
    ///
    /// A snapshot rather than a borrow: the lock is released before the caller
    /// looks at the result, so reading telemetry cannot stall a run.
    pub fn observed(&self) -> Vec<String> {
        self.lock().clone()
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
        let hook = AuditHook::new();
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
        let hook = AuditHook::new();
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
        assert_eq!(host.receipts().calls[0].outcome, "refused");
    }

    /// The edge of what a receipt can see, pinned so that it stays visible.
    ///
    /// Rig deserializes `Tool::Args` before it calls `Tool::call`, and there is
    /// no trait method between the two. A call whose arguments do not match the
    /// schema is therefore answered by the framework and the tool body is never
    /// entered, so it cannot record itself — and the run continues as if the
    /// model had said nothing.
    ///
    /// The hook sees it, because `on_tool_call` fires on the raw JSON. That is
    /// the one thing the telemetry knows that the record does not, and it is the
    /// reason this asymmetry is worth a test rather than a comment: an operator
    /// reading a bundle with no receipts must not conclude that the model made
    /// no tool calls. Closing it means giving up serde-level argument validation
    /// and refusing inside the tool instead, which is a change to the tools'
    /// argument types and belongs to whoever decides that trade, not here.
    #[tokio::test]
    async fn a_call_the_model_malformed_is_seen_by_the_hook_and_by_no_receipt() {
        let (host, _g) = test_host();
        let hook = AuditHook::new();
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
        agent
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
        assert!(
            host.receipts().calls.is_empty(),
            "a body that never ran cannot have recorded itself; if this ever \
             fails, the gap has been closed and this test should be rewritten \
             rather than deleted"
        );
    }
}
