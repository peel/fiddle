# Claude-5-Era Skill Slim-Down Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use fiddle:develop to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite all 51 skill files to the judgment-plus-rationale house style, move the two remaining mechanical gates into exit-2 validators, dedup the config schema, and keep every ordering/honesty invariant as one plain statement in its owning file.

**Architecture:** Two new bash validators with test suites land first; then four staged family rewrites, each gated by the full test sweep. The spec (docs/specs/2026-07-29-claude5-slimdown-design.md) is the rewrite contract: its House Style and Prompt-Side Invariant Set sections define exactly what survives and how it reads.

**Tech Stack:** bash + jq validators with `test-*.sh` harnesses; markdown skill rewrites.

---

### Task 1: validate-bean-body.sh

**Files:**
- Create: `scripts/validate-bean-body.sh`
- Test: `scripts/test-validate-bean-body.sh`

- [ ] Write failing test (assert_exit/assert_json harness, mktemp fixtures): (1) complete body (fenced ```eval with `domains:` and `criteria:`, a `## Files` section with a `- Create:`/`- Modify:`/`- Test:` line, a `- [ ]` checklist) → exit 0; (2) missing eval block → exit 2, JSON error on stderr naming "eval block"; (3) eval block without `criteria:` → exit 2; (4) missing files section → exit 2 naming "files"; (5) no checkbox steps → exit 2 naming "steps"; (6) container feature bean flag `--container` → exit 0 regardless (exempt); (7) missing --body file → exit 2.
- [ ] Run test, verify failure (script missing).
- [ ] Implement: `validate-bean-body.sh --body <file> [--container]`. Greps: fenced eval block containing `domains:` and `criteria:`; `## Files`-or-`Files:` with at least one Create/Modify/Test line; at least one `- [ ]`. All failures collected into one JSON error array on stderr, exit 2. Header comment documents exit codes 0/2. chmod +x.
- [ ] Run test green; full sweep `for t in scripts/test-*.sh; do bash "$t" >/dev/null 2>&1 || echo "FAIL: $t"; done` clean.
- [ ] Commit (PREK_ALLOW_NO_CONFIG=1; imperative title, Previously/Now body).

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: complete-body-passes
      check: "A body with fenced eval block, files section, and checklist exits 0"
    - id: each-gap-named
      check: "Missing eval block, criteria key, files section, or steps each exit 2 with a JSON error naming the gap"
    - id: container-exempt
      check: "--container exits 0 regardless of body content"
thresholds: {}
```

### Task 2: validate-scorecard.sh

**Files:**
- Create: `scripts/validate-scorecard.sh`
- Test: `scripts/test-validate-scorecard.sh`

- [ ] Write failing test: (1) valid scorecard (provider, dimensions object with non-empty evidence per scored dimension, criteria matching --criteria-ids "a,b" each with non-empty evidence, no spec_defect) → exit 0; (2) missing provider → exit 2; (3) criteria id not in --criteria-ids, or an expected id missing → exit 2 naming the id; (4) empty evidence on a criterion or scored dimension → exit 2; (5) dimensions present but not an object → exit 2; (6) explicit empty dimensions `{}` → exit 0 (evidence-only is valid); (7) spec_defect present with detected true but no reason → exit 2; (8) malformed JSON → exit 2.
- [ ] Run test, verify failure.
- [ ] Implement: `validate-scorecard.sh --scorecard <file> --criteria-ids <comma-list>` (ids extracted by the orchestrator from the bean's eval block, mirroring how resolve-domains.sh receives --domains). jq checks per the test matrix; all failures in one JSON error array on stderr, exit 2.
- [ ] Run test green; full sweep clean.
- [ ] Commit.

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: valid-passes-invalid-named
      check: "Valid scorecards exit 0; each malformation (provider, criteria ids, empty evidence, dimensions type, spec_defect shape, bad JSON) exits 2 with a JSON error naming it"
    - id: evidence-only-valid
      check: "Explicit empty dimensions object exits 0"
    - id: interface-convention
      check: "Criteria ids arrive via --criteria-ids argument; the script parses no YAML"
thresholds: {}
```

### Task 3: Develop core rewrite + validator wiring

**Files:**
- Modify: `skills/develop/SKILL.md`, `skills/develop-loop/SKILL.md`, `skills/develop-loop/scorecard-merge.md`, `skills/develop-loop/restart-recovery.md`, `skills/develop-loop/attended-gate.md`, `skills/develop-loop/context-loading-order.md`, `skills/develop/implementer-prompt.md`, `skills/develop/provider-context.md`
- Delete: `skills/develop/iron-laws.md`

- [ ] Rewrite each file to the spec's House Style: strip HARD-GATE/GATE markup, caps emphasis, rationalization tables, Red Flags, Announce lines; keep flow, plain script invocations, cross-skill pointers, frontmatter descriptions verbatim. Retain as single plain statements with one-line rationale (spec's Invariant Set): implementer-DONE-is-a-claim, budget-exceeded-ask-human, spec-defect routing, hold-out redaction, evaluator distrust/evidence/output contract, runtime-evidence-before-scoring, attended and blind ordering, holistic-after-loop-before-finish. Keep the Stop-hook marker lifecycle lines and all script invocations exactly.
- [ ] Wire validators: develop Step 1 becomes "For each task bean run scripts/validate-bean-body.sh --body <body-file> (--container for pure container features); stop on exit 2 and report the JSON errors." develop-loop 1f gains, after each evaluator returns: "Run scripts/validate-scorecard.sh --scorecard <file> --criteria-ids <ids from the eval block>; on exit 2 re-dispatch that evaluator once, then mark the bean needs-attention." Delete iron-laws.md and both references to it.
- [ ] Verify: `grep -rn 'HARD-GATE\|Rationalization\|## Red Flags\|\*\*Announce\|iron-laws' skills/develop skills/develop-loop` → empty; caps greps (`grep -rw 'MUST\|NEVER'`) only inside frontmatter descriptions, JSON schema/interface text, or quoted external content; full sweep clean; word count of the covered files reduced by at least a third (record before/after in the bean summary).
- [ ] Commit.

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No HARD-GATE, rationalization table, Red Flags, Announce, or iron-laws reference remains in the develop core files"
    - id: invariants-survive
      check: "Each Invariant Set statement owned by these files appears exactly once, plainly, with rationale"
    - id: validators-wired
      check: "develop runs validate-bean-body.sh per bean; develop-loop runs validate-scorecard.sh per evaluator return with the re-dispatch-once policy"
    - id: sweep-green
      check: "Full test sweep passes at the family checkpoint"
thresholds: {}
```

### Task 3b: Evaluation family rewrite

**Files:**
- Modify: `skills/evaluate/SKILL.md`, `skills/evaluate/evaluator-general.md`, `skills/evaluate/evaluator-infrastructure.md`, `skills/evaluate/evaluator-frontend.md`, `skills/evaluate/evaluator-backend.md`, `skills/develop-holistic/SKILL.md`, `skills/develop/holistic-review.md`, `skills/develop/holistic-dimensions.md`, `skills/develop/holistic-scorecard-schema.md`, `skills/runtime-evidence/SKILL.md`

- [ ] Rewrite to House Style (same removals/retentions as Task 3). Invariants surviving plainly here: evaluator distrust/evidence citation/output contract (the scorecard JSON schema stays verbatim — it is an interface), runtime-evidence-before-scoring, holistic-after-loop. Keep the explicit dimensions:{} contract text.
- [ ] Verify: emphasis greps empty for `skills/evaluate skills/develop-holistic skills/develop/holistic-*.md skills/runtime-evidence`; caps allowance as in Task 3; full sweep clean; word counts recorded.
- [ ] Commit.

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No emphatic markup remains in the evaluation family"
    - id: contracts-survive
      check: "Scorecard JSON schema, dimensions:{} contract, distrust and evidence-citation invariants each appear once, plainly"
    - id: sweep-green
      check: "Full test sweep passes at the family checkpoint"
thresholds: {}
```

### Task 4: Lifecycle family rewrite + config dedup

**Files:**
- Modify: `skills/orchestrate/SKILL.md`, `skills/discover/SKILL.md`, `skills/discover-docs/SKILL.md`, `skills/define/SKILL.md`, `skills/deliver/SKILL.md`, `skills/deliver/blind-spot-check.md`, `skills/deliver-docs/SKILL.md`, `skills/quickfix/SKILL.md`

- [ ] Rewrite to House Style (same removals/retentions as Task 3; blind-before-reveal invariant survives in blind-spot-check.md as one plain statement with its anchoring rationale).
- [ ] Config dedup: orchestrate/SKILL.md keeps the single full orchestrate.json schema; develop and develop-loop (already rewritten in Task 3 — touch only their config sections if the pointer wasn't placed then) and the lifecycle skills replace their embedded config JSON blocks with one line: "Config schema: see skills/orchestrate/SKILL.md; this skill reads <keys>." Resolve any contradictions found between the copies (record them in the commit body).
- [ ] Verify: `grep -rln 'max_dispatches_per_task\|"providers": {\|"evaluators": {' skills/` → schema blocks in skills/orchestrate/SKILL.md only (key-name prose mentions elsewhere allowed, embedded JSON schema blocks not); emphasis greps empty for the family; full sweep clean.
- [ ] Commit.

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No emphatic markup remains in the lifecycle family"
    - id: schema-single-home
      check: "The orchestrate.json schema block exists only in orchestrate/SKILL.md; other skills name the keys they read and point there"
    - id: sweep-green
      check: "Full test sweep passes at the family checkpoint"
thresholds: {}
```

### Task 5: Process family rewrite

**Files:**
- Modify: `skills/brainstorm/SKILL.md`, `skills/write-plan/SKILL.md`, `skills/define-beans/SKILL.md`, `skills/challenge/SKILL.md`, `skills/panel/SKILL.md`, `skills/tdd/SKILL.md`, `skills/debug/SKILL.md`, `skills/debug/root-cause-tracing.md`, `skills/debug/defense-in-depth.md`, `skills/debug/condition-based-waiting.md`, `skills/verify/SKILL.md`, `skills/worktrees/SKILL.md`, `skills/finish-branch/SKILL.md`

- [ ] Rewrite to House Style. Invariants surviving plainly here: approval-before-implementation (brainstorm), red-first ordering (tdd), root-cause-before-fix and stop-after-repeated-failures (debug), verification-before-claiming (verify), typed-confirmation-before-discard (finish-branch). Tables of rationalizations/red flags collapse to at most one sentence each where the insight is real.
- [ ] Verify: emphasis greps empty for the family; cross-skill pointers intact (grep each "Use the fiddle:" reference still present); full sweep clean.
- [ ] Commit.

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No emphatic markup remains in the process family"
    - id: ordering-invariants-survive
      check: "Approval-first, red-first, root-cause-first, verify-first, and typed-discard each appear once, plainly, with rationale"
    - id: handoffs-intact
      check: "Every cross-skill invocation pointer present before the rewrite is still present"
thresholds: {}
```

### Task 6: Utilities family + anti-regression + final audit

**Files:**
- Modify: `skills/using-fiddle/SKILL.md` + its references/, `skills/adr/SKILL.md`, `skills/backlog/SKILL.md`, `skills/feedback/SKILL.md`, `skills/archive/SKILL.md`, `skills/init/SKILL.md`, `skills/insights/SKILL.md`, and every `skills/**/*.md` not covered by Tasks 3-5 (generate the list at task start with `find skills -name '*.md'` minus the enumerated files; record it in the bean summary so completion is checkable)
- Modify: `docs/technical/SYSTEM.md`

- [ ] Rewrite remaining files to House Style; frontmatter `description` fields and every cross-skill handoff pointer preserved verbatim, as in all families.
- [ ] Anti-regression: add one invariant to SYSTEM.md ("Skills are written as judgment plus rationale; mechanical invariants live in scripts with exit-code contracts; no emphatic markup") and a three-line authoring note in using-fiddle stating the house style for contributors.
- [ ] Final audit across the whole tree: `grep -rn 'HARD-GATE\|<GATE>\|Rationalization Prevention\|## Red Flags\|\*\*Announce' skills/` → empty; `grep -rwn 'MUST\|NEVER' skills/` hits only frontmatter descriptions, JSON schema/interface text, or quoted external content (list every hit with its justification in the bean's Summary of Changes — durable on the bean, not only the commit body); record tree word-count before/after in the bean summary (target: substantial reduction).
- [ ] Full sweep + portability (run the main-checkout check-portability.sh against the tree if present); commit.

```eval
domains: [general]
criteria:
  general:
    - id: tree-audit-clean
      check: "Guardrail greps across skills/ are empty except justified frontmatter/schema/quoted hits, each listed in the commit body"
    - id: anti-regression-landed
      check: "SYSTEM.md carries the house-style invariant and using-fiddle carries the authoring note"
    - id: sweep-green
      check: "Full test sweep and available portability checks pass"
thresholds: {}
```

## Critique Round Record (2026-07-30)

Codex findings folded: Task 3 split (3 + 3b), references enumerated (holistic-*, debug/*), Task 6 remaining-files scope made mechanical and audit made durable on the bean, Task 4 dedup grep strengthened, caps allowance aligned with the spec, description/handoff preservation restated in Task 6. Rejected: splitting Tasks 4-6 further — they are zero-TDD-cycle doc sweeps within one implementer's turn budget, and finer splits multiply evaluation dispatches without adding safety beyond the family checkpoints. Gemini returned no output (second consecutive failure; reduced coverage recorded).
