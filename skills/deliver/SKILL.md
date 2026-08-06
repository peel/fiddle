---
name: deliver
description: Use after implementation to analyze drift, update documentation, evolve evaluation, and close a completed epic.
---

# Deliver

Invoke as `fiddle:deliver --epic <id>`.

Analyze design-versus-implementation drift, update documentation, evolve the evaluator, and close the epic.

## Configuration

`--epic <id>` is required. Read `providers.phases.deliver`, provider commands and flags, timeouts, product artifacts, spot-check, and aging settings from [orchestrate configuration](../orchestrate/configuration.md). Internal subagent models resolve through `scripts/resolve-subagent-model.sh`: `models.roles.<role>` overrides `models.phases.<phase>`, and `default` inherits the current session model; this is independent of provider CLI configuration.

## 1. Validate the epic

Run `beans show <epic-id> --json`. If any child remains `todo` or `in-progress`, warn and ask whether to continue.

## 2. Analyze drift and update artifacts

Follow [drift analysis, documentation, and product artifacts](drift-and-docs.md). Each confirmation point there is required before moving on.

## 3. Evolve the evaluator

Follow [evaluator evolution](evaluator-evolve.md). Blind spot-checking precedes scorecard disclosure, and every calibration, threshold, aging, and close decision requiring confirmation remains attended.

## 4. Close

After the user confirms evaluator evolution, run `beans update <epic-id> --status completed`. Delivery never runs repository-wide bean maintenance implicitly; when explicitly requested, `beans archive` remains a direct maintenance command.
