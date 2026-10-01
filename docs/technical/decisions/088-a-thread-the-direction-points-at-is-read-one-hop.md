# 088 — A thread the direction points at is read, one hop, by the rules that admit direction

Status: accepted; amended 2026-10-01 to carry answered direction as context
Cites: crates/fiddle-runtime/src/github/references.rs, referenced, admitted, MAX_REFERENCED, Referenced, REFERENCED_FRAME, referenced_task, Direction, spoken, cited, entitled, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-acceptance/tests/toil.rs, a_review_that_points_at_another_pull_request_brings_the_comments_it_names_into_the_brief, a_named_bot_and_an_entitled_person_are_admitted_and_nobody_else_is, what_is_not_a_reference_is_not_followed, a_link_into_this_repository_is_a_reference_and_one_into_another_is_not, at_most_three_threads_are_read_whatever_the_direction_names, already_answered, Earlier, EARLIER_FRAME, earlier_task, a_reply_to_answered_direction_is_read_with_the_direction_it_replies_to, what_was_answered_before_is_kept_as_context_and_what_was_not_is_left_out, a_review_left_after_the_reply_steers_the_run_again

## Context

OBSERVED on 2026-10-01, live run 12 of `jira:ISP-263`. The review on snowplow-incubator/snowplow-identities#275 says `Reproducing spenes's review from #270` and `Claude's comment 1 and 2 seems legit ones`. Those comments are on #270. The steer step read only #275, so the agent answered that their text was not available to it.

On #270 itself the comments would have been read: a bot an entitled reviewer names is admitted on the same pull request, since commit `001a6ca`. A reviewer who points at another thread, by `#270` or by a link to a comment, is doing an ordinary thing, and the agent was told to act on text it could not see.

## Decision

**When an entitled person's direction names another pull request or issue in the same repository, the steer step reads that thread's conversation and quotes what the rules for direction admit from it.**

- `referenced` reads the text of the direction that steers the run: the reviews that steer and the comments of entitled people. A `#N` that stands as its own word, or a link to `github.com/<this repository>/pull/N` or `/issues/N`, names thread N. The pull request the run is on is not a reference to itself. At most `MAX_REFERENCED`, three, threads are read.
- `admitted` keeps from that thread what direction admits on the run's own pull request: a comment by an OWNER, MEMBER or COLLABORATOR, and a bot comment whose account the direction names. A comment that carries a fiddle marker is not kept.
- `referenced_task` quotes it after the direction, as `<author> wrote on #N:`, under `REFERENCED_FRAME`, which says it is quoted so the agent can read what the direction means and is not new direction.
- A thread that cannot be read is said to be unreadable, with the reason. A thread from which nothing is admitted is said to hold nothing quoted. Neither is left out silently.

fiddle reads the thread, not the agent. A tool that let the model fetch links would widen what text can reach it, and the reviewer's own pointer is the only fetch this needs.

## What is not followed

- A thread in another repository, MEASURED by `a_link_into_this_repository_is_a_reference_and_one_into_another_is_not`.
- A thread named only by what was read from a referenced thread. The reference is one hop.
- A `#N` inside a word or a path, such as `ABC#270`, `#270fff` or `path/#270`, MEASURED by `what_is_not_a_reference_is_not_followed`.
- The reviews on the referenced thread. Its conversation is read; its reviews are not. A reviewer who wants a review from another thread followed can reproduce it, as #275 did.

## The rows

- `a_review_that_points_at_another_pull_request_brings_the_comments_it_names_into_the_brief`: the review names #270 and Claude's comments. `claude[bot]`'s findings reach the brief as `claude[bot] wrote on #270`. A person who does not speak for the project and a bot nobody named also wrote on #270, and neither reaches it. With the step that follows references removed, the row fails.
- `a_named_bot_and_an_entitled_person_are_admitted_and_nobody_else_is` holds the admission rule alone.

## Consequences

- A steered run makes up to three more conversation reads.
- A long comment on the referenced thread is quoted whole. The claude[bot] review on #270 is about 8,000 characters.
- Text from a referenced thread is quoted as data, under the same rule as the direction it explains: it describes work and gives the agent no instruction.

## Amended 2026-10-01: a reply is read with the direction it replies to

OBSERVED on live run 17. The operator answered fiddle on #275 with `Yes, do Claude's comments 1 and 2.` The review that named #270 was already answered, so the steer step had dropped it, and its reference with it. The agent was told to do comments it could not see, searched twice, and asked whether the ticket was written against another branch.

- When new direction steers a run, `already_answered` keeps what a marker answered before it: the answered reviews and comments of entitled people, and fiddle's own answers without their marker. `earlier_task` quotes them first, under `EARLIER_FRAME`, which says they are already answered and are not work for this run.
- References in that earlier direction are followed by the rules above, so a reply that says `do them` reaches the thread `them` names. fiddle's own answers are quoted but not read for references.
- Answered direction alone still steers nothing. Context is added only to a run that new direction already steers.

`a_reply_to_answered_direction_is_read_with_the_direction_it_replies_to` replays run 17: the reply names no thread, the review it replies to names #270, and the brief carries the reply as work, the review and fiddle's answer as context, and `claude[bot] wrote on #270`. The run publishes the change and answers the reply by its id. Without the context it fails. `a_review_left_after_the_reply_steers_the_run_again` now holds that the answered review is quoted as context and not as a review to answer.
