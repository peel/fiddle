---
name: debug
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

# Systematic Debugging

Establish the root cause of a bug, then fix that cause.

The root cause is established before any fix is proposed or applied. A fix chosen before the cause is known is a guess: it patches whichever symptom was visible, leaves the real defect in place, and usually adds a new one. Guessing feels faster under pressure and is not: the thrashing it produces costs more than the investigation would have.

Use this for any technical issue: test failures, production bugs, unexpected behavior, performance problems, build failures, integration issues. Simple-looking issues have root causes too, and the moments where skipping is most tempting (an emergency, an obvious one-line fix, a fix that just failed) are the moments where guessing is most expensive.

## Phase 1: Root Cause Investigation

Complete this phase before proposing a fix.

1. **Read error messages carefully.** Don't skip past errors or warnings; they often contain the exact solution. Read stack traces completely, and note line numbers, file paths, and error codes.

2. **Reproduce consistently.** Can you trigger it reliably? What are the exact steps? Does it happen every time? If it is not reproducible, gather more data rather than guessing.

3. **Check recent changes.** What changed that could cause this: git diff, recent commits, new dependencies, config changes, environmental differences?

4. **Gather evidence in multi-component systems.** When the system spans components (CI → build → signing, API → service → database), add diagnostic instrumentation at each boundary before proposing fixes:

   ```
   For each component boundary:
     - Log what data enters the component
     - Log what data exits the component
     - Verify environment/config propagation
     - Check state at each layer

   Run once to gather evidence showing where it breaks
   Then analyze the evidence to identify the failing component
   Then investigate that specific component
   ```

   For a multi-layer system:

   ```bash
   # Layer 1: Workflow
   echo "=== Secrets available in workflow: ==="
   echo "IDENTITY: ${IDENTITY:+SET}${IDENTITY:-UNSET}"

   # Layer 2: Build script
   echo "=== Env vars in build script: ==="
   env | grep IDENTITY || echo "IDENTITY not in environment"

   # Layer 3: Signing script
   echo "=== Keychain state: ==="
   security list-keychains
   security find-identity -v

   # Layer 4: Actual signing
   codesign --sign "$IDENTITY" --verbose=4 "$APP"
   ```

   This shows which layer fails: secrets → workflow ✓, workflow → build ✗.

5. **Trace data flow.** Where does the bad value originate, what called this with it, and what called that? Keep tracing up until you find the source, and fix there rather than at the symptom. For errors deep in a call stack, see `root-cause-tracing.md` in this directory for the complete backward tracing technique.

## Phase 2: Pattern Analysis

1. **Find working examples.** Locate similar code in the same codebase that works.

2. **Compare against references.** When implementing a pattern, read the reference implementation completely rather than skimming. Partial understanding of a pattern reproduces its shape without its guarantees.

3. **Identify differences.** List every difference between the working and broken cases, however small, without assuming any of them cannot matter.

4. **Understand dependencies.** What other components, settings, config, or environment does this need, and what does it assume?

## Phase 3: Hypothesis and Testing

1. **Form a single hypothesis.** State it specifically and write it down: "I think X is the root cause because Y."

2. **Test minimally.** Make the smallest change that tests the hypothesis, one variable at a time. Several changes at once leave you unable to tell which one mattered.

3. **Verify before continuing.** If it worked, go to Phase 4. If it did not, form a new hypothesis rather than stacking another fix on top.

4. **When you don't know, say so.** "I don't understand X" is a usable statement; a confident guess is not. Ask, or research further.

## Phase 4: Implementation

1. **Create a failing test case:** the simplest reproduction, automated if a framework exists, a one-off script otherwise. Write it before the fix, so the fix has something to prove. Use the `fiddle:tdd` skill for writing proper failing tests.

2. **Implement a single fix** addressing the identified root cause. One change, no "while I'm here" improvements, no bundled refactoring.

3. **Verify the fix:** the test passes, no other tests broke, the original issue is actually resolved.

4. **If the fix didn't work,** count the fixes attempted so far. Under three: return to Phase 1 and re-analyze with what you just learned. Three or more: stop, and question the architecture instead of attempting another fix.

5. **After three failed fixes, question the architecture.** The pattern to recognize is each fix revealing new shared state or coupling somewhere else, each fix requiring massive refactoring to implement, each fix creating new symptoms elsewhere. That pattern is not a failed hypothesis; it is a wrong architecture, and a fourth fix will find a fourth symptom of it. Ask whether the pattern is fundamentally sound and whether it is being kept through inertia, and discuss with your human partner before attempting more fixes.

## When Investigation Finds No Root Cause

If systematic investigation shows the issue is genuinely environmental, timing-dependent, or external: you have completed the process. Document what you investigated, implement appropriate handling (retry, timeout, error message), and add monitoring or logging for future investigation. Treat the conclusion with suspicion first, though: most "no root cause" findings are incomplete investigations.

Redirections from your human partner are a signal that this happened: "is that not happening?", "will it show us...?", "stop guessing", "ultrathink this", "we're stuck?" all mean an assumption went unverified. Return to Phase 1.

## Supporting Techniques

These techniques are part of systematic debugging and available in this directory:

- **`root-cause-tracing.md`** - Trace bugs backward through call stack to find original trigger
- **`defense-in-depth.md`** - Add validation at multiple layers after finding root cause
- **`condition-based-waiting.md`** - Replace arbitrary timeouts with condition polling

**Related skills:**
- **fiddle:tdd** - For creating failing test case (Phase 4, Step 1)
- **fiddle:verify** - Verify fix worked before claiming success
