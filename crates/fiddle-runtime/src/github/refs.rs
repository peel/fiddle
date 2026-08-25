use crate::effect::{
    required, AuthorizedEffect, Effect, EffectContext, EffectError, Executor, FromStepParams,
    ObservedState, StepParams,
};
use crate::git::PublishedBranch;
use crate::github::GhError;
use fiddle_core::{effect_id, EffectName, ENSURE_BRANCH_PUBLISHED};

const NAMESPACE: &str = "fiddle";

pub fn branch_name(project: &str, invocation_ref: &str) -> String {
    let id = effect_id(project, invocation_ref, ENSURE_BRANCH_PUBLISHED, project);
    format!("{NAMESPACE}/{}", id.0)
}

pub fn branch_target(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRef {
    pub branch: String,
    pub sha: String,
}

impl ObservedState for BranchRef {
    type Value = PublishedBranch;

    fn describe(&self) -> String {
        format!("refs/heads/{} points at {}", self.branch, self.sha)
    }

    fn reference(&self) -> Option<String> {
        Some(self.sha.clone())
    }

    fn into_value(self) -> PublishedBranch {
        PublishedBranch {
            branch: self.branch,
            sha: self.sha,
        }
    }
}

#[derive(Effect)]
#[effect(
    name = ENSURE_BRANCH_PUBLISHED,
    minimum = "automatic",
    target = "refs/heads/{branch}",
    state = BranchRef,
    error = GhError
)]
pub struct EnsureBranchPublished {
    #[payload]
    repo: String,
    branch: String,
    #[payload(rename = "sha")]
    intended_sha: String,
}

impl FromStepParams for EnsureBranchPublished {
    fn from_params(_executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(ENSURE_BRANCH_PUBLISHED);
        Ok(Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.branch, &kind, "branch")?,
            required(&params.head_sha, &kind, "head_sha")?,
        ))
    }
}

impl EnsureBranchPublished {
    pub fn new(repo: String, branch: String, intended_sha: String) -> Self {
        Self {
            repo,
            branch,
            intended_sha,
        }
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    fn ref_path(&self) -> String {
        format!("/repos/{}/git/ref/heads/{}", self.repo, self.branch)
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<BranchRef>, GhError> {
        let response = match ctx.gh.api("GET", &self.ref_path(), None, &ctx.cancel).await {
            Ok(response) => response,
            Err(GhError::Http { status: 404, .. }) => return Ok(None),
            Err(error) => return Err(error),
        };

        let sha = response.body["object"]["sha"]
            .as_str()
            .ok_or_else(|| {
                GhError::Malformed(format!(
                    "{} answered {} with no object sha",
                    self.ref_path(),
                    response.status
                ))
            })?
            .to_string();

        match sha == self.intended_sha {
            true => Ok(Some(BranchRef {
                branch: self.branch.clone(),
                sha,
            })),
            false => Ok(None),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        ctx.git
            .publish(&ctx.work, &self.branch, &ctx.cancel)
            .await
            .map(|_published| ())
            .map_err(GhError::Push)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branch_name_is_pinned_to_the_identity_derivation() {
        assert_eq!(
            branch_name("acme/widget", "beans:w-1"),
            "fiddle/6d5aa806964432bc"
        );
    }

    #[test]
    fn the_target_is_the_full_ref() {
        assert_eq!(branch_target("fiddle/abc"), "refs/heads/fiddle/abc");
    }
}
