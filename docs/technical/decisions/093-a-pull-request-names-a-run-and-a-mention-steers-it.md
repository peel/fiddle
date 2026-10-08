# 093 — A pull request names a run, and a mention steers it

Status: accepted
Cites: InvocationScheme::Pr, PullRequestRef, InvocationRef::record_key, InvocationRef::slug, InvocationRefError::MalformedPullRequest, PullRequestFacts, read_pull_request, served_pull_request, open_on_the_served_repository, ForeignPullRequest, PullRequestRefused, PullRequestUnread, ToilOverAPullRequest, Selection::PullRequest, workflow_of, PULL_REQUEST_DOCUMENT, PULL_REQUEST_STAGE, ensure_branch_published, MENTION, mentions, without_code_or_quotes, mentioning, directing, with_the_bots_they_cite, is_the_handle, cited, speaks, entitled, ENTITLED, AuthorKind, PersonThroughAnApp, is_a_bot, is_automated, unanswered, already_answered, is_account, written_by, trusted, pull_request_answered, Author, CommitAuthor, committer, FIDDLE_NAME, FIDDLE_EMAIL, HOOKS_DISABLED, crates/fiddle-core/src/identity.rs, crates/fiddle-runtime/src/github/pulls.rs, crates/fiddle-runtime/src/github/mentions.rs, crates/fiddle-runtime/src/github/answer.rs, crates/fiddle-runtime/src/github/comments.rs, crates/fiddle-runtime/src/capability/cve.rs, crates/fiddle-runtime/src/capability/commit.rs, crates/fiddle-cli/src/config.rs, crates/fiddle-cli/src/main.rs, workflows/pull_request.toml, workflows/prompts/pull_request.md, workflows/prompts/pull_request_evaluate.md, docs/technical/host-workflow-fiddle-ai.yml, scripts/test-host-workflow-fiddle-ai.sh, scripts/local-toil-rerun.sh, scripts/test-local-toil-rerun.sh, crates/fiddle-acceptance/tests/pull_request.rs, crates/fiddle-acceptance/tests/toil.rs, a_member_mention_steers_a_pull_request_run_onto_its_own_branch, a_configured_author_signs_the_commit_and_without_one_fiddle_does, a_pull_request_run_needs_no_tracker_table, a_mention_by_someone_who_does_not_speak_for_the_project_steers_nothing, a_member_comment_without_the_mention_steers_nothing, a_mention_written_by_the_account_fiddle_writes_as_steers_nothing, a_closed_a_merged_and_a_forked_pull_request_are_refused_by_name, a_pull_request_on_another_repository_is_refused_before_any_request, a_branch_that_moved_during_the_run_is_not_forced, a_reference_and_a_head_repository_spelled_in_another_case_are_the_served_repository, toil_over_a_pull_request_is_refused_and_checks_over_one_runs, a_bot_comment_an_entitled_mention_names_is_read_as_context_and_one_nobody_names_is_not, a_bot_steers_nothing_even_as_a_member_mentioning_the_handle, a_member_bots_mention_is_not_marked_answered_beside_a_persons, a_second_run_steers_on_the_new_mention_alone_and_a_third_finds_nothing_new, an_answer_the_app_wrote_settles_the_next_run_only_when_the_account_is_configured, a_persons_mention_posted_through_an_app_reaches_the_brief_and_is_answered, a_bot_review_a_mention_names_is_quoted_and_never_read_as_work_to_do, inspect_refuses_toil_over_a_pull_request_as_run_does, a_bot_comment_is_never_marked_answered_so_a_later_comment_naming_it_still_reaches_it, a_mention_is_the_handle_as_a_word, a_mention_ends_at_the_text_and_starts_after_punctuation_or_a_line_break, a_word_mention_after_a_code_span_counts_and_a_code_span_with_an_address_does_not, a_mention_inside_an_inline_code_span_is_left_out, an_unmatched_backtick_is_literal_so_the_mention_after_it_counts, a_mention_inside_a_fenced_block_is_left_out, a_mention_on_a_quoted_line_is_left_out, multibyte_text_around_left_out_regions_does_not_panic, an_empty_handle_is_never_mentioned, only_mentions_are_kept_from_reviews_and_the_conversation, only_an_entitled_authors_mention_directs, a_bot_comment_an_entitled_mention_names_is_read_and_one_nobody_names_is_not, a_bot_comment_with_no_mention_naming_it_is_not_read, fiddles_own_account_is_never_read_as_a_cited_bot, one_rule_reads_a_bot_review_and_a_bot_comment_and_not_a_person_posting_through_an_app, a_bot_never_directs_whatever_its_association, a_marker_the_configured_account_wrote_is_trusted_though_its_association_is_none, the_configured_account_is_matched_whatever_the_case_of_its_login, a_login_that_only_starts_like_the_account_is_not_trusted, the_configured_accounts_comments_are_never_direction, a_review_the_configured_account_wrote_is_never_direction, what_the_configured_account_answered_is_context_and_its_reply_is_fiddles, a_bot_is_never_marked_answered, a_bot_review_a_marker_names_is_never_quoted_as_earlier_direction, a_pull_request_reference_names_its_repository_and_number, a_pull_request_reference_that_is_not_owner_repo_number_is_refused, each_refused_pull_request_parses_once_its_one_fault_is_corrected, a_dot_is_a_name_character_and_the_other_schemes_keep_their_rule, only_a_pull_request_reference_answers_a_pull_request, two_pull_requests_whose_names_differ_only_where_a_dash_falls_keep_apart, a_pull_request_record_key_holds_no_path_or_number_separator

## Context

`fiddle-40l6`. Before this record a run on a pull request needed a Jira ticket. `fiddle run jira:ISP-263 --capability checks` found the ticket's pull request and worked it (ADR 090). A pull request that no ticket names could not be worked, and a person on the pull request could not start a run. ADR 090 named a `pr:` scheme and a mention trigger as the next slice.

The operator decided on 2026-10-08 that a pull request names a run by itself, and that a person starts the run by mentioning `@fiddle-ai` on it. fiddle writes as a GitHub App named `fiddle-ai`. The host workflow mints the App's installation token and passes it as `FIDDLE_GITHUB_TOKEN`, so fiddle holds no identity code beyond the commit author. Creating the App needs an organisation admin and is not part of this record.

## Decision

**`pr:<owner>/<repo>#<n>` names a run. Only a person who speaks for the project, mentioning `@fiddle-ai` as a word in their own text, steers it. fiddle applies that rule itself, whatever started it.**

### The scheme and the record key

- `InvocationScheme::Pr` is spelled `pr`. `PullRequestRef` parses its value: owner and repository of ASCII letters, digits, `-`, `_` and `.`, and a positive decimal number with no leading zero. Any other value is `InvocationRefError::MalformedPullRequest`, which names the form `pr:<owner>/<repo>#<number>`. Every other scheme keeps the value rule of ADR 011.
- `InvocationRef::record_key` is `<owner>+<repo>+<n>`, and `InvocationRef::slug` is `pr-<owner>+<repo>+<n>`. A completion record is `<[stub] root>/changes/<owner>+<repo>+<n>.json`. `+` is in no GitHub name and `-` is in many, so two pull requests never share a key (ADR 011, amended).
- `pr:` with no `--capability` selects `Selection::PullRequest`. `workflow_of` maps it to `PULL_REQUEST_DOCUMENT`, `workflows/pull_request.toml`, under `PULL_REQUEST_STAGE`, read beside the deployment document. Its steps are steer, checks, agent, evaluate, commit and `ensure_branch_published`. There is no ticket gate, no tracker table and no `ensure_pull_request`: the pull request exists.
- `--capability toil` over a `pr:` reference exits 2 with `ToilOverAPullRequest`, in `run` and in `inspect`, before any port or the forge. Toil works a ticket and would open a second pull request. `--capability checks` over a `pr:` reference runs (ADR 022, amended).

### The read and its refusals

Before the run, `served_pull_request` compares the repository the reference names with `[github] repo`, ignoring ASCII case, because GitHub names do. A different repository is `ForeignPullRequest`, a configuration error, exit 2, before any request.

Then `open_on_the_served_repository` reads `GET /repos/{repo}/pulls/{n}` once with `read_pull_request`, into `PullRequestFacts`. It refuses with `PullRequestRefused`, exit 2, and a reason that names the pull request:

- a closed or merged pull request: nothing to do;
- a head branch in another repository: naming the fork, because fiddle cannot push to it.

An unreadable pull request is `PullRequestUnread`, a forge failure, exit 11, retryable.

The run's branch is the head ref and its base the base ref. The push never forces, so a branch that moved during the run is refused and the next mention starts again.

### The mention rule

On a `pr:` run, `directing` keeps a comment or review only when two things hold.

- `mentions` finds `@fiddle-ai`, the handle `MENTION` names, as a word: not inside a longer word, an address or another handle, and ASCII case aside. `without_code_or_quotes` first leaves out inline code spans, fenced blocks and quoted lines. A mention someone quotes or shows as code asks for nothing.
- `speaks` holds for its author: `entitled`, so OWNER, MEMBER or COLLABORATOR as `ENTITLED` lists them, and not a bot. `AuthorKind` reads who wrote it from the account type, the same way for comments and reviews. `is_a_bot` is true for a `Bot` account whatever its association. A person posting through an App is `PersonThroughAnApp`: `is_automated` but not `is_a_bot`, so the person speaks.

A stranger's mention steers nothing, and a member's comment without the mention steers nothing. A bot never steers, even as a member and even when it mentions the handle.

`with_the_bots_they_cite` adds a bot's comment or review that a kept mention names by its login, such as `claude[bot]` in "do Claude's comments 1 and 2". `cited` decides the naming, and `is_the_handle` keeps fiddle's own account out of it. Such a comment is quoted as context, never followed as work, and it steers nothing by itself.

No bot is ever marked answered. A bot's comment that a later mention names still reaches that later run.

### The account fiddle writes as, and the answer

`[github] account`, for example `fiddle-ai[bot]`, is optional. An App's comment carries the association `NONE`. `unanswered` and `already_answered` trust an answered marker only in a `trusted` comment, which is an entitled author's or, through `written_by` and `is_account`, the configured account's. The login is matched ignoring ASCII case, and a login that only starts like the account is not it. The account's comments and reviews are never direction, and its replies are fiddle's own.

The answer effect, `pull_request_answered`, observes its own reply after it posts it. It trusts a reply the configured account wrote, as `unanswered` does. Without `account` the App's reply is never observed, the run ends retryable, and the next run answers the same mention again. When `account` is absent the rule before this record holds.

### The author

`[github.author]`, a table of `name` and `email`, is optional. `Author` reads it, refusing an empty name, an address without `@` and a key it does not read. It resolves to `CommitAuthor`, and `committer` makes it the author and committer of every commit fiddle makes. Absent, commits carry `FIDDLE_NAME` and `FIDDLE_EMAIL`, `fiddle <fiddle@invalid>`, as before. `HOOKS_DISABLED` holds in either case: a hook on the machine does not refuse fiddle's commit. For the App the author is `fiddle-ai[bot]` and `<app-user-id>+fiddle-ai[bot]@users.noreply.github.com`.

### The trigger

`docs/technical/host-workflow-fiddle-ai.yml` is a template for the target repository's `.github/workflows/`. It runs on `issue_comment` created on a pull request, `pull_request_review` submitted and `pull_request_review_comment` created. Its job `if` requires the text to mention `@fiddle-ai`, the author association to be OWNER, MEMBER or COLLABORATOR, and the sender not to be a `Bot`. One concurrency group per pull request, with `cancel-in-progress: false`, so a run is never stopped between its push and its answer. Its own `GITHUB_TOKEN` reads contents only. It checks out the default branch with `persist-credentials: false`, mints the installation token with `actions/create-github-app-token`, verifies the fiddle release against a pinned digest, and runs `fiddle run "pr:${{ github.repository }}#<n>"`. Its header lists what the App needs: `contents: write`, `pull-requests: write`, `issues: write`, `checks: read`, `actions: read`, and the three events.

**fiddle re-applies its own rule, and the `if` is only a filter.** The `if` cannot see code spans, fences or quotes, and GitHub's `contains` matches `@fiddle-ai` inside a longer handle. A run that the `if` lets through reads every comment and review again under the mention rule above. So a wrong or edited `if` costs a run that completes with nothing to do. It cannot widen who steers. `scripts/test-host-workflow-fiddle-ai.sh` holds the template's guard in the gate, so an edit that widens the `if` fails there too.

`scripts/local-toil-rerun.sh` takes a `pr:` reference. It names and, with `--forget`, removes `changes/<owner>+<repo>+<n>.json`, and refuses a malformed reference with the form it should take. It resolves a relative `[stub] root` against the working directory, as fiddle does (ADR 092).

## The rows

In `crates/fiddle-acceptance/tests/pull_request.rs`:

- `a_member_mention_steers_a_pull_request_run_onto_its_own_branch`: the head moves to a commit with the agent's message, no other branch opens, and the mention is answered.
- `a_pull_request_run_needs_no_tracker_table`: a deployment with no `[jira]` runs a `pr:` reference.
- `a_mention_by_someone_who_does_not_speak_for_the_project_steers_nothing`, `a_member_comment_without_the_mention_steers_nothing` and `a_mention_written_by_the_account_fiddle_writes_as_steers_nothing`: each run completes and asks no model.
- `a_bot_steers_nothing_even_as_a_member_mentioning_the_handle` and `a_member_bots_mention_is_not_marked_answered_beside_a_persons`.
- `a_persons_mention_posted_through_an_app_reaches_the_brief_and_is_answered`.
- `a_bot_comment_an_entitled_mention_names_is_read_as_context_and_one_nobody_names_is_not` and `a_bot_review_a_mention_names_is_quoted_and_never_read_as_work_to_do`.
- `a_closed_a_merged_and_a_forked_pull_request_are_refused_by_name`, `a_pull_request_on_another_repository_is_refused_before_any_request` and `a_reference_and_a_head_repository_spelled_in_another_case_are_the_served_repository`.
- `a_branch_that_moved_during_the_run_is_not_forced`: the run does not complete, and the commit someone else pushed stays the head.
- `toil_over_a_pull_request_is_refused_and_checks_over_one_runs` and `inspect_refuses_toil_over_a_pull_request_as_run_does`.
- `a_second_run_steers_on_the_new_mention_alone_and_a_third_finds_nothing_new` and `an_answer_the_app_wrote_settles_the_next_run_only_when_the_account_is_configured`: with `account` set, a second run with nothing new asks no model and answers nothing. Without it, the first run cannot observe its reply and the second answers the mention again.
- `a_configured_author_signs_the_commit_and_without_one_fiddle_does`.

In `crates/fiddle-acceptance/tests/toil.rs`, `a_bot_comment_is_never_marked_answered_so_a_later_comment_naming_it_still_reaches_it`.

In `crates/fiddle-core/src/identity.rs`, the grammar and the key: `a_pull_request_reference_names_its_repository_and_number`, `a_pull_request_reference_that_is_not_owner_repo_number_is_refused`, `each_refused_pull_request_parses_once_its_one_fault_is_corrected`, `a_dot_is_a_name_character_and_the_other_schemes_keep_their_rule`, `only_a_pull_request_reference_answers_a_pull_request`, `two_pull_requests_whose_names_differ_only_where_a_dash_falls_keep_apart` and `a_pull_request_record_key_holds_no_path_or_number_separator`.

In `crates/fiddle-runtime/src/github/mentions.rs`, the mention as a word: `a_mention_is_the_handle_as_a_word`, `a_mention_ends_at_the_text_and_starts_after_punctuation_or_a_line_break`, `an_empty_handle_is_never_mentioned`. What is left out: `a_mention_inside_an_inline_code_span_is_left_out`, `a_word_mention_after_a_code_span_counts_and_a_code_span_with_an_address_does_not`, `an_unmatched_backtick_is_literal_so_the_mention_after_it_counts`, `a_mention_inside_a_fenced_block_is_left_out`, `a_mention_on_a_quoted_line_is_left_out`, `multibyte_text_around_left_out_regions_does_not_panic`. Who directs: `only_mentions_are_kept_from_reviews_and_the_conversation`, `only_an_entitled_authors_mention_directs`, `a_bot_never_directs_whatever_its_association`. The bots a mention names: `a_bot_comment_an_entitled_mention_names_is_read_and_one_nobody_names_is_not`, `a_bot_comment_with_no_mention_naming_it_is_not_read`, `fiddles_own_account_is_never_read_as_a_cited_bot`, `one_rule_reads_a_bot_review_and_a_bot_comment_and_not_a_person_posting_through_an_app`.

In `crates/fiddle-runtime/src/github/answer.rs`, the account: `a_marker_the_configured_account_wrote_is_trusted_though_its_association_is_none`, `the_configured_account_is_matched_whatever_the_case_of_its_login`, `a_login_that_only_starts_like_the_account_is_not_trusted`, `the_configured_accounts_comments_are_never_direction`, `a_review_the_configured_account_wrote_is_never_direction`, `what_the_configured_account_answered_is_context_and_its_reply_is_fiddles`. No bot answered: `a_bot_is_never_marked_answered`, `a_bot_review_a_marker_names_is_never_quoted_as_earlier_direction`.

`scripts/test-host-workflow-fiddle-ai.sh` holds the template's events, guard, concurrency, checkout, tokens and `pr:` run, and that the guard's handle is `MENTION`. `scripts/test-local-toil-rerun.sh` holds the `pr:` record name, the refusals and the working-directory root.

## Consequences

- A pull request that no ticket names can be worked, and the person who asks is on the pull request.
- A run costs one pull-request read before anything else. A refused pull request costs nothing more.
- Two runs on one pull request queue. A queued run that a later mention replaces is not lost: the run that follows answers every mention not yet answered.
- A deployment with an App must set `[github] account`, or every run answers every mention again. `fiddle config check` does not print `account` or `author`, so a missing `account` shows only as a second answer.
- `jira:` runs keep their direction rule. The mention rule applies to `pr:` runs only.
