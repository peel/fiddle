---
# fiddle-4ask
title: Harden the evaluator loop against quality drift
status: in-progress
type: epic
priority: normal
tags:
    - orchestrate-phase:DELIVER
created_at: 2026-07-23T13:31:39Z
updated_at: 2026-07-24T08:39:08Z
---

Improvements derived from mapping the NATO 1968/1969 software engineering reports onto fiddle's factory model. Common thread: the develop loop's quality signals must stay independent, grounded in the actual code, and measured over time, or the system converges to threshold-level output whose decay surfaces months later (Fraser, NATO 1968: apparent productivity increase, quality drop that did not come to light until very much later).

Scope: evaluator provider independence, blind calibration, longitudinal decay metrics, spec-defect routing, threshold equilibrium and hold-out criteria, calibration anchor aging.
