use crate::jira::{canonical_revision, project, ConfiguredNames, JiraError, JiraHttp};
use crate::ports::WorkItemPort;
use fiddle_core::{Observation, SourceRef, WorkItemState};
use tokio_util::sync::CancellationToken;

const MYSELF: &str = "/rest/api/3/myself";

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
        format!("{}: {}", self.site, self.http.quotable(reason))
    }

    async fn read(
        &self,
        work_id: &str,
        cancel: &CancellationToken,
    ) -> Result<ReadIssue, JiraError> {
        let path = format!("/rest/api/3/issue/{work_id}?fields=status,updated");
        let answered = self.http.api("GET", &path, None, cancel).await?;
        match answered.status {
            status if (200..300).contains(&status) => issue_from(&answered.body),
            status => Err(read_failure(&self.http, status, work_id, &answered.body, cancel).await),
        }
    }
}

pub(crate) async fn read_failure(
    http: &JiraHttp,
    status: u16,
    work_id: &str,
    body: &serde_json::Value,
    cancel: &CancellationToken,
) -> JiraError {
    match failure_for(status, work_id, http.quoted(body).as_deref()) {
        JiraError::Absent { .. } => absent_or_refused(work_id, credential(http, cancel).await),
        named => named,
    }
}

async fn credential(http: &JiraHttp, cancel: &CancellationToken) -> Credential {
    match http.api("GET", MYSELF, None, cancel).await {
        Ok(answered) if answered.status == 401 => Credential::Refused,
        Ok(answered) if (200..300).contains(&answered.status) => Credential::Accepted,
        Ok(answered) => Credential::Unchecked(explained(
            answered.status,
            http.quoted(&answered.body).as_deref(),
        )),
        Err(failed) => Credential::Unchecked(failed.to_string()),
    }
}

enum Credential {
    Accepted,
    Refused,
    Unchecked(String),
}

fn absent_or_refused(work_id: &str, credential: Credential) -> JiraError {
    match credential {
        Credential::Refused => JiraError::Unauthorized { status: 401 },
        Credential::Accepted => JiraError::Absent {
            key: work_id.to_string(),
        },
        Credential::Unchecked(why) => JiraError::AbsentOrRefused {
            key: work_id.to_string(),
            why,
        },
    }
}

pub(crate) fn failure_for(status: u16, work_id: &str, quoted: Option<&str>) -> JiraError {
    match status {
        401 => JiraError::Unauthorized { status },
        403 => JiraError::Forbidden { status },
        404 => JiraError::Absent {
            key: work_id.to_string(),
        },
        429 => JiraError::RateLimited(explained(status, quoted)),
        500..=599 => JiraError::Unreachable(explained(status, quoted)),
        _ => JiraError::Malformed(explained(status, quoted)),
    }
}

fn explained(status: u16, quoted: Option<&str>) -> String {
    match quoted {
        Some(spoken) => format!("HTTP {status}: {spoken}"),
        None => format!("HTTP {status}"),
    }
}

#[async_trait::async_trait]
impl WorkItemPort for JiraWorkItemPort {
    async fn observe(
        &self,
        work_id: &str,
        cancel: &CancellationToken,
    ) -> Observation<WorkItemState> {
        let source = self.source(work_id);
        let issue = match self.read(work_id, cancel).await {
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

pub(crate) fn named(held: &serde_json::Value, path: &str) -> Result<String, JiraError> {
    match held.as_str() {
        Some(held) => Ok(held.to_string()),
        None => Err(JiraError::Malformed(format!("no `{path}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(error: &JiraError) -> &'static str {
        match error {
            JiraError::Unauthorized { .. } => "unauthorized",
            JiraError::Forbidden { .. } => "forbidden",
            JiraError::Absent { .. } => "absent",
            JiraError::AbsentOrRefused { .. } => "absent or refused",
            JiraError::RateLimited(_) => "rate limited",
            JiraError::Malformed(_) => "malformed",
            JiraError::Unreachable(_) => "unreachable",
            JiraError::Unconfigured => "unconfigured",
            JiraError::Ambiguous { .. } => "ambiguous",
            JiraError::Claimed { .. } => "claimed",
            JiraError::NotSent(_) => "not sent",
        }
    }

    #[test]
    fn each_status_the_port_cannot_read_reaches_the_failure_that_names_it() {
        let cases = [
            (
                401,
                "unauthorized",
                "the site refused the credential with 401",
            ),
            (
                403,
                "forbidden",
                "the credential may not read this issue: 403",
            ),
            (404, "absent", "the site holds no issue `IDENT-1`"),
            (
                429,
                "rate limited",
                "the site limited this request and it can be sent again: HTTP 429",
            ),
            (
                500,
                "unreachable",
                "the site could not be reached: HTTP 500",
            ),
            (
                502,
                "unreachable",
                "the site could not be reached: HTTP 502",
            ),
            (
                503,
                "unreachable",
                "the site could not be reached: HTTP 503",
            ),
            (
                599,
                "unreachable",
                "the site could not be reached: HTTP 599",
            ),
            (
                400,
                "malformed",
                "the site answered with something that is not an issue: HTTP 400",
            ),
            (
                405,
                "malformed",
                "the site answered with something that is not an issue: HTTP 405",
            ),
            (
                428,
                "malformed",
                "the site answered with something that is not an issue: HTTP 428",
            ),
            (
                499,
                "malformed",
                "the site answered with something that is not an issue: HTTP 499",
            ),
            (
                600,
                "malformed",
                "the site answered with something that is not an issue: HTTP 600",
            ),
        ];

        for (status, named, expected) in cases {
            let failure = failure_for(status, "IDENT-1", None);
            assert_eq!(
                variant(&failure),
                named,
                "HTTP {status} must reach the {named} failure, not the {} one",
                variant(&failure)
            );
            assert_eq!(
                format!("{failure}"),
                expected,
                "HTTP {status} must read as the {named} failure reads"
            );
        }
    }

    #[test]
    fn a_status_the_site_uses_for_two_causes_is_named_by_what_the_credential_check_answered() {
        let cases = [
            (
                Credential::Refused,
                "unauthorized",
                "the site refused the credential with 401",
            ),
            (
                Credential::Accepted,
                "absent",
                "the site holds no issue `IDENT-1`",
            ),
            (
                Credential::Unchecked("HTTP 503".to_string()),
                "absent or refused",
                "the site holds no issue `IDENT-1`, or it refused the credential, and \
                 `/rest/api/3/myself` could not say which: HTTP 503",
            ),
        ];

        for (credential, named, expected) in cases {
            let failure = absent_or_refused("IDENT-1", credential);
            assert_eq!(
                variant(&failure),
                named,
                "the credential check must reach the {named} failure, not the {} one",
                variant(&failure)
            );
            assert_eq!(
                format!("{failure}"),
                expected,
                "a 404 the credential check explained must read as the {named} failure reads"
            );
        }
    }

    #[test]
    fn a_credential_that_was_not_checked_says_so_and_never_reads_as_a_settled_absence() {
        let unchecked = absent_or_refused(
            "IDENT-1",
            Credential::Unchecked("the site could not be reached".to_string()),
        );
        let settled = absent_or_refused("IDENT-1", Credential::Accepted);

        assert_ne!(
            format!("{unchecked}"),
            format!("{settled}"),
            "an unchecked credential must not read as a checked one, or a probe that failed \
             reports an absence nothing established"
        );
        assert!(
            format!("{unchecked}").contains(MYSELF),
            "the reason must name the endpoint that settles it: {unchecked}"
        );
    }

    #[test]
    fn an_outage_and_a_rate_limit_carry_the_sites_words_and_the_status_alone_without_them() {
        for status in [429, 500, 503, 400] {
            let quoted = failure_for(status, "IDENT-1", Some("the site said this"));
            let silent = failure_for(status, "IDENT-1", None);
            assert!(
                format!("{quoted}").ends_with(&format!("HTTP {status}: the site said this")),
                "the site's own words must reach the {} failure a {status} names: {quoted}",
                variant(&quoted)
            );
            assert!(
                format!("{silent}").ends_with(&format!("HTTP {status}")),
                "a site that supplies no words leaves the status as the whole reason: {silent}"
            );
        }
    }

    #[test]
    fn a_refused_credential_and_an_absent_issue_quote_no_body_so_their_words_are_unchanged() {
        let planted = "a body no refusal may quote";
        for (status, expected) in [
            (401, "the site refused the credential with 401"),
            (403, "the credential may not read this issue: 403"),
            (404, "the site holds no issue `IDENT-1`"),
        ] {
            let failure = failure_for(status, "IDENT-1", Some(planted));
            assert_eq!(
                format!("{failure}"),
                expected,
                "a {status} says only what its status means, so this change reaches none of its words"
            );
        }
    }
}
