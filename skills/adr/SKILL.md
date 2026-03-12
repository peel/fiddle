---
name: adr
description: Create a new Architecture Decision Record in docs/technical/decisions/. Asks what was decided and why, writes the ADR file.
disable-model-invocation: true
argument-hint: <title> — short description of the decision
---

# ADR

Create a new Architecture Decision Record.

## Process

1. Read the user's argument as the decision title. If no argument, ask for one.
2. Find the next ADR number: `ls docs/technical/decisions/*.md | grep -v template | sort -n | tail -1`. Extract number, increment. If no existing ADRs, start at 001.
3. Ask the user (briefly — 2-3 questions max):
   - What's the context? (What prompted this decision?)
   - What did you decide? (Be specific.)
   - What are the consequences? (Tradeoffs, what gets easier/harder.)
   If the user already provided enough detail in the argument or conversation, skip questions and draft directly.
4. Write `docs/technical/decisions/NNN-kebab-case-title.md`:

```markdown
# NNN — Title

**Date:** YYYY-MM-DD
**Status:** accepted

## Context

[2-3 sentences]

## Decision

[Concrete statement]

## Consequences

[Tradeoffs — what gets easier, what gets harder]
```

5. Show the user the file content. Write after confirmation.

## Rules

- Keep it short. An ADR is 10-20 lines, not a design doc.
- Use kebab-case for filenames: `003-use-alloydb-over-cloudsql.md`
- Never edit existing ADRs. To change a past decision, create a new ADR with status `supersedes NNN`.
- If `docs/technical/decisions/` doesn't exist, create it.
