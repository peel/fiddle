---
name: challenge
description: Use when a plan or design needs assumptions, constraints, and decision branches resolved before progressing.
---

Challenge every aspect of this plan or design until we reach shared understanding. Walk down each branch of the decision tree, resolving dependencies between decisions one-by-one.

If a question can be answered by exploring the codebase, explore the codebase instead of asking.

ARGUMENTS: {ARGS}

## Phase behavior

**`--phase discover`:** Read docs in `docs/product/` and `docs/technical/`, recent beans, and any `--context` files. Open by synthesizing what discover-docs and external research found — present the scope as you understand it, confirm the user's intent, then challenge assumptions and constraints. Done when scope has no open branches. State "Scope is solid — ready for DEFINE" and stop.

**`--phase define`:** Read the design doc (most recent in `docs/plans/`), panel commentary if present, relevant source files, and any `--context` files. Challenge edge cases, integration points, panel dissent, failure modes, and sizing. Done when every decision branch is resolved. State "Design holds up — ready for implementation planning" and stop.

**Standalone:** Challenge whatever the user presents. No phase-specific framing. Done when all branches are resolved — summarize the resolved decision tree and stop.

## Usage

Invoke as `fiddle:challenge [--phase discover|define] [--context file1 file2]`.

