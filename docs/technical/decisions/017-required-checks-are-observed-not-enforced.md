# 017 — `github.required_checks` is observed and reported, and enforced by nothing

Status: accepted
Cites: GitHub::required_checks, Executor::observe_checks, fiddle_core::VerificationState, fiddle_core::assess, OBSERVED_NOT_ENFORCED, REQUIRED_CHECKS_DECISION, crates/fiddle-acceptance/tests/config_check.rs

`crates/fiddle-cli/src/render.rs` holds this file's stem in `REQUIRED_CHECKS_DECISION`. Renaming the file breaks the `config check` payload.

## Context

`[github] required_checks` parses, defaults to the empty list, and is consumed. `Executor::observe_checks` looks each name up against the published head, and the bundle carries the result as `observations.verification`. Nothing branches on it, because `fiddle_core::assess` reads the work item and the change set alone.

## Decision

Disclose the key rather than make it decide. Report it in `config check` as an object carrying `configured`, `enforced`, a `status` and the `decision`. Use the status word `observed-not-enforced`, because the other unenforced key's promise would mislead.

## Consequences

- An operator who writes `required_checks` learns that it gates nothing when they check the document. They do not learn it from a green run over a red check.
- The `config check --json` payload changes shape for that key. `github.required_checks` was an array and is now an object with the array under `configured`. Any consumer other than `config_check.rs` reads a different type.
- The project gave up enforcement, and disclosed the gap instead. Enforcement is still owed. It is now a decision with its sub-questions written down, rather than an omission nobody noticed.
- The three cases are not alike, so enforcement cannot be guessed at. A failed check is a conclusion. A pending one resolves without anybody acting, and a missing one may only mean CI has not started.
- Enforcement belongs with the decision channel. Waiting for a pending check and waiting for a human are the same mechanism.

A deployment naming `required_checks = ["build"]` requires nothing of CI: the run reports the name and never requires it. That is the shape `agent.max_capability_attempts` shipped, and ADR 013 had to price it after the fact. One difference matters for how this key is disclosed. Nothing reads `max_capability_attempts`. This key is read, acted on and published; what it does not do is decide.

Making a required check decide means adding an arm to `fiddle_core::assess`, the pure core's decision function. Its `Blocked ⇒ Failed` rule is load-bearing for M0's frozen acceptance lane. Two places realise that rule, and both are documented as agreeing. `Blocked`, `Retryable` and waiting are the three plausible answers, and they map to three different exit rows. Guessing inside a remediation bean would give `RunOutcome` a new producer with none of the analysis. That is the mistake ADR 016 exists to correct one milestone later.
