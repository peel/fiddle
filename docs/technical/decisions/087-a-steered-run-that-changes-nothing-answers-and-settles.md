# 087 — A steered run that changes nothing answers the direction once, and settles

Status: accepted
Cites: crates/fiddle-runtime/src/github/answer.rs, Answered, AnswerPullRequest, AnsweredComment, unanswered, NO_CHANGE, PULL_REQUEST_ANSWERED, crates/fiddle-runtime/src/capability/workflow.rs, SteeredBy, ANSWERED_WITHOUT_A_CHANGE, answered_without_a_change, Reviewed, crates/fiddle-acceptance/tests/toil.rs, a_steered_rerun_that_changes_nothing_answers_the_review_once_and_settles, a_review_fiddle_already_answered_settles_the_next_run_without_the_agent, a_review_left_after_the_reply_steers_the_run_again, workflows/toil.toml

## Context

OBSERVED on 2026-09-25, live run 6 of `jira:ISP-263` against snowplow-incubator/snowplow-identities#275. The steer step read a member's review. The repair step verified that both of the ticket's decisions were already on the branch, made no edit, and reported that in 9 turns. The evaluation accepted it.

The run then failed with exit 20. `ensure_branch_published` refused, because no step before it had committed the workspace. Nothing reached the pull request. The review stayed unanswered, so the next trigger would steer on it again and pay for the agent again, for ever.

The operator decided: settle, and reply on the pull request.

## Decision

**When the commit step finds nothing to commit and a direction on the pull request steered the run, the run answers that direction once on the pull request and settles. The publish steps do not run.**

- The reply is the effect `pull_request_answered`. It carries the agent's summary after a fixed sentence, `NO_CHANGE`, and a marker, `<!-- fiddle:answered v1 reviews=… comments=… -->`, naming the reviews and comments that steered the run.
- The run ends `Executed::Settled`, reported as `completed`, with `ANSWERED_WITHOUT_A_CHANGE` and the pull request in the reason. A settled run records no completion, as before, so the next run reads the forge again.
- The steer step drops, before it renders direction, every review and comment a marker names, and every comment that carries a marker. So an answered review does not steer again, and fiddle's own reply is not read as direction. A review or comment written after the reply has a new id and still steers.
- `inspect` finds a comment whose marker names the same set, so a repeated run does not post the answer twice.
- `Reviewed` now carries the review's `id`, which GitHub already returned and this build discarded.

A first run that changes nothing is unchanged by this record. It has no pull request to answer.

## Only entitled authors can answer

A marker is honoured only in a comment whose author association is OWNER, MEMBER or COLLABORATOR. fiddle posts with the deployment's token, which is an entitled account. Without that bar, anybody who can comment could post a marker and stop a member's review from steering. MEASURED by `a_marker_from_someone_the_project_does_not_entitle_silences_nothing`.

## The rows

- `a_steered_rerun_that_changes_nothing_answers_the_review_once_and_settles`: the run completes, the branch head is unchanged, no second pull request is opened, and exactly one reply names the review it answers.
- `a_review_fiddle_already_answered_settles_the_next_run_without_the_agent`: the next run makes one model call, the eligibility review, and posts no second reply.
- `a_review_left_after_the_reply_steers_the_run_again`: a later review reaches the agent's brief, the answered one does not, and the later one gets its own reply.

Removing the filter in the steer step fails the second and third rows. Removing the reply-and-settle branch fails all three.

## Consequences

- `pull_request_answered` is an eleventh registered effect. A deployment's `[github.policy]` may deny it. With no rule it is allowed, as every effect is.
- The reply reaches the pull request only. Nothing is written on the ticket.
- The reply carries the agent's summary, which the model wrote. It is posted as fiddle's answer, and a reader should read it as the agent's account of what it checked.
- No workflow step names this effect. The workflow performs it from the commit step, as `workflows/toil.toml` says above that step.
