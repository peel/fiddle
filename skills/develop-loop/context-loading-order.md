# Evaluator Context Loading Order

Provide to all evaluators (claude and external) in the following order:

1. **Evaluation protocol** — `skills/evaluate/SKILL.md`
2. **Domain template** — `skills/evaluate/evaluator-<domain>.md` (as specified in the resolved domain's `template` field, e.g. `evaluator-general.md`, `evaluator-frontend.md`)
3. **Project calibration** (if it exists) — read `evaluators.domains.<domain>.calibration` from `orchestrate.json`. If the key is present, read the file at that path (relative to project root) and include its content immediately after the domain template. If the key is absent, fall back to the default path `docs/evaluator-calibration-<domain>.md` (written by the attended gate in step 1i) and load it if it exists. If neither exists, skip. Position 3 is load-bearing: the anchors have to be in context before the evaluator forms its own scale, or the project's corrections do not apply. Stop reading at the `## Retired` heading if present — retired anchors (deliver 5g) are kept for audit only.
4. **Runtime evidence** (if runtime configured) — `skills/runtime-evidence/SKILL.md` content, plus runtime state (port, domain) so the evaluator can interact with the running app
5. **Runtime/stack agents** (if configured) — if `runtime_agent` or `stack_agents` are configured for the domain in orchestrate.json, read those agent files and include their content
6. **Task criteria** — the bean's acceptance criteria and the domain template's scoring dimensions
7. **Prior scorecards** (if iteration 2+) — the full diff since BASE_SHA (`git diff {BASE_SHA}...HEAD`) and the previous iteration's scorecard with evaluator guidance
8. **Antipatterns** (if configured) — if `evaluators.domains.<domain>.antipatterns` is configured in orchestrate.json, read the antipatterns file and inject its content into the evaluator's `{ANTIPATTERNS}` placeholder, stopping at the `## Retired` heading as above. Loaded last.
