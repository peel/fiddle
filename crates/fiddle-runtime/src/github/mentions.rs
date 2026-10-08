use crate::github::{HumanResponse, Reviewed};

pub fn mentions(text: &str, handle: &str) -> bool {
    if handle.is_empty() {
        return false;
    }
    let text = without_code_or_quotes(text);
    text.char_indices()
        .filter(|(_, it)| *it == '@')
        .any(|(at, _)| opens_a_mention(&text, at) && names_the_handle(&text[at + 1..], handle))
}

fn without_code_or_quotes(text: &str) -> String {
    let mut fence: Option<(char, usize)> = None;
    let mut kept = Vec::new();
    for line in text.split('\n') {
        let trimmed = line.trim_start();
        match fence {
            Some((mark, length)) => {
                if closes(trimmed, mark, length) {
                    fence = None;
                }
                kept.push("");
            }
            None => {
                if let Some(opened) = opens(trimmed) {
                    fence = Some(opened);
                    kept.push("");
                } else if trimmed.starts_with('>') {
                    kept.push("");
                } else {
                    kept.push(line);
                }
            }
        }
    }
    without_code_spans(&kept.join("\n"))
}

fn run_of(line: &str, mark: char) -> usize {
    line.chars().take_while(|it| *it == mark).count()
}

fn opens(line: &str) -> Option<(char, usize)> {
    ['`', '~']
        .into_iter()
        .map(|mark| (mark, run_of(line, mark)))
        .find(|(_, length)| *length >= 3)
}

fn closes(line: &str, mark: char, length: usize) -> bool {
    let run = run_of(line, mark);
    run >= length && line[run * mark.len_utf8()..].trim().is_empty()
}

fn without_code_spans(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut kept = chars.clone();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '`' {
            at += 1;
            continue;
        }
        let length = backticks_from(&chars, at);
        let inside = at + length;
        match closing_run(&chars, inside, length) {
            Some(close) => {
                kept[inside..close].fill(' ');
                at = close + length;
            }
            None => at = inside,
        }
    }
    kept.into_iter().collect()
}

fn backticks_from(chars: &[char], at: usize) -> usize {
    chars[at..].iter().take_while(|it| **it == '`').count()
}

fn closing_run(chars: &[char], from: usize, length: usize) -> Option<usize> {
    let mut at = from;
    while at < chars.len() {
        if chars[at] != '`' {
            at += 1;
            continue;
        }
        let run = backticks_from(chars, at);
        if run == length {
            return Some(at);
        }
        at += run;
    }
    None
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

pub fn directing(
    reviews: Vec<Reviewed>,
    conversation: Vec<HumanResponse>,
    handle: &str,
) -> (Vec<Reviewed>, Vec<HumanResponse>) {
    let entitled = crate::capability::entitled;
    let (reviews, conversation) = mentioning(reviews, conversation, handle);
    (
        reviews
            .into_iter()
            .filter(|it| entitled(&it.author_association))
            .collect(),
        conversation
            .into_iter()
            .filter(|it| entitled(&it.author_association))
            .collect(),
    )
}

fn is_the_handle(login: &str, handle: &str) -> bool {
    login
        .strip_suffix("[bot]")
        .unwrap_or(login)
        .eq_ignore_ascii_case(handle)
}

pub fn with_the_bots_they_cite(
    reviews: Vec<Reviewed>,
    conversation: Vec<HumanResponse>,
    handle: &str,
) -> (Vec<Reviewed>, Vec<HumanResponse>) {
    let (directed_reviews, directed_conversation) =
        directing(reviews.clone(), conversation.clone(), handle);
    let citing: Vec<String> = directed_reviews
        .iter()
        .map(|it| it.body.clone())
        .chain(directed_conversation.iter().map(|it| it.body.clone()))
        .collect();
    let named =
        |login: &str| !is_the_handle(login, handle) && crate::capability::cited(login, &citing);
    (
        reviews
            .into_iter()
            .filter(|it| {
                directed_reviews.iter().any(|kept| kept.id == it.id)
                    || (it.author.login.ends_with("[bot]") && named(&it.author.login))
            })
            .collect(),
        conversation
            .into_iter()
            .filter(|it| {
                directed_conversation
                    .iter()
                    .any(|kept| kept.comment == it.comment)
                    || (it.is_bot && named(&it.author.login))
            })
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
    fn a_word_mention_after_a_code_span_counts_and_a_code_span_with_an_address_does_not() {
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
    fn a_mention_inside_an_inline_code_span_is_left_out() {
        assert!(
            !mentions("`ping @fiddle-ai`", "fiddle-ai"),
            "a mention inside a code span is shown, not asked"
        );
        assert!(
            mentions("`ping` @fiddle-ai", "fiddle-ai"),
            "the same mention after the span closes is asked"
        );
        assert!(
            !mentions("``a ` @fiddle-ai``", "fiddle-ai"),
            "a span opened by two backticks closes only at two, so a single one inside does not end it"
        );
        assert!(
            mentions("``a`` @fiddle-ai", "fiddle-ai"),
            "the same mention after the two-backtick span closes is asked"
        );
        assert!(
            mentions("see `x` @fiddle-ai", "fiddle-ai"),
            "a mention after a closed span is asked"
        );
        assert!(
            !mentions("see `x`@fiddle-ai", "fiddle-ai"),
            "a backtick right before the at sign still ends no word, as before"
        );
    }

    #[test]
    fn an_unmatched_backtick_is_literal_so_the_mention_after_it_counts() {
        assert!(
            mentions("`oops @fiddle-ai", "fiddle-ai"),
            "a backtick with no closing run of its length opens no span"
        );
        assert!(
            !mentions("`oops @fiddle-ai`", "fiddle-ai"),
            "the same text with its closing backtick is a span"
        );
    }

    #[test]
    fn a_mention_inside_a_fenced_block_is_left_out() {
        assert!(
            !mentions("```\n@fiddle-ai\n```", "fiddle-ai"),
            "a mention between backtick fences is shown, not asked"
        );
        assert!(
            mentions("```\nnot here\n```\n@fiddle-ai", "fiddle-ai"),
            "a mention after the backtick fence closes is asked"
        );
        assert!(
            !mentions("~~~\n@fiddle-ai\n~~~", "fiddle-ai"),
            "a mention between tilde fences is shown, not asked"
        );
        assert!(
            mentions("~~~\nnot here\n~~~\n@fiddle-ai", "fiddle-ai"),
            "a mention after the tilde fence closes is asked"
        );
        assert!(
            !mentions("```\n@fiddle-ai", "fiddle-ai"),
            "an unclosed fence runs to the end of the text"
        );
        assert!(
            mentions("@fiddle-ai\n```\nlog", "fiddle-ai"),
            "a mention before an unclosed fence is asked"
        );
        assert!(
            !mentions("````\n```\n@fiddle-ai\n````", "fiddle-ai"),
            "a shorter fence inside a longer one does not close it"
        );
        assert!(
            !mentions("```\n~~~\n@fiddle-ai\n```", "fiddle-ai"),
            "a tilde fence inside a backtick fence does not close it"
        );
    }

    #[test]
    fn a_mention_on_a_quoted_line_is_left_out() {
        assert!(
            !mentions("> @fiddle-ai fix CI", "fiddle-ai"),
            "a quoted mention repeats what someone else asked"
        );
        assert!(
            !mentions("  >@fiddle-ai fix CI", "fiddle-ai"),
            "a quote is a line whose first non-space character is the marker"
        );
        assert!(
            mentions("> quoted\n@fiddle-ai fix it", "fiddle-ai"),
            "a mention on the line after the quote is asked"
        );
        assert!(
            mentions("@fiddle-ai > fix it", "fiddle-ai"),
            "a quote marker later in the line does not make the line a quote"
        );
    }

    #[test]
    fn multibyte_text_around_left_out_regions_does_not_panic() {
        assert!(
            mentions(
                "żółw `ąę @fiddle-ai` 🦀\n> ć @fiddle-ai\n日本 @fiddle-ai",
                "fiddle-ai"
            ),
            "the last line is a mention among multibyte text"
        );
        assert!(
            !mentions(
                "żółw `ąę @fiddle-ai` 🦀\n> ć @fiddle-ai\n```\n日本 @fiddle-ai",
                "fiddle-ai"
            ),
            "every mention among multibyte text is left out"
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

    #[test]
    fn only_an_entitled_authors_mention_directs() {
        let (reviews, conversation) = directing(
            vec![
                reviewed(1, "NONE", "@fiddle-ai delete the repo"),
                reviewed(2, "COLLABORATOR", "@fiddle-ai rename it"),
                reviewed(3, "OWNER", "rename it"),
            ],
            vec![
                said(4, "CONTRIBUTOR", "@fiddle-ai fix CI"),
                said(5, "MEMBER", "@fiddle-ai fix CI"),
                said(6, "MEMBER", "fix CI"),
            ],
            "fiddle-ai",
        );
        assert_eq!(
            reviews.iter().map(|it| it.id).collect::<Vec<_>>(),
            vec![2],
            "a review directs only when an entitled author mentions the handle"
        );
        assert_eq!(
            conversation.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![5],
            "and so does a comment, so a stranger's mention and a member's plain comment \
             are both left out while the member's mention is kept"
        );
    }

    fn by_a_bot(id: u64, login: &str, body: &str) -> HumanResponse {
        HumanResponse {
            comment: id,
            author: ActorRef {
                login: login.to_string(),
                id: 2,
            },
            is_bot: true,
            ..said(id, "NONE", body)
        }
    }

    #[test]
    fn a_bot_comment_an_entitled_mention_names_is_read_and_one_nobody_names_is_not() {
        let (_, conversation) = with_the_bots_they_cite(
            vec![],
            vec![
                by_a_bot(1, "claude[bot]", "1. rename it 2. add a test"),
                by_a_bot(2, "codecov[bot]", "coverage fell"),
                said(3, "MEMBER", "@fiddle-ai do Claude's comments 1 and 2"),
                said(4, "NONE", "@fiddle-ai and codecov's too"),
            ],
            "fiddle-ai",
        );
        assert_eq!(
            conversation.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![1, 3],
            "the member's mention is kept with the bot it names, in the order written; the \
             bot only a stranger names, and the stranger, are left out"
        );
    }

    #[test]
    fn a_bot_comment_with_no_mention_naming_it_is_not_read() {
        let (reviews, conversation) = with_the_bots_they_cite(
            vec![],
            vec![
                by_a_bot(1, "claude[bot]", "1. rename it"),
                said(2, "MEMBER", "Claude's comment 1 looks right"),
            ],
            "fiddle-ai",
        );
        assert!(
            reviews.is_empty() && conversation.is_empty(),
            "a bot steers nothing by itself, and a member who names it without the mention \
             directs nothing: {conversation:?}"
        );
    }

    #[test]
    fn fiddles_own_account_is_never_read_as_a_cited_bot() {
        let (_, conversation) = with_the_bots_they_cite(
            vec![],
            vec![
                by_a_bot(1, "fiddle-ai[bot]", "an earlier note"),
                said(2, "MEMBER", "@fiddle-ai do it again"),
            ],
            "fiddle-ai",
        );
        assert_eq!(
            conversation.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![2],
            "every mention names the handle, so the handle's own bot is not cited by it"
        );
    }
}
