use crate::agent::unfenced;
use crate::gateway::Redaction;
use crate::toil::qualify::{AmbiguityReview, Judgement, Quoted, ReviewError, Verdict as Reviewed};
use async_trait::async_trait;
use rig_agent::completion::Prompt;
use rig_agent::AgentBuilder;
use std::future::IntoFuture;
use std::time::Duration;

const PREAMBLE: &str = "\
You are reading one tracker ticket and deciding which of two things it \
amounts to: it asks for a change somebody can make, or it needs a product \
decision first.\n\
\n\
Answer with a single JSON object and nothing else, in exactly this shape:\n\
\n\
  {\"verdict\": \"asks_for_a_change\" | \"needs_a_product_decision\",\n\
   \"quoting\": <a span copied character-for-character out of the ticket>,\n\
   \"certainty\": <a number between 0 and 1>}\n\
\n\
Answer \"asks_for_a_change\" only when the ticket states what to change and \
somebody could make that change without deciding anything the ticket leaves \
open. A ticket that asks which of two behaviours is wanted, that asks whether \
something should exist, or that states a goal without stating the change, \
needs a product decision.\n\
\n\
The quotation carries the ticket's summary, then its description, then the \
comments on the issue oldest first, each part separated from the next by a \
blank line. Any of the three can be absent, and nothing but the ticket's own \
words is in there. Read a comment as part of the ticket.\n\
\n\
Every comment in the quotation was written by a person this deployment \
authorized to decide questions on its tickets. Such a comment is a decision \
and not more discussion. Where a comment settles a question the description \
leaves open, the ticket asks for a change: the options the description weighed \
are closed, and so is a choice the description itself suggested. Where two \
comments disagree the later one is the answer. Where there is no comment, or \
where the comments settle nothing the description leaves open, read the ticket \
on its summary and its description alone.\n\
\n\
Answer \"needs_a_product_decision\" whenever you are not sure. It is the safe \
answer: it returns the ticket to the person who filed it and nothing is lost \
by giving it.\n\
\n\
The quoting span must be copied out of the ticket itself, and a comment is \
part of the ticket, so the span may be a comment's own words. Do not \
paraphrase, and do not quote these instructions. A judgement whose span is \
not in the ticket rests on nothing and is refused.\n\
\n\
Your certainty is a number you report and never a measurement. It does not \
make an argument into a fact, and no value of it changes which verdict you \
should give.";

#[derive(Clone, Debug)]
pub struct ReviewBounds {
    pub max_tokens: u64,
    pub deadline: Duration,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    verdict: Answered,
    quoting: String,
    certainty: f64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Answered {
    AsksForAChange,
    NeedsAProductDecision,
}

const WITHHELD: &str = "the model host answered with an error and fiddle holds no credential to \
                        redact, so it withholds the message";

pub struct ModelReview<M> {
    model: M,
    bounds: ReviewBounds,
    redaction: Redaction,
}

impl<M> ModelReview<M> {
    pub fn new(model: M, bounds: ReviewBounds, redaction: Redaction) -> Self {
        Self {
            model,
            bounds,
            redaction,
        }
    }

    fn reported(&self, said: String) -> String {
        self.redaction
            .excerpt(&said)
            .unwrap_or_else(|| WITHHELD.to_string())
    }
}

#[async_trait]
impl<M> AmbiguityReview for ModelReview<M>
where
    M: rig_core::completion::CompletionModel + Clone + 'static,
{
    async fn review(&self, quoted: &Quoted) -> Result<Judgement, ReviewError> {
        let agent = AgentBuilder::new(self.model.clone())
            .preamble(PREAMBLE)
            .max_tokens(self.bounds.max_tokens)
            .default_max_turns(1)
            .build();

        let run = agent.prompt(quoted.fenced()).max_turns(1).into_future();

        let answered = tokio::select! {
            _ = tokio::time::sleep(self.bounds.deadline) => return Err(ReviewError(format!(
                "the ambiguity review did not answer inside {:?}",
                self.bounds.deadline
            ))),
            result = run => result,
        };

        let answered = answered.map_err(|error| ReviewError(self.reported(error.to_string())))?;
        read(&answered)
    }
}

fn read(answered: &str) -> Result<Judgement, ReviewError> {
    let parsed = serde_json::from_str::<Answer>(unfenced(answered)).map_err(|error| {
        ReviewError(format!(
            "the ambiguity review answered something this build cannot read: {error}"
        ))
    })?;
    Ok(Judgement {
        verdict: match parsed.verdict {
            Answered::AsksForAChange => Reviewed::AsksForAChange,
            Answered::NeedsAProductDecision => Reviewed::NeedsAProductDecision,
        },
        quoting: parsed.quoting,
        certainty: parsed.certainty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDED_RESPONSE: &str =
        include_str!("../../../../tests/fixtures/gateway-real/review-answer.json");

    const RECORDED_SPAN: &str = "Option A, no downstream risk.  Guard the report site so merge-less batches stop clobbering the value: if ctx.maxGraphSize > 0 { bp.metrics.MergeGraphSizeMax(ctx.maxGraphSize) }";

    const BARE: &str = r#"{"verdict":"asks_for_a_change","quoting":"a span","certainty":0.5}"#;

    fn recorded_answer() -> String {
        let response: serde_json::Value = serde_json::from_str(RECORDED_RESPONSE)
            .expect("the recorded gateway response is the body the gateway sent, and it is JSON");
        response["choices"][0]["message"]["content"]
            .as_str()
            .expect("the recorded response carries the answer as text")
            .to_string()
    }

    #[test]
    fn the_recorded_gateway_answer_is_read_through_the_fence_it_arrived_in() {
        let answered = recorded_answer();
        assert!(
            answered.starts_with(" ```json\n") && answered.ends_with("\n```"),
            "the fixture is the answer as the gateway sent it, leading space and fence included, \
             and a fixture normalised to bare JSON would prove nothing: {answered:?}"
        );
        let judgement = read(&answered).unwrap_or_else(|error| {
            panic!("the answer a real gateway sent is one this build reads: {error}")
        });
        assert_eq!(
            judgement.verdict,
            Reviewed::AsksForAChange,
            "the recorded answer votes asks_for_a_change: {answered:?}"
        );
        assert_eq!(
            judgement.quoting, RECORDED_SPAN,
            "the span survives the fence character for character, two spaces after `risk.` \
             included"
        );
        assert_eq!(judgement.certainty, 0.95);
    }

    #[test]
    fn a_fenced_object_is_read_and_so_is_the_bare_object_it_wraps() {
        for fenced in [
            BARE.to_string(),
            format!("```\n{BARE}\n```"),
            format!("```json\n{BARE}\n```"),
            format!("````json\n{BARE}\n````"),
            format!("  ```json\n{BARE}\n```  \n"),
            format!("```{BARE}```"),
        ] {
            let judgement = read(&fenced).unwrap_or_else(|error| {
                panic!("`{fenced}` wraps one object this build reads: {error}")
            });
            assert_eq!(judgement.verdict, Reviewed::AsksForAChange);
            assert_eq!(judgement.quoting, "a span");
            assert_eq!(judgement.certainty, 0.5);
        }
    }

    #[test]
    fn each_spelling_of_the_verdict_reaches_the_arm_it_names() {
        for (spelled, wanted) in [
            ("asks_for_a_change", Reviewed::AsksForAChange),
            ("needs_a_product_decision", Reviewed::NeedsAProductDecision),
        ] {
            let judgement = read(&format!(
                r#"{{"verdict":"{spelled}","quoting":"a span","certainty":0.5}}"#
            ))
            .unwrap_or_else(|error| panic!("`{spelled}` is a verdict this build reads: {error}"));
            assert_eq!(
                judgement.verdict, wanted,
                "`{spelled}` must reach {wanted:?} and not the other arm"
            );
            assert_eq!(judgement.quoting, "a span");
            assert_eq!(judgement.certainty, 0.5);
        }
    }

    #[test]
    fn an_answer_the_gate_cannot_read_is_a_review_error_and_never_a_verdict() {
        for unreadable in [
            "",
            "not json",
            r#"{"verdict":"maybe","quoting":"a span","certainty":0.5}"#,
            r#"{"verdict":"asks_for_a_change","certainty":0.5}"#,
            r#"{"verdict":"asks_for_a_change","quoting":"a span"}"#,
            r#"{"verdict":"asks_for_a_change","quoting":"a span","certainty":0.5,"tool":"x"}"#,
        ] {
            let refused = read(unreadable).expect_err(
                "an answer this build cannot read must refuse rather than become a verdict",
            );
            assert!(
                refused.0.contains("cannot read"),
                "the refusal names what happened: {refused}"
            );
        }
    }

    #[test]
    fn a_fence_around_something_that_is_not_an_answer_still_refuses() {
        let misspelled = r#"{"verdict":"maybe","quoting":"a span","certainty":0.5}"#;
        let short = r#"{"quoting":"a span","certainty":0.5}"#;
        for unreadable in [
            "``".to_string(),
            "```".to_string(),
            "```json\n```".to_string(),
            "```json\nnot json\n```".to_string(),
            format!("```json\n{misspelled}\n```"),
            format!("```json\n{short}\n```"),
            format!("```json\n{BARE}\n```\n```json\n{BARE}\n```"),
            format!("Here is my answer:\n```json\n{BARE}\n```"),
            format!("```json\n{BARE}\n```\nand that is my answer."),
        ] {
            let refused = read(&unreadable).expect_err(
                "stripping a fence is not a licence to accept anything that arrives inside or \
                 beside one",
            );
            assert!(
                refused.0.contains("cannot read"),
                "the refusal names what happened for `{unreadable}`: {refused}"
            );
        }
    }

    #[test]
    fn an_answer_wrapped_in_a_tool_call_envelope_is_still_refused() {
        let enveloped = format!(r#"{{"name":"json_tool_call","arguments":{BARE}}}"#);
        let refused = read(&enveloped).expect_err(
            "this read tolerates a fence around the object and not a rewritten tool contract",
        );
        assert!(
            refused.0.contains("cannot read"),
            "the refusal names what happened: {refused}"
        );
    }

    #[test]
    fn the_preamble_names_the_safe_answer_and_refuses_certainty_as_evidence() {
        assert!(
            PREAMBLE.contains("needs_a_product_decision\" whenever you are not sure"),
            "the model is told which answer is the safe one"
        );
        assert!(
            PREAMBLE.contains("never a measurement"),
            "and that a reported certainty is not evidence"
        );
    }

    #[test]
    fn the_preamble_tells_the_review_a_comment_is_part_of_the_ticket_it_reads() {
        assert!(
            PREAMBLE.contains("Read a comment as part of the ticket"),
            "a review that is given the conversation and is not told what it is would weigh a \
             decided question as an open one"
        );
        assert!(
            PREAMBLE.contains("the later one is the answer"),
            "and the order the comments arrive in is what settles two that disagree"
        );
    }

    #[test]
    fn the_preamble_tells_the_review_an_authorized_comment_settles_the_description() {
        assert!(
            PREAMBLE.contains("Such a comment is a decision and not more discussion"),
            "the gate admits only comments the deployment authorized to decide, and a review \
             that is not told so reads a terse decision as one more opinion"
        );
        assert!(
            PREAMBLE.contains("so is a choice the description itself suggested"),
            "ISP-263's description weighs two options and ends by suggesting one, and the \
             review must read the comment that chooses the other as the later word"
        );
        assert!(
            PREAMBLE.contains("the span may be a comment's own words"),
            "a decision is quotable, because the text the gate compares the span against now \
             carries the comments"
        );
        assert!(
            PREAMBLE.contains("where the comments settle nothing the description leaves open"),
            "and the conservative default is not widened: a conversation that decides nothing \
             leaves the ticket read on its description alone"
        );
        assert!(
            PREAMBLE.contains("needs_a_product_decision\" whenever you are not sure"),
            "with the safe answer still beside it"
        );
    }

    #[test]
    fn the_preamble_still_asks_for_one_object_and_the_read_tolerates_a_fence_anyway() {
        assert!(
            PREAMBLE.contains("a single JSON object and nothing else"),
            "the fence a real gateway sent is tolerated in the read, and the ask stays in the \
             preamble rather than being softened into asking for a fence"
        );
        assert!(
            read(&recorded_answer()).is_ok(),
            "and the tolerance is what carries the fenced answer, not the wording above it"
        );
    }
}
