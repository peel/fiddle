---
name: adr
description: Use when recording a consequential technical decision; captures context, the chosen option, and consequences in a new architecture decision record.
---

# ADR


## Usage

Invoke as `fiddle:adr <title> — short description of the decision`.

Create a new Architecture Decision Record. Read
`skills/using-fiddle/references/writing-templates.md` first: it holds the ADR
template, the `Cites:` line the gate enforces, and the amendment convention.

## Process

1. Read the user's argument as the decision title. If no argument, ask for one.
2. Find the next ADR number: `ls docs/technical/decisions/[0-9][0-9][0-9]-*.md | sort -n | tail -1`. Extract number, increment. If no existing ADRs, start at 001.
3. Ask the user (briefly — 2-3 questions max):
   - What's the context? (What prompted this decision?)
   - What did you decide? (Be specific.)
   - What are the consequences? (Tradeoffs, what gets easier/harder.)
   If the user already provided enough detail in the argument or conversation, skip questions and draft directly.
4. Write `docs/technical/decisions/NNN-kebab-case-title.md` to the ADR template in
   `skills/using-fiddle/references/writing-templates.md`. The `Cites:` line names every
   symbol or file the record's claims depend on; `scripts/check-adr-cites.sh` fails the
   gate on a symbol no file under `crates/` contains, on a path that names no file, and
   on an ADR numbered 021 or above that omits the line.
   Write `Cites: none` when the decision genuinely turns on no symbol, as ADR 024 does.
5. Verify with `scripts/check-adr-cites.sh` before showing the user.
6. Show the user the file content. Write after confirmation, creating `docs/technical/decisions/` if it does not exist.

Keep it to 10-20 lines; an ADR records a decision, not a design. Filenames are kebab-case, as in `003-use-alloydb-over-cloudsql.md`.

An accepted ADR's reasoning is not rewritten. A decision that changes is superseded by a
new record; reasoning that a later change falsified is amended by one. The amended ADR
gains the `amended in` Status form and a `>` marker under each sentence that no longer
holds — a Status pointer alone leaves the false sentence intact for every reader who
meets it first. ADR 021 is the worked example.
