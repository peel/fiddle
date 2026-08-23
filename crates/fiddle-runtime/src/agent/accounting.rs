use super::{accounting, RepairReport};
use crate::agent::transcript::{Record, Transcripts, RETURNED};
use crate::gateway::Redaction;
use rig_agent::agent::hook::{AgentHook, HookContext, ModelTurnAction, ModelTurnFinished};
use rig_core::completion::message::AssistantContent;
use rig_core::OneOrMany;
use std::sync::{Arc, Mutex};

pub const RETURNS: usize = 2;

const REFUSED: &str = "fiddle refused that report:";

const AGAIN: &str = "Continue the work, then send one report that accounts for every advisory \
                     this task showed you.";

pub fn returned_to_the_model(reason: &str) -> String {
    format!("{REFUSED} {reason}. {AGAIN}")
}

pub fn report_in(content: &OneOrMany<AssistantContent>) -> Option<RepairReport> {
    let text: String = content
        .iter()
        .filter_map(|block| match block {
            AssistantContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    serde_json::from_str(text.trim()).ok()
}

fn calls_a_tool(content: &OneOrMany<AssistantContent>) -> bool {
    content
        .iter()
        .any(|block| matches!(block, AssistantContent::ToolCall(_)))
}

#[derive(Clone)]
pub struct AccountingHook {
    shown: Arc<Vec<String>>,
    bound: usize,
    returns: Arc<Mutex<usize>>,
    transcripts: Option<Transcripts>,
    redaction: Redaction,
}

impl AccountingHook {
    pub fn holding(
        shown: &[&str],
        bound: usize,
        redaction: &Redaction,
        transcripts: Option<&Transcripts>,
    ) -> Self {
        AccountingHook {
            shown: Arc::new(shown.iter().map(|cve| cve.to_string()).collect()),
            bound,
            returns: Arc::new(Mutex::new(0)),
            transcripts: transcripts.cloned(),
            redaction: redaction.clone(),
        }
    }

    pub fn returned(&self) -> usize {
        *self.locked()
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, usize> {
        self.returns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn record(&self, turn: u64, returns: usize, reason: &str) {
        let Some(transcripts) = &self.transcripts else {
            return;
        };
        transcripts.append(
            &self.redaction,
            Record::of(RETURNED)
                .number("turn", turn)
                .number("returns", returns as u64)
                .number("bound", self.bound as u64)
                .text("reason", reason),
        );
    }

    fn failure(&self, content: &OneOrMany<AssistantContent>) -> Option<String> {
        let report = report_in(content)?;
        let shown: Vec<&str> = self.shown.iter().map(String::as_str).collect();
        accounting(&shown, &report.findings)
    }
}

impl AgentHook for AccountingHook {
    async fn on_model_turn_finished(
        &self,
        ctx: &HookContext,
        event: ModelTurnFinished<'_>,
    ) -> ModelTurnAction {
        if self.shown.is_empty() || calls_a_tool(event.content) {
            return ModelTurnAction::Continue;
        }
        let Some(reason) = self.failure(event.content) else {
            return ModelTurnAction::Continue;
        };
        let mut returns = self.locked();
        if *returns >= self.bound {
            return ModelTurnAction::Continue;
        }
        *returns += 1;
        self.record(ctx.turn() as u64, *returns, &reason);
        ModelTurnAction::retry_with_feedback(returned_to_the_model(&reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::FindingDisposition;

    const SHOWN: &str = "CVE-2025-30204";

    fn text(body: &str) -> OneOrMany<AssistantContent> {
        OneOrMany::one(AssistantContent::text(body))
    }

    fn report(findings: &[&str]) -> String {
        let disposed: Vec<FindingDisposition> = findings
            .iter()
            .map(|cve| FindingDisposition {
                cve: cve.to_string(),
                attempted: true,
                note: "bumped it".to_string(),
            })
            .collect();
        serde_json::to_string(&RepairReport {
            changed_files: vec!["go.mod".to_string()],
            summary: "bumped the module".to_string(),
            claimed_complete: true,
            findings: disposed,
        })
        .expect("a report serializes")
    }

    #[test]
    fn the_sentence_the_model_receives_names_the_advisory_it_left_out() {
        let reason = accounting(&[SHOWN], &[]).expect("nothing was disposed of");
        let sentence = returned_to_the_model(&reason);

        assert!(
            sentence.contains(SHOWN),
            "a return the model cannot act on is a generic instruction: {sentence}"
        );
        assert!(
            sentence.contains("shown and not reported"),
            "the model gets fiddle's own accounting failure: {sentence}"
        );
        assert!(
            sentence.contains("send one report"),
            "the sentence has to say what to do next: {sentence}"
        );
    }

    #[test]
    fn the_plan_that_ended_run_32634427291_carries_an_accounting_failure() {
        let planning = serde_json::json!({
            "summary": "Let me first check if there are any direct usages",
            "changed_files": [],
            "claimed_complete": false,
            "findings": [],
        })
        .to_string();
        let hook = AccountingHook::holding(&[SHOWN], RETURNS, &Redaction::unknown(), None);

        let reason = hook
            .failure(&text(&planning))
            .expect("the turn-3 content parses and accounts for nothing");
        assert!(
            reason.contains(SHOWN),
            "the plan that ended the run must return the advisory it left out: {reason}"
        );
    }

    #[test]
    fn a_turn_that_is_not_a_report_carries_no_accounting_failure() {
        let hook = AccountingHook::holding(&[SHOWN], RETURNS, &Redaction::unknown(), None);

        assert!(
            hook.failure(&text("I will look at go.mod next")).is_none(),
            "prose is not a report, and rig already re-prompts for one"
        );
        assert!(
            hook.failure(&text(&report(&[SHOWN]))).is_none(),
            "a report that accounts for the advisory is an answer"
        );
        assert!(
            hook.failure(&text(&report(&[]))).is_some(),
            "a report that accounts for nothing is the failure this hook returns"
        );
    }

    #[test]
    fn a_run_shown_no_advisory_has_no_accounting_rule_to_fail() {
        let hook = AccountingHook::holding(&[], RETURNS, &Redaction::unknown(), None);

        assert!(
            hook.shown.is_empty(),
            "the repair path shows no advisory, so it installs an empty hook"
        );
        assert!(
            hook.failure(&text(&report(&[]))).is_none(),
            "an empty shown set disposes of itself"
        );
    }

    #[test]
    fn a_turn_calling_a_tool_is_never_returned() {
        let call = OneOrMany::one(AssistantContent::tool_call(
            "c1",
            "read_file",
            serde_json::json!({"path": "go.mod"}),
        ));

        assert!(
            calls_a_tool(&call),
            "rig refuses to retry a tool-bearing turn, so this hook must not ask"
        );
        assert!(
            !calls_a_tool(&text(&report(&[]))),
            "a report arriving as text is the turn this hook returns"
        );
    }
}
