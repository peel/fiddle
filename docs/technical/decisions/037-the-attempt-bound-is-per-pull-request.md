# 037 — The attempt bound counts attempts per pull request, and the count lives in the body

Status: accepted
Cites: cve/attempts.rs, capability/mitigate.rs, capability/cve.rs::land, lands_as, Landed::Reverted, AttemptsError::Unreadable, bound_reached, max_capability_attempts, fiddle-attempts:start, tests/cve_feedback.rs, an_attempt_that_reverts_raises_the_count_the_commit_log_cannot_show, a_count_that_cannot_be_read_makes_no_attempt, ATTEMPT_BOUND_DECISION, RunDisposition::attempt_bound, crates/fiddle-acceptance/tests/config_check.rs::config_check_reports_the_attempt_bound_it_enforces_and_where_the_count_lives

`crates/fiddle-cli/src/render.rs` holds this file's stem in `ATTEMPT_BOUND_DECISION`. Renaming the file breaks the `config check` payload.

## Context

A bound stops fiddle from reworking one pull request forever. `capability/cve.rs::land` commits a clean attempt, and it reverts an attempt that needs work. So the attempts the bound exists to count leave nothing in the commit log.

## Decision

Count attempts against one pull request, and hold the count in that pull request's body. Write the count between `<!-- fiddle-attempts:start -->` and `<!-- fiddle-attempts:end -->`, and read it from there. Refuse to attempt when that block does not parse.

## Consequences

- The commit log cannot hold the count. `lands_as` sends a group that needs work to `Landed::Reverted`, and a reverted attempt pushes nothing. `fiddle-pdz9` pinned both halves with a pair of tests in `tests/cve_feedback.rs` that differ only in the rescan arm.
- An unreadable count refuses to attempt. `AttemptsError::Unreadable` stops the run, because an attempt without a bound is worse than no attempt. `a_count_that_cannot_be_read_makes_no_attempt` holds that.
- A body with no marker reads as zero, and a malformed block does not. A first attempt meets an absent block, and it is not an error.
- The body becomes state a person can edit. An operator can reset or raise a count by hand. So fiddle reads the number each run, and never assumes the number it wrote.
- What was given up: the count is per pull request, not per repository and not per CVE. `max_capability_attempts` bounds the rework of one pull request, and a run that opens a new pull request starts at zero.
- This closes the deferral [013](013-one-attempt-bound-not-two.md) recorded, and it pays none of the five costs 013 priced, because no retry loop was built. 013 carries the amendment.
- `config check` reports the key as `status: enforced-per-pull-request` with `counted_in: pull-request-body`, and names this record. The document's number is the bound, so no key reports a second one.
- A run stopped by the bound publishes both numbers. `RunDisposition` carries `attempt_bound` as `{spent, bound}`, because the row name and a pull request number cannot tell 2 of 2 from 5 of 5.
