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

Turn a spec into an implementation plan and then into beans. Write for an engineer who is skilled but has zero context for this codebase, does not know the toolset or problem domain, and does not know good test design: which files to touch for each task, the actual code, the exact commands with expected output, the docs worth checking. Bite-sized tasks, exact paths always, DRY, YAGNI, TDD, frequent commits.

Run this in a dedicated worktree (created by the `fiddle:brainstorm` skill).

Save plans to `docs/plans/YYYY-MM-DD-<feature-name>.md` (user preferences for plan location override this default).

## Scope Check

If the spec covers multiple independent subsystems, it should have been broken into sub-project specs during brainstorming. If it was not, suggest splitting it into separate plans, one per subsystem, each producing working testable software on its own.

## File Structure

Before defining tasks, map out which files will be created or modified and what each is responsible for. This is where decomposition decisions get locked in, and it informs the task breakdown: each task should produce self-contained changes that make sense independently.

- Give each file one clear responsibility, with well-defined interfaces at its boundaries.
- Prefer smaller focused files: you reason best about code you can hold in context at once, and your edits are more reliable when files are focused.
- Files that change together live together. Split by responsibility, not by technical layer.
- In existing codebases, follow established patterns. Don't unilaterally restructure a codebase that uses large files, but planning a split of a file you are already modifying is reasonable.

## Bite-Sized Task Granularity

Each step is one action taking 2-5 minutes: "Write the failing test", "Run it to make sure it fails", "Implement the minimal code to make the test pass", "Run the tests and make sure they pass", "Commit".

## Plan Document Header

Every plan starts with this header:

```markdown
# [Feature Name] Implementation Plan

> **For agentic workers:** Use fiddle:develop to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

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

Every task carries an Evaluation block — a fenced YAML block with language tag `eval`. It is what tells the evaluator what to check; without it the evaluator has only generic dimension scoring and nothing task-specific to verify.

```eval
domains: [general]
criteria:
  general:
    - id: descriptive-criterion-id
      check: "Human-readable description of what to verify"
thresholds: {}
```

Schema rules:

- `domains`: array of domain names (`general` for non-frontend/backend tasks)
- `criteria`: keyed by domain, each entry with a stable kebab-case `id` and `check` text
- `thresholds`: optional overrides (empty means use domain defaults)
- Criterion ids are unique within the task and stable across edits

## No Placeholders

Every step contains the actual content an engineer needs. These are plan failures, not shortcuts:

- "TBD", "TODO", "implement later", "fill in details"
- "Add appropriate error handling" / "add validation" / "handle edge cases"
- "Write tests for the above" without actual test code
- "Similar to Task N" — repeat the code, since the engineer may read tasks out of order
- Steps that describe what to do without showing how (code steps need code blocks)
- References to types, functions, or methods not defined in any task

## Self-Review

After writing the complete plan, look at the spec with fresh eyes and check the plan against it. This is a checklist you run yourself, not a subagent dispatch.

1. **Spec coverage:** skim each section and requirement in the spec. Can you point to a task that implements it? List any gaps, and add a task for any requirement that has none.
2. **Placeholder scan:** search the plan for the patterns in "No Placeholders" above.
3. **Type consistency:** do the types, method signatures, and property names used in later tasks match what earlier tasks defined? A function called `clearLayers()` in Task 3 and `clearFullLayers()` in Task 7 is a bug.

Fix what you find inline and move on; no re-review pass.

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

After the plan is saved and self-reviewed, materialize it as beans. `fiddle:develop` runs `scripts/validate-bean-body.sh` on every bean and stops on exit 2, so beans created outside this shape fail validation before the loop starts.

### Step 1: Load the Bean Sizing Rules

Use the `fiddle:define-beans` skill.

This loads the sizing heuristic (1–2 TDD cycles → task bean; 3+ cycles → feature bean with child task beans), the bean body template (Files / Steps / Evaluation sections), and the Bean Body Completeness Gate. Apply these rules to every plan task.

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
2. Create the bean(s) under the epic with `--parent <epic-id>`. The body takes this exact shape:
   - `## Context` — repo path + a sentence on what/why
   - `## Files` — the `Files:` block from the plan task copied verbatim (paths only, one per line, prefixed `- Create:` / `- Modify:` / `- Test:`)
   - `## Steps` — the plan task's `- [ ]` checklist copied verbatim, including code blocks
   - `## Evaluation` — the fenced ` ```eval ` block copied verbatim from the plan task
3. Wire `--blocked-by` for any sequential dependencies between behaviors of a feature, and feature-level `--blocked-by` for cross-task dependencies (per `fiddle:define-beans` rules).

If a plan task has no fenced ` ```eval ` block, stop and add one to the plan first. Eval criteria invented during bean creation are criteria the spec never agreed to.

### Step 4: Run the Completeness Gate

After all beans are created, list children of the epic and verify each task bean body passes the Bean Body Completeness Gate from `fiddle:define-beans` (steps exist and are actionable; eval block present; files specified; sufficient context). Feature beans that are pure containers are exempt.

Fix any failing body inline. Do not exit Step 4 until every task bean passes.

### Step 5: Report

Print a summary line:

```
Epic <epic-id> ready: <N> task beans, <M> feature beans, all gate-checked.
```

## Execution Handoff

If `--from-orchestrate` was set, return control to the caller without prompting the user.

Otherwise, once the plan is saved, self-reviewed, and critiqued, offer execution:

**"Plan complete and saved to `docs/plans/<filename>.md`. Epic `<epic-id>` ready with `<N>` beans. Ready to execute?"**

When execution begins:

- Use fiddle:develop to implement the plan task-by-task
- Fresh subagent per task + evaluation between tasks
