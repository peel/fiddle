use crate::jira::{project, ConfiguredNames, JiraError, JiraHttp};
use crate::ports::WorkItemPort;
use fiddle_core::{Observation, SourceRef, WorkItemState};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};
use tokio_util::sync::CancellationToken;

pub struct JiraWorkItemPort {
    http: JiraHttp,
    names: ConfiguredNames,
    site: String,
}

struct ReadIssue {
    status_id: String,
    status_name: String,
    status_category: String,
    updated: String,
}

impl JiraWorkItemPort {
    pub fn new(http: JiraHttp, names: ConfiguredNames, site: &str) -> Self {
        Self {
            http,
            names,
            site: site.to_string(),
        }
    }

    fn source(&self, work_id: &str) -> SourceRef {
        SourceRef(format!("jira:{}/{work_id}", self.site))
    }

    fn said(&self, reason: &str) -> String {
        format!("{}: {reason}", self.site)
    }

    async fn read(&self, work_id: &str) -> Result<ReadIssue, JiraError> {
        let path = format!("/rest/api/3/issue/{work_id}?fields=status,updated");
        let answered = self
            .http
            .api("GET", &path, None, &CancellationToken::new())
            .await?;
        match answered.status {
            status if (200..300).contains(&status) => issue_from(&answered.body),
            status @ 401 => Err(JiraError::Unauthorized { status }),
            status @ 403 => Err(JiraError::Forbidden { status }),
            404 => Err(JiraError::Absent {
                key: work_id.to_string(),
            }),
            status => Err(JiraError::Malformed(format!("HTTP {status}"))),
        }
    }
}

#[async_trait::async_trait]
impl WorkItemPort for JiraWorkItemPort {
    async fn observe(&self, work_id: &str) -> Observation<WorkItemState> {
        let source = self.source(work_id);
        let issue = match self.read(work_id).await {
            Ok(issue) => issue,
            Err(failed) => {
                return Observation::Unavailable {
                    source,
                    reason: self.said(&failed.to_string()),
                }
            }
        };
        let Some(revision) = canonical_revision(&issue.updated) else {
            return Observation::Unavailable {
                source,
                reason: self.said(&format!(
                    "the issue's `fields.updated` is not a time this port can read: `{}`",
                    issue.updated
                )),
            };
        };
        Observation::Available {
            value: WorkItemState {
                id: work_id.to_string(),
                status: issue.status_name.clone(),
                projected_status: Some(project(
                    &self.names,
                    &issue.status_id,
                    &issue.status_name,
                    &issue.status_category,
                )),
            },
            source,
            revision: Some(revision),
        }
    }
}

fn issue_from(body: &serde_json::Value) -> Result<ReadIssue, JiraError> {
    let status = &body["fields"]["status"];
    Ok(ReadIssue {
        status_id: named(&status["id"], "fields.status.id")?,
        status_name: named(&status["name"], "fields.status.name")?,
        status_category: named(
            &status["statusCategory"]["name"],
            "fields.status.statusCategory.name",
        )?,
        updated: named(&body["fields"]["updated"], "fields.updated")?,
    })
}

fn named(held: &serde_json::Value, path: &str) -> Result<String, JiraError> {
    match held.as_str() {
        Some(held) => Ok(held.to_string()),
        None => Err(JiraError::Malformed(format!("no `{path}`"))),
    }
}

fn canonical_revision(updated: &str) -> Option<String> {
    read_instant(updated)?
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .ok()
}

fn read_instant(updated: &str) -> Option<OffsetDateTime> {
    let subsecond = format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond][offset_hour \
         sign:mandatory][offset_minute]"
    );
    let whole_second = format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory][offset_minute]"
    );
    OffsetDateTime::parse(updated, &Rfc3339)
        .or_else(|_| OffsetDateTime::parse(updated, &subsecond))
        .or_else(|_| OffsetDateTime::parse(updated, &whole_second))
        .ok()
}
