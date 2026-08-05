---
name: orchestrate
description: Use when starting a full development lifecycle for a feature or epic — chains discover, define, develop, and deliver.
---

# Orchestrate

Invoke as `fiddle:orchestrate <topic> [--epic <id>] [--no-triage] [--skip-discover] [--skip-challenge]`.

Sequence DISCOVER → DEFINE → DEVELOP → DELIVER. Each phase remains an independent skill; orchestrate routes between them, propagates flags, tracks state, and resumes from beans.

## Setup

Read [configuration](configuration.md) and then follow [setup and resumption](resumption.md). Store final settings before any phase runs.

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

## DEVELOP

Invoke `fiddle:develop --epic <epic-id>`. Only after it returns, replace `orchestrate-phase:DEVELOP` with `orchestrate-phase:DELIVER`.

## DELIVER

Invoke `fiddle:deliver --epic <epic-id>`.

## Cleanup

Remove `orchestrate-phase:DELIVER`, then list epic children and report completed and `needs-attention` counts. Remind the user to run `fiddle:deliver-docs --epic <epic-id>` only when deliver did not already run it.
