# Evaluator Context Loading Order

Provide to all evaluators (claude and external) in the following order:

1. **Evaluation protocol** — `skills/evaluate/SKILL.md`
2. **Domain template** — `skills/evaluate/evaluator-<domain>.md` (as specified in the resolved domain's `template` field, e.g., `evaluator-general.md`, `evaluator-frontend.md`)
3. **Project calibration** (if exists) — read `evaluators.domains.<domain>.calibration` from `orchestrate.json`. If the key is present, read the file at that path (relative to project root) and include its content immediately after the domain template. If the key is absent, check whether the default path `docs/evaluator-calibration-<domain>.md` exists (this file is created when the attended gate writes anchors in step 1i). If the default file exists, load it. If neither the config key nor the default file exists, skip.
4. **Runtime evidence** (if runtime configured) — `skills/runtime-evidence/SKILL.md` content, plus runtime state (port, domain) so the evaluator can interact with the running app
5. **Runtime/stack agents** (if configured) — if `runtime_agent` or `stack_agents` are configured for the domain in orchestrate.json, read those agent files and include their content
6. **Task criteria** — the bean's acceptance criteria and the domain template's scoring dimensions
7. **Prior scorecards** (if iteration 2+) — the full diff since BASE_SHA (`git diff {BASE_SHA}...HEAD`) and the previous iteration's scorecard with evaluator guidance
8. **Antipatterns** (if configured) — if `evaluators.domains.<domain>.antipatterns` is configured in orchestrate.json, read the antipatterns file and inject its content into the evaluator's `{ANTIPATTERNS}` placeholder. This is loaded last in the evaluator context.
