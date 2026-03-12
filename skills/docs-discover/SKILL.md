---
name: docs-discover
description: Socratic dialogue for bootstrapping or reviewing project docs. Walks through curated docs (VISION, MARKET, PRICING, GTM, SYSTEM, RUNBOOKS), asks probing questions, challenges assumptions, identifies gaps and inconsistencies, and writes/updates the docs based on your answers. Use for initial population or periodic review.
disable-model-invocation: true
argument-hint: [scope] — optional: vision, market, pricing, gtm, system, runbooks, or omit for full discovery
---

# Docs Discovery

Socratic dialogue to populate or review curated project docs. You question, challenge, and probe — then write the docs.

## Docs schema

Each doc has a defined purpose, sections, and size constraint. NEVER exceed these. The docs are for future-you and future-agents — concise, high-signal, no filler.

### docs/product/VISION.md
Purpose: what this is, who it's for, why it exists.
Sections: What (1 paragraph), Who (1 paragraph), Success (2-5 concrete signals), Non-goals (bullet list), Open questions (bullet list).
Constraint: Under 1 page total. If longer, you're over-specifying.

### docs/product/MARKET.md
Purpose: evolving understanding of the competitive landscape and users.
Sections: Landscape (1-2 paragraphs), Alternatives (short entry per competitor — name, what it does, strengths, weaknesses), Positioning (2-3 sentences), Users (what you know about actual/potential users), Research log (dated entries, append-only).
Constraint: Main sections should fit on one screen. Research log grows unbounded.

### docs/product/PRICING.md
Purpose: how this makes money or why it doesn't.
Sections: Model (1 paragraph or options list), Costs (rough numbers), Comparable pricing (reference alternatives), Current state (what's live today), Research log.
Constraint: Half a page for main sections. Skip entirely for projects that will never monetize.

### docs/product/GTM.md
Purpose: how people find and start using this.
Sections: Channels (where you promote), Messaging (one-sentence descriptions and hooks), Traction (dated log of notable events).
Constraint: Keep it short. Solo dev GTM is often one paragraph. That's fine.

### docs/technical/SYSTEM.md
Purpose: how the system actually works RIGHT NOW. Ground truth, not aspirational.
Sections: Overview (1 paragraph), Components (one entry per component — what, tech, where, dependencies), Data (flows, storage, key schemas), Infrastructure (where it runs), Invariants (must-be-true constraints), Known issues (specific, with file references where possible).
Constraint: 1-2 pages max. If longer, the system is complex enough to warrant splitting into sub-docs.

### docs/technical/RUNBOOKS.md
Purpose: operational procedures for 2am-you.
Sections: Deploy (commands, not prose), Rollback (specific steps), Common issues (symptom → cause → fix).
Constraint: Skip entirely for libraries or tools that don't run as services.

## Process

### 1. Assess current state

Read all docs in `docs/`. Also read:
- The codebase structure (ls, key config files, README)
- Any context the user provides (conversation dumps, notes)
- `.beans/` if present — open epics and features for project direction

Classify each doc as: **empty** (template only), **populated** (has content), or **stale** (Last reviewed date > 30 days or content contradicts codebase).

### 2. Determine scope

If the user provided a scope argument (e.g., `market`), focus only on that doc.
If no scope: work through docs in this order — VISION first (everything else depends on it), then SYSTEM (ground technical truth), then MARKET, PRICING, GTM, RUNBOOKS.
Skip docs the user says aren't relevant (e.g., PRICING for a free tool, RUNBOOKS for a library).

### 3. For each doc in scope — Socratic dialogue

**Do NOT just ask the user to fill in sections.** That's a form, not a dialogue.

Instead:

**If the doc is empty:**
- Start with an open question about the topic area. For VISION: "Tell me what you're building and why." For MARKET: "Who would use this instead of what?"
- Listen to the answer. Ask follow-up questions that probe assumptions, surface unstated constraints, and explore alternatives.
- After 3-5 exchanges, summarize what you've heard and propose content for each section.
- Ask the user to confirm or correct.

**If the doc is populated:**
- Read the content. Cross-reference with other docs and the codebase.
- Identify: gaps (sections that are thin or empty), staleness (content that doesn't match current reality), inconsistencies (VISION says X but SYSTEM shows Y), missing non-goals (scope creep signals).
- Present your findings as questions, not accusations. "VISION says this is for solo devs, but SYSTEM describes three microservices — is that complexity still justified?"
- For each issue, discuss and then propose an update.

**If the user provides conversation dumps:**
- Extract relevant information first, then use it as input to the dialogue. Don't just transcribe — synthesize and challenge. "From your conversations it looks like you considered X and Y but went with Y. Is that still the right call? The landscape may have shifted."

### 4. Write the docs

After each doc's dialogue is complete:
- Write the content following the schema constraints above.
- Preserve any existing research log entries — append, never delete.
- Set `Last reviewed:` to today's date.
- Show the user the full doc content before writing.
- Write only after confirmation.

### 5. Cross-doc consistency check

After all scoped docs are written, do a final pass:
- Does VISION's "Who" align with MARKET's "Users"?
- Does VISION's "Non-goals" align with what SYSTEM actually does?
- Does PRICING's model make sense for MARKET's positioning?
- Does GTM's channels match where MARKET's users actually are?
- Flag any inconsistencies for discussion.

## Rules

- Never write docs without the user's confirmation.
- Never bloat a section beyond its schema constraint.
- Never remove research log entries or feedback entries.
- Challenge assumptions — that's the point. Be direct, not sycophantic.
- If the user doesn't know something, capture it as an Open Question in VISION or a gap note in the relevant doc. Unknown is a valid state.
- If a doc isn't relevant to this project, skip it. Don't force structure.
