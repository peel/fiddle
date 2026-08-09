# Backlog

<!-- Ideas, technical debt, someday/maybe items. Cross-cutting — covers both
     product and technical concerns. Append-only with dates.
     
     Periodically review: promote to beans, delete, or leave.
     Agents can suggest additions here during brainstorming.
     Agents should read this before planning to avoid re-discovering known items. -->

<!-- ### YYYY-MM-DD — Title
     Description of the idea or debt item.
     Origin: brainstorm session / feedback / code review / noticed during work
     Tags: #idea #debt #optimization #feature #experiment #infrastructure
-->

### 2026-04-02 — Skill size estimates are unreliable for verbatim extraction
Design spec predicted 12KB for develop-loop; actual was 20.5KB (71% over). JSON examples, code blocks, and HARD-GATE blocks are denser than prose and harder to estimate. Future specs involving skill extraction should measure the source line range directly rather than estimating.
Origin: implementation (develop modularization epic fiddle-wdg0)
Tags: #debt #optimization


### 2026-07-28 — PR-review feedback channel into calibration/antipattern memory
Harvest human review comments from finish-branch PRs back into calibration and antipattern files, /iterate-style, closing the reviewer-to-memory loop that only the attended gate feeds today. Deferred from epic fiddle-sip9 (one loop at a time).
Origin: brainstorm (fiddle-sip9), research: humanlayer/skills design-control-loop
Tags: #idea #feature

### 2026-07-28 — Scheduled antipattern-eradication maintenance loop
Scheduled CI workflow that consumes deliver's antipattern files: a script scans the codebase for occurrences, one is picked per run, a coding agent fixes it and opens a PR, at most one PR open at a time, human merges. Continuous cleanup between epics, complementary to develop-loop. Deferred from epic fiddle-sip9.
Origin: brainstorm (fiddle-sip9), research: humanlayer/skills design-control-loop
Tags: #idea #infrastructure #experiment

### 2026-07-29 — check-convergence.sh budget-count convention and double-pass headroom
The budget check runs before verdict evaluation, so post-dispatch counts flag DISPATCHES_EXCEEDED for a completed passing run; a pass on iteration N can never confirm within budget N. Decide pre-dispatch counting explicitly and set defaults with double-pass headroom.
Origin: implementation (epic fiddle-sip9, hit at both per-task and holistic budgets)
Tags: #debt #infrastructure

### 2026-07-29 — Holistic scorecard shape vs merge-scorecards input
holistic-scorecard-schema.md's example (top-level domain/dimensions, no criteria array) does not match merge-scorecards.sh expectations (domains wrapper + criteria required); the wrapping step is unspecified and lives in orchestrator judgment. Specify the shape or add a wrapper.
Origin: implementation (epic fiddle-sip9 holistic phase, fails loud exit 2 since the criteria validation)
Tags: #debt

### 2026-07-29 — develop-loop 1f wording and per-domain selected-provider files
Reword 1f's "the evaluator may interact with the running app" to match the interpret-only role, and name selection output selected-provider-{domain}.json so multi-domain PASS_PENDING reuse reads the right provider.
Origin: code-review (holistic review of epic fiddle-sip9)
Tags: #debt

### 2026-07-29 — patsub_replacement mangles ampersands in dispatch payloads
On bash 5.2+ with patsub_replacement, "&" in --diff-file/--evidence-file content is rewritten to the placeholder text during PROMPT substitution in dispatch-provider.sh. Quote the replacement or use a temp-free jq substitution.
Origin: code-review (fiddle-im2e confirming evaluation; pre-existing, affects --diff-file too)
Tags: #bug #debt
Status: Resolved 2026-08-05 by literal marker splitting in `dispatch-provider.sh`, covered across instructions, diff, evidence, and prior feedback.

### 2026-07-30 — Three debug reference files were pointed at but never written
skills/debug/SKILL.md referenced root-cause-tracing.md, defense-in-depth.md, and condition-based-waiting.md, none of which have ever existed in skills/debug/. The pointers were dropped and their one-line substance folded into the surrounding prose; if backward call-stack tracing, layered validation, or condition-based waiting deserve full treatments, they need writing rather than referencing.
Origin: implementation (epic fiddle-85jh, Claude-5 skill slim-down, utilities family)
Tags: #debt #idea

### 2026-08-08 — Permission-injection tests no-op silently under a root identity
Three tests in `crates/fiddle-runtime/tests/attempt.rs` return early via `if record.published.is_some() { return; }`, an escape hatch for an identity that ignores permission bits. Under a root CI runner they pass without asserting anything instead of skipping visibly, so the fail-closed guarantees they cover would go unverified without anyone noticing.
Origin: holistic review iteration 2 (epic fiddle-7lmw, bean fiddle-9mgy)
Tags: #debt #test

### 2026-08-08 — Acceptance lane parity is maintained by hand
`docs/technical/acceptance-repository.md` states the in-repo `m0_skeleton.rs` and the external `scenarios/m0_skeleton.sh` "assert the same properties by design", and warns that divergence makes one of them the weaker proof. Nothing checks it mechanically. The two have already drifted once: the in-repo lane was missing the fail-closed step and the non-empty `attempt_id` assertion, and the in-repo lane is the one CI names and later milestone seeds inherit as their baseline.
Origin: holistic review iterations 1 and 2 (epic fiddle-7lmw, beans fiddle-nciw, fiddle-89lv)
Tags: #debt #test

### 2026-08-08 — ASCII-only invocation values may reject M1's external identifiers
ADR 011 constrains an invocation reference value to ASCII letters, digits, `-`, `_` and `:` at the parse boundary, which is the safe direction for path derivation. M1 introduces `jira`, `scheduled` and `scanner` references from external systems whose identifiers may contain non-ASCII characters and would now be rejected with exit 2. Confirm against real identifier formats before those adapters land.
Origin: implementation (epic fiddle-7lmw, bean fiddle-1p8q)
Tags: #idea #risk

### 2026-08-08 — ReportBundle.work_ref is Option<WorkRef> but the design requires it
Design §4.7 models `work_ref` as a required `WorkRef`; `crates/fiddle-core/src/report.rs` declares `Option<crate::identity::WorkRef>`. The runtime always supplies `Some` and the emitted bundle always carries it, but the type permits `None` and tests construct it, so the type is weaker than the contract it stands for. Either tighten the type or amend the design.
Origin: deliver drift analysis (epic fiddle-7lmw)
Tags: #debt

### 2026-08-09 — A capability's attempt id is not the bundle's attempt id
`RepairConfig.attempt` names the per-attempt worktree and is the suffix of the evidence reference `repair:<changed>:<attempt>`; `capability/repair.rs` states that this lets a reader tie the reference back to the record of the same attempt. It does not. `fiddle_runtime::attempt` mints the run's id itself — so that no caller can hand it a duplicate and collide two bundles on one path — while the capability is constructed by the CLI *before* that call and therefore mints its own. Both ids are unique and nothing on disk is malformed; the cross-reference simply is not real. Closing it means deciding where an attempt id is minted: passing one into `AttemptContext` gives up the "minted once, here" property that makes a bundle collision impossible, and handing the id to the capability at `execute` time instead changes the `Capability` trait. Either is a decision about the orchestration's contract rather than about wiring.
Origin: implementation (epic fiddle-y1w6, Task 12 wiring the capability selection)
Tags: #debt

### 2026-08-09 — `[workspace] fixture` and `check` are absent from the approved schema enumeration
Design §6.6 enumerates `[workspace]` as `root`, `isolation`, `command_timeout`, `cleanup`, and `[agent]` without a repository or a check. `fiddle_runtime::RepairConfig` needs both, and `deny_unknown_fields` leaves an operator no other way to supply them, so Task 12 added `workspace.fixture` and `workspace.check = { program, args }` as `Option` with no default. The design text should catch up, or the keys should be moved to wherever the milestone that owns the deployment shape wants them.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt

### 2026-08-09 — `agent.max_capability_attempts` has no consumer
The outer attempt bound parses, defaults to 3, and is read by nothing: `fiddle_runtime::attempt` runs one attempt and reports `RunOutcome::Retryable` for a caller to repeat. It carries the one remaining `#[allow(dead_code)]` in `fiddle-cli`. Reading it means writing a retry loop, which changes what every existing retryable outcome does — M0's included — and belongs to the milestone that owns the durable lifecycle.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt

### 2026-08-09 — The second interrupt's exit path is untested
`fiddle run --capability fixture_repair` installs a `SIGINT` handler: the first interrupt cancels the token, the second exits 130. The first is pinned by `capability_selection::an_interrupt_cancels_the_attempt_rather_than_killing_the_runner_under_it`; the second is not, because a cancelled attempt concludes in tens of milliseconds and racing a second signal into that window is not something a test can do reliably. Reaching it deterministically needs a capability that can be made to hang *after* cancellation.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt
