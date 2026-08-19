use crate::github::{GhCli, GhError};
use fiddle_core::ActorRef;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const PER_PAGE: u32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanResponse {
    pub comment: u64,
    pub author: ActorRef,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_bot: bool,
    pub author_association: String,
}

#[derive(Debug, Deserialize)]
struct ListedComment {
    id: u64,
    body: String,
    created_at: String,
    updated_at: String,
    author_association: String,
    user: ListedUser,
    performed_via_github_app: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ListedUser {
    login: String,
    id: u64,
    #[serde(rename = "type")]
    kind: String,
}

impl From<ListedComment> for HumanResponse {
    fn from(listed: ListedComment) -> Self {
        Self {
            comment: listed.id,
            is_bot: listed.user.kind == "Bot" || !listed.performed_via_github_app.is_null(),
            author: ActorRef {
                login: listed.user.login,
                id: listed.user.id,
            },
            body: listed.body,
            created_at: listed.created_at,
            updated_at: listed.updated_at,
            author_association: listed.author_association,
        }
    }
}

pub async fn read_conversation(
    gh: &GhCli,
    repo: &str,
    pr: u64,
    max_pages: u32,
    cancel: &CancellationToken,
) -> Result<Vec<HumanResponse>, GhError> {
    let mut conversation = Vec::new();
    for page in 1..=max_pages {
        let path = format!("/repos/{repo}/issues/{pr}/comments?per_page={PER_PAGE}&page={page}");
        let response = gh.api("GET", &path, None, cancel).await?;
        let more = has_a_next_page(response.link.as_deref());
        let listed: Vec<ListedComment> =
            serde_json::from_value(response.body).map_err(|error| {
                GhError::Malformed(format!(
                    "{path} answered something that is not a list of comments: {error}"
                ))
            })?;
        conversation.extend(listed.into_iter().map(HumanResponse::from));
        if !more {
            return Ok(conversation);
        }
    }
    Err(GhError::Malformed(format!(
        "the conversation of {repo}#{pr} runs to more than {max_pages} pages and was not read"
    )))
}

pub async fn read_one_comment(
    gh: &GhCli,
    repo: &str,
    comment: u64,
    cancel: &CancellationToken,
) -> Result<HumanResponse, GhError> {
    let path = format!("/repos/{repo}/issues/comments/{comment}");
    let response = gh.api("GET", &path, None, cancel).await?;
    let listed: ListedComment = serde_json::from_value(response.body).map_err(|error| {
        GhError::Malformed(format!("{path} answered no readable comment: {error}"))
    })?;
    Ok(listed.into())
}

pub(crate) fn has_a_next_page(link: Option<&str>) -> bool {
    let header = link.unwrap_or_default().trim();
    if header.is_empty() {
        return false;
    }
    let mut read_a_link_value = false;
    for segment in header.split(',') {
        let parameters = match segment.rsplit_once('>') {
            Some((_target, parameters)) => {
                read_a_link_value = true;
                parameters
            }
            None => segment,
        };
        if parameters.split(';').any(is_a_next_relation) {
            return true;
        }
    }
    !read_a_link_value
}

fn is_a_next_relation(parameter: &str) -> bool {
    let Some((name, value)) = parameter.split_once('=') else {
        return false;
    };
    if !name.trim().eq_ignore_ascii_case("rel") {
        return false;
    }
    value
        .trim()
        .trim_matches('"')
        .split_whitespace()
        .any(|relation| relation.eq_ignore_ascii_case("next"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_next_relation_is_a_further_page() {
        assert!(has_a_next_page(Some(
            "<https://api.github.com/repositories/1/issues/7/comments?page=2>; rel=\"next\", \
             <https://api.github.com/repositories/1/issues/7/comments?page=9>; rel=\"last\""
        )));
    }

    #[test]
    fn a_last_page_still_carries_a_link_and_is_still_the_end() {
        assert!(!has_a_next_page(Some(
            "<https://api.github.com/repositories/1/issues/7/comments?page=1>; rel=\"first\", \
             <https://api.github.com/repositories/1/issues/7/comments?page=8>; rel=\"prev\""
        )));
        assert!(!has_a_next_page(None));
    }

    #[test]
    fn a_url_that_reads_like_a_relation_is_not_one() {
        assert!(!has_a_next_page(Some(
            "<https://api.github.com/x?cursor=rel%3D%22next%22&z=;rel=\"next\">; rel=\"prev\""
        )));
    }

    #[test]
    fn every_legal_spelling_of_next_is_a_further_page() {
        for header in [
            r#"<https://api.github.com/x?page=2>; rel="next last""#,
            r#"<https://api.github.com/x?page=2>; rel = "next""#,
            r#"<https://api.github.com/x?page=2>; rel=next"#,
            r#"<https://api.github.com/x?page=2>; rel="NEXT""#,
            r#"<https://api.github.com/x?page=2>; type="application/json"; rel="next""#,
            r#"<https://api.github.com/x?page=1>; rel="first", <https://api.github.com/x?page=2>; rel="prev next""#,
        ] {
            assert!(has_a_next_page(Some(header)), "{header:?} read as an end");
        }
    }

    #[test]
    fn a_relation_named_somewhere_else_is_not_a_further_page() {
        for header in [
            r#"<https://api.github.com/x?page=2>; title="rel=next""#,
            r#"<https://api.github.com/x/next?page=2>; rel="prev""#,
            r#"<https://api.github.com/x?page=2>; rel="nextish""#,
            r#"<https://api.github.com/x?page=2>; relation="next""#,
            r#"<https://api.github.com/x?page=2>; rel="prev first""#,
        ] {
            assert!(
                !has_a_next_page(Some(header)),
                "{header:?} read as a further page"
            );
        }
    }

    #[test]
    fn a_header_that_cannot_be_read_is_not_an_end() {
        assert!(has_a_next_page(Some("something else entirely")));
        assert!(has_a_next_page(Some("https://api.github.com/x?page=2")));
        assert!(!has_a_next_page(None));
        assert!(!has_a_next_page(Some("   ")));
    }
}
