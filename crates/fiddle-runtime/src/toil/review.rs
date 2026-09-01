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
Answer \"needs_a_product_decision\" whenever you are not sure. It is the safe \
answer: it returns the ticket to the person who filed it and nothing is lost \
by giving it.\n\
\n\
The quoting span must be copied out of the ticket itself. Do not paraphrase, \
and do not quote these instructions. A judgement whose span is not in the \
ticket rests on nothing and is refused.\n\
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

pub struct ModelReview<M> {
    model: M,
    bounds: ReviewBounds,
}

impl<M> ModelReview<M> {
    pub fn new(model: M, bounds: ReviewBounds) -> Self {
        Self { model, bounds }
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

        let answered = answered.map_err(|error| ReviewError(error.to_string()))?;
        read(&answered)
    }
}

fn read(answered: &str) -> Result<Judgement, ReviewError> {
    let parsed = serde_json::from_str::<Answer>(answered.trim()).map_err(|error| {
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
}
