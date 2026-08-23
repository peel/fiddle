use super::{accounting, RepairReport};
use crate::agent::transcript::{Record, Transcripts, RETURNED};
use crate::capability::{breached, DeclarationBreach};
use crate::gateway::Redaction;
use crate::workspace::Workspace;
use rig_agent::agent::hook::{AgentHook, HookContext, ModelTurnAction, ModelTurnFinished};
use rig_core::completion::message::AssistantContent;
use rig_core::OneOrMany;
use std::sync::{Arc, Mutex};

pub const RETURNS: usize = 2;

pub const ACCOUNTING: &str = "accounting";

pub const DECLARATION: &str = "declaration";

const REFUSED: &str = "fiddle refused that report:";

const ACCOUNT_FOR_IT: &str = "Continue the work, then send one report that accounts for every \
                              advisory this task showed you.";

const DO_THE_WORK: &str = "Change every file you declared, then send one report whose \
                           changed_files names every file you changed.";

const DECLARE_THE_WORK: &str = "Send one report whose changed_files names every file you changed.";

pub fn returned_to_the_model(reason: &str) -> String {
    format!("{REFUSED} {reason}. {ACCOUNT_FOR_IT}")
}

pub fn declaration_returned(breach: &DeclarationBreach) -> String {
    let ask = match breach.unmet.is_empty() {
        true => DECLARE_THE_WORK,
        false => DO_THE_WORK,
    };
    format!("{REFUSED} {breach}. {ask}")
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

pub fn declaration(
    excused: &[String],
    report: &RepairReport,
    touched: &[&str],
) -> Option<DeclarationBreach> {
    let declared: Vec<String> = report
        .changed_files
        .iter()
        .cloned()
        .chain(excused.iter().cloned())
        .collect();
    breached(&declared, touched)
}

#[derive(Clone)]
pub enum Declarations {
    Unchecked,

    Held {
        workspace: Arc<Workspace>,
        excused: Arc<Vec<String>>,
    },
}

impl Declarations {
    pub fn held(workspace: &Arc<Workspace>, excused: &[String]) -> Self {
        Declarations::Held {
            workspace: Arc::clone(workspace),
            excused: Arc::new(excused.to_vec()),
        }
    }
}

pub struct Held<'a> {
    pub shown: &'a [&'a str],

    pub declarations: Declarations,
}

struct Returned {
    rule: &'static str,
    reason: String,
    sentence: String,
}

#[derive(Clone)]
pub struct ReturnHook {
    shown: Arc<Vec<String>>,
    declarations: Declarations,
    bound: usize,
    returns: Arc<Mutex<usize>>,
    transcripts: Option<Transcripts>,
    redaction: Redaction,
}

impl ReturnHook {
    pub fn holding(
        held: &Held<'_>,
        bound: usize,
        redaction: &Redaction,
        transcripts: Option<&Transcripts>,
    ) -> Self {
        ReturnHook {
            shown: Arc::new(held.shown.iter().map(|cve| cve.to_string()).collect()),
            declarations: held.declarations.clone(),
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

    fn record(&self, turn: u64, returns: usize, rule: &str, reason: &str) {
        let Some(transcripts) = &self.transcripts else {
            return;
        };
        transcripts.append(
            &self.redaction,
            Record::of(RETURNED)
                .number("turn", turn)
                .number("returns", returns as u64)
                .number("bound", self.bound as u64)
                .text("rule", rule)
                .text("reason", reason),
        );
    }

    fn accounting_failure(&self, report: &RepairReport) -> Option<String> {
        if self.shown.is_empty() {
            return None;
        }
        let shown: Vec<&str> = self.shown.iter().map(String::as_str).collect();
        accounting(&shown, &report.findings)
    }

    fn declaration_failure(&self, report: &RepairReport) -> Option<DeclarationBreach> {
        let Declarations::Held { workspace, excused } = &self.declarations else {
            return None;
        };
        let changed = workspace.changed_files().ok()?;
        let touched: Vec<&str> = changed.iter().map(|path| path.as_str()).collect();
        declaration(excused, report, &touched)
    }

    fn failure(&self, content: &OneOrMany<AssistantContent>) -> Option<Returned> {
        let report = report_in(content)?;
        if let Some(reason) = self.accounting_failure(&report) {
            let sentence = returned_to_the_model(&reason);
            return Some(Returned {
                rule: ACCOUNTING,
                reason,
                sentence,
            });
        }
        let breach = self.declaration_failure(&report)?;
        Some(Returned {
            rule: DECLARATION,
            reason: breach.to_string(),
            sentence: declaration_returned(&breach),
        })
    }
}

impl AgentHook for ReturnHook {
    async fn on_model_turn_finished(
        &self,
        ctx: &HookContext,
        event: ModelTurnFinished<'_>,
    ) -> ModelTurnAction {
        if calls_a_tool(event.content) {
            return ModelTurnAction::Continue;
        }
        let Some(failure) = self.failure(event.content) else {
            return ModelTurnAction::Continue;
        };
        let mut returns = self.locked();
        if *returns >= self.bound {
            return ModelTurnAction::Continue;
        }
        *returns += 1;
        self.record(ctx.turn() as u64, *returns, failure.rule, &failure.reason);
        ModelTurnAction::retry_with_feedback(failure.sentence)
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

    fn accounted_for(findings: &[&str], changed: &[&str]) -> RepairReport {
        RepairReport {
            changed_files: changed.iter().map(|path| path.to_string()).collect(),
            summary: "bumped the module".to_string(),
            claimed_complete: true,
            findings: findings
                .iter()
                .map(|cve| FindingDisposition {
                    cve: cve.to_string(),
                    attempted: true,
                    note: "bumped it".to_string(),
                })
                .collect(),
        }
    }

    fn report(findings: &[&str]) -> String {
        serde_json::to_string(&accounted_for(findings, &["go.mod"])).expect("a report serializes")
    }

    fn shown_only(shown: &[&str]) -> ReturnHook {
        ReturnHook::holding(
            &Held {
                shown,
                declarations: Declarations::Unchecked,
            },
            RETURNS,
            &Redaction::unknown(),
            None,
        )
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
        let hook = shown_only(&[SHOWN]);

        let failure = hook
            .failure(&text(&planning))
            .expect("the turn-3 content parses and accounts for nothing");
        assert_eq!(failure.rule, ACCOUNTING);
        assert!(
            failure.reason.contains(SHOWN),
            "the plan that ended the run must return the advisory it left out: {}",
            failure.reason
        );
    }

    #[test]
    fn a_turn_that_is_not_a_report_carries_no_failure() {
        let hook = shown_only(&[SHOWN]);

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
        let hook = shown_only(&[]);

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

    #[test]
    fn a_path_declared_and_not_changed_asks_the_model_for_the_work() {
        let report = accounted_for(&[SHOWN], &["go.mod"]);
        let breach = declaration(&[], &report, &[]).expect("go.mod was declared and not changed");
        let sentence = declaration_returned(&breach);

        assert!(
            sentence.contains("declared without changing: go.mod"),
            "the return carries the same reason the refusal renders: {sentence}"
        );
        assert!(
            sentence.contains(DO_THE_WORK),
            "the model reported work it did not do, so the return asks for the \
             work: {sentence}"
        );
    }

    #[test]
    fn a_path_changed_and_not_declared_asks_the_model_for_a_corrected_report() {
        let report = accounted_for(&[SHOWN], &["go.mod"]);
        let breach = declaration(&[], &report, &["go.mod", "main.go"])
            .expect("main.go was changed and not declared");
        let sentence = declaration_returned(&breach);

        assert!(
            sentence.contains("changed without declaring: main.go"),
            "the return names the file: {sentence}"
        );
        assert!(
            sentence.contains(DECLARE_THE_WORK) && !sentence.contains(DO_THE_WORK),
            "the model did work it did not report, so the return asks for a \
             corrected report and never for more work: {sentence}"
        );
    }

    #[test]
    fn a_report_wrong_in_both_directions_is_asked_for_the_work_it_declared() {
        let report = accounted_for(&[SHOWN], &["go.mod"]);
        let breach =
            declaration(&[], &report, &["main.go"]).expect("one path each way is a breach");
        let sentence = declaration_returned(&breach);

        assert!(
            sentence.contains("changed without declaring: main.go")
                && sentence.contains("declared without changing: go.mod"),
            "both halves reach the model: {sentence}"
        );
        assert!(
            sentence.contains(DO_THE_WORK),
            "the undone work is the larger of the two failures, and the ask for \
             it already asks for a corrected report: {sentence}"
        );
    }

    #[test]
    fn the_two_asks_are_different_sentences() {
        assert_ne!(
            DO_THE_WORK, DECLARE_THE_WORK,
            "a model told to report differently when it should write a file has \
             been told the wrong thing"
        );
    }

    #[test]
    fn a_path_the_run_changed_before_briefing_is_excused() {
        let report = accounted_for(&[SHOWN], &[]);
        let excused = vec!["go.mod".to_string()];

        assert!(
            declaration(&excused, &report, &["go.mod"]).is_none(),
            "the sweep bumped go.mod before the model was briefed, so an empty \
             declaration is the honest report"
        );
        let breach = declaration(&excused, &report, &["go.mod", "main.go"])
            .expect("the attempt's own edit is not excused");
        assert_eq!(breach.unannounced, vec!["main.go".to_string()]);
        assert!(breach.unmet.is_empty(), "{breach:?}");
    }

    #[test]
    fn a_run_whose_declarations_are_unchecked_returns_no_breach() {
        let hook = shown_only(&[SHOWN]);

        assert!(
            hook.declaration_failure(&accounted_for(&[SHOWN], &["go.mod"]))
                .is_none(),
            "the repair path has no post-run declaration check, so a return \
             there would invent a rule nothing enforces"
        );
    }

    #[test]
    fn a_report_failing_both_rules_is_returned_on_the_accounting_rule_first() {
        let hook = shown_only(&[SHOWN]);
        let failure = hook
            .failure(&text(&report(&[])))
            .expect("a report accounting for nothing is a failure");

        assert_eq!(
            failure.rule, ACCOUNTING,
            "the post-run check refuses on accounting before it reads the diff, \
             so the return has to agree with it"
        );
    }
}
