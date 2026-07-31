# Claude-5-Era Skill Tree Slim-Down — Design

Epic: TBD at bean creation. Date: 2026-07-29.
Source: claude.com/blog/the-new-rules-of-context-engineering-for-claude-5-generation-models

## Problem

Fiddle's 51 skill files carry the guardrail style the Claude 5 guide identifies as counterproductive for current models: 47 HARD-GATE blocks, 44 MUSTs, 45 "Do NOT", 6 rationalization tables, 10 Red Flags sections, 5 Announce lines, an iron-laws file loaded twice, and the orchestrate.json schema documented three times. Anthropic removed over 80% of Claude Code's system prompt for Claude 5 models with no measurable loss; fiddle bets the same holds for its skills. Confirmed scope decisions: one slim text for all harnesses (no density overlays), full sweep this epic, no eval harness.

## House Style

A slimmed skill contains: one purpose sentence; the flow; plain script invocations ("Run X; act on its verdict"); explicit cross-references to sub-skills and reference files; and a one-line rationale wherever an invariant needs weight. Removed: HARD-GATE/GATE markup, caps emphasis, rationalization tables, Red Flags sections, Announce lines. Preserved verbatim: frontmatter `description` fields (they are the trigger selector; slimming them risks non-activation) and every "use the X skill" handoff pointer (slimming callers breaks progressive disclosure on harnesses without hooks).

## Prompt-Side Invariant Set

Codex/Pi have no hook parity, so ordering and honesty invariants survive as single plain statements with rationale, each in the one file that owns it:

- brainstorm: design approved by the human before any implementation action
- tdd: test written and seen failing before implementation
- debug: root cause established before fixes; repeated failed fixes mean stop and question the approach
- verify: verification output precedes any claim of success
- evaluate: do not trust implementer claims; score only what evidence supports, cite the artifact; output contract (JSON shape, provider field, explicit `dimensions: {}` for evidence-only)
- develop-loop: implementer DONE is a claim, not evaluation; budget exceeded means stop and ask the human; spec-defect routes to needs-attention without re-dispatch
- runtime-evidence: evidence recorded from the live app before scoring runtime dimensions
- blind-spot-check: human scores committed before any evaluator scorecard is revealed
- hold-out criteria: never shown to implementers, in prompts or feedback
- develop: holistic review runs after per-task loop, before finish-branch

## New Validators (exit-2 contracts)

- `scripts/validate-bean-body.sh` — eval block present (fenced, domains + criteria), files section, steps checklist. Replaces develop Step 1's prose gate; develop runs it per bean and stops on exit 2.
- `scripts/validate-scorecard.sh` — criteria ids exactly match the bean's eval block, every criterion and scored dimension carries non-empty evidence, provider field present, `dimensions` is an object, `spec_defect` shape valid when present. develop-loop runs it on each evaluator scorecard before the merge; invalid scorecard means one re-dispatch, then needs-attention (this also writes down the previously unwritten re-dispatch policy).

Both get `test-*.sh` suites in the existing harness style. `validate-orchestrate-config.sh` is backlogged, not built.

## Dedup and Contradiction Sweep

The orchestrate.json schema lives in orchestrate/SKILL.md alone; develop and develop-loop link to it. iron-laws.md is deleted; its content is absorbed by the invariant set above. Duplicated evaluator-config extraction text collapses to the owning file. While deduplicating, contradictions between the surviving copies are resolved, not just merged (the guide names conflicting guidance as the primary anti-pattern).

## Execution: Staged Full Sweep

Four families, each ending with the full `test-*.sh` sweep plus portability checks green before the next begins:

1. develop family: develop, develop-loop + references, develop-holistic, evaluate + four templates, runtime-evidence
2. lifecycle: orchestrate, discover, discover-docs, define, deliver + references, deliver-docs, quickfix
3. process: brainstorm, write-plan, define-beans, challenge, panel, tdd, debug + references, verify, worktrees, finish-branch
4. utilities: using-fiddle, adr, backlog, feedback, archive, init, insights, evaluate helpers, remaining references

## Anti-Regression

SYSTEM.md gains one invariant: skills are written in the house style (judgment plus rationale; invariants in scripts; no emphatic markup). using-fiddle gains a three-line authoring note stating the same for contributors.

## Success Measure

Guardrail greps near zero outside the invariant set (HARD-GATE 0, rationalization tables 0, Red Flags 0, Announce 0; MUST/NEVER only inside frontmatter descriptions or quoted external content). Tree size reduced substantially. All existing test suites and the two new validator suites green at every family checkpoint. No behavioral eval harness this epic (accepted risk): live usage is the detector, and the staged checkpoints bound the blast radius.

## Out of Scope

- validate-orchestrate-config.sh (backlogged)
- Density overlays for weaker models (decided against)
- Behavioral eval harness (decided against; revisit if regressions surface)
- CLAUDE.md/README top-level rewrites beyond what dedup touches

## Decisions Log

- Single slim text, full sweep, no eval harness: user-confirmed 2026-07-29 (discover challenge).
- C-prime shape (compact prompt invariant set, second validator, staged families): codex panel critique folded in; gemini returned no output (reduced coverage recorded).
- Design approved: user-confirmed 2026-07-29.
