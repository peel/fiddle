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

### 2026-08-09 — What M1's isolation does not claim: egress, injection, hostile processes
The ephemeral worktree, the `env_clear` allowlist and the `WorkspacePath` containment together bound what an attempt can *reach on this filesystem* and what it can *see of the host environment*. Three things they do not bound, all of them deliberately out of M1's scope and none of them stated anywhere else. Network egress is not sandboxed: the check command runs with a real `PATH` and nothing stops a build script or a test from opening a socket. Prompt injection from repository contents is untested: `read_file` returns whatever the fixture holds, straight into the model's context, and no scenario places adversarial instructions in a file to see what happens. Hostile-process containment is out of scope entirely: the process group and the timeout stop a *hung* child, not a determined one, and a check command is by construction arbitrary code the operator asked to run. A milestone that runs this capability over a repository it did not author needs all three revisited.
Origin: implementation (epic fiddle-y1w6, M1 threat-model boundary)
Tags: #risk #debt

### 2026-08-09 — `RunOutcome::Suspended` is the one exit-code row never exercised end to end
Every other row of the exit-code table is driven by a real scenario. `Suspended` maps to exit 10 and is reachable only through a human decision point that nothing in M0 or M1 has: `main.rs` covers it in a unit test of the mapping function itself, and no capability can be driven into producing it. It stays an untested path in the one table an operator reads first. The milestone that introduces an attended decision closes this by construction; until then the row is documentation.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt #test

### 2026-08-09 — Claude-family models finalise after one tool call through this gateway
Measured over the trivial Tier 1 fixture at reference bounds: `claude-haiku-4-5` makes one `list_files` call and stops, and `claude-sonnet-5` does the same and then fails its own report schema with `missing field changed_files at line 1 column 11`. `bedrock/moonshotai.kimi-k2.5`, `deepseek.v3.2` and `zai.glm-5` all drive the full loop — list, read, write, check — and earn the marker. The mechanism is unpinned. Sonnet's diagnostic suggests it calls the synthetic output tool `OutputMode::Tool` registers with the wrong arguments, and that rig's re-prompt path does not recover from it; that is a hypothesis, not a diagnosis. This is a property of the gateway's translation rather than of the models, the deterministic suite structurally cannot see it (ADR 012), and it is the reason both real-model tiers default to kimi rather than to a Claude model.
Origin: implementation (epic fiddle-y1w6, Task 14 Tier 1 measurement)
Tags: #risk #debt

### 2026-08-09 — `read_file` and `list_files` are uncapped
Neither tool bounds what it returns. `read_file` hands back a whole file however large it is, and `list_files` walks the whole worktree; both results go straight into the model's context and are billed as input tokens. Over M1's deliberately trivial fixture this is invisible. Over any real repository a single `read_file` on a large generated file, or a `list_files` over a large tree, blows the context window or the budget with no bound anywhere in the path — `max_tokens` bounds the *completion*, not the prompt. A cap needs a decision about what truncation looks like to a model that then acts on the truncated view.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt #risk

### 2026-08-09 — `CheckFailed.stderr` is unbounded and reaches a published bundle
`CapabilityError::CheckFailed` carries the check's stderr verbatim so an operator can see why the repair was refused, and the rendered error reaches the evidence bundle. The check is an arbitrary operator-configured program: a failing `cargo test` over a real project emits kilobytes, and nothing truncates it. The path is already relativised, so this is a size problem rather than a leak, but a published artifact with an unbounded field in it is a published artifact whose size nobody controls.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt

### 2026-08-09 — Receipts publish as a summary; durations are collected and unpublished
`ToolReceipt` carries `tool`, `outcome` and `duration_ms`, and the bundle receives `tools:<n>` plus per-tool outcome counts as `EvidenceRef` strings. `EvidenceRef` is a string and the bundle's evidence is a list of them, so the records themselves have no home in the report schema — widening a published contract was out of scope for the task that added them. The consequence is that `duration_ms` is measured on every call and read by nobody: latency per tool, which is the first thing anyone tuning `tool_timeout` would want, exists in memory and is discarded. Giving receipts a typed home in the report schema is the fix, and it is a schema change.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt

### 2026-08-09 — `WorkspacePath` rejects `a:b/c.rs`, a legal Unix filename
`WorkspacePath::parse` refuses any path whose second character is `:`, which is the Windows drive-letter shape and a cheap syntactic rule that cannot be defeated by a race. It also rejects legal Unix filenames: `a:b/c.rs` names a perfectly ordinary file that no model could ever read or write through these tools. Nothing in M1's fixtures is affected, and the rule is the safe direction, but a capability pointed at a real repository containing such a path would refuse it with a diagnostic about escaping the workspace, which is not what happened.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt

### 2026-08-09 — The broken-fixture builder is duplicated in three places
`crates/fiddle-runtime/tests/fixture.rs`, the unit tests inside `crates/fiddle-runtime/src/capability/repair.rs`, and `crates/fiddle-cli/tests/smoke.rs` each build the same small broken crate. They are three copies because a `src/` unit test cannot reach a `tests/` module, and a `tests/` module of one package cannot be reached from another package at all. Removing the duplication means either a `#[cfg(feature = "test-fixtures")]` module in `fiddle-runtime` — test scaffolding in a shipped library — or a fifth workspace member existing to hold thirty lines of `std::fs::write`. Both are larger than the duplication they remove, which is why it is recorded rather than fixed; the cost is that a change to the fixture's shape has to be made three times.
Origin: implementation (epic fiddle-y1w6, Tasks 13 and 14)
Tags: #debt #test

### 2026-08-09 — Tool-output relativisation is a prefix rewrite, not a redactor
`relativised` rewrites both spellings of the workspace root out of a check's stdout and stderr before the model sees them, which is what stops `cargo`'s `Compiling foo (/…/ws/<attempt>)` from handing over the operator's directory layout. It is a string replacement and nothing more. A child process is free to print any other absolute path it likes — a toolchain in the Nix store, a registry checkout under `~/.cargo`, a path in a panic message — and none of those are rewritten. What the function guarantees is that the model cannot learn *where this attempt is working*; it does not guarantee that no host path reaches the model.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt #risk

### 2026-08-09 — Single-provider plan critique is now the intended state, not a gap
`gemini` has been removed from provider dispatch in `orchestrate.json`. It failed authentication with exit 41 in both the M0 and the M1 plan critique, so every multi-provider critique this project has actually run has been a single-provider one wearing a two-provider configuration — which is worse than one honestly configured, because the missing second opinion looked like a degradation rather than a decision. The removal makes the configuration match reality. Restoring adversarial breadth means either fixing gemini auth or adding a different second provider, and either is a deliberate act rather than a regression to notice.
Origin: implementation (epic fiddle-y1w6, plan critique)
Tags: #debt #infrastructure

### 2026-08-09 — The design's credential-scrub requirement is stale, and the code is right
M1 design §6 item 4 states that "the acceptance lanes scrub [`LITELLM_API_KEY`] alongside the four M0 already removes". They do not, and should not. `support::CREDENTIAL_VARS` is a four-name list — `GITHUB_TOKEN`, `GH_TOKEN`, `ANTHROPIC_API_KEY`, `JIRA_API_TOKEN` — pinned by an assertion inside `m0_skeleton.rs` itself and mirrored by hand in `peel/fiddle-acceptance`, so extending it is a two-repository change. Extending it would also prove nothing: the M0 scenario runs `stub_mark`, which never reaches a model, so removing a gateway credential from it demonstrates only that an unused variable was unused. The property the design was reaching for is proved instead, and more strongly, by `capability_selection.rs`, which sets `LITELLM_API_KEY` to a sentinel and asserts the sentinel appears in no stdout, no diagnostic and no published bundle. Recorded so the next reader closes this by amending the design text rather than by lengthening a pinned, cross-repository list.
Origin: implementation (epic fiddle-y1w6, Task 15 verification)
Tags: #debt
