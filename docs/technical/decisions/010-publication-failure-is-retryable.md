# 010 — A publication failure is retryable, and intent is journaled before the effect

Status: accepted

## Context

M0's `fiddle run` executed its capability and then published an evidence bundle, with the two halves split across the crate boundary: `fiddle-runtime` executed, `fiddle-cli` assembled the bundle, published it, and overwrote the outcome on failure. Nothing owned "execute then record". With `<report.dir>` unwritable the marker was written, publication failed, the process exited 20, and no bundle existed anywhere — the world moved and nothing recorded that it moved. M0 self-healed only because `stub_mark` writes a deterministic correlation key, which is a property of the stub rather than of the design; M2's GitHub effects (a branch, a pull request) are not idempotent.

Exit 20 also contradicted its own documentation: `RunOutcome::Failed` means "will not succeed by being repeated as invoked", yet repeating the run after an operator fixed the directory permissions succeeded.

## Decision

The whole attempt lives behind one entry point in `fiddle-runtime`. `fiddle-cli` keeps argument handling, rendering, and exit-code mapping only.

The attempt's intent is journaled under `<report.dir>/.attempts/` **before** the capability mutates anything. A capability whose intent cannot be recorded does not run at all. An attempt interrupted between effect and publication leaves a record a later reader can find, carrying an unknown effect rather than an assumed one.

A publication failure caused by a correctable environment problem reports `Retryable` and exits **11**, not `Failed`/20. Exit 20 stays reachable through an unobservable `<stub.root>`, which asking again genuinely does not fix.

## Consequences

The exit code an operator sees now matches what repeating will actually do, and the two producers of exit 11 — a capability write failure and a publication failure — stay distinguishable by their reason text.

This supersedes what task bean `fiddle-tmcy` converged on, whose criterion `m0-bundle-atomic` asserted exit 20 for this path. That criterion is recorded as superseded on the bean.

The journal is a second on-disk artifact with its own failure mode, and it is written even for attempts that go on to publish successfully, where it is then superseded. Fail-closed on the journal write means a misconfigured `<report.dir>` now prevents work from running at all rather than allowing unrecorded work — the safer direction, but a stricter one: an operator who could previously run with a broken report directory no longer can.
