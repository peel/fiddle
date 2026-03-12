# Project Documentation

Living docs for product and technical decisions. Persistent knowledge that informs all work — separate from beans (actionable work) and plans (session-scoped execution).

**Flow:** `/docs-discover` → `superpowers:brainstorming` → `beans` → `superpowers:writing-plans` → `/ralph-subs-implement` → `/docs-evolve`

## Structure

```
docs/
├── product/
│   ├── VISION.md        — what, who, why, non-goals
│   ├── MARKET.md         — landscape, competitors, positioning
│   ├── PRICING.md        — business model, costs, revenue
│   ├── GTM.md            — distribution, channels, messaging
│   └── FEEDBACK.md       — user signals (append-only)
├── technical/
│   ├── SYSTEM.md         — how it works now
│   ├── decisions/
│   │   └── NNN-title.md  — ADRs (append-only)
│   └── RUNBOOKS.md       — deploy, rollback, common issues
└── BACKLOG.md            — pre-bean ideas and debt (append-only)
```

## Skills

- `/docs-discover [scope]` — Socratic dialogue to bootstrap or review docs
- `/docs-evolve [--epic <id>]` — post-ship update of technical docs, ADRs, backlog
- `/adr <title>` — new architecture decision record
- `/feedback <signal>` — append user feedback
- `/backlog <idea>` — append idea or debt item

## Conventions

- Product docs: overwrite freely. Technical decisions: append-only, supersede with new records.
- Append-only logs (FEEDBACK, BACKLOG, decisions/) are never edited or deleted.
- Every curated doc has a `Last reviewed:` date.
- Keep it short. A doc you won't read doesn't exist.
