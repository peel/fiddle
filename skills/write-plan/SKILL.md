---
name: write-plan
description: Use when you have a spec or requirements for a multi-step task, before touching code
---

# Writing Plans


## Usage

Invoke as `fiddle:write-plan [--from-orchestrate] [--epic <id>]`.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--from-orchestrate` | false | Suppress the interactive execution handoff and return control to the caller after bean creation |
| `--epic <id>` | none | Reuse an existing epic bean as the parent for child beans. If omitted, this skill creates a new epic from the plan |

## Overview

Write comprehensive implementation plans assuming the engineer has zero context for our codebase and questionable taste. Document everything they need to know: which files to touch for each task, code, testing, docs they might need to check, how to test it. Give them the whole plan as bite-sized tasks. DRY. YAGNI. TDD. Frequent commits.

Assume they are a skilled developer, but know almost nothing about our toolset or problem domain. Assume they don't know good test design very well.

**Announce at start:** "I'm using the fiddle:write-plan skill to create the implementation plan."

**Context:** This should be run in a dedicated worktree (created by fiddle:brainstorm skill).

**Save plans to:** `docs/plans/YYYY-MM-DD-<feature-name>.md`
- (User preferences for plan location override this default)

## Scope Check

If the spec covers multiple independent subsystems, it should have been broken into sub-project specs during brainstorming. If it wasn't, suggest breaking this into separate plans — one per subsystem. Each plan should produce working, testable software on its own.

## File Structure

Before defining tasks, map out which files will be created or modified and what each one is responsible for. This is where decomposition decisions get locked in.

- Design units with clear boundaries and well-defined interfaces. Each file should have one clear responsibility.
- You reason best about code you can hold in context at once, and your edits are more reliable when files are focused. Prefer smaller, focused files over large ones that do too much.
- Files that change together should live together. Split by responsibility, not by technical layer.
- In existing codebases, follow established patterns. If the codebase uses large files, don't unilaterally restructure - but if a file you're modifying has grown unwieldy, including a split in the plan is reasonable.

This structure informs the task decomposition. Each task should produce self-contained changes that make sense independently.

## Bite-Sized Task Granularity

**Each step is one action (2-5 minutes):**
- "Write the failing test" - step
- "Run it to make sure it fails" - step
- "Implement the minimal code to make the test pass" - step
- "Run the tests and make sure they pass" - step
- "Commit" - step

## Plan Document Header

**Every plan MUST start with this header:**

```markdown
# [Feature Name] Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use fiddle:develop to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [One sentence describing what this builds]

**Architecture:** [2-3 sentences about approach]

**Tech Stack:** [Key technologies/libraries]

---
```

## Task Structure

````markdown
### Task N: [Component Name]

**Files:**
- Create: `exact/path/to/file.py`
- Modify: `exact/path/to/existing.py:123-145`
- Test: `tests/exact/path/to/test.py`

- [ ] **Step 1: Write the failing test**

```python
def test_specific_behavior():
    result = function(input)
    assert result == expected
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest tests/path/test.py::test_name -v`
Expected: FAIL with "function not defined"

- [ ] **Step 3: Write minimal implementation**

```python
def function(input):
    return expected
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest tests/path/test.py::test_name -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/path/test.py src/path/file.py
git commit -m "feat: add specific feature"
```
````

## Evaluation Block

**Every task MUST include an Evaluation block** — a fenced YAML block with language tag `eval`:

```eval
domains: [general]
criteria:
  general:
    - id: descriptive-criterion-id
      check: "Human-readable description of what to verify"
thresholds: {}
```

**Schema rules:**
- `domains`: array of domain names (use `general` for non-frontend/backend tasks)
- `criteria`: keyed by domain, each with stable `id` (kebab-case) and `check` text
- `thresholds`: optional overrides (empty = use domain defaults)
- Criterion IDs must be unique within the task, stable across edits

**The Evaluation block tells the evaluator what to check.** Without it, the evaluator has no task-specific criteria — only generic dimension scoring. Every plan task needs specific, verifiable criteria.

## No Placeholders

Every step must contain the actual content an engineer needs. These are **plan failures** — never write them:
- "TBD", "TODO", "implement later", "fill in details"
- "Add appropriate error handling" / "add validation" / "handle edge cases"
- "Write tests for the above" (without actual test code)
- "Similar to Task N" (repeat the code — the engineer may be reading tasks out of order)
- Steps that describe what to do without showing how (code blocks required for code steps)
- References to types, functions, or methods not defined in any task

## Remember
- Exact file paths always
- Complete code in every step — if a step changes code, show the code
- Exact commands with expected output
- DRY, YAGNI, TDD, frequent commits

## Self-Review

After writing the complete plan, look at the spec with fresh eyes and check the plan against it. This is a checklist you run yourself — not a subagent dispatch.

**1. Spec coverage:** Skim each section/requirement in the spec. Can you point to a task that implements it? List any gaps.

**2. Placeholder scan:** Search your plan for red flags — any of the patterns from the "No Placeholders" section above. Fix them.

**3. Type consistency:** Do the types, method signatures, and property names you used in later tasks match what you defined in earlier tasks? A function called `clearLayers()` in Task 3 but `clearFullLayers()` in Task 7 is a bug.

If you find issues, fix them inline. No need to re-review — just fix and move on. If you find a spec requirement with no task, add the task.

## Plan Critique (external providers)

After self-review and before bean creation, give external providers one
critique pass. Read `providers.phases.define` from `orchestrate.json`; skip
this section when the list is empty or no listed provider is installed.

For each available external provider:

    hooks/dispatch-provider.sh <provider> \
      --role plan-critic \
      --topic "Critique implementation plan: <feature name>" \
      --instructions "Review this implementation plan against the design
        doc. Report only: spec requirements with no covering task, steps
        that cannot be verified as written, files referenced but never
        created or modified, and tasks too large for 1-2 TDD cycles.
        Be terse; one finding per line; no rewrites." \
      --design-doc-file <spec-path> \
      --diff-file <plan-path>

Fold accepted findings into the plan inline. Reject findings that conflict
with the spec or the user's recorded decisions, and note why. One round
only; do not re-dispatch after folding.

## Create Beans from Plan

After the plan is saved and self-reviewed, materialize it as beans. **Do not skip this step** — `fiddle:develop` enforces a hard-gate on bean body shape (eval block, files section, steps checklist) and beans created any other way will fail validation.

### Step 1: Load the Bean Sizing Rules

Use the `fiddle:define-beans` skill.

This loads the sizing heuristic (1–2 TDD cycles → task bean; 3+ cycles → feature bean with child task beans), the mandatory bean body template (Files / Steps / Evaluation sections), and the Bean Body Completeness Gate. Apply these rules to every plan task.

### Step 2: Resolve the Epic

If `--epic <id>` was provided, reuse that epic. Verify it exists and is type `epic`:

```bash
beans show <id> --json
```

Otherwise, create a new epic from the plan's header (Goal, Architecture) and any `## Contracts` / hard-constraints sections from the design doc:

```bash
beans create --json "<feature-name from plan title>" -t epic -s todo -d "$(cat <<'EOF'
Plan: <plan-path>

<Goal sentence from plan header>

## Architecture
<Architecture paragraph from plan header>

## Contracts
<contracts captured from design doc, if any>

## Hard constraints
<constraints captured from spec, if any>
EOF
)"
```

Capture the epic ID for the next step.

### Step 3: Materialize Each `### Task N:` as Beans

Iterate through every `### Task N:` heading in the plan in document order. For each:

1. Count the TDD cycles in the task's checklist (each "Write the failing test" step is one cycle). Apply the sizing rule from `fiddle:define-beans` to choose **task** vs. **feature + children**.
2. Create the bean(s) under the epic with `--parent <epic-id>`. The body MUST include, in this exact shape:
   - `## Context` — repo path + a sentence on what/why
   - `## Files` — the `Files:` block from the plan task copied verbatim (paths only, one per line, prefixed `- Create:` / `- Modify:` / `- Test:`)
   - `## Steps` — the plan task's `- [ ]` checklist copied verbatim, including code blocks
   - `## Evaluation` — the fenced ` ```eval ` block copied verbatim from the plan task
3. Wire `--blocked-by` for any sequential dependencies between behaviors of a feature, and feature-level `--blocked-by` for cross-task dependencies (per `fiddle:define-beans` rules).

**The eval block is a hard requirement.** If a plan task has no fenced ` ```eval ` block, stop and add one to the plan first — do not invent eval criteria during bean creation.

### Step 4: Run the Completeness Gate

After all beans are created, list children of the epic and verify each task bean body passes the Bean Body Completeness Gate from `fiddle:define-beans` (steps exist + actionable; eval block present; files specified; sufficient context). Feature beans that are pure containers are exempt.

If any bean fails, fix the body inline. Do not exit Step 4 until every task bean passes.

### Step 5: Report

Print a summary line:

```
Epic <epic-id> ready: <N> task beans, <M> feature beans, all gate-checked.
```

## Execution Handoff

If `--from-orchestrate` was set, return control to the caller. Do not prompt the user.

Otherwise, once the plan is saved, self-reviewed, and critiqued, offer execution:

**"Plan complete and saved to `docs/plans/<filename>.md`. Epic `<epic-id>` ready with `<N>` beans. Ready to execute?"**

**When execution begins:**
- **REQUIRED SUB-SKILL:** Use fiddle:develop
- Fresh subagent per task + evaluation between tasks
