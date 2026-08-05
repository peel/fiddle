# Plan document format

Save plans to `docs/plans/YYYY-MM-DD-<feature-name>.md` unless a user preference overrides it. Plans are local lifecycle artifacts and are not committed.

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
