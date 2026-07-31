---
name: insights
description: Use when preparing for a planning cycle or when feedback has accumulated — synthesizes structured feedback entries into persona files and insight summaries that discover and brainstorm phases consume.
---

# Insights


## Usage

Invoke as `fiddle:insights [--personas-only] [--insights-only]`.

Synthesize structured feedback into reusable personas and insight summaries.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--personas-only` | false | Only update personas, skip insight synthesis |
| `--insights-only` | false | Only produce insight summary, skip persona updates |

## Outputs

| Artifact | Path | Lifecycle |
|---|---|---|
| Persona files | `docs/product/personas/<persona-slug>.md` | Created once, updated as feedback accumulates |
| Insight summary | `docs/product/insights/YYYY-MM-DD-synthesis.md` | New file each synthesis cycle |

Create directories if they do not exist.

## Process

### Step 1: Read Feedback

Read all entries from `docs/product/FEEDBACK.md`. Entries follow the structured format defined by `fiddle:feedback` — each entry has Who, Context, Observation, Implication, Confidence, and Tags fields. If the file does not exist or has no entries, stop: "No feedback entries to synthesize."

### Step 2: Update Personas

Skip if `--insights-only`.

Scan feedback entries for recurring actors. Two entries describe the same persona when they share role + segment (e.g., "Senior developer at a fintech company" appearing twice).

For each identified persona, check if a file already exists in `docs/product/personas/`. If it does, update it. If not, create it. Slug is derived from role + segment (e.g., `senior-fintech-dev`). If two different people map to the same slug, append a disambiguator (e.g., `solo-founder-2`).

**Merging vs splitting:** If a persona's context changes over time (e.g., a solo founder hires a team), update the existing persona file — don't create a new one. The Trajectory section captures this evolution. Only split into a new persona if the role + segment genuinely diverge (e.g., they moved from a startup to an enterprise).

#### Persona File Format

```markdown
---
slug: <persona-slug>
role: <role description>
segment: <company type or context>
---

# <Persona Label> (e.g., "Fintech Senior Dev")

## Profile
- **Role:** <role and experience level>
- **Segment:** <company type, size, context>
- **First seen:** YYYY-MM-DD
- **Last seen:** YYYY-MM-DD
- **Entries:** <count of feedback entries from this persona>

## Trajectory
<2-3 sentences on how this persona's experience has evolved across feedback entries. What did they try? What worked? What didn't?>

## Needs
- <Need derived from their feedback — one bullet per distinct need>

## Risk
<One sentence: churn risk, satisfaction level, or engagement trajectory>
```

Each synthesis refreshes Last seen, Entries count, Trajectory, Needs, and Risk. Keep Trajectory factual by referencing specific feedback dates and observations, and leave needs identified by previous syntheses in place unless later feedback contradicts them: a need that stops being mentioned has usually just stopped being reported, not stopped existing.

### Step 3: Synthesize Insights

Skip if `--personas-only`.

Read:
- All feedback entries from `docs/product/FEEDBACK.md`
- All persona files from `docs/product/personas/` (if they exist)
- Product docs if they exist: `docs/product/VISION.md`, `docs/product/MARKET.md`, `docs/product/GTM.md`
- Previous insight summaries from `docs/product/insights/` (most recent, if any)

#### Insight Summary Format

```markdown
---
date: YYYY-MM-DD
feedback_count: <number of entries synthesized>
previous_synthesis: <path to previous summary, or "none">
---

# Feedback Synthesis — YYYY-MM-DD

## Themes

### <Theme Title>
**Signal strength:** <strong|moderate|weak> — <N entries, N personas> (strong: 3+ entries or 2+ personas; moderate: 2 entries or repeat persona; weak: single entry, single persona)
**Entries:** YYYY-MM-DD "<observation title>", YYYY-MM-DD "<observation title>"
**Pattern:** <What's happening across these entries>
**Implication:** <What this means for the product>
**Alignment:** <How this aligns with or contradicts VISION.md/MARKET.md — write "no product docs available" if they don't exist or are empty templates>

### <Theme Title>
...

## New vs Recurring

- **New themes this cycle:** <list>
- **Recurring from previous synthesis:** <list with trend direction — growing, stable, declining>
- **Resolved since last synthesis:** <list — themes present before that no longer appear>

## Priority Actions

| Priority | Action | Theme | Impact | Personas Affected |
|---|---|---|---|---|
| 1 | <concrete action> | <theme> | <what improves> | <which personas> |
| 2 | ... | ... | ... | ... |

## Open Questions

- <Questions this feedback raises but doesn't answer — useful for brainstorm sessions>
```

Cross-reference the themes against the product docs and flag mismatches: if VISION targets enterprise teams and every friction report comes from solo founders, that gap is the most useful thing in the summary. Compare against the previous synthesis where one exists, so each theme carries a direction rather than only a count. Write priority actions concrete enough to become backlog items or bean descriptions, and keep the whole summary under 200 lines, since planning loads it as context.

### Step 4: Present and Confirm

Present a summary of what was produced:
```
"Insights synthesis complete:
- Personas updated: [list with paths]
- Insight summary: [path]

[Brief highlight: top theme, top priority action, any product alignment concerns]

Review and confirm?"
```

Wait for user confirmation before writing files.

## How Insights Are Consumed

Discover and brainstorm phases load these artifacts automatically when they exist:
- **Discover phase:** Personas and latest insight summary inform scope decisions
- **Brainstorm phase:** Insight themes and persona needs inform approach selection and design tradeoffs

This is the compound loop: Feedback → Insights → Planning → Features → Feedback.
