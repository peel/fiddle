---
name: deliver-docs
description: Use after a completed epic or significant change to update technical docs and capture resulting decisions or debt.
---

# Deliver Docs


## Usage

Invoke as `fiddle:deliver-docs [--epic <id>] [--diff] [--dry-run]`.

Post-ship documentation update. Reads what changed, updates curated docs, creates ADRs, appends to BACKLOG.

## Configuration

Flags (all optional):
- `--epic <id>` — scope to beans under this epic
- `--diff` — also read git diff of recent work (default: infer from completed beans)
- `--dry-run` — show proposed changes without writing

## Docs schema (constraints)

The evolve skill is the authority on doc structure. Apply these constraints to every update.

**VISION.md** — Under 1 page. Sections: What (1 para), Who (1 para), Success (2-5 signals), Non-goals (bullets), Open questions (bullets). Evolve rarely touches this doc; only flag it if completed work contradicts it.

**MARKET.md** — Main sections fit one screen. Research log grows unbounded. Only update main sections if work revealed new competitive information. Append to research log if applicable.

**PRICING.md** — Half page for main sections. Only update if work changed cost structure or revealed pricing-relevant information.

**GTM.md** — Short. Only update Traction log if a notable event occurred.

**SYSTEM.md** — 1-2 pages max. Sections: Overview (1 para), Components (entry per component), Data (flows/storage), Infrastructure (where it runs), Invariants (must-be-true), Known issues (specific). This is the primary target of evolve; update it after any architectural change.

**RUNBOOKS.md** — Commands not prose. Update if deploy process, rollback steps, or common issues changed.

**decisions/NNN-title.md** — Context (2-3 sentences), Decision (concrete), Consequences (tradeoffs). Create new files, never edit old ones. Supersede with a new record.

**BACKLOG.md** — Dated entries with origin and tags. Append-only.

## Process

### 1. Gather context

- Read all existing docs in `docs/`.
- If `--epic <id>`: run `beans list --parent <id> --json`, read completed beans.
- If no `--epic`: run `beans list --json`, identify recently completed beans.
- Read bean details: `beans show <id> --json` for each completed bean.
- If `--diff` or if it helps understanding: `git log --oneline -20` and `git diff` for recent changes.

### 2. Analyze what changed

For each completed bean and its associated changes, classify:

**Architectural changes** — new components, changed data flows, new dependencies, infrastructure changes, new invariants. → Update SYSTEM.md.

**Significant decisions** — technology choices, pattern choices, tradeoff decisions that constrain future work. → Create ADR.

**Discovered issues** — tech debt found during implementation, fragile areas, things that work but shouldn't. → Append to Known issues in SYSTEM.md or to BACKLOG.md.

**Ideas surfaced** — features or improvements that came up during work but weren't in scope. → Append to BACKLOG.md.

**Product implications** — changes that affect pricing, positioning, user-facing behavior, or go-to-market. → Flag for manual review, do not auto-update product docs.

### 3. Propose changes

Present every proposed change to the user before writing. Group by doc:

```
## SYSTEM.md
- Update Components: added new-service (reason)
- Update Data: changed flow from X to Y
- Add Known issue: description

## decisions/
- New ADR: 003-use-alloydb-over-cloudsql.md

## BACKLOG.md
- Append: idea description (origin: epic-xyz implementation)

## Flagged for review (no auto-update)
- PRICING.md: new service adds $X/month infrastructure cost
- VISION.md: non-goal "no real-time" may need revisiting given new websocket component
```

### 4. Write changes

After user confirmation:

**SYSTEM.md** — Update in place. Follow schema constraints. Keep entries concise. Don't expand sections beyond their purpose. Set `Last reviewed:` to today.

**ADRs** — Find highest numbered existing ADR: `ls docs/technical/decisions/`. Create next number. Use format: `NNN-kebab-case-title.md`. Follow template: Status: accepted, Context (2-3 sentences), Decision (concrete), Consequences (tradeoffs).

**BACKLOG.md** — Append new entries at the end. Format: `### YYYY-MM-DD — Title`, Description, `Origin:` tag, hashtag tags.

**RUNBOOKS.md** — Update if deploy/rollback/common-issues changed. Commands not prose.

**Product docs** — Flag VISION, MARKET, PRICING, and GTM for manual review instead of auto-updating them. These require deliberate human thinking, not mechanical updates.

### 5. Verify

After writing:
- Read back each modified file.
- Check that no doc exceeds its schema constraint.
- Check that no research log or append-only entries were deleted.
- Report what was updated.

## Rules

- Never update product docs (VISION, MARKET, PRICING, GTM) without explicit user instruction. Only flag.
- Never edit existing ADRs. Create new ones that supersede.
- Never delete append-only entries (FEEDBACK, BACKLOG, research logs).
- Never bloat docs beyond schema constraints. If a section is getting long, suggest splitting or pruning.
- If nothing meaningful changed, say so and stop. Don't create busywork updates.
- Always show proposed changes before writing. `--dry-run` skips the write step entirely.
