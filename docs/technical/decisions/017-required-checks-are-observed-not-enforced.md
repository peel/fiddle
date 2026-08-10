# 017 — `github.required_checks` is observed and reported, and enforced by nothing

Status: accepted

## Context

`[github] required_checks` parses, defaults to the empty list, and is consumed. `capability::publish` hands it to `Executor::observe_checks`, which looks each name up against the published head and splits what it finds into `fiddle_core::VerificationState`'s three lists — `required_missing`, `failed`, `pending` — and that value reaches the published bundle as `observations.verification`.

Nothing branches on it. `fiddle_core::assess` matches on `work_item` and `changes` and on nothing else, so a required check that is missing, that failed, or that is still pending leaves a run's outcome exactly where an all-green one does. A deployment naming `required_checks = ["build"]` requires nothing of CI: the name is reported, never required.

This is the shape `agent.max_capability_attempts` shipped and ADR 013 had to price after the fact — with one difference that matters for how it is disclosed. `max_capability_attempts` is read by *nothing*: no code path anywhere consults the value. `required_checks` is read, acted on, and published; what it does not do is decide.

## Decision

Disclose it, rather than make it decide.

`config check` reports the key the way it reports `max_capability_attempts`: an object rather than a scalar, carrying what the document `configured`, what is `enforced` (the empty list, whatever the document says), a `status` a machine can key on, and the `decision` that explains it. The plain rendering says the same in words. The status word is `observed-not-enforced` rather than `accepted-not-enforced`, because the two are different and an operator debugging one would be misled by the other's promise.

Enforcement was the alternative, and it is a larger change than it looks. Making a required check decide means adding an arm to `fiddle_core::assess`, which is the pure core's decision function, whose `Blocked ⇒ Failed` rule is load-bearing for M0's frozen acceptance lane and is realised in two places that are documented as agreeing. It also means deciding **what** it decides, and the three cases are not alike: a `failed` required check is a conclusion, a `pending` one is a transient state that resolves without anybody doing anything, and a `required_missing` one may only mean CI has not started yet. `Blocked`, `Retryable` and *wait* are the three plausible answers and they map to three different exit rows — the middle one being a suspension, which is M3's. Guessing at that inside a remediation bean would give `RunOutcome` a new producer with none of the analysis, which is the mistake decision 016 exists to correct one milestone later.

## Consequences

An operator who writes `required_checks` learns from `config check` that it gates nothing, at the moment they check the document, rather than from a green run over a red check.

The `config check --json` payload changes shape for that key: `github.required_checks` was an array and is now an object with the array under `configured`. `config_check.rs` reads it the new way; any other consumer reads a different type than it did.

The finding is not closed, only disclosed. Enforcement is still owed, and it is now a decision with its three sub-questions written down rather than an omission nobody had noticed. It belongs with M3's decision channel, because *wait for a pending check* and *wait for a human* are the same mechanism.
