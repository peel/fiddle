use crate::github::{HumanResponse, Reviewed};

pub fn mentions(text: &str, handle: &str) -> bool {
    if handle.is_empty() {
        return false;
    }
    text.char_indices()
        .filter(|(_, it)| *it == '@')
        .any(|(at, _)| opens_a_mention(text, at) && names_the_handle(&text[at + 1..], handle))
}

fn opens_a_mention(text: &str, at: usize) -> bool {
    match text[..at].chars().next_back() {
        None => true,
        Some(before) => !(before.is_alphanumeric() || before == '`' || before == '@'),
    }
}

fn names_the_handle(rest: &str, handle: &str) -> bool {
    let Some(named) = rest.get(..handle.len()) else {
        return false;
    };
    if !named.eq_ignore_ascii_case(handle) {
        return false;
    }
    match rest[handle.len()..].chars().next() {
        None => true,
        Some(after) => !(after.is_alphanumeric() || after == '-' || after == '_'),
    }
}

pub fn mentioning(
    reviews: Vec<Reviewed>,
    conversation: Vec<HumanResponse>,
    handle: &str,
) -> (Vec<Reviewed>, Vec<HumanResponse>) {
    (
        reviews
            .into_iter()
            .filter(|it| mentions(&it.body, handle))
            .collect(),
        conversation
            .into_iter()
            .filter(|it| mentions(&it.body, handle))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::ActorRef;

    fn reviewed(id: u64, association: &str, body: &str) -> Reviewed {
        Reviewed {
            id,
            submitted_at: None,
            author: ActorRef {
                login: "peel".to_string(),
                id: 1,
            },
            author_association: association.to_string(),
            state: "COMMENTED".to_string(),
            body: body.to_string(),
            commit_id: "3acb655".to_string(),
        }
    }

    fn said(id: u64, association: &str, body: &str) -> HumanResponse {
        HumanResponse {
            comment: id,
            author: ActorRef {
                login: "peel".to_string(),
                id: 1,
            },
            body: body.to_string(),
            created_at: "2026-10-08T12:00:00Z".to_string(),
            updated_at: "2026-10-08T12:00:00Z".to_string(),
            is_bot: false,
            author_association: association.to_string(),
        }
    }

    #[test]
    fn a_mention_is_the_handle_as_a_word() {
        assert!(mentions("@fiddle-ai fix the CI failure", "fiddle-ai"));
        assert!(mentions(
            "please, @Fiddle-AI: do comments 1 and 2",
            "fiddle-ai"
        ));
        assert!(mentions("cc @fiddle-ai.", "fiddle-ai"));
        assert!(!mentions("fiddle-ai fix it", "fiddle-ai"));
        assert!(!mentions("@fiddle-ai-bot fix it", "fiddle-ai"));
        assert!(!mentions("mail me at x@fiddle-ai", "fiddle-ai"));
        assert!(!mentions("`@fiddle-ai` is how you call it", "fiddle-ai"));
    }

    #[test]
    fn a_mention_ends_at_the_text_and_starts_after_punctuation_or_a_line_break() {
        assert!(
            mentions("thanks @fiddle-ai", "fiddle-ai"),
            "a mention at the end of the text is a mention"
        );
        assert!(
            mentions("(@fiddle-ai) rename it", "fiddle-ai"),
            "a mention after an opening parenthesis is a mention"
        );
        assert!(
            mentions("first line\n@FIDDLE-ai second", "fiddle-ai"),
            "a mention that opens a line is a mention, in any case"
        );
        assert!(
            !mentions("@fiddle-ai_two rename it", "fiddle-ai"),
            "an underscore continues the handle, so this is another handle"
        );
        assert!(
            !mentions("@@fiddle-ai rename it", "fiddle-ai"),
            "an at sign before the at sign is not a mention"
        );
        assert!(
            !mentions("@fiddle rename it", "fiddle-ai"),
            "a shorter handle is not the handle"
        );
    }

    #[test]
    fn a_later_mention_counts_when_an_earlier_one_is_inside_code() {
        assert!(
            mentions("call it as `@fiddle-ai`, so @fiddle-ai do it", "fiddle-ai"),
            "the second occurrence is a word, so the text mentions the handle"
        );
        assert!(
            !mentions("call it as `@fiddle-ai` or x@fiddle-ai", "fiddle-ai"),
            "no occurrence is a word, so the text does not mention the handle"
        );
    }

    #[test]
    fn an_empty_handle_is_never_mentioned() {
        assert!(
            !mentions("@ fix it", ""),
            "a bare at sign does not mention an empty handle"
        );
        assert!(
            mentions("@ fix it @fiddle-ai", "fiddle-ai"),
            "the same text with a handle after it does mention that handle"
        );
    }

    #[test]
    fn only_mentions_are_kept_from_reviews_and_the_conversation() {
        let (reviews, conversation) = mentioning(
            vec![
                reviewed(1, "MEMBER", "looks good"),
                reviewed(2, "MEMBER", "@fiddle-ai rename it"),
            ],
            vec![
                said(3, "MEMBER", "agreed"),
                said(4, "MEMBER", "@fiddle-ai fix CI"),
            ],
            "fiddle-ai",
        );
        assert_eq!(reviews.iter().map(|it| it.id).collect::<Vec<_>>(), vec![2]);
        assert_eq!(
            conversation.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![4]
        );
    }
}
