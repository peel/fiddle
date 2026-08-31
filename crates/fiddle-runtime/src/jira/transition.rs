use crate::effect::{
    AuthorizedEffect, Effect, EffectContext, EffectError, Executor, FromStepParams, ObservedState,
    StepParams,
};
use crate::jira::work_item::{named, read_failure};
use crate::jira::{canonical_revision, project, ConfiguredNames, JiraError, JiraHttp};
use fiddle_core::{EffectName, ProjectedStatus, JIRA_ISSUE_TRANSITIONED};
use tokio_util::sync::CancellationToken;

pub struct TransitionedIssue {
    pub key: String,
    pub status: ProjectedStatus,
    pub revision: String,
}

impl ObservedState for TransitionedIssue {
    type Value = ProjectedStatus;

    fn describe(&self) -> String {
        format!(
            "`{}` is `{}` and this build reads that as {:?}",
            self.key, self.status.jira_status_name, self.status.state
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.revision.clone())
    }

    fn into_value(self) -> ProjectedStatus {
        self.status
    }
}

#[derive(Effect)]
#[effect(
    name = JIRA_ISSUE_TRANSITIONED,
    minimum = "automatic",
    target = "{issue_key}@{issue_updated}",
    state = TransitionedIssue,
    error = JiraError
)]
pub struct TransitionIssue {
    issue_key: String,
    issue_updated: String,
    #[payload]
    to: String,
}

impl FromStepParams for TransitionIssue {
    fn from_params(_executor: &Executor<'_>, _params: &StepParams) -> Result<Self, EffectError> {
        Err(EffectError::Unbuildable {
            kind: EffectName::shipped(JIRA_ISSUE_TRANSITIONED),
            reason: "a step names no issue key, no observed revision and no state to reach, so \
                     this operation is built from an observation and not resolved from a name"
                .to_string(),
        })
    }
}

impl TransitionIssue {
    pub fn new(issue_key: &str, issue_updated: &str, to: &str) -> Result<Self, JiraError> {
        Ok(Self {
            issue_key: issue_key.to_string(),
            issue_updated: revision_of(issue_key, issue_updated)?,
            to: to.to_string(),
        })
    }

    fn issue_path(&self) -> String {
        format!("/rest/api/3/issue/{}?fields=status,updated", self.issue_key)
    }

    fn transitions_path(&self) -> String {
        format!("/rest/api/3/issue/{}/transitions", self.issue_key)
    }

    async fn read(&self, ctx: &EffectContext) -> Result<TransitionedIssue, JiraError> {
        let http = ctx.jira_client()?;
        let answered = http
            .api("GET", &self.issue_path(), None, &ctx.cancel)
            .await?;
        if !(200..300).contains(&answered.status) {
            return Err(read_failure(
                http,
                answered.status,
                &self.issue_key,
                &answered.body,
                &ctx.cancel,
            )
            .await);
        }
        let status = &answered.body["fields"]["status"];
        let unconfigured = ConfiguredNames::new(None, None, None, None, None);
        Ok(TransitionedIssue {
            key: self.issue_key.clone(),
            status: project(
                &unconfigured,
                &named(&status["id"], "fields.status.id")?,
                &named(&status["name"], "fields.status.name")?,
                &named(
                    &status["statusCategory"]["name"],
                    "fields.status.statusCategory.name",
                )?,
            ),
            revision: revision_of(
                &self.issue_key,
                &named(&answered.body["fields"]["updated"], "fields.updated")?,
            )?,
        })
    }

    async fn transition_id(
        &self,
        http: &JiraHttp,
        cancel: &CancellationToken,
    ) -> Result<String, JiraError> {
        let answered = http
            .api("GET", &self.transitions_path(), None, cancel)
            .await?;
        if !(200..300).contains(&answered.status) {
            return Err(read_failure(
                http,
                answered.status,
                &self.issue_key,
                &answered.body,
                cancel,
            )
            .await);
        }
        let Some(offered) = answered.body["transitions"].as_array() else {
            return Err(JiraError::Malformed(format!(
                "`{}` answered with no `transitions` array, so no transition could be resolved to \
                 an id",
                self.transitions_path()
            )));
        };
        self.resolved(offered)
    }

    fn resolved(&self, offered: &[serde_json::Value]) -> Result<String, JiraError> {
        let leading: Vec<&serde_json::Value> = offered
            .iter()
            .filter(|held| held["to"]["name"].as_str() == Some(self.to.as_str()))
            .collect();
        let ids: Vec<&str> = leading
            .iter()
            .filter_map(|held| held["id"].as_str())
            .collect();
        if ids.len() != leading.len() {
            return Err(JiraError::Malformed(format!(
                "`{}` offers a transition to `{}` that carries no `id`, and a transition is sent \
                 by id",
                self.issue_key, self.to
            )));
        }
        match ids.as_slice() {
            [only] => Ok((*only).to_string()),
            [] => Err(JiraError::NotSent(format!(
                "`{}` offers no transition to `{}`; its workflow offers {}",
                self.issue_key,
                self.to,
                offers(offered)
            ))),
            many => Err(JiraError::NotSent(format!(
                "`{}` offers {} transitions to `{}` ({}), and a lookup by name would send the \
                 first of them",
                self.issue_key,
                many.len(),
                self.to,
                many.join(", ")
            ))),
        }
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<TransitionedIssue>, JiraError> {
        let read = self.read(ctx).await?;
        Ok(match read.status.jira_status_name == self.to {
            true => Some(read),
            false => None,
        })
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), JiraError> {
        let http = ctx.jira_client()?;
        let id = self.transition_id(http, &ctx.cancel).await?;
        let answered = http
            .api(
                "POST",
                &self.transitions_path(),
                Some(&serde_json::json!({"transition": {"id": id}})),
                &ctx.cancel,
            )
            .await?;
        match (200..300).contains(&answered.status) {
            true => Ok(()),
            false => Err(read_failure(
                http,
                answered.status,
                &self.issue_key,
                &answered.body,
                &ctx.cancel,
            )
            .await),
        }
    }
}

fn revision_of(issue_key: &str, updated: &str) -> Result<String, JiraError> {
    canonical_revision(updated).ok_or_else(|| {
        JiraError::Malformed(format!(
            "`{issue_key}` was read with `fields.updated` of `{updated}`, which is not a time \
             this build can read, so no identity can name the state it was read in"
        ))
    })
}

fn offers(transitions: &[serde_json::Value]) -> String {
    let named: Vec<String> = transitions
        .iter()
        .filter_map(|held| {
            let id = held["id"].as_str()?;
            let leads_to = held["to"]["name"].as_str()?;
            Some(format!("{id} to `{leads_to}`"))
        })
        .collect();
    match named.is_empty() {
        true => "nothing".to_string(),
        false => named.join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{AdapterError, EffectOutcome, EffectPhase, IntegrationOperation};
    use serde_json::json;

    fn offered(named: &[(&str, &str)]) -> Vec<serde_json::Value> {
        named
            .iter()
            .map(|(id, leads_to)| json!({"id": id, "to": {"name": leads_to}}))
            .collect()
    }

    fn heading_to(to: &str) -> TransitionIssue {
        TransitionIssue::new("IDENT-7", "2026-08-27T09:14:02Z", to)
            .expect("a readable `fields.updated` builds the operation")
    }

    fn transition(issue_updated: &str) -> TransitionIssue {
        TransitionIssue::new("IDENT-1", issue_updated, "In Review")
            .expect("a readable `fields.updated` builds the operation")
    }

    #[test]
    fn the_target_names_the_issue_and_the_state_it_was_read_in() {
        assert_eq!(
            transition("2026-08-27T09:14:02Z").target(),
            "IDENT-1@2026-08-27T09:14:02Z",
            "an identity that named only the issue would let an approval of one state be spent \
             on another"
        );
    }

    #[test]
    fn the_target_changes_when_the_issue_changes() {
        assert_ne!(
            transition("2026-08-27T09:14:02Z").target(),
            transition("2026-08-27T11:40:55Z").target(),
            "an identity must change when the issue changes, or a stale approval is acted on"
        );
    }

    #[test]
    fn one_instant_jira_spells_two_ways_reaches_one_target() {
        assert_eq!(
            transition("2026-08-27T09:14:02.000+0000").target(),
            transition("2026-08-27T09:14:02Z").target(),
            "jira cloud sends a colonless offset, so a raw `fields.updated` would give one state \
             two identities and split one effect in two"
        );
    }

    #[test]
    fn the_raw_updated_field_never_reaches_the_target() {
        let target = transition("2026-08-27T09:14:02.000+0000").target();
        assert!(
            !target.contains("+0000"),
            "the target must carry the canonicalised revision and not the field jira sent: \
             {target}"
        );
    }

    #[test]
    fn an_updated_field_that_is_not_a_time_builds_no_operation_and_names_why() {
        let refused = TransitionIssue::new("IDENT-1", "yesterday", "In Review")
            .err()
            .expect("an unreadable revision cannot name a state");
        assert!(
            format!("{refused}").contains("yesterday"),
            "the refusal must quote what it could not read: {refused}"
        );
    }

    #[test]
    fn the_state_to_reach_is_payload_and_never_identity() {
        let review = transition("2026-08-27T09:14:02Z");
        let done = TransitionIssue::new("IDENT-1", "2026-08-27T09:14:02Z", "Done")
            .expect("a readable revision builds the operation");

        assert_eq!(
            review.target(),
            done.target(),
            "both name one issue in one state, so they are one effect a human decides once"
        );
        assert_ne!(
            review.payload(),
            done.payload(),
            "and the state each would move it to must move the payload hash"
        );
    }

    #[test]
    fn the_effect_is_named_on_the_wire_as_the_registry_will_spell_it() {
        assert_eq!(JIRA_ISSUE_TRANSITIONED, "jira.issue_transitioned");
        assert!(
            EffectName::parse(JIRA_ISSUE_TRANSITIONED).is_ok(),
            "a name no document could spell could never be registered"
        );
    }

    #[test]
    fn a_workflow_that_offers_nothing_reads_as_nothing_rather_than_an_empty_list() {
        assert_eq!(offers(&[]), "nothing");
    }

    #[test]
    fn one_offered_route_to_the_named_state_resolves_to_the_id_the_site_gave_it() {
        let heading = heading_to("Done");
        let routes = offered(&[("31", "Done"), ("41", "In Review")]);

        assert_eq!(
            heading
                .resolved(&routes)
                .expect("one offered route resolves"),
            "31",
            "a lookup that resolved nothing here could not fail the refusals below for the \
             right reason"
        );
    }

    #[test]
    fn a_lookup_that_found_no_route_sent_no_write_and_says_so_in_its_outcome() {
        let heading = heading_to("Done");
        let routes = offered(&[("31", "In Review")]);

        let refused = heading
            .resolved(&routes)
            .expect_err("a workflow that offers no route to a state cannot reach it");

        assert_eq!(
            refused.outcome(EffectPhase::Apply),
            EffectOutcome::NotCommitted,
            "the lookup runs before the write, so a refusal here left the site untouched, and \
             an Unknown would report an ambiguous write for a request never sent: {refused}"
        );
        assert!(
            format!("{refused}").contains("31 to `In Review`"),
            "and the reader still learns what the workflow does offer: {refused}"
        );
    }

    #[test]
    fn a_lookup_that_found_two_routes_sent_no_write_and_still_names_both_ids() {
        let heading = heading_to("Done");
        let routes = offered(&[("31", "Done"), ("41", "Done")]);

        let refused = heading
            .resolved(&routes)
            .expect_err("two routes to one state cannot be told apart by that state's name");

        assert_eq!(
            refused.outcome(EffectPhase::Apply),
            EffectOutcome::NotCommitted,
            "this build refused to choose between two routes and sent nothing, so nothing was \
             written and no answer was lost: {refused}"
        );
        let said = format!("{refused}");
        assert!(
            said.contains("31") && said.contains("41"),
            "and the reader learns which two routes it could not choose between: {said}"
        );
        assert_eq!(
            refused.duplicates(),
            None,
            "two routes to one state are not two states, so the executor must not report the \
             postcondition as duplicated and drop the ids this refusal carries"
        );
    }
}
