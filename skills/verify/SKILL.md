---
name: verify
description: Use when about to claim work is complete, fixed, or passing, before committing or creating PRs - requires running verification commands and confirming output before making any success claims; evidence before assertions always
---

# Verification Before Completion

Run the verification, read its output, and only then state the result.

Verification output precedes any claim of success. A claim made without it is a statement about what you expect rather than what happened, and the reader cannot tell the difference — which makes it a false report rather than an efficient one. Confidence, a previous run, a passing linter, and an agent's success message are all not the output of the command that proves the claim.

## The Gate

Before claiming any status or expressing satisfaction with the work:

1. **Identify** the command that proves the claim.
2. **Run** it in full, fresh and complete.
3. **Read** the whole output: exit code, failure count, warnings.
4. **Check** whether the output confirms the claim. If it does, state the claim with the evidence. If it does not, state the actual status with the evidence.

This applies to exact phrases, paraphrases, and anything that implies success — including "great", "perfect", and "done" — and it applies immediately before committing, opening a PR, marking a task complete, moving on, or delegating.

## What Each Claim Requires

| Claim | Requires | Not Sufficient |
|-------|----------|----------------|
| Tests pass | Test command output: 0 failures | Previous run, "should pass" |
| Linter clean | Linter output: 0 errors | Partial check, extrapolation |
| Build succeeds | Build command: exit 0 | Linter passing, logs look good |
| Bug fixed | Test original symptom: passes | Code changed, assumed fixed |
| Regression test works | Red-green cycle verified | Test passes once |
| Agent completed | VCS diff shows changes | Agent reports "success" |
| Requirements met | Line-by-line checklist | Tests passing |

A partial check extrapolated to the whole is the most common failure here: it produces a claim with evidence attached that the evidence does not cover.

## Patterns

**Tests:** run the test command, see 34/34 pass, then say "all tests pass" — not "should pass now" or "looks correct".

**Regression tests:** write the test, run it (passes), revert the fix, run it again and see it fail, restore the fix, run it once more. Without the red step, "I've written a regression test" is unproven.

**Build:** run the build, see exit 0. A passing linter says nothing about compilation.

**Requirements:** re-read the plan, build a checklist, verify each item, and report gaps or completion. "Tests pass, phase complete" skips the requirements entirely.

**Agent delegation:** when an agent reports success, check the VCS diff, verify the changes, and report the actual state.
