# 057 — The brief states that nothing has been changed yet

Status: accepted
Cites: FINDINGS_FRAME, SCOPE_RULES, TASK, migration_task, the_brief_claims_no_change_was_already_made

## Context

ADR 027 deleted `cve/version.rs` and moved version choice to the agent. Three constants in `capability/cve.rs` kept the premise that came before it. `FINDINGS_FRAME` said a dependency bump had already been applied. `SCOPE_RULES` said the bump was already in the tree and was not the agent's to declare. `TASK` asked whether the bump had cleared the advisory.

Nothing applies a bump. The host workflow scans, projects its lists, and runs fiddle.

Run 32646907465 shows what the false premise costs. The model read `go.mod` three times and wrote: "The user mentioned a dependency bump has already been applied but looking at go.mod, the version is still at 4.5.0." It then declared without changing, twice, with identical text. That is the failure that dominated every production run.

## Decision

State the premise the agent works from: nothing has been changed for these advisories yet, and the agent chooses the version and makes the change.

Keep the case where the tree already carries a fix. It is an observation the model reports, not a rule that stops it from acting.

## Consequences

- The brief now agrees with the code. A model that reads the tree and finds no fix has no sentence telling it one exists.
- `SCOPE_RULES` no longer forbids the model from claiming the change it makes. The declaration rule still requires the file list to be exact.
- A test composes the brief and fails on five phrasings that claim prior work. It also asserts the brief asks which version to move to, so deleting the premise does not pass by saying nothing.
- The scanner's own report is now the only account of the tree's state in the brief. If a scan is stale, the model reads the tree and says so.
