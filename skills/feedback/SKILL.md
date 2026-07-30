---
name: feedback
description: Append a structured user feedback entry to docs/product/FEEDBACK.md. Captures who, context, observation, implication, and confidence for each signal.
---

# Feedback


## Usage

Invoke as `fiddle:feedback <feedback> — what the user said or experienced`.

Append a structured user feedback entry.

## Process

1. Read the user's argument as the feedback content.
2. If no argument, ask: "What did the user say or experience?"
3. Ask for missing fields (one question at a time, skip fields the user already provided):
   - **Who** — role, segment, experience level (e.g., "senior dev at fintech", "solo founder", "enterprise team")
   - **Context** — where/when/how they encountered this (e.g., "during onboarding", "in Slack support channel", "user interview")
   - If context is only available for some actors, use what you have and write "not stated" for the rest. Don't ask follow-ups for every actor.
4. For each distinct actor or observation, append a structured entry to `docs/product/FEEDBACK.md`.

## Entry Format

```markdown
### YYYY-MM-DD — <short observation title>

**Who:** <role, segment, experience level — or "internal" for team observations>
**Context:** <where/when/how this was observed>
**Observation:** <what happened — raw signal, no interpretation>
**Implication:** <what this suggests — one sentence>
**Confidence:** <high|medium|low>
Tags: #tag1 #tag2
```

### Field Guidelines

- **Who:** Enough to identify the persona, not the person. "Senior dev at fintech" not "John from Acme Corp."
- **Context:** Channel and circumstance. "Slack support thread during first week of use."
- **Observation:** What happened — what they said, did, or struggled with. Facts only, no interpretation.
- **Implication:** What it means — one sentence on what this suggests for the product. This is where interpretation goes.
- **Confidence:** `high` = direct quote or observed behavior. `medium` = secondhand report or partial observation. `low` = inference from indirect signal.

### Multi-Actor Scenarios

When a single report contains multiple actors (e.g., "three customers said X"):

- Write **one entry per actor** with the same date and a shared observation title describing the common theme (e.g., "Evaluator domain creation is opaque").
- Each actor gets their own Who/Context/Observation/Implication/Confidence.
- Don't flatten distinct experiences into one entry — the differences between actors are signal.

## Tags

Auto-assign 1-3 tags from: `#feature-request` `#bug` `#confusion` `#praise` `#churn-signal` `#ux` `#performance` `#onboarding` `#docs`.

## Rules

- Append only. Never edit or delete existing entries.
- If `docs/product/FEEDBACK.md` doesn't exist, create it with the header `# User Feedback` and then append.
- Show the entry. Append after confirmation.
- Keep Observation to 1-3 sentences. Implication to one sentence.
