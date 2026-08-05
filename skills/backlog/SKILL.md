---
name: backlog
description: Use when capturing an idea, technical debt, or observation that is not ready for a planned bean.
---

# Backlog


## Usage

Invoke as `fiddle:backlog <item> — idea, debt, or observation to capture`.

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

5. Assign 1-3 tags from: `#idea` `#debt` `#optimization` `#feature` `#experiment` `#infrastructure` `#ux` `#security`.
6. Scan existing entries for a near-duplicate before writing. If one exists, mention it and ask whether this is the same item or a new one.
7. Show the entry. Append after confirmation, creating `docs/BACKLOG.md` with the header `# Backlog` if it does not exist.

The file is append-only: existing entries are never edited or deleted, since the dated record is what makes a stale item recognizable later. Keep each entry to 2-4 lines, enough to remember what it was rather than a full spec.
