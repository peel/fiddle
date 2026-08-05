# Plan document format

Save plans to `docs/plans/YYYY-MM-DD-<feature-name>.md` unless a user preference overrides it. A plan is self-contained for an engineer with no repository context: exact files, actual code, commands, expected results, and the few docs worth reading.

Start every plan with a title, an agentic-worker note, a one-sentence goal, architecture, and tech stack. Map file responsibilities before tasks: one clear responsibility per file, focused interfaces, existing project patterns, and no unrelated restructuring.

Each task has exact create/modify/test paths and bite-sized test-driven steps: write one failing behavior test, observe the expected failure, make the minimal implementation, observe the pass, then commit. A task must include a fenced `eval` block with domains, stable criterion IDs, readable verifiable checks, and optional threshold overrides.

Do not leave placeholders, vague error-handling requests, implicit tests, cross-task references, or undefined types/functions. After drafting, self-review spec coverage, placeholder absence, and type consistency; fix findings inline. Then use configured external providers for one concise plan critique: only uncovered requirements, unverifiable steps, missing file ownership, and oversized 1–2 cycle tasks. Fold accepted findings once and do not re-dispatch.
