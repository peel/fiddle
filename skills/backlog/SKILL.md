---
name: backlog
description: Append an idea, tech debt item, or someday/maybe to docs/BACKLOG.md. Quick capture for things that aren't beans yet.
disable-model-invocation: true
argument-hint: <item> — idea, debt, or observation to capture
---

# Backlog

Append an item to the project backlog.

## Process

1. Read the user's argument as the backlog item.
2. If no argument, ask: "What's the idea or issue?"
3. Determine origin from context: brainstorm session, feedback, code review, noticed during implementation, external research, or user-provided.
4. Append to `docs/BACKLOG.md`:

```markdown
### YYYY-MM-DD — Title
Description of the idea or debt item.
Origin: [brainstorm | feedback | code-review | implementation | research | observation]
Tags: #tag1 #tag2
```

5. Auto-assign tags from: `#idea` `#debt` `#optimization` `#feature` `#experiment` `#infrastructure` `#ux` `#security`. Use 1-3 tags.
6. Show the entry. Append after confirmation.

## Rules

- Append only. Never edit or delete existing entries.
- Keep entries to 2-4 lines. Enough to remember what it was, not a full spec.
- If `docs/BACKLOG.md` doesn't exist, create it with the header `# Backlog` and then append.
- Don't duplicate — scan existing entries briefly. If a similar item exists, mention it and ask if this is the same thing or new.
