# Skill Authoring Improvements Design

**Date:** 2026-08-05
**Status:** Draft for review

## Goal

Improve Fiddle's skill discoverability and maintainability without changing lifecycle semantics, while making internal subagent model selection configurable.

## Scope

1. Rewrite 11 weak skill descriptions as compact, trigger-first one-sentence routers of roughly 15–25 words.
2. Split `orchestrate`, `deliver`, `write-plan`, and `develop-loop` into thin routing skills plus focused reference files. Preserve their invariants, phase flow, and behavior.
3. Add a skill-audit validator for description quality, broken references, orphaned companion files, and oversized `SKILL.md` files; integrate it with portability checks and CI.
4. Extend `models` configuration so internal subagents can inherit the session model or select explicit phase/role overrides. Keep this separate from external provider CLI selection.
5. Add optional agent-empathy prompts to discovery or brainstorming for capabilities, needed context/tools, and lessons from the previous run.

## Non-goals

- No session-history usage study or automatic skill consolidation.
- No change to lifecycle phase semantics, evaluator verdict rules, or external provider selection.
- No mandatory questionnaire on every task.

## Design constraints

- Keep the canonical shared `skills/` tree and all harness mappings portable.
- Keep mechanical validation in scripts with exit-code contracts.
- Keep primary skill files short enough to route; load detailed procedures only when the selected path requires them.
- Preserve existing defaults: an unspecified model inherits the current session model.
- Validate reference resolution and model configuration with focused deterministic tests.

## Acceptance criteria

- All 11 selected descriptions are concise, trigger-first, and remain valid portable frontmatter.
- The four split skills retain all required instructions, with no broken or orphaned references.
- CI fails on malformed metadata, broken references, orphaned companion files, or configured size violations.
- Internal subagent dispatches resolve a configured model deterministically, with explicit overrides taking precedence over phase defaults and otherwise inheriting the session model.
- Optional empathy prompts appear only in the intended discovery/brainstorming paths.
- Existing portability, context-assembly, bean-body, and scorecard checks remain passing.
