# 006 — Keep the code_quality threshold at 6 and watch drift with the decay alarm

Date: 2026-07-23
Status: accepted
Cites: skills/evaluate/evaluator-general.md, scripts/trend-eval-history.sh, skills/deliver/evaluator-evolve.md

## Context

An evaluator threshold acts as an equilibrium, not a floor. An unattended loop settles on the lowest passing score, and `code_quality` carries the lowest bar at 6. Raising that bar to 7 moves the equilibrium and leaves the dynamic in place. `skills/evaluate/evaluator-general.md` writes "Default threshold: 6" against that dimension.

## Decision

Keep the per-task `code_quality` threshold at 6. Control long-term drift with the decay alarm instead, which `scripts/trend-eval-history.sh` computes and the DELIVER evaluator-evolve step reports. Fold every dimension the alarm names into that step's calibration work. `skills/deliver/evaluator-evolve.md` prints the alarm on a line that starts "Decay alarm".

## Consequences

- A small or short-lived task does not have to clear a bar written for long-lived code.
- The project gives up per-task regression cover. A single task may land at exactly 6, and the alarm catches the decline only at the epic boundary.
- The alarm needs two epics of eval-log history. With less, `trends` is null and the run reports insufficient history.
- The control only works if somebody runs the evaluator-evolve step every epic.
- A later decision may still raise the threshold, and would supersede this one.

This ADR placed the alarm at "DELIVER step 5f". The deliver skill now has five steps, and the alarm lives in `skills/deliver/evaluator-evolve.md`, which step 3 reaches. `skills/evaluate/evaluator-general.md` still names step 5f as well.
