use crate::effect::{
    required, AuthorizedEffect, EffectContext, EffectError, Executor, FromStepParams,
    IntegrationOperation, ObservedState, StepParams,
};
use crate::github::comments::has_a_next_page;
use crate::github::{encode, GhCli, GhError};
use fiddle_core::{
    content_digest, EffectName, HumanDecisionRequirement, ENSURE_PULL_REQUEST,
    ENSURE_PULL_REQUEST_BODY,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequest {
    pub number: u64,
    pub head: String,
    pub base: String,
    pub title: String,
    pub labels: Vec<String>,
}

impl ObservedState for PullRequest {
    type Value = PullRequest;

    fn describe(&self) -> String {
        format!(
            "pull request #{} from {} into {} ({:?})",
            self.number, self.head, self.base, self.title
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.number.to_string())
    }

    fn into_value(self) -> PullRequest {
        self
    }
}

pub fn pull_request_target(repo: &str, head_owner: &str, head: &str, base: &str) -> String {
    format!("{repo}/pulls/{base}...{head_owner}:{head}")
}

pub struct EnsurePullRequest {
    repo: String,
    head_owner: String,
    head: String,
    base: String,
    title: String,
    body: String,
    draft: bool,
    labels: Vec<String>,
}

impl EnsurePullRequest {
    pub fn new(
        repo: String,
        head_owner: String,
        head: String,
        base: String,
        title: String,
        body: String,
        draft: bool,
    ) -> Self {
        Self {
            repo,
            head_owner,
            head,
            base,
            title,
            body,
            draft,
            labels: Vec::new(),
        }
    }

    pub fn labelled(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    fn draft_key(&self) -> Option<(String, serde_json::Value)> {
        self.draft
            .then(|| ("draft".to_string(), serde_json::Value::Bool(true)))
    }

    fn labels_key(&self) -> Option<(String, serde_json::Value)> {
        (!self.labels.is_empty()).then(|| ("labels".to_string(), self.labels.clone().into()))
    }

    fn labels_path(&self, pr: u64) -> String {
        format!("/repos/{}/issues/{pr}/labels", self.repo)
    }

    pub fn target(&self) -> String {
        pull_request_target(&self.repo, &self.head_owner, &self.head, &self.base)
    }

    fn head_label(&self) -> String {
        format!("{}:{}", self.head_owner, self.head)
    }

    fn lookup_path(&self) -> String {
        format!(
            "/repos/{}/pulls?head={}&base={}&state=open",
            self.repo,
            encode(&self.head_label()),
            encode(&self.base),
        )
    }

    fn read(&self, listed: &serde_json::Value) -> Result<PullRequest, GhError> {
        let field = |value: Option<&str>, name: &str| {
            value.map(str::to_string).ok_or_else(|| {
                GhError::Malformed(format!("a listed pull request carried no {name}"))
            })
        };
        let number = listed["number"].as_u64().ok_or_else(|| {
            GhError::Malformed("a listed pull request carried no number".to_string())
        })?;
        let head = field(listed["head"]["label"].as_str(), "head label")?;
        let base = field(listed["base"]["ref"].as_str(), "base ref")?;

        if head != self.head_label() || base != self.base {
            return Err(GhError::Malformed(format!(
                "asked for {}...{} and was answered #{number}, which is {}...{}",
                self.base,
                self.head_label(),
                base,
                head
            )));
        }

        Ok(PullRequest {
            number,
            head,
            base,
            title: listed["title"].as_str().unwrap_or_default().to_string(),
            labels: label_names(listed),
        })
    }

    fn carries_the_labels(&self, observed: &PullRequest) -> bool {
        self.labels
            .iter()
            .all(|wanted| observed.labels.contains(wanted))
    }
}

fn label_names(listed: &serde_json::Value) -> Vec<String> {
    listed["labels"]
        .as_array()
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl FromStepParams for EnsurePullRequest {
    fn from_params(_executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(ENSURE_PULL_REQUEST);
        Ok(Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.head_owner, &kind, "head_owner")?,
            required(&params.branch, &kind, "branch")?,
            required(&params.base, &kind, "base")?,
            required(&params.title, &kind, "title")?,
            required(&params.body, &kind, "body")?,
            params.draft,
        ))
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsurePullRequest {
    type State = PullRequest;

    type Error = GhError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(ENSURE_PULL_REQUEST)
    }

    fn target(&self) -> String {
        EnsurePullRequest::target(self)
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        let mut request = serde_json::Map::from_iter([
            ("base".to_string(), self.base.clone().into()),
            ("body".to_string(), self.body.clone().into()),
            ("head".to_string(), self.head_label().into()),
            ("repo".to_string(), self.repo.clone().into()),
            ("title".to_string(), self.title.clone().into()),
        ]);
        request.extend(self.draft_key());
        request.extend(self.labels_key());
        serde_json::Value::Object(request).to_string()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<PullRequest>, GhError> {
        let response = ctx
            .gh_client()?
            .api("GET", &self.lookup_path(), None, &ctx.cancel)
            .await?;

        let listed = response.body.as_array().ok_or_else(|| {
            GhError::Malformed(format!(
                "{} answered {} with something that is not a list",
                self.lookup_path(),
                response.status
            ))
        })?;

        match listed.as_slice() {
            [] => Ok(None),
            [one] => self
                .read(one)
                .map(|found| self.carries_the_labels(&found).then_some(found)),
            many => Err(GhError::Duplicate { count: many.len() }),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        let mut body = serde_json::Map::from_iter([
            ("title".to_string(), self.title.clone().into()),
            ("body".to_string(), self.body.clone().into()),
            ("head".to_string(), self.head_label().into()),
            ("base".to_string(), self.base.clone().into()),
        ]);
        body.extend(self.draft_key());
        let created = ctx
            .gh_client()?
            .api(
                "POST",
                &format!("/repos/{}/pulls", self.repo),
                Some(&serde_json::Value::Object(body)),
                &ctx.cancel,
            )
            .await?;

        if self.labels.is_empty() {
            return Ok(());
        }

        let number = created.body["number"].as_u64().ok_or_else(|| {
            GhError::Malformed(format!(
                "the create answered {} with no pull request number, so the \
                 {:?} it must carry cannot be addressed",
                created.status, self.labels
            ))
        })?;

        ctx.gh_client()?
            .api(
                "POST",
                &self.labels_path(number),
                Some(&serde_json::json!({ "labels": self.labels })),
                &ctx.cancel,
            )
            .await
            .map(|_response| ())
    }
}

const SEARCH_PAGE: u32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedPullRequest {
    pub number: u64,
    pub head: String,
    pub head_sha: String,
    pub base: String,
    pub title: String,
    pub duplicates: Vec<u64>,
}

pub async fn find_labelled_pull_request(
    gh: &GhCli,
    repo: &str,
    label: &str,
    cancel: &CancellationToken,
) -> Result<Option<SharedPullRequest>, GhError> {
    let path = format!(
        "/repos/{repo}/issues?labels={}&state=open&per_page={SEARCH_PAGE}",
        encode(label)
    );
    let response = gh.api("GET", &path, None, cancel).await?;

    if has_a_next_page(response.link.as_deref()) {
        return Err(GhError::Malformed(format!(
            "{path} answered more than one page, so no pull request on it can be \
             called the lowest"
        )));
    }

    let listed = response.body.as_array().ok_or_else(|| {
        GhError::Malformed(format!(
            "{path} answered {} with something that is not a list",
            response.status
        ))
    })?;

    let mut numbers: Vec<u64> = listed
        .iter()
        .filter(|it| it.get("pull_request").is_some_and(|it| !it.is_null()))
        .filter(|it| label_names(it).iter().any(|name| name == label))
        .filter(|it| it["state"].as_str() == Some("open"))
        .filter_map(|it| it["number"].as_u64())
        .collect();
    numbers.sort_unstable();

    let Some((&number, duplicates)) = numbers.split_first() else {
        return Ok(None);
    };

    let pull_request = gh
        .api(
            "GET",
            &format!("/repos/{repo}/pulls/{number}"),
            None,
            cancel,
        )
        .await?;
    let head = pull_request.body["head"]["ref"]
        .as_str()
        .ok_or_else(|| GhError::Malformed(format!("pull request #{number} carried no head ref")))?;
    let head_sha = pull_request.body["head"]["sha"]
        .as_str()
        .filter(|sha| !sha.trim().is_empty())
        .ok_or_else(|| GhError::Malformed(format!("pull request #{number} carried no head sha")))?;
    let base = pull_request.body["base"]["ref"]
        .as_str()
        .ok_or_else(|| GhError::Malformed(format!("pull request #{number} carried no base ref")))?;

    Ok(Some(SharedPullRequest {
        number,
        head: head.to_string(),
        head_sha: head_sha.to_string(),
        base: base.to_string(),
        title: pull_request.body["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        duplicates: duplicates.to_vec(),
    }))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequestBody {
    pub number: u64,
    pub body: String,
}

impl ObservedState for PullRequestBody {
    type Value = PullRequestBody;

    fn describe(&self) -> String {
        format!(
            "pull request #{} carries the intended body ({} characters)",
            self.number,
            self.body.chars().count()
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.number.to_string())
    }

    fn into_value(self) -> PullRequestBody {
        self
    }
}

pub fn pull_request_body_target(repo: &str, pr: u64, body: &str) -> String {
    format!("{repo}#{pr}@body:{}", content_digest(body))
}

pub struct EnsurePullRequestBody {
    repo: String,
    pr: u64,
    body: String,
}

impl EnsurePullRequestBody {
    pub fn new(repo: String, pr: u64, body: String) -> Self {
        Self { repo, pr, body }
    }

    pub fn target(&self) -> String {
        pull_request_body_target(&self.repo, self.pr, &self.body)
    }

    fn path(&self) -> String {
        format!("/repos/{}/pulls/{}", self.repo, self.pr)
    }

    fn read(&self, response: &serde_json::Value) -> Result<String, GhError> {
        match &response["body"] {
            serde_json::Value::Null if response.get("body").is_some() => Ok(String::new()),
            serde_json::Value::String(body) => Ok(body.clone()),
            _ => Err(GhError::Malformed(format!(
                "{} carried no readable body",
                self.path()
            ))),
        }
    }

    fn request(&self) -> serde_json::Value {
        serde_json::json!({ "body": self.body })
    }

    async fn held(&self, ctx: &EffectContext) -> Result<String, GhError> {
        let response = ctx
            .gh_client()?
            .api("GET", &self.path(), None, &ctx.cancel)
            .await?;
        self.read(&response.body)
    }
}

pub async fn read_pull_request_body(
    ctx: &EffectContext,
    repo: &str,
    pr: u64,
) -> Result<String, GhError> {
    EnsurePullRequestBody::new(repo.to_string(), pr, String::new())
        .held(ctx)
        .await
}

impl FromStepParams for EnsurePullRequestBody {
    fn from_params(_executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(ENSURE_PULL_REQUEST_BODY);
        Ok(Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.pull_request, &kind, "pull_request")?,
            required(&params.body, &kind, "body")?,
        ))
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsurePullRequestBody {
    type State = PullRequestBody;

    type Error = GhError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(ENSURE_PULL_REQUEST_BODY)
    }

    fn target(&self) -> String {
        EnsurePullRequestBody::target(self)
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        serde_json::Value::Object(serde_json::Map::from_iter([
            ("body".to_string(), self.body.clone().into()),
            ("pr".to_string(), self.pr.into()),
            ("repo".to_string(), self.repo.clone().into()),
        ]))
        .to_string()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<PullRequestBody>, GhError> {
        let held = self.held(ctx).await?;

        Ok((held == self.body).then_some(PullRequestBody {
            number: self.pr,
            body: held,
        }))
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        ctx.gh_client()?
            .api("PATCH", &self.path(), Some(&self.request()), &ctx.cancel)
            .await
            .map(|_response| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure(title: &str, base: &str) -> EnsurePullRequest {
        EnsurePullRequest::new(
            "peel/fiddle".to_string(),
            "peel".to_string(),
            "fiddle/abc".to_string(),
            base.to_string(),
            title.to_string(),
            "the body".to_string(),
            false,
        )
    }

    #[test]
    fn the_target_is_the_head_and_base_and_carries_no_title() {
        assert_eq!(
            ensure("a title", "main").target(),
            "peel/fiddle/pulls/main...peel:fiddle/abc"
        );
        assert!(!ensure("a title", "main").target().contains("a title"));
        assert_ne!(
            ensure("a title", "main").target(),
            ensure("a title", "release").target()
        );
    }

    fn rewriting(body: &str) -> EnsurePullRequestBody {
        EnsurePullRequestBody::new("peel/fiddle".to_string(), 7, body.to_string())
    }

    #[test]
    fn the_body_target_carries_a_digest_and_never_the_body() {
        let ours = rewriting("covers 1 CVE");

        assert_eq!(
            ours.target(),
            format!("peel/fiddle#7@body:{}", content_digest("covers 1 CVE"))
        );
        assert!(!ours.target().contains("covers"), "{}", ours.target());
        assert_ne!(ours.target(), rewriting("covers 3 CVEs").target());
        assert_ne!(
            ours.target(),
            EnsurePullRequestBody::new("peel/fiddle".to_string(), 8, "covers 1 CVE".to_string())
                .target()
        );
    }

    #[test]
    fn an_absent_description_is_read_and_an_absent_field_is_refused() {
        let ours = rewriting("covers 1 CVE");

        assert_eq!(
            ours.read(&serde_json::json!({"body": null})).unwrap(),
            "",
            "no description is a description of nothing"
        );
        assert_eq!(
            ours.read(&serde_json::json!({"body": "covers 1 CVE"}))
                .unwrap(),
            "covers 1 CVE"
        );
        assert!(
            ours.read(&serde_json::json!({"number": 7})).is_err(),
            "a response with no body field said nothing about the description"
        );
        assert!(
            ours.read(&serde_json::json!({"body": 4})).is_err(),
            "and neither did one whose description is not text"
        );
    }

    #[test]
    fn the_write_sends_the_body_and_no_other_field() {
        let request = rewriting("covers 1 CVE").request();

        assert_eq!(request, serde_json::json!({"body": "covers 1 CVE"}));
        assert_eq!(
            request.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["body"]
        );
    }

    #[test]
    fn the_read_and_the_write_share_one_path() {
        assert_eq!(rewriting("a").path(), "/repos/peel/fiddle/pulls/7");
    }

    #[test]
    fn a_changed_body_moves_the_payload_that_step_six_checks() {
        use fiddle_core::payload_hash;

        assert_ne!(
            payload_hash(&rewriting("covers 1 CVE").payload()),
            payload_hash(&rewriting("covers 3 CVEs").payload())
        );
        assert_eq!(
            payload_hash(&rewriting("covers 1 CVE").payload()),
            payload_hash(&rewriting("covers 1 CVE").payload()),
            "and the payload is canonical: the same request hashes the same"
        );
    }

    #[test]
    fn a_query_value_is_percent_encoded() {
        assert_eq!(encode("peel:fiddle/abc"), "peel%3Afiddle%2Fabc");
        assert_eq!(encode("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(encode("-._~AZaz09"), "-._~AZaz09");
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SteerablePullRequest {
    pub number: u64,
    pub head_sha: String,
}

pub async fn open_pull_request_on(
    gh: &GhCli,
    repo: &str,
    head_owner: &str,
    head: &str,
    base: &str,
    cancel: &CancellationToken,
) -> Result<Option<SteerablePullRequest>, GhError> {
    let path = format!(
        "/repos/{repo}/pulls?head={}&base={}&state=open",
        encode(&format!("{head_owner}:{head}")),
        encode(base),
    );
    let response = gh.api("GET", &path, None, cancel).await?;
    let listed = response.body.as_array().ok_or_else(|| {
        GhError::Malformed(format!(
            "{path} answered {} with something that is not a list",
            response.status
        ))
    })?;
    let Some(first) = listed.first() else {
        return Ok(None);
    };
    let number = first["number"].as_u64().ok_or_else(|| {
        GhError::Malformed(format!("{path} answered a pull request with no number"))
    })?;
    let Some(head_sha) = first["head"]["sha"].as_str() else {
        return Ok(None);
    };
    Ok(Some(SteerablePullRequest {
        number,
        head_sha: head_sha.to_string(),
    }))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequestFacts {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub open: bool,
    pub merged: bool,
    pub head_ref: String,
    pub head_sha: String,
    pub head_repo: Option<String>,
    pub base_ref: String,
}

impl PullRequestFacts {
    pub fn from_json(path: &str, body: &serde_json::Value) -> Result<Self, GhError> {
        let missing = |what: &str| {
            GhError::Malformed(format!("{path} answered a pull request with no {what}"))
        };
        let text = |value: &serde_json::Value, what: &str| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| missing(what))
        };
        Ok(PullRequestFacts {
            number: body["number"].as_u64().ok_or_else(|| missing("number"))?,
            title: body["title"].as_str().unwrap_or_default().to_string(),
            body: body["body"].as_str().unwrap_or_default().to_string(),
            open: text(&body["state"], "state")? == "open",
            merged: body["merged"].as_bool().unwrap_or(false),
            head_ref: text(&body["head"]["ref"], "head ref")?,
            head_sha: text(&body["head"]["sha"], "head sha")?,
            head_repo: match &body["head"]["repo"] {
                serde_json::Value::Null => None,
                repo => Some(text(&repo["full_name"], "head repository")?),
            },
            base_ref: text(&body["base"]["ref"], "base ref")?,
        })
    }
}

pub async fn read_pull_request(
    gh: &GhCli,
    repo: &str,
    number: u64,
    cancel: &CancellationToken,
) -> Result<PullRequestFacts, GhError> {
    let path = format!("/repos/{repo}/pulls/{number}");
    let response = gh.api("GET", &path, None, cancel).await?;
    PullRequestFacts::from_json(&path, &response.body)
}

#[cfg(test)]
mod steerable {
    use super::*;

    fn listed(value: serde_json::Value) -> Option<SteerablePullRequest> {
        let first = value.as_array().unwrap().first().unwrap().clone();
        let number = first["number"].as_u64()?;
        let head_sha = first["head"]["sha"].as_str()?;
        Some(SteerablePullRequest {
            number,
            head_sha: head_sha.to_string(),
        })
    }

    #[test]
    fn a_pull_request_whose_head_is_gone_steers_nothing_rather_than_failing_the_run() {
        assert_eq!(
            listed(serde_json::json!([{"number": 7, "head": {"sha": null}}])),
            None,
            "no head means no review on it can be shown current, so there is no direction \
             to read and the run proceeds as it did before any steering step existed"
        );
        assert_eq!(
            listed(serde_json::json!([{"number": 7, "head": {"sha": "abc"}}])),
            Some(SteerablePullRequest {
                number: 7,
                head_sha: "abc".to_string(),
            }),
            "and a head that is there is read, so the case above is not the only answer"
        );
    }
}

#[cfg(test)]
mod facts {
    use super::*;

    fn complete() -> serde_json::Value {
        serde_json::json!({
            "number": 7, "title": "T", "body": "b", "state": "open", "merged": false,
            "head": {"ref": "x", "sha": "abc", "repo": {"full_name": "acme/icecube"}},
            "base": {"ref": "main"}
        })
    }

    fn malformed_without(remove: impl FnOnce(&mut serde_json::Value), what: &str) {
        let path = "/repos/acme/icecube/pulls/7";
        assert!(
            PullRequestFacts::from_json(path, &complete()).is_ok(),
            "the complete pull request reads, so the failure below is the missing {what} alone"
        );
        let mut body = complete();
        remove(&mut body);
        match PullRequestFacts::from_json(path, &body) {
            Err(GhError::Malformed(message)) => {
                assert!(
                    message.contains(path) && message.contains(what),
                    "a pull request with no {what} is malformed with a message naming the \
                     path and the {what}, and it said {message:?}"
                );
            }
            other => panic!("a pull request with no {what} is malformed, and it read {other:?}"),
        }
    }

    #[test]
    fn a_pull_request_reads_as_its_facts() {
        let body = serde_json::json!({
            "number": 275, "title": "T", "body": null, "state": "open", "merged": false,
            "head": {"ref": "fiddle/ISP-263", "sha": "abc", "repo": {"full_name": "acme/icecube"}},
            "base": {"ref": "main"}
        });
        let facts = PullRequestFacts::from_json("/repos/acme/icecube/pulls/275", &body).unwrap();
        assert_eq!(
            facts,
            PullRequestFacts {
                number: 275,
                title: "T".into(),
                body: String::new(),
                open: true,
                merged: false,
                head_ref: "fiddle/ISP-263".into(),
                head_sha: "abc".into(),
                head_repo: Some("acme/icecube".into()),
                base_ref: "main".into(),
            },
            "every fact GitHub reports about the pull request is read, and a null body reads as empty"
        );
    }

    #[test]
    fn a_closed_and_merged_pull_request_reads_as_closed_and_merged() {
        let mut body = complete();
        body["state"] = serde_json::json!("closed");
        body["merged"] = serde_json::json!(true);
        let facts = PullRequestFacts::from_json("p", &body).unwrap();
        assert!(
            !facts.open && facts.merged,
            "a closed and merged pull request reads as closed and merged, and it read {facts:?}"
        );
    }

    #[test]
    fn a_pull_request_whose_head_repository_was_deleted_has_none() {
        let body = serde_json::json!({
            "number": 7, "title": "T", "body": "b", "state": "open", "merged": false,
            "head": {"ref": "x", "sha": "abc", "repo": null}, "base": {"ref": "main"}
        });
        assert_eq!(
            PullRequestFacts::from_json("p", &body).unwrap().head_repo,
            None,
            "a head repository GitHub reports as null reads as absent"
        );
        assert_eq!(
            PullRequestFacts::from_json("p", &complete())
                .unwrap()
                .head_repo,
            Some("acme/icecube".to_string()),
            "and a head repository that is there is read, so absent is not the only answer"
        );
        let mut absent = complete();
        absent["head"].as_object_mut().unwrap().remove("repo");
        assert_eq!(
            PullRequestFacts::from_json("p", &absent).unwrap().head_repo,
            None,
            "a head repository GitHub leaves out reads as absent, the same as one it reports as null"
        );
    }

    #[test]
    fn a_head_repository_with_no_full_name_is_malformed_rather_than_deleted() {
        malformed_without(
            |body| {
                body["head"]["repo"]
                    .as_object_mut()
                    .unwrap()
                    .remove("full_name");
            },
            "head repository",
        );
        malformed_without(
            |body| body["head"]["repo"]["full_name"] = serde_json::json!(42),
            "head repository",
        );
    }

    #[test]
    fn a_pull_request_with_no_head_sha_is_malformed() {
        let body = serde_json::json!({"number": 7, "state": "open", "head": {"ref": "x"}, "base": {"ref": "main"}});
        assert!(
            matches!(
                PullRequestFacts::from_json("p", &body),
                Err(GhError::Malformed(_))
            ),
            "a pull request with no head sha is malformed"
        );
        malformed_without(
            |body| body["head"]["sha"] = serde_json::Value::Null,
            "head sha",
        );
    }

    #[test]
    fn a_pull_request_with_no_head_ref_is_malformed() {
        malformed_without(
            |body| {
                body["head"].as_object_mut().unwrap().remove("ref");
            },
            "head ref",
        );
    }

    #[test]
    fn a_pull_request_with_no_base_ref_is_malformed() {
        malformed_without(
            |body| {
                body["base"].as_object_mut().unwrap().remove("ref");
            },
            "base ref",
        );
    }

    #[test]
    fn a_pull_request_with_no_state_is_malformed() {
        malformed_without(
            |body| {
                body.as_object_mut().unwrap().remove("state");
            },
            "state",
        );
    }

    #[test]
    fn a_pull_request_with_no_number_is_malformed() {
        malformed_without(
            |body| {
                body.as_object_mut().unwrap().remove("number");
            },
            "number",
        );
    }
}
