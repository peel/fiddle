# 060 — A closed pull request never blocks the next run

Status: accepted
Cites: stamped, plan_shared_pull_request, plan_unproved_pull_request, dated_unproved_branch, a_fresh_branch_carries_the_runs_own_stamp_so_a_closed_pull_request_cannot_block_it

## Context

`plan_unproved_pull_request` finds its draft by label. When none is open it planned a fresh branch named for the date alone.

Closing a draft leaves its branch on the remote. A second run on the same day then chose that same name, and `GitCli::publish` refused it, correctly, because the branch was not an ancestor of the work. Run 32645593374 lost a finished repair that way. The proved shared pull request had the same shape.

`fiddle-zroj` first read this as a per-day reset of the attempt bound. That was wrong: the lookup is by label and works. The count resets because a human closes the draft, and #252 and #253 reset it twice on one day with no date change.

Three answers were put to the user. Refuse and name the cause. Count the close against the bound. Open a fresh draft every time.

## Decision

Open a fresh draft every time. A fresh branch carries the run's own work id, so no two runs choose one name and a closed pull request cannot block the next run.

The stamp applies to both the shared branch and the unproved branch, because the collision does not care which one it is.

## Consequences

- Closing a draft never costs a night. The next run publishes.
- **The bound cannot stop the unproved path.** The count lives in the pull request body, and a fresh draft has a fresh body. An advisory the agent cannot fix produces a new draft every night, without end. This is accepted, not overlooked: it is the price of the chosen answer, and it is the thing to revisit if the drafts become noise.
- Closed drafts leave their branches behind and they accumulate. Nothing deletes them.
- A branch name now carries eight characters of a work id after the date. The date still reads first, so a human scanning branches still sees the day.
- ADR 037's rule still holds where the pull request survives: the shared remediation request is reused by label across runs and keeps counting.
