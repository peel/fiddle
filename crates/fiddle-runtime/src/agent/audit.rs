use super::{ToolError, ToolHost, ToolReceipt, ToolReceipts};
use rig_agent::agent::hook::{
    AgentHook, HookContext, InvalidToolCallAction, InvalidToolCallContext, ToolCall,
    ToolCallAction, ToolResultAction, ToolResultEvent,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AuditHook {
    observed: Arc<Mutex<Vec<String>>>,
    receipts: Arc<Mutex<ToolReceipts>>,
}

impl AuditHook {
    pub fn new(receipts: Arc<Mutex<ToolReceipts>>) -> Self {
        Self {
            observed: Arc::new(Mutex::new(Vec::new())),
            receipts,
        }
    }

    pub fn for_host(host: &ToolHost) -> Self {
        Self::new(Arc::clone(&host.receipts))
    }

    pub fn observed(&self) -> Vec<String> {
        self.lock().clone()
    }

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

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.observed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn rejected_before_any_body_ran(result: &rig_agent::tool::ToolResult) -> bool {
        result
            .error()
            .or_else(|| result.refusal())
            .is_some_and(|error| error.downcast_ref::<ToolError>().is_none())
    }
}

impl AgentHook for AuditHook {
    async fn on_tool_call(&self, _ctx: &HookContext, event: ToolCall<'_>) -> ToolCallAction {
        self.lock().push(event.tool_name.to_string());
        ToolCallAction::Run
    }

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
        assert_eq!(
            host.receipts().calls.len(),
            1,
            "the call was let through, and the tool body recorded itself"
        );
    }

    #[tokio::test]
    async fn a_run_with_no_hook_at_all_produces_the_same_evidence() {
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
        assert_eq!(
            receipts.calls.len(),
            1,
            "a refusal the tool body recorded must not be recorded again: {receipts:?}"
        );
    }

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
        assert_eq!(answer, "done");
    }

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
