# 037 — The attempt bound counts attempts per pull request, and the count lives in the body

Status: accepted
Cites: cve/attempts.rs, capability/mitigate.rs, capability/cve.rs::land, lands_as, Landed::Reverted, AttemptsError::Unreadable, bound_reached, max_capability_attempts, fiddle-attempts:start, tests/cve_feedback.rs, an_attempt_that_reverts_raises_the_count_the_commit_log_cannot_show, a_count_that_cannot_be_read_makes_no_attempt

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
