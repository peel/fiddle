# Writing templates

Caps are targets; exceeding one is a signal the content belongs somewhere else, not an error. One line is enforced: `scripts/check-adr-cites.sh` runs in `scripts/gate.sh` and fails the gate on a `Cites:` entry that resolves to nothing, or on an ADR numbered 021 or above that carries no `Cites:` line at all.

`Cites:` names the symbols a document's claims depend on, so changing a symbol greps to every document asserting something about it. M4c lost a holistic pass because `fiddle_core::selected` changed and ADR 021's rationale — which depended on it — was never checked. ADRs 001 to 020 predate the line and are not retrofitted; the check's floor is where the retrofit stopped.

An entry is a **path** if it contains `/` and ends in `.rs`, `.sh`, `.toml` or `.md`, and it must name a file that exists — `workspace/command.rs` resolves as a suffix, so a partial path is enough and `target/` cannot satisfy one. Anything else is a **symbol**, and it must appear in the text of some file under `crates/`. `cve/project.rs::names_a_fix` is a symbol: the check reads what follows the last `::`. Getting that distinction wrong is what shipped a red gate — a real file that no file happens to mention failed a content grep.

Cite a symbol, not a line. A line number is invalidated by any edit above it, including an edit in the same commit: every one of ADR 028's three references into ADR 021 landed two lines short because the commit that wrote them also grew 021's Status by two lines. Quote the sentence or name the function. Where a line number is unavoidable, point it at a file the same change does not touch.

## ADR

```
Status: accepted | superseded by NNN | accepted; amended in <milestone> by NNN
Cites: fiddle_core::selected, cve::verdict::Row
Context: <= 3 sentences
Decision: <= 3 sentences, imperative
Consequences: <= 5 bullets, <= 2 lines each, one naming what was given up
```

`Status:` and `Cites:` are plain lines, not bold ones. Thirteen ADRs use a bold `**Status:**` beside a `**Date:**` line, most recently 024; they are left alone, and `docs/technical/decisions/` carries no second template offering that form.

Reasoning lives here and nowhere else. If SYSTEM.md or a commit body is explaining why, it belongs in an ADR instead.

An accepted ADR is not rewritten, and it is not left reading as though nothing happened either. A later record amends it; the amended ADR gains the `amended in` Status form and a one-line marker, as a `>` quote, under each sentence that no longer holds. ADR 021 carries four. A pointer in the Status line alone leaves the false sentence intact for every reader who meets it before the pointer.

An amendment that has nothing to correct in the wording of the record it amends belongs inside that record as an `## Amendment (<milestone>) — <what changed>` section, as in ADRs 019 and 025. One that contradicts a sentence belongs in a record of its own.

## Bean body

```
Why: <= 2 sentences
Cites: symbols and file:line the task turns on
Files: list
Steps: checkboxes, <= 1 line each
Evaluation: one line per dimension
Findings: table -- file:line | what | owner
```

Append findings to the table. Do not append essays.

## Commit

```
subject <= 72 chars
body <= 5 lines: what changed, why, what it cost
```

No narration of the process. The diff shows what happened.

## SYSTEM.md component

```
name | path | what it does (<= 1 line) | invariant (<= 1 line)
```

No paragraphs. A component needing three paragraphs is either several components or an ADR.

## Lane brief

```
Task: 1 sentence
Base: SHA + measured gate numbers
Scope: what is yours / what is not
Traps: <= 5 bullets, each a measured fact
Verify: commands + expected numbers
Report: the numbers wanted back
```

Every trap must be something measured, not something feared. An unmeasured warning sends a lane hunting a phantom.
