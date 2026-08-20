# Writing templates

Advisory, not gate-enforced. Caps are targets; exceeding one is a signal the content belongs somewhere else, not an error.

`Cites:` is the load-bearing line. It names the symbols a document's claims depend on, so changing a symbol greps to every document asserting something about it. M4c lost a holistic pass because `fiddle_core::selected` changed and ADR 021's rationale — which depended on it — was never checked.

## ADR

```
Status: accepted | superseded by NNN
Cites: fiddle_core::selected, cve/project.rs::names_a_fix
Context: <= 3 sentences
Decision: <= 3 sentences, imperative
Consequences: <= 5 bullets, <= 2 lines each, one naming what was given up
```

Reasoning lives here and nowhere else. If SYSTEM.md or a commit body is explaining why, it belongs in an ADR instead.

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
