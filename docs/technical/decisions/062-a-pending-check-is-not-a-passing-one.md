# 062 — A pending check is not a passing one, and only a caller that can wait waits

Status: accepted
Cites: Settlement, Settlement::has_settled, Feedback::Unsettled, CompleteFindings, OrchestrationCve::settle

## Context

`observe_genuine_failure` answered `None` for two different worlds: every check finished and passed, or no check had finished. `Feedback::BlamingNothing` covered both, so a run could not tell a green pull request from one it had not finished reading.

Nightly, the ambiguity rarely shows, because the checks settled hours earlier. It decides the outcome under an event trigger. On `workflow_run` only the workflow that finished is done. On a fresh push every check is queued. A run in either case would read no failure and treat a broken pull request as fine.

Observed: run 32654571617 was dispatched while `ci` on #254 was `in_progress` with `build` already `failure`. A run a minute earlier would have seen only queued checks.

## Decision

`Settlement` carries `read` and `settled` beside the failure, so the count is reported rather than implied. `Feedback::Unsettled { pending, read }` is its own state, and a run in that state makes no fresh attempt and does not claim the pull request is green. The sentence reaches `findings.json`.

Whether to wait is the caller's, not the product's. `[orchestration.cve] settle` is a duration, and it defaults to zero.

- Zero, which is what a CI job uses: read once, report what had not settled, do not wait. A job cannot hold a runner idle.
- Non-zero, which a local or agent-driven run can set: poll until every check settles or the window expires, then act on the real result.

## Consequences

- A half-settled forge still blames a check that has already failed. A landed failure does not wait for its neighbours.
- With the default, a genuine failure waits for the next run, which is the nightly behaviour that has always applied.
- With a window set, one run can repair a failure it caused, which is what makes an event trigger worth adding.
- `SETTLE_POLL` is 20 seconds. A window shorter than one poll reads once and returns, which is the same as zero.
- Cancellation is honoured inside the wait, so a cancelled run does not sit in the loop.
