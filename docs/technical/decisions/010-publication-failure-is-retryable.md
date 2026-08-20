# 010 — A publication failure is retryable, and the journal records the intent first

Status: accepted
Cites: fiddle_runtime::attempt, journal::JOURNAL_DIR, RunOutcome::Retryable, RunOutcome::Failed, exit_code_for

## Context

M0's `fiddle run` executed a capability and then published a bundle, with the crate boundary splitting the two halves. `fiddle-cli` assembled the bundle, published it and overwrote the outcome on failure, so nothing owned "execute then record". With an unwritable report directory the marker was written, publication failed, the process exited 20, and no bundle existed.

## Decision

Put the whole attempt behind one entry point in `fiddle-runtime`, leaving `fiddle-cli` arguments, rendering and exit codes. Journal the attempt's intent before the capability mutates anything, and refuse a capability whose intent cannot be recorded. Report a publication failure that an operator can correct as `Retryable`, and exit 11.

## Consequences

- The exit code an operator sees now matches what repeating will do. The two producers of exit 11 stay apart by their reason text.
- Exit 20 stays reachable through an unobservable stub root, which asking again genuinely does not fix.
- An attempt interrupted between the effect and the publication leaves a record. That record carries an unknown effect rather than an assumed one.
- The journal is a second on-disk artifact with its own failure mode. A run writes it even for an attempt that goes on to publish.
- The project gave up the looser configuration. An operator who could run with a broken report directory can no longer run at all. That is safer, and stricter.

M0 self-healed only because `stub_mark` writes a deterministic correlation key. That is a property of the stub rather than of the design. M2's GitHub effects are not idempotent. Exit 20 also contradicted `RunOutcome::Failed`'s own promise: the run will not succeed by being repeated as invoked. Repeating after an operator fixed the permissions did succeed.

This supersedes what task bean `fiddle-tmcy` converged on. Its criterion `m0-bundle-atomic` asserted exit 20 for this path, and the bean records that criterion as superseded.
