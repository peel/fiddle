use crate::effect::{
    AuthorizedEffect, Effect, EffectContext, EffectError, Executor, FromStepParams, ObservedState,
    StepParams,
};
use crate::jira::JiraError;
use fiddle_core::{EffectName, JIRA_ISSUE_FILED};
use serde_json::{json, Value};

const SEARCH: &str = "/rest/api/3/search/jql";
const CREATE: &str = "/rest/api/3/issue";
const PAGE_WALK_BOUND: usize = 1000;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FiledIssue {
    pub key: String,
    pub marker: String,
}

impl ObservedState for FiledIssue {
    type Value = FiledIssue;

    fn describe(&self) -> String {
        format!("issue {} carries the marker {}", self.key, self.marker)
    }

    fn reference(&self) -> Option<String> {
        Some(self.key.clone())
    }

    fn into_value(self) -> FiledIssue {
        self
    }
}

#[derive(Effect)]
#[effect(
    name = JIRA_ISSUE_FILED,
    minimum = "automatic",
    target = "{project_key}/{marker}",
    state = FiledIssue,
    error = JiraError
)]
pub struct FileVerdict {
    #[payload]
    cve: String,
    #[payload]
    severity: String,
    #[payload]
    package: String,
    #[payload]
    rationale: String,
    #[payload]
    label: String,
    project_key: String,
    marker: String,
}

impl FromStepParams for FileVerdict {
    fn from_params(_executor: &Executor<'_>, _params: &StepParams) -> Result<Self, EffectError> {
        Err(EffectError::Unbuildable {
            kind: EffectName::shipped(JIRA_ISSUE_FILED),
            reason: "a step carries no advisory, no package and no rationale, so this operation \
                     is built by the capability that holds the verdict and never from a step"
                .to_string(),
        })
    }
}

impl FileVerdict {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cve: String,
        severity: String,
        package: String,
        rationale: String,
        label: String,
        project_key: String,
        marker: String,
    ) -> Self {
        Self {
            cve,
            severity,
            package,
            rationale,
            label,
            project_key,
            marker,
        }
    }

    pub fn marker(&self) -> &str {
        &self.marker
    }

    fn jql(&self) -> String {
        format!(
            "project = {} AND labels = {}",
            self.project_key, self.marker
        )
    }

    fn summary(&self) -> String {
        format!("{}: {} in {}", self.severity, self.cve, self.package)
    }

    fn body(&self) -> Value {
        json!({
            "fields": {
                "project": {"key": self.project_key},
                "summary": self.summary(),
                "labels": [self.label.clone(), self.marker.clone()],
                "description": described(&self.rationale),
            }
        })
    }

    async fn every_issue_carrying_the_marker(
        &self,
        ctx: &EffectContext,
    ) -> Result<Vec<FiledIssue>, JiraError> {
        let client = ctx.jira_client()?;
        let jql = self.jql();
        let mut found = Vec::new();
        let mut token: Option<String> = None;

        for _ in 0..PAGE_WALK_BOUND {
            let answered = client
                .api(
                    "GET",
                    &search_path(&jql, token.as_deref()),
                    None,
                    &ctx.cancel,
                )
                .await?;
            if !(200..300).contains(&answered.status) {
                return Err(refused(
                    answered.status,
                    client.quoted(&answered.body).as_deref(),
                ));
            }
            let page = answered.body["issues"].as_array().ok_or_else(|| {
                JiraError::Malformed(format!(
                    "a search for `{jql}` answered {} and no `issues` array",
                    answered.status
                ))
            })?;
            for issue in page {
                found.push(self.filed(issue)?);
            }
            match answered.body["nextPageToken"].as_str() {
                None => return Ok(found),
                Some(next) => token = Some(next.to_string()),
            }
        }

        Err(JiraError::Malformed(format!(
            "a search for `{jql}` offered a further page after {PAGE_WALK_BOUND} of them, and a \
             count taken from part of a result is a floor and never a total"
        )))
    }

    fn filed(&self, issue: &Value) -> Result<FiledIssue, JiraError> {
        match issue["key"].as_str() {
            Some(key) => Ok(FiledIssue {
                key: key.to_string(),
                marker: self.marker.clone(),
            }),
            None => Err(JiraError::Malformed(format!(
                "a search for `{}` answered an issue with no `key`",
                self.jql()
            ))),
        }
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<FiledIssue>, JiraError> {
        let mut found = self.every_issue_carrying_the_marker(ctx).await?;
        match found.len() {
            0 => Ok(None),
            1 => Ok(found.pop()),
            count => Err(JiraError::Ambiguous {
                marker: self.marker.clone(),
                count,
            }),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), JiraError> {
        let client = ctx.jira_client()?;
        let answered = client
            .api("POST", CREATE, Some(&self.body()), &ctx.cancel)
            .await?;
        match answered.status {
            201 => Ok(()),
            status => Err(refused(status, client.quoted(&answered.body).as_deref())),
        }
    }
}

fn search_path(jql: &str, token: Option<&str>) -> String {
    let mut path = format!("{SEARCH}?jql={}", encoded(jql));
    if let Some(token) = token {
        path.push_str(&format!("&nextPageToken={}", encoded(token)));
    }
    path
}

fn encoded(raw: &str) -> String {
    let mut written = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                written.push(*byte as char)
            }
            byte => written.push_str(&format!("%{byte:02X}")),
        }
    }
    written
}

fn described(rationale: &str) -> Value {
    json!({
        "type": "doc",
        "version": 1,
        "content": [
            {"type": "paragraph", "content": [{"type": "text", "text": rationale}]}
        ]
    })
}

fn refused(status: u16, quoted: Option<&str>) -> JiraError {
    let said = match quoted {
        Some(spoken) => format!("HTTP {status}: {spoken}"),
        None => format!("HTTP {status}"),
    };
    match status {
        401 => JiraError::Unauthorized { status },
        403 => JiraError::Forbidden { status },
        429 => JiraError::RateLimited(said),
        500..=599 => JiraError::Unreachable(said),
        _ => JiraError::Malformed(said),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::IntegrationOperation;

    fn verdict() -> FileVerdict {
        FileVerdict::new(
            "CVE-2025-1".to_string(),
            "high".to_string(),
            "acme-parser".to_string(),
            "the advisory reaches this build".to_string(),
            "security".to_string(),
            "IDENT".to_string(),
            "fx-abc123".to_string(),
        )
    }

    #[test]
    fn the_target_names_the_project_and_the_marker_the_search_selects_on() {
        assert_eq!(IntegrationOperation::target(&verdict()), "IDENT/fx-abc123");
    }

    #[test]
    fn the_payload_carries_the_words_a_reader_judges_and_never_the_marker() {
        let said = IntegrationOperation::payload(&verdict());
        assert_eq!(
            said,
            r#"{"cve":"CVE-2025-1","label":"security","package":"acme-parser","rationale":"the advisory reaches this build","severity":"high"}"#,
            "the derive writes the payload from the marked fields alone"
        );
        assert!(
            !said.contains("fx-abc123"),
            "the marker is identity and not payload, so re-filing the same verdict under a new \
             marker is a new effect and not a diverged payload: {said}"
        );
    }

    #[test]
    fn the_search_selects_on_the_project_and_the_marker_together() {
        assert_eq!(verdict().jql(), "project = IDENT AND labels = fx-abc123");
    }

    #[test]
    fn a_query_reaches_the_site_percent_encoded_so_no_space_ends_the_request_line() {
        let path = search_path("project = IDENT AND labels = fx-abc123", None);
        assert_eq!(
            path,
            "/rest/api/3/search/jql?jql=project%20%3D%20IDENT%20AND%20labels%20%3D%20fx-abc123"
        );
        assert!(
            !path.contains(' '),
            "a space in a request line ends the target, so the site would read a different \
             query from the one asked: {path}"
        );
    }

    #[test]
    fn a_page_token_rides_the_next_request_beside_the_same_query() {
        let path = search_path("labels = fx-abc123", Some("tok-1"));
        assert_eq!(
            path,
            "/rest/api/3/search/jql?jql=labels%20%3D%20fx-abc123&nextPageToken=tok-1"
        );
    }

    #[test]
    fn the_marker_and_the_label_both_ride_the_create_and_no_later_edit_carries_them() {
        let body = verdict().body();
        assert_eq!(body["fields"]["labels"], json!(["security", "fx-abc123"]));
        assert_eq!(body["fields"]["project"]["key"], "IDENT");
        assert_eq!(body["fields"]["summary"], "high: CVE-2025-1 in acme-parser");
        assert_eq!(
            body["fields"]["description"]["content"][0]["content"][0]["text"],
            "the advisory reaches this build",
            "the rationale reaches a person as a document the site renders"
        );
    }

    #[test]
    fn each_status_a_write_or_a_search_cannot_read_reaches_the_failure_that_names_it() {
        let cases = [
            (401, "the site refused the credential with 401"),
            (403, "the credential may not read this issue: 403"),
            (
                429,
                "the site limited this request and it can be sent again: HTTP 429",
            ),
            (500, "the site could not be reached: HTTP 500"),
            (503, "the site could not be reached: HTTP 503"),
            (
                400,
                "the site answered with something that is not an issue: HTTP 400",
            ),
            (
                404,
                "the site answered with something that is not an issue: HTTP 404",
            ),
        ];
        for (status, expected) in cases {
            assert_eq!(
                format!("{}", refused(status, None)),
                expected,
                "HTTP {status} must read as the failure that names it"
            );
        }
    }

    #[test]
    fn an_issue_a_search_answered_without_a_key_is_refused_and_never_counted() {
        let verdict = verdict();
        assert!(
            verdict
                .filed(&json!({"fields": {"labels": ["fx-abc123"]}}))
                .is_err(),
            "an answer with no key cannot be a receipt's external reference, so counting it \
             would report a match nothing can be read back from"
        );
        assert_eq!(
            verdict
                .filed(&json!({"key": "IDENT-7"}))
                .expect("an answered key is read")
                .key,
            "IDENT-7"
        );
    }
}
