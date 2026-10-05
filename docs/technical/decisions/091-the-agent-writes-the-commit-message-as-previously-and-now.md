# 091 — The agent writes the commit message, as Previously and Now

Status: accepted
Cites: CommitMessage, commit_message, TITLE_LIMIT, fault, subject, body, Held, described, DESCRIPTION, description, DESCRIBE_THE_WORK, RepairReport, crates/fiddle-runtime/src/agent/mod.rs, crates/fiddle-runtime/src/agent/returns.rs, crates/fiddle-runtime/src/capability/workflow.rs, workflows/prompts/toil.md, workflows/prompts/checks.md, crates/fiddle-acceptance/tests/toil.rs, a_report_that_changes_files_without_a_commit_message_is_returned_and_the_message_it_sends_is_the_commit, a_member_review_widens_the_change_and_the_change_it_earns_is_published_and_answered

## Context

OBSERVED on 2026-10-05. The operator read the commits fiddle made on snowplow-incubator/snowplow-identities#275 and said the messages do not look good. Commit `f3a89ca` reads `identities: jira:ISP-263, answering the direction on snowplow-incubator/snowplow-identities#275`, with the agent's Markdown answer as its body. ADR 089 built the subject from the invocation, and the body from the ticket or from the summary written for the pull request. Neither says what the change does.

The repository's own rule, in its `CLAUDE.md`, is an imperative title of at most 70 characters and a body in two paragraphs, one that opens with `Previously` and one that opens with `Now`, with no bullet points, statistics or attribution lines. The operator decided that fiddle's commits follow that pattern.

## Decision

**The agent of a workflow run writes the commit message, and fiddle checks its form.**

- `RepairReport` carries `commit_message`, a `CommitMessage` of `title`, `previously` and `now`. The schema and `toil.md` and `checks.md` say what each holds.
- The workflow's agent step holds its report with `described`. A report that names changed files and carries no `commit_message`, or one whose `fault` is not none, is returned to the model under the rule `DESCRIPTION`. `fault` requires a title of one line, at most `TITLE_LIMIT` characters, with no period at the end, a `previously` that opens with `Previously`, and a `now` that opens with `Now`. The model gets the returns every other rule gets.
- The commit step uses the title as the subject and the two paragraphs, a blank line between them, as the body. A steered commit is still dated at its direction.
- A report that names no file but leaves a change in the workspace carries no message, and the commit falls back to the messages ADR 089 states. No message is invented for it.

The CVE capabilities do not set `described`. Their commit subjects name the advisories and are unchanged.

## What is given up

ADR 089 built a first run's message from the ticket and nothing the model wrote, so that a retry rebuilds the same commit. A model-written message differs between attempts. A retry's change differs between attempts too, so the identity of the commit was already the model's. A retry that rebuilds the same tree with a new message is a new commit on the same parent, which `ensure_branch_published` refuses without force, as it refuses any diverged retry.

The commits already on #275 keep their messages. Rewriting them needs a force push.

## The rows

`a_report_that_changes_files_without_a_commit_message_is_returned_and_the_message_it_sends_is_the_commit`: the first report names a changed file and no message, the model is told what it lacks, and the commit carries the message of the second report. Without the rule, the first report is accepted and the commit carries the old subject, and the row fails. `a_member_review_widens_the_change_and_the_change_it_earns_is_published_and_answered` now reads both commits as the agent's title, `Previously` paragraph and `Now` paragraph.
