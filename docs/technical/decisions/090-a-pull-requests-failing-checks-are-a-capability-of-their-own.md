# 090 — A pull request's failing checks are a capability of their own

Status: accepted; amended 2026-10-02 so a rejected steered run answers
Cites: CHECKS, CAPABILITIES, Selection, workflow_of, CHECKS_DOCUMENT, CHECKS_STAGE, Step, Ready, checks_task, CHECKS_FRAME, NOTHING_FAILS, with_checks, failing_checks, failing_section, behind_base, FailedCheck, LOG_SECTION_BYTES, api_text, without_escapes, workflows/checks.toml, workflows/prompts/checks.md, workflows/prompts/checks_evaluate.md, crates/fiddle-runtime/src/github/checks.rs, crates/fiddle-runtime/src/github/cli.rs, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-cli/src/main.rs, crates/fiddle-acceptance/tests/toil.rs, a_failing_check_reaches_the_agent_with_its_log_and_a_branch_behind_its_base_is_diagnosed, the_section_is_the_step_that_failed_last_without_timestamps, a_long_section_keeps_its_end, a_text_body_keeps_its_lines_and_loses_its_terminal_escapes, rejected, REJECTED, crates/fiddle-runtime/src/github/answer.rs, a_steered_run_its_evaluation_rejects_answers_with_the_findings

## Context

OBSERVED on 2026-10-02. The operator wrote `Fix the CI failure` on snowplow-incubator/snowplow-identities#275. The failing job is `build`, and its failing step is Validate swagger, which runs `swag init -g pkg/manage/api.go`. The pull request's checks ran with the workflow definitions `main` holds now, against the branch's files. `main` gained `pkg/manage` in #266, #276 and #277, and the branch is 9 commits behind `main`, so the step could not find it. No file fiddle changed is involved, and no change to the branch's files fixes it.

The toil route gives the agent the direction on the pull request and no check logs. Its own check, `go build ./...`, passes. The agent would have been told to fix a failure it could not see.

The operator decided that a pull request's failing checks are their own capability.

## Decision

**`checks` is a capability run by the workflow engine from `workflows/checks.toml`, with its own prompts.**

- Its steps are steer, checks, agent, evaluate, commit, and the branch and pull request effects. It reuses everything toil does on a pull request: the steer step, standing on the head, the context of answered direction, references, the commit format and the answer.
- The `checks` step needs the pull request a direction steered the run from. `failing_checks` reads the check runs that fail on its head. For a GitHub Actions job it reads the job's log through `api_text` and keeps `failing_section`: from the last `##[group]Run` before the last `##[error]` to that error, without timestamps, at most `LOG_SECTION_BYTES`. `behind_base` reads how far the branch is behind its base. `checks_task` renders all of it into the brief under `CHECKS_FRAME`, or says `NOTHING_FAILS`.
- `checks.md` tells the agent to make the smallest change the failing log names, and to change nothing when the cause is not in the files: a branch behind its base, a credential or service, an unreliable check. Its summary is the answer and says what a person has to do. `checks_evaluate.md` accepts no change, and judges a change against the failure.
- The toil scope sentences, `STEERED_SCOPE` and `STEERED_EVALUATION`, are added only when the capability is `toil`. They speak of a ticket, and a checks run has none to scope it.
- `api_text` passes `--allow-escape-sequences`, because `gh` refuses to print a job log with terminal escapes in it, and `without_escapes` removes them.

## How it is invoked

For now, `fiddle run jira:ISP-263 --capability checks`. The run finds the ticket's pull request as toil does. A `pr:` invocation scheme, so that a pull request names the run without a ticket and a mention can trigger it, is the next slice and is not in this record.

## The rows

`a_failing_check_reaches_the_agent_with_its_log_and_a_branch_behind_its_base_is_diagnosed` replays #275: a failing `build` job whose log holds the swag step, a branch 9 commits behind `main`, and `Fix the CI failure`. The run makes two model calls, the agent and the evaluation, and no eligibility review. The brief carries the failing step without timestamps and the 9 commits. It carries no toil scope sentence. Nothing is published, and the answer is the diagnosis, naming the comment. Without the checks section in the brief it fails.

## Consequences

- A checks run makes one check-runs read, one log read per failing Actions job, and one comparison.
- A check from another app carries its reported summary and no log.
- `CAPABILITIES` holds seven ids, and a workflow step is one of seven kinds.

## Amended 2026-10-02: a rejected steered run answers

OBSERVED on 2026-10-02, live run 22 of `jira:ISP-263 --capability checks`. The agent changed nothing and diagnosed the branch as 9 commits behind `main`. The evaluation's findings said the same, but it returned rejected: it applied the rule that rejects a change made for a cause outside the files to a run that made no change. A rejected run posted nothing, so the diagnosis did not reach #275.

- `checks_evaluate.md` now decides in order. A project that holds no change is accepted, and the cause of the failure is the reason the run changed nothing, not a reason to reject it. The reject rules name only a change.
- A steered run that its evaluation rejects answers the direction with `rejected`: `REJECTED`, the findings as a list, and the marker. This holds for every capability the workflow engine runs, so direction on a pull request is never left without an answer.

`a_steered_run_its_evaluation_rejects_answers_with_the_findings` replays run 22's verdict. Without the answer it fails.
