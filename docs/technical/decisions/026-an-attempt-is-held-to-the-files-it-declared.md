# 026 — An attempt is held to the files it declared, and to nothing else

Status: accepted
Cites: cve::undeclared, cve::DeclarationBreach, GroupStatus::of, MigrationAttempt::undeclared, RepairReport::changed_files, `[[workspace.checks]]`
Removed in M4c and named here so a reader can grep them: ForbiddenShape, ASSERTIONS, is_go, is_go_test, is_go_mod, replaces

## Context

M4a refused an edit for what the file was. Eight things read Go syntax to decide whether a change was legitimate, and the `Cites` line names them. That is ecosystem semantics on the Rust side of the line ADR 025 moves, so it is deleted.

## Decision

Hold an attempt to one rule: the diff touched exactly the files it declared. Compare the declared paths against the diff's paths set-wise, and make either difference a `DeclarationBreach`. Excuse the paths the run edited itself, and nothing beside them.

## Consequences

- "Don't silence the tests" stops being a Rust guarantee. The rule permits a declared test-file edit, and no code can object any more.
- That guarantee survives only where a deployment declares a test check in `[[workspace.checks]]`. The project gave up a product guarantee for a deployment responsibility, and the manual has to say so.
- The check is post-hoc, deliberately. M1's tool surface has no mid-attempt handshake, so the declaration arrives with the result. The rule rejects an undeclared edit rather than preventing one.
- What replaced a semantic judgement is a set comparison. So the rule holds identically in every ecosystem, and says nothing about whether an edit was wise. An agent that pins the wrong distribution in the right file passes it.
- The refusal names paths, so a breach is diagnosable. `DeclarationBreach` renders `changed without declaring: …` and `declared without changing: …`.

A breach sends the attempt to needs-work. `undeclared` reports `unannounced` for a path changed and undeclared, and `unmet` for a path declared and unchanged. Rust does not know what any of those paths mean.

**One exclusion, and the rule refuses every run without it.** A sweep bumps a dependency before the model is briefed. So the worktree is already dirty when the attempt begins. The call site chains the bumped paths onto the report's `changed_files`, which is the whole of the excusing. The honest report of a bump needing no further work is therefore an empty `changed_files`. The lane holding this puts both kinds of path in one diff. An exclusion widened later to cover the whole diff fails it.

ADR 055 returns a breach to the model as a turn, twice, before this check ends the attempt. The rule below is unchanged, and the check below is still what ends it.

A pre-declaration handshake needs a new tool round-trip and is out of scope. If the post-hoc form proves insufficient, that is a later milestone's change and not a defect in this one. The rescan is what judges the change, which ADR 027 records. A reader who expected the declared-files rule to be a safety check should expect only bookkeeping.
