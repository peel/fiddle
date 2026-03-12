---
name: feedback
description: Append a user feedback entry to docs/product/FEEDBACK.md. Quick capture of what users said, asked, or struggled with.
disable-model-invocation: true
argument-hint: <feedback> — what the user said or experienced
---

# Feedback

Append a user feedback entry.

## Process

1. Read the user's argument as the feedback content.
2. If no argument, ask: "What did the user say or experience?"
3. Ask for source/context if not obvious: who, what channel, when. Keep it to one question.
4. Append to `docs/product/FEEDBACK.md`:

```markdown
### YYYY-MM-DD — Source (user/channel/context)
What they said/asked/struggled with.
Tags: #tag1 #tag2
```

5. Auto-assign tags from: `#feature-request` `#bug` `#confusion` `#praise` `#churn-signal` `#ux` `#performance`. Use 1-3 tags.
6. Show the entry. Append after confirmation.

## Rules

- Append only. Never edit or delete existing entries.
- Capture raw signal. Don't editorialize or interpret — just record what happened.
- If `docs/product/FEEDBACK.md` doesn't exist, create it with the header `# User Feedback` and then append.
- Keep entries to 2-4 lines. If there's more context, the user can expand later.
