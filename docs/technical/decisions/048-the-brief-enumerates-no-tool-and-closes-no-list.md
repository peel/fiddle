# 048 — The brief enumerates no tool and closes no list

Status: accepted
Cites: fiddle_runtime::agent::PREAMBLE, fiddle_runtime::agent::briefed, fiddle_runtime::agent::denies_an_ability, no_brief_denies_an_ability_the_tool_set_gives, the_brief_claims_no_ecosystem_and_no_size_for_the_project, MIGRATION_PREAMBLE, the_migration_brief_denies_no_ability_the_tool_set_gives

## Context

The brief opened with three sentences. It called the project a small Rust project, listed four capabilities, and then said the model could do nothing else. The deployment repairs a large Go project, and a deployment that declares a program gets five tools.

ADR 044 added `run_command`. ADR 047 named the declared programs in the brief. `run_command` was then called zero times across runs 32581705211, 32586384068 and 32589291582. The appendix that offers the tool is appended after the sentence that denies it, so the model read the prohibition first and the permission second. It rewrote `go.sum` with `write_file` instead, from 985 lines to 2, in pull request #251.

## Decision

**The brief describes the tool set by reference and never by enumeration.** It says "Use the tools this run offers you". The provider carries the schemas, so one list of tools reaches the model and there is no second list to disagree with it.

**The brief states no closure of what the model can do.** A sentence that denies every other action denies a tool the run registered, whichever tool that is.

**The brief claims nothing about the project's language or its size.** fiddle knows the tools, the check, the path boundary and the report it needs. It does not know the ecosystem, and ADR 025 leaves that with the agent.

## Why the enumeration was the fault, and not the count

Correcting four to five would be wrong again at the next tool. The enumeration also repeated the tool schemas, and a repeated fact goes stale on one side alone. Naming no tool costs the model nothing, because the schemas name all of them.

## How the claim is asserted

`denies_an_ability` splits a brief into sentences and returns the sentences that both deny an ability and quantify over every action. `no_brief_denies_an_ability_the_tool_set_gives` runs it over two briefs, one from a deployment that declares a program and one from a deployment that declares none.

That test carries two controls, because a text check which matches nothing proves nothing. It asserts that the removed sentence is caught. It asserts that a sentence which denies one thing and quantifies over none is not caught, so the check is not flagging every denial in the brief.

## Consequences

- **A new tool needs no change to the brief.** The registration and the schema are the whole of the work, and no prose states a count.
- **ADR 047's appendix now contradicts nothing above it.** The sentence it follows offers the tool set instead of closing it.
- **The migration brief in `cve.rs` carries the same two opening sentences.** It runs against the same tools, so the same claim is asserted over it.
- **The brief is 21 words shorter.** Length is not free: a brief with six added lines returned nothing parseable in 38 seconds, and the same binary without them ran for 13 minutes and returned a well-formed report.
- **What was given up: the brief no longer tells the model that it can read a file.** A model that ignores the schemas has one fewer place to learn its tools. The refusal ADR 044 names is the other.
