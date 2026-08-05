# Skill Authoring Improvements Implementation Plan

> **For agentic workers:** Use fiddle:develop to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve skill routing, progressive disclosure, validation, and internal subagent model selection while preserving Fiddle's lifecycle behavior.

**Architecture:** Keep `skills/*/SKILL.md` as concise routers and move phase-local detail into companion references. Add one repository audit script with deterministic shell tests and invoke it from the existing portability check and CI. Add a shared model-resolution contract for internal subagents; explicit role overrides win over phase defaults, and `default` means omit the model override so the current harness session model is inherited.

**Tech Stack:** Markdown skill files, Bash, jq, JSON configuration, GitHub Actions.

---

### Task 1: Skill audit validator and CI gate

**Files:**
- Create: `scripts/audit-skills.sh`
- Create: `scripts/test-audit-skills.sh`
- Modify: `scripts/check-portability.sh`
- Create: `.github/workflows/skill-quality.yml`

- [ ] **Step 1: Write failing audit tests**

Create isolated fixture directories under a temporary directory in `scripts/test-audit-skills.sh`. Assert that `audit-skills.sh` exits 0 for valid frontmatter, existing references, dynamic evaluator-template references, and skills under the configured size limit; assert exit 2 with machine-readable errors for malformed frontmatter, a non-router description, a missing referenced file, an orphaned companion file, and an oversized `SKILL.md`.

- [ ] **Step 2: Run the audit tests and verify they fail**

Run: `rtk bash scripts/test-audit-skills.sh`
Expected: FAIL because `scripts/audit-skills.sh` does not exist.

- [ ] **Step 3: Implement the validator**

Implement `scripts/audit-skills.sh` with explicit exit contracts: exit 0 for a clean tree, exit 2 for skill-quality violations, and JSON errors on stderr. Validate frontmatter name/directory agreement, concise trigger-first descriptions, relative reference existence, companion-file reachability, and the maximum primary skill size. Permit dynamic evaluator template references documented by the existing assembly script.

- [ ] **Step 4: Run the audit tests and verify they pass**

Run: `rtk bash scripts/test-audit-skills.sh`
Expected: all fixture cases pass, including every intentional failure being reported with exit 2.

- [ ] **Step 5: Integrate the audit gate**

Call `scripts/audit-skills.sh` from `scripts/check-portability.sh` after existing metadata checks. Add `.github/workflows/skill-quality.yml` to run the portability check and focused audit tests on pushes and pull requests, without requiring external providers.

- [ ] **Step 6: Run the integrated checks**

Run: `rtk bash scripts/check-portability.sh && rtk bash scripts/test-audit-skills.sh`
Expected: portability and audit checks pass on the repository tree.

- [ ] **Step 7: Commit**

Commit: `test: enforce skill quality and disclosure invariants`

```eval
domains: [general]
criteria:
  general:
    - id: validator-contract
      check: "The audit script returns exit 0 for valid fixtures, including documented dynamic evaluator-template references, and exit 2 with machine-readable errors for malformed frontmatter and every other invalid fixture category."
    - id: portability-integration
      check: "The existing portability check invokes the audit and still passes on the repository tree."
    - id: ci-gate
      check: "A CI workflow runs portability and audit checks without external provider dependencies."
thresholds: {}
```

### Task 2: Rewrite trigger-first descriptions

**Files:**
- Modify: `skills/adr/SKILL.md`
- Modify: `skills/backlog/SKILL.md`
- Modify: `skills/brainstorm/SKILL.md`
- Modify: `skills/challenge/SKILL.md`
- Modify: `skills/define/SKILL.md`
- Modify: `skills/define-beans/SKILL.md`
- Modify: `skills/deliver/SKILL.md`
- Modify: `skills/deliver-docs/SKILL.md`
- Modify: `skills/discover/SKILL.md`
- Modify: `skills/discover-docs/SKILL.md`
- Modify: `skills/feedback/SKILL.md`

- [ ] **Step 1: Write the failing metadata assertion**

Extend `scripts/test-audit-skills.sh` fixtures with the eleven current descriptions and assert that the repository snapshot fails the trigger-first rule before the descriptions are changed.

- [ ] **Step 2: Run the focused assertion and verify it fails**

Run: `rtk bash scripts/test-audit-skills.sh`
Expected: FAIL for the eleven descriptions that do not begin with a supported trigger phrase.

- [ ] **Step 3: Rewrite the descriptions**

Replace only the `description:` frontmatter values with one concise sentence of approximately 15–25 words. Each sentence must say when to load the skill first, then its scope; omit workflow steps, examples, and implementation details.

- [ ] **Step 4: Run the focused assertion and verify it passes**

Run: `rtk bash scripts/test-audit-skills.sh && rtk bash scripts/check-portability.sh`
Expected: all description and portability checks pass.

- [ ] **Step 5: Commit**

Commit: `docs: make skill descriptions trigger-first`

```eval
domains: [general]
criteria:
  general:
    - id: concise-routing
      check: "All eleven targeted descriptions are one sentence, trigger-first, and approximately 15–25 words without procedural detail."
    - id: metadata-preservation
      check: "Skill names, directory names, and all non-description frontmatter remain unchanged."
thresholds: {}
```

### Task 3: Split orchestrate and deliver routers

**Files:**
- Modify: `skills/orchestrate/SKILL.md`
- Create: `skills/orchestrate/configuration.md`
- Create: `skills/orchestrate/resumption.md`
- Create: `skills/deliver/drift-and-docs.md`
- Create: `skills/deliver/evaluator-evolve.md`
- Modify: `skills/deliver/SKILL.md`
- Test: `scripts/test-audit-skills.sh`

- [ ] **Step 1: Write the reference-integrity fixture**

Add fixtures asserting that the router files reference each extracted companion and that the companion files are reachable from a skill entrypoint.

- [ ] **Step 2: Run the fixture and verify it fails**

Run: `rtk bash scripts/test-audit-skills.sh`
Expected: FAIL because the new companion files do not exist.

- [ ] **Step 3: Extract durable sections without changing semantics**

Keep each primary file's frontmatter, purpose, usage, high-level phase flow, invariants, and links. Move configuration schema/defaults and resumption details from `orchestrate` into its references; move drift/documentation and evaluator-evolve procedures from `deliver` into its references. Link each reference at the point where it is needed and remove duplicated prose from the routers.

- [ ] **Step 4: Run reference and behavioral checks**

Run: `rtk bash scripts/test-audit-skills.sh && rtk bash scripts/check-portability.sh`
Expected: no broken or orphaned references, and existing portability checks pass.

- [ ] **Step 5: Commit**

Commit: `docs: route orchestration details through references`

```eval
domains: [general]
criteria:
  general:
    - id: router-thinning
      check: "Orchestrate and deliver primary files retain routing purpose, high-level flow, invariants, and links while moving detailed procedures into reachable references."
    - id: semantic-preservation
      check: "All configuration, phase-transition, drift, documentation, and evaluator-evolve requirements from the original files remain represented exactly once."
    - id: reference-integrity
      check: "The audit and portability checks report no missing or orphaned references after the split."
thresholds: {}
```

### Task 4: Split write-plan and develop-loop routers

**Files:**
- Modify: `skills/write-plan/SKILL.md`
- Create: `skills/write-plan/plan-format.md`
- Create: `skills/write-plan/bean-materialization.md`
- Modify: `skills/develop-loop/SKILL.md`
- Create: `skills/develop-loop/dispatch-and-evidence.md`
- Create: `skills/develop-loop/convergence-and-recovery.md`
- Modify: `scripts/test-audit-skills.sh`

- [ ] **Step 1: Write extraction-integrity fixtures**

Add fixtures that require every new write-plan and develop-loop reference to be linked from its primary router and that each router remains below the configured size limit.

- [ ] **Step 2: Run the fixtures and verify they fail**

Run: `rtk bash scripts/test-audit-skills.sh`
Expected: FAIL because the new reference files and links do not exist.

- [ ] **Step 3: Extract detailed procedures**

Keep each primary file's purpose, usage, high-level lifecycle, hard invariants, and references. Move plan templates/evaluation blocks/bean materialization from `write-plan` into focused references; move implementer/evidence dispatch and convergence/recovery procedures from `develop-loop` into focused references. Preserve exact output contracts, hold-out behavior, budget handling, and restart semantics.

- [ ] **Step 4: Run all disclosure checks**

Run: `rtk bash scripts/test-audit-skills.sh && rtk bash scripts/check-portability.sh && rtk bash scripts/test-assemble-evaluator-context.sh`
Expected: audit, portability, and evaluator-context tests pass.

- [ ] **Step 5: Commit**

Commit: `docs: split planning and evaluation protocols`

```eval
domains: [general]
criteria:
  general:
    - id: protocol-preservation
      check: "Write-plan and develop-loop retain their required output schemas, bean gates, evidence ordering, convergence exits, hold-out handling, and restart behavior."
    - id: progressive-routing
      check: "The four primary files are under the configured size limit and route to reachable references for detailed procedures."
    - id: regression-safety
      check: "Audit, portability, and evaluator-context tests all pass after extraction."
thresholds: {}
```

### Task 5: Configurable internal subagent models

**Files:**
- Modify: `orchestrate.json`
- Create: `scripts/resolve-subagent-model.sh`
- Create: `scripts/test-resolve-subagent-model.sh`
- Modify: `skills/using-fiddle/SKILL.md`
- Modify: `skills/using-fiddle/references/claude-tools.md`
- Modify: `skills/using-fiddle/references/codex-tools.md`
- Modify: `skills/using-fiddle/references/pi-tools.md`
- Modify: `skills/orchestrate/SKILL.md`
- Modify: `skills/define/SKILL.md`
- Modify: `skills/develop-loop/SKILL.md`
- Modify: `skills/develop-holistic/SKILL.md`
- Modify: `skills/panel/SKILL.md`
- Modify: `skills/brainstorm/SKILL.md`
- Modify: `skills/develop/SKILL.md`
- Modify: `skills/deliver/SKILL.md`

- [ ] **Step 1: Write model-resolution tests**

Create fixtures covering explicit role override, phase default, `"default"` session inheritance, missing configuration, every internal dispatch role (`panel`, `brainstorm`, `implementer`, `evaluator`, `holistic`, and `deliver`), and the rule that external provider selection is not affected. Assert the resolver emits a model only when a non-default value is selected.

- [ ] **Step 2: Run the model-resolution tests and verify they fail**

Run: `rtk bash scripts/test-resolve-subagent-model.sh`
Expected: FAIL because `scripts/resolve-subagent-model.sh` does not exist.

- [ ] **Step 3: Implement deterministic model resolution**

Implement `scripts/resolve-subagent-model.sh --phase <phase> --role <role> --config orchestrate.json`. Resolve `models.roles.<role>` first, then `models.phases.<phase>`, then `"default"`; emit JSON with `source` and `model`, omitting the model value for session inheritance. Reject malformed or unsupported values with exit 2 and a JSON error.

- [ ] **Step 4: Document and wire the contract**

Add the `models` schema to `orchestrate.json` with `"default"` values and a small set of role keys for internal subagents. Update the harness mappings and every internal dispatch path in `panel`, `brainstorm`, `develop`, `develop-loop`, `develop-holistic`, and `deliver` so each consults the resolver, passes an explicit model only when configured, and never treats provider CLI names as model values.

- [ ] **Step 5: Run model and portability checks**

Run: `rtk bash scripts/test-resolve-subagent-model.sh && rtk bash scripts/check-portability.sh`
Expected: all resolver cases and portability checks pass.

- [ ] **Step 6: Commit**

Commit: `feat: configure internal subagent models`

```eval
domains: [general]
criteria:
  general:
    - id: model-precedence
      check: "The resolver selects role override before phase default and otherwise returns session inheritance for default or absent configuration."
    - id: dispatch-contract
      check: "All internal dispatch paths in panel, brainstorm, develop, develop-loop, develop-holistic, and deliver use the resolver contract, while external provider CLI selection remains independent."
    - id: invalid-config
      check: "Malformed or unsupported model configuration exits 2 with a machine-readable error and does not silently fall back."
thresholds: {}
```

### Task 6: Optional agent-empathy prompts

**Files:**
- Modify: `skills/discover-docs/SKILL.md`
- Modify: `skills/brainstorm/SKILL.md`
- Modify: `skills/using-fiddle/SKILL.md`
- Modify: `scripts/test-audit-skills.sh`

- [ ] **Step 1: Write prompt-presence assertions**

Add fixture assertions that the optional prompts appear in discovery/brainstorming guidance, are phrased as conditional questions, and do not add a mandatory step to every run.

- [ ] **Step 2: Run the assertions and verify they fail**

Run: `rtk bash scripts/test-audit-skills.sh`
Expected: FAIL because the new prompt guidance is absent.

- [ ] **Step 3: Add the optional prompts**

Add concise guidance asking, when relevant, what the agent can do with available tools, what context or capability it needs, and what the previous run suggests improving. Keep the prompts conditional and preserve the existing discovery flow.

- [ ] **Step 4: Run prompt and portability checks**

Run: `rtk bash scripts/test-audit-skills.sh && rtk bash scripts/check-portability.sh`
Expected: all prompt and portability checks pass.

- [ ] **Step 5: Commit**

Commit: `docs: add optional agent-feedback prompts`

```eval
domains: [general]
criteria:
  general:
    - id: optional-prompts
      check: "Discovery and brainstorming contain concise conditional prompts for capabilities, needed context/tools, and prior-run improvement."
    - id: no-mandatory-ceremony
      check: "The prompts do not introduce a required questionnaire or alter the existing phase routing."
thresholds: {}
```

### Task 7: Full regression verification

**Files:**
- Modify: `docs/technical/SYSTEM.md`
- Test: `scripts/test-audit-skills.sh`
- Test: `scripts/test-resolve-subagent-model.sh`
- Test: `scripts/test-assemble-evaluator-context.sh`
- Test: `scripts/test-validate-bean-body.sh`
- Test: `scripts/test-validate-scorecard.sh`

- [ ] **Step 1: Run the complete focused suite**

Run: `rtk bash scripts/check-portability.sh && rtk bash scripts/test-audit-skills.sh && rtk bash scripts/test-resolve-subagent-model.sh && rtk bash scripts/test-assemble-evaluator-context.sh && rtk bash scripts/test-validate-bean-body.sh && rtk bash scripts/test-validate-scorecard.sh`
Expected: every command exits 0 and every test reports zero failures.

- [ ] **Step 2: Update ground-truth documentation**

Update `docs/technical/SYSTEM.md` only if the final implementation changes the documented skill tree, validation gates, or model-resolution contract. Keep the entry factual and within the existing schema.

- [ ] **Step 3: Commit**

Commit: `test: verify skill authoring improvements`

```eval
domains: [general]
criteria:
  general:
    - id: full-regression
      check: "Portability, skill-audit, model-resolution, evaluator-context, bean-body, and scorecard checks all pass with zero failures."
    - id: documentation-accuracy
      check: "SYSTEM.md accurately describes any changed validation or model-resolution behavior, or remains unchanged when no documented behavior changed."
thresholds: {}
```

## Self-review

- Scope covers all five approved changes and excludes usage-history review.
- CI validation is included without external provider dependencies.
- Model precedence is explicit: role override, phase default, then session inheritance.
- Each task has exact files, actionable steps, deterministic commands, and an evaluation block.
- Reference splits preserve the required lifecycle and evaluator contracts rather than changing behavior.