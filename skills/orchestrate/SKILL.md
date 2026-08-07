---
name: orchestrate
description: Use when starting or resuming a full development lifecycle for a feature or explicit epic.
---

# Orchestrate

Invoke as `fiddle:orchestrate [<topic>] [--epic <id>] [--no-triage] [--skip-discover] [--skip-challenge]`.

Sequence DISCOVER → DEFINE → SEED → DEVELOP → DELIVER. SEED applies to an existing seed-aware epic; legacy epics retain the old DEFINE/DEVELOP path. Each phase remains an independent skill or private protocol. Orchestrate routes between them, propagates flags, tracks state, and resumes from durable beans.

## Setup

Read [configuration](configuration.md) and then follow [setup and resumption](resumption.md). Store final settings and the canonical main-worktree Beans path before any phase runs. Use `scripts/resolve-orchestrate-phase.sh` to select every resumed phase; never infer a phase from a single status or phase tag.

## Triage

Skip triage with `--epic`, `--no-triage`, or `--skip-discover`. Otherwise use `fiddle:quickfix` only when every condition holds:

1. One clear, self-contained change.
2. An obvious implementation path.
3. At most five files.
4. No new infrastructure, patterns, or pipelines.
5. No cross-cutting subsystem coordination.

If quickfix returns `TOO_COMPLEX`, continue at DISCOVER. A successful quickfix ends this lifecycle.

## DISCOVER

Skip when `--skip-discover` is set or resumption already has child beans. Invoke `fiddle:discover <topic>` and pass `--skip-docs` and `--skip-challenge` when set. After discovery, replace `orchestrate-phase:DISCOVER` with `orchestrate-phase:DEFINE` when an epic exists.

## DEFINE

Invoke `fiddle:define <topic>`, passing `--skip-challenge` and `--skip-panel` when set. If this invocation created the epic, find the most recently created todo epic with `beans list --json -t epic -s todo`. Replace `orchestrate-phase:DEFINE` with `orchestrate-phase:DEVELOP`.

## SEED

When the resolver returns `SEED`, follow [seed planning](seed-planning.md). Rerun setup and resumption afterward; proceed only when the resolver returns `DEVELOP`. `NEEDS_CONTEXT` and `INVALID` are blocking results and must be reported with their reason.

## DEVELOP

Only when the resolver returns `DEVELOP`, invoke `fiddle:develop --epic <epic-id>`. After it returns, rerun the resolver instead of assuming delivery is ready.

## DELIVER

Only when the resolver returns `DELIVER`, invoke `fiddle:deliver --epic <epic-id>`.

## Cleanup

When the resolver returns `DONE`, remove the phase tag, then list epic children and report completed and `needs-attention` counts. Remind the user to run `fiddle:deliver-docs --epic <epic-id>` only when deliver did not already run it.
