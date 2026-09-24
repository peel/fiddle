use rig_agent::agent::hook::{AgentHook, CompletionResponse, HookContext, ObservationAction};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[derive(Clone)]
pub struct SpendHook {
    spent: Arc<AtomicU64>,
    bound: Option<u64>,
    stopped: Arc<std::sync::Mutex<Option<String>>>,
}

impl SpendHook {
    pub fn bounded_by(bound: Option<u64>) -> Self {
        Self {
            spent: Arc::new(AtomicU64::new(0)),
            bound,
            stopped: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn stopped(&self) -> Option<String> {
        self.stopped
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn spent(&self) -> u64 {
        self.spent.load(Ordering::Relaxed)
    }

    fn over(&self, spent: u64) -> Option<String> {
        let bound = self.bound?;
        (spent >= bound).then(|| reached(spent, bound))
    }
}

pub fn reached(spent: u64, bound: u64) -> String {
    format!(
        "the token bound of {bound} was reached at {spent}. Every turn resends the history, so \
         a run that is not converging costs more per turn than the one before it, and this \
         bound stops it whatever the cause"
    )
}

impl AgentHook for SpendHook {
    async fn on_completion_response(
        &self,
        _ctx: &HookContext,
        event: CompletionResponse<'_>,
    ) -> ObservationAction {
        let turn = event.usage.input_tokens + event.usage.output_tokens;
        let spent = self.spent.fetch_add(turn, Ordering::Relaxed) + turn;
        match self.over(spent) {
            Some(reason) => {
                *self
                    .stopped
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(reason.clone());
                ObservationAction::Stop(reason)
            }
            None => ObservationAction::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_with_no_bound_never_stops_however_much_it_spends() {
        let hook = SpendHook::bounded_by(None);
        assert_eq!(
            hook.over(u64::MAX),
            None,
            "a deployment that set no bound is not bounded"
        );
    }

    #[test]
    fn a_bound_stops_at_it_and_not_before_it() {
        let hook = SpendHook::bounded_by(Some(100));
        assert_eq!(hook.over(99), None, "under the bound the run continues");
        assert!(
            hook.over(100).is_some_and(|it| it.contains("100")),
            "at the bound it stops, and the reason names the bound"
        );
        assert!(
            hook.over(250).is_some_and(|it| it.contains("250")),
            "and past it the reason names what was actually spent, not the bound alone"
        );
    }

    #[test]
    fn the_reason_names_both_numbers_so_a_narrow_bound_can_be_told_from_a_runaway() {
        let said = reached(7_020_009, 1_000_000);
        assert!(said.contains("7020009"), "{said}");
        assert!(said.contains("1000000"), "{said}");
    }
}

#[cfg(test)]
mod driving {
    use super::*;
    use crate::agent::tools::tests::test_host;
    use crate::agent::ReadFile;
    use rig_agent::completion::Prompt;
    use rig_agent::tool::ToolContext;
    use rig_agent::AgentBuilder;
    use rig_core::completion::Usage;
    use rig_core::test_utils::{MockCompletionModel, MockTurn};

    fn spending(input: u64, output: u64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            ..Usage::new()
        }
    }

    fn reading(call: &str) -> MockTurn {
        MockTurn::tool_call(call, "read_file", serde_json::json!({"path":"src/lib.rs"}))
            .with_usage(spending(100, 10))
    }

    fn three_turns() -> MockCompletionModel {
        MockCompletionModel::new([
            reading("call_1"),
            reading("call_2"),
            MockTurn::text("done").with_usage(spending(100, 10)),
        ])
    }

    async fn run_with(hook: SpendHook) -> Result<String, String> {
        let (host, _guard) = test_host();
        let agent = AgentBuilder::new(three_turns())
            .tool(ReadFile)
            .add_hook(hook)
            .build();
        let mut ctx = ToolContext::new();
        ctx.insert(host);
        agent
            .prompt("go")
            .tool_context(ctx)
            .max_turns(5)
            .await
            .map_err(|error| error.to_string())
    }

    #[tokio::test]
    async fn a_run_over_the_token_bound_stops_and_reports_why() {
        let hook = SpendHook::bounded_by(Some(150));
        let answered = run_with(hook.clone()).await;

        assert!(
            hook.spent() >= 110,
            "the row's own premise: the hook saw a turn's usage, and it was {}",
            hook.spent()
        );
        assert!(
            hook.stopped().is_some_and(|it| it.contains("token bound")),
            "the hook reports why it stopped, because `Stop` ends the loop with whatever it \
             has rather than an error, and a caller reading only the answer would report \
             something else: spent {}, answered {answered:?}",
            hook.spent()
        );
        assert_ne!(
            answered.as_deref(),
            Ok("done"),
            "and it stopped before the run's own last turn, so the bound cut it short"
        );
    }

    #[tokio::test]
    async fn a_run_under_the_bound_finishes_and_the_hook_counted_it() {
        let hook = SpendHook::bounded_by(Some(u64::MAX));
        let answered = run_with(hook.clone()).await;

        assert_eq!(
            answered.as_deref(),
            Ok("done"),
            "a bound nothing reaches stops nothing, so the row above is not passing on a \
             hook that stops every run"
        );
        assert!(hook.stopped().is_none(), "and it reports no stop");
        assert!(
            hook.spent() >= 330,
            "and it counted every turn, so the bound is measured rather than assumed: {}",
            hook.spent()
        );
    }

    #[tokio::test]
    async fn a_run_with_no_bound_finishes_however_much_it_spends() {
        let hook = SpendHook::bounded_by(None);
        assert_eq!(
            run_with(hook.clone()).await.as_deref(),
            Ok("done"),
            "a deployment that configured no bound runs as it did before this hook existed"
        );
        assert!(hook.stopped().is_none());
    }
}
