---
name: develop-loop
description: Use when a single task bean needs implementation and evaluation — called by fiddle:develop, not directly.
---

# Develop Loop — Single Bean Evaluation

Invoke as `fiddle:develop-loop --bean <id> --epic <id>`.

For every task, implement, gather evidence, evaluate each domain, merge scorecards, and iterate to a recorded terminal verdict. A shortened chain cannot turn an implementer claim into evidence.

## Configuration

Both `--bean` and `--epic` are required. Read evaluator configuration from [orchestrate configuration](../orchestrate/configuration.md): attended mode, dispatch budget, and per-domain template, providers, calibration, antipatterns, and thresholds.

## 1. Dispatch and evidence

Follow [dispatch and evidence](dispatch-and-evidence.md). It owns restart entry, evaluation-log initialization, active-marker lifecycle, domain resolution, implementer dispatch, evidence capture, runtime lifecycle, provider selection, evaluator dispatch, and scorecard validation.

## 2. Merge and converge

Follow [convergence and recovery](convergence-and-recovery.md). It owns scorecard merging, spec-defect exits, attended corrections, threshold and convergence verdicts, durable logging, re-dispatch behavior, and terminal states.
