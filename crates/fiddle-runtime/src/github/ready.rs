use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::github::GhError;
use fiddle_core::HumanDecisionRequirement;
use std::sync::OnceLock;

const READY_FOR_REVIEW: &str = "mutation($id: ID!) { markPullRequestReadyForReview(input: \
                                {pullRequestId: $id}) { pullRequest { isDraft } } }";

type Mutation<'a> = (&'static str, [(&'a str, &'a str); 1]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadyPullRequest {
    pub number: u64,
    pub node_id: String,
}

impl ObservedState for ReadyPullRequest {
    type Value = ReadyPullRequest;

    fn describe(&self) -> String {
        format!("pull request #{} is ready for review", self.number)
    }

    fn reference(&self) -> Option<String> {
        Some(self.number.to_string())
    }

    fn into_value(self) -> ReadyPullRequest {
        self
    }
}

pub fn pull_request_ready_target(repo: &str, pr: u64, head_sha: &str) -> String {
    format!("{repo}#{pr}@{head_sha}")
}

pub struct EnsurePullRequestReady {
    repo: String,
    pr: u64,
    head_sha: String,
    node_id: OnceLock<String>,
}

impl EnsurePullRequestReady {
    pub fn new(repo: String, pr: u64, head_sha: String) -> Self {
        Self {
            repo,
            pr,
            head_sha,
            node_id: OnceLock::new(),
        }
    }

    pub fn target(&self) -> String {
        pull_request_ready_target(&self.repo, self.pr, &self.head_sha)
    }

    fn lookup_path(&self) -> String {
        format!("/repos/{}/pulls/{}", self.repo, self.pr)
    }

    fn mutation(&self) -> Result<Mutation<'_>, GhError> {
        let node_id = self.node_id.get().ok_or_else(|| {
            GhError::NotSent(format!(
                "the node id of {} was not read before the mutation",
                self.target()
            ))
        })?;
        Ok((READY_FOR_REVIEW, [("id", node_id.as_str())]))
    }

    fn read(&self, body: &serde_json::Value) -> Result<(bool, String), GhError> {
        let draft = body["draft"].as_bool().ok_or_else(|| {
            GhError::Malformed(format!("{} carried no draft state", self.lookup_path()))
        })?;
        let node_id = body["node_id"].as_str().ok_or_else(|| {
            GhError::Malformed(format!("{} carried no node id", self.lookup_path()))
        })?;
        Ok((draft, node_id.to_string()))
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsurePullRequestReady {
    type State = ReadyPullRequest;

    type Error = GhError;

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Human
    }

    fn payload(&self) -> String {
        serde_json::Value::Object(serde_json::Map::from_iter([
            ("head".to_string(), self.head_sha.clone().into()),
            ("pr".to_string(), self.pr.into()),
            ("repo".to_string(), self.repo.clone().into()),
        ]))
        .to_string()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<ReadyPullRequest>, GhError> {
        let response = ctx
            .gh
            .api("GET", &self.lookup_path(), None, &ctx.cancel)
            .await?;
        let (draft, node_id) = self.read(&response.body)?;

        let _ = self.node_id.set(node_id.clone());

        match draft {
            true => Ok(None),
            false => Ok(Some(ReadyPullRequest {
                number: self.pr,
                node_id,
            })),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        let (query, variables) = self.mutation()?;

        ctx.gh
            .graphql(query, &variables, &ctx.cancel)
            .await
            .map(|_data| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{AdapterError, EffectOutcome, EffectPhase};
    use fiddle_core::payload_hash;

    fn ready_at(head_sha: &str) -> EnsurePullRequestReady {
        EnsurePullRequestReady::new("acme/r".to_string(), 7, head_sha.to_string())
    }

    #[test]
    fn a_mutation_with_no_node_id_in_hand_is_not_sent() {
        let refusal = ready_at("aaaa")
            .mutation()
            .expect_err("a node id that was never read is not an input");

        assert!(matches!(refusal, GhError::NotSent(_)), "got {refusal:?}");
        assert_eq!(
            refusal.outcome(EffectPhase::Apply),
            EffectOutcome::NotCommitted,
            "nothing was sent, so there is nothing to go and look for"
        );
        assert!(
            refusal.to_string().contains("acme/r#7@aaaa"),
            "and it names the effect at the revision it refused: {refusal}"
        );
    }

    #[test]
    fn the_mutation_binds_its_input_rather_than_spelling_it() {
        let ready = ready_at("aaaa");
        ready.node_id.set("PR_kwDOabcdef".to_string()).unwrap();

        let (query, variables) = ready.mutation().unwrap();

        assert_eq!(variables, [("id", "PR_kwDOabcdef")]);
        assert!(query.contains("markPullRequestReadyForReview"));
        assert!(query.contains("mutation($id: ID!)"));
        assert!(
            !query.contains("PR_kwDOabcdef"),
            "the id is bound, not spelled: {query}"
        );
    }

    #[test]
    fn a_pull_request_missing_either_field_is_not_read() {
        let ready = ready_at("aaaa");
        assert!(ready
            .read(&serde_json::json!({"node_id": "PR_abc"}))
            .is_err());
        assert!(ready.read(&serde_json::json!({"draft": true})).is_err());
        assert_eq!(
            ready
                .read(&serde_json::json!({"draft": false, "node_id": "PR_abc"}))
                .unwrap(),
            (false, "PR_abc".to_string())
        );
    }

    #[test]
    fn the_read_addresses_one_pull_request() {
        assert_eq!(ready_at("aaaa").lookup_path(), "/repos/acme/r/pulls/7");
    }

    #[test]
    fn a_moved_head_moves_the_payload_that_step_six_checks() {
        assert_ne!(
            payload_hash(&ready_at("aaaa").payload()),
            payload_hash(&ready_at("bbbb").payload())
        );
        assert_eq!(
            payload_hash(&ready_at("aaaa").payload()),
            payload_hash(&ready_at("aaaa").payload()),
            "and the payload is canonical: the same request hashes the same"
        );
    }
}
