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
            .gh
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
            .gh
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

        ctx.gh
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
        let response = ctx.gh.api("GET", &self.path(), None, &ctx.cancel).await?;
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
        ctx.gh
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
