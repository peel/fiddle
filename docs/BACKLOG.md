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

### 2026-08-09 — What the scripted-gateway acceptance lane proves, and what it does not
`crates/fiddle-acceptance/tests/binary_repair.rs` closes the gap that `build_capability`'s document-to-capability wiring was gated by nothing: it binds a loopback port, answers the OpenAI chat-completions requests the real gateway client sends, and drives the compiled binary through a repair that writes the fix, passes the configured check and earns the correlation marker — offline, with a sentinel credential that authenticates nothing because the endpoint is the test's own socket. Three things it does not prove. First, only one bound is shown to travel from the document into `AgentBudget`: the paired scenario flips `max_turns` from 4 to 1 over an otherwise identical setup and watches the outcome change, and the other four bounds (`max_tokens`, `deadline`, `max_changed_files`, `tool_timeout`) are still carried by nothing but the code reading right — a swap of two of them would leave this lane green. Second, it is a *scripted* model: it says nothing about whether any real model can drive the loop, which is Tier 1's job and is deliberately never asserted there either. Third, its check compiles nothing — `grep` for the repaired text rather than the fixture crate's own test suite — for the toolchain-environment reason recorded under *A workspace check cannot find the macOS SDK* below; the `cargo test --offline` flavour of a check is gated by `repair_protocol` and not by this lane. Closing the first needs a scenario per bound whose configured value is small enough to be the thing that stops the run.
Origin: implementation (epic fiddle-y1w6, holistic remediation of the M1 seams)
Tags: #debt #test

### 2026-08-09 — `inspect --capability` names a capability `run` might refuse to build
`inspect` now takes the same `--capability` flag as `run`, so the two can no longer disagree about which capability is next. They can still differ about whether it can be *run*: `inspect` carries the id as far as `derive_next` and builds nothing from it, so `fiddle inspect beans:x --capability fixture_repair` reports `execute fixture_repair` over an M0 document with no `[agent]` table, while `fiddle run` over the same document exits 2 naming the missing table. This is deliberate — validating the deployment would mean `inspect` resolving a credential, which would end its read-only, offline, credential-free contract for the sake of a diagnostic `run` already gives — and it is not the defect that was closed, because both commands still name the same capability. But a caller reading `inspect` as "this will work" is reading more into it than it says. If that becomes a real confusion, the fix is a configured/unconfigured field in the `inspect` payload derived from which tables are present, never from resolving anything.
Origin: implementation (epic fiddle-y1w6, holistic remediation of the M1 seams)
Tags: #debt

### 2026-08-09 — A workspace check cannot find the macOS SDK, because the allowlist has no locator for it
`workspace::command` builds a child's environment from `env_clear` plus two inherited locators, `PATH` and `RUSTUP_HOME`, under the rule that *a locator may be inherited, an authority may not*. On macOS under this project's Nix dev shell that list is one entry short for anything that links: the shell also exports `DEVELOPER_DIR` and `SDKROOT` (and `MACOSX_DEPLOYMENT_TARGET`, `NIX_LDFLAGS`, `NIX_CFLAGS_COMPILE`), and stripped of them a nested `cargo test` prints `warning: failed running "xcrun" "--sdk" "macosx" "--show-sdk-path" to find MacOSX.sdk … unable to find sdk: 'macosx'` and links against whatever it can find. The consequence is not theoretical: driving the new black-box repair lane with `check = cargo test --offline` produced a test binary that failed nine consecutive runs — cargo reporting `error: test failed` over a tree whose source was verifiably repaired and whose tests pass when the same command is run with the shell's environment intact — and then passed twenty-nine consecutive runs, including eight under deliberate load, with no change to the sources. `repair_protocol` in `fiddle-runtime` gates the same `cargo test --offline` check and is exposed to the same thing; it has not been seen failing, which makes this a latent flake in the gate rather than a known one. `binary_repair` avoids it by pointing its check at a program that compiles nothing. Closing it means deciding whether an SDK path is a locator in the sense the module's rule means — it says *where a toolchain is* and grants no authority, which is exactly the `RUSTUP_HOME` argument — and, if so, which of the Nix shell's compiler variables belong on the list.
Origin: implementation (epic fiddle-y1w6, holistic remediation of the M1 seams)
Tags: #debt #test #risk

### 2026-08-09 — What M1's isolation does not claim, the other two: provider serialization and model refusal
Completes the set begun by *What M1's isolation does not claim: egress, injection, hostile processes* above. M1's design named five boundaries; that entry recorded three, and read as the whole list. The two it left out are recorded here so the five exist in a committed document rather than only in a gitignored spec.

**Provider-specific serialization is not claimed.** Every deterministic assertion about the tool protocol is made against what our client *builds*, never against what an upstream provider *receives*. `MockCompletionModel` replaces the provider entirely and serialises nothing to anyone, and `binary_repair.rs` serialises a real OpenAI chat-completions request only to its own loopback socket, which proves our client speaks the wire format and says nothing about LiteLLM's translation of it into whatever the upstream provider actually wants. The `OutputMode::Auto` defect in ADR 012 is the proof that this boundary has teeth: a gateway that reported `composes_native_output_with_tools()` truthfully about OpenAI and falsely about itself made the model call no tools at all, and no deterministic test could see it. Revisiting means a lane that inspects the request as the *upstream* provider received it — which needs either a gateway that echoes its translated request, or a per-provider fixture corpus captured from real traffic — plus a decision about which providers on this gateway are in scope, since the translation differs per upstream.

**Model refusal and truncation behaviour is not claimed.** The failure taxonomy covers the model producing the wrong *shape* — `AgentError::Protocol` for a report that misses the schema, for empty content, for an unregistered tool name. It does not cover the model declining the task on policy grounds, nor a completion cut short by `max_tokens` mid-tool-call, nor a long tool result silently truncated on the way back. All three arrive as either a schema failure or an ordinary unrepaired fixture, and the operator reads "the model did not hold up its end" for a run where the model held up its end and was stopped. Revisiting means deciding whether a refusal is a distinct outcome class or a `Retryable` like any other unrepaired run, and it interacts with the uncapped `read_file`/`list_files` entry above: a truncation the runtime causes and a truncation the provider causes should not be indistinguishable to a reader.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — design §6.8 named five boundaries, three were recorded)
Tags: #risk #debt

### 2026-08-09 — A spend-cap refusal is not distinguishable from any other provider failure
The gateway key carries a $100 hard cap, so requests eventually start failing on *spend* rather than on correctness, and every run from that moment reads as a broken capability. `AgentError` has four variants and `agent::classify` matches Rig's typed variants deliberately rather than its message text; a spend-cap refusal is an HTTP error with no typed variant, so it lands in the wildcard arm as `Provider { reason }` carrying Rig's rendering of the response body. `scripts/tier2.sh` records the outcome kind and the first 300 characters of that reason, which makes it legible to a human reading a Tier 2 artifact and to nothing else. Tier 1 does not classify at all. ADR 012 states this as an open consequence rather than as satisfied policy.

Not classified now because the only available signal is the gateway's error *text*, and this project has never seen it — the cap has never been reached. A classifier written against a guessed string fails open silently while claiming the coverage, and no test can pin it. Closing this needs, in order: a gateway key minted with a token `max_budget`, spent deliberately, and the response captured; then either a fifth `AgentError` variant in `crates/fiddle-runtime/src/agent/mod.rs` or a typed field on `Provider` that `scripts/tier2.sh` can key on without parsing prose. Both are small once the observation exists; neither is honest before it.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — ADR 012 consequence, deferred rather than asserted)
Tags: #debt #risk

### 2026-08-09 — Three backlog actions resolve to amending a gitignored file, and cannot be closed as written
`.gitignore` excludes `docs/plans/` and `docs/specs/` wholesale, and `.beans/` with them. A backlog action whose resolution is "amend the design" therefore names a document that exists on one machine, and the durable fallback the project states for design text — the bean body — is gitignored too. Three entries above are unclosable for that reason. This entry supersedes their *actions* only; their findings stand unchanged and their original text is left alone, because this file is append-only.

- **2026-08-08 — `ReportBundle.work_ref is Option<WorkRef>`**, whose action ends "Either tighten the type or amend the design." Only the first half is closable. The real action: change `crates/fiddle-core/src/report.rs` to a required `WorkRef`, fix the tests that construct `None`, and add the resulting guarantee to the invariant list in `docs/technical/SYSTEM.md`. If instead the `Option` is judged correct, say why *there*, in the same invariant list, since that is where a future reader looks for the contract.
- **2026-08-09 — `[workspace] fixture` and `check` are absent from the approved schema enumeration**, whose action is "The design text should catch up." It already has, in the only place that matters: `docs/technical/SYSTEM.md`'s **Data** section documents `fiddle.toml` with both keys named, so the committed record and the code agree and the entry is closable today by marking it so. What remains open is the second half — whether these keys belong to the deployment shape at all — and that is an ADR, not a text edit, because relocating them changes a document an operator writes by hand.
- **2026-08-09 — The design's credential-scrub requirement is stale, and the code is right**, whose action is "closes this by amending the design text". The finding is already durable: `docs/technical/SYSTEM.md`'s M0 acceptance paragraph states the four-name list, why it is not extended per milestone, and that `LITELLM_API_KEY` is covered instead by `capability_selection.rs`'s sentinel assertion. The entry is closable today by marking it so; nothing further is owed to a gitignored spec.

The general rule this establishes: a backlog action is written against `docs/BACKLOG.md`, `docs/technical/SYSTEM.md`, an ADR under `docs/technical/decisions/`, or a named code path. Never against `docs/specs/`, `docs/plans/`, or a bean body.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — holistic_spec_fidelity)
Tags: #debt #process

### 2026-08-09 — What the committed-ignore-rule boundary still does not cover
Deriving the changed-file set under the project's ignore rules *as committed* closes the case where an attempt writes `.gitignore` to hide what it created. Three things it does not close, recorded because each is a deliberate choice rather than an oversight, and each is documented at `Workspace::baseline_ignore` in `crates/fiddle-runtime/src/workspace/mod.rs`.

- **A file written into a path the project already excludes is still not counted.** `write_file("target/x")` lands somewhere the committed rules exclude, so `changed_files()` does not name it and the cap does not see it. This is the residue of the trade the exclusion exists for: the only alternatives are counting the whole `target/` tree a `run_check` produces, which drowns the evidence, or letting the worktree's own rules decide, which is the defect that was just fixed. It earns nothing — the check still decides the verdict, and a marker still requires a passing check — but it is a real gap in the *count*. Closing it needs a rule about where a repair may write at all, which is a policy nobody has argued for yet; a first step would be refusing writes under a baseline-ignored directory, which `git ls-files --others --ignored --exclude-from --directory` can enumerate.
- **Ignore files in subdirectories are not honoured.** `--exclude-from` reads one flat list whose patterns are relative to the top, and concatenating nested files would change what they mean. A monorepo that keeps its build-output rule in `crates/foo/.gitignore` would therefore have that output counted. The error is towards reporting *more*, which is the safe direction, and no fixture here has a nested ignore file — but the first repository that does will see noise in its evidence rather than a defect anyone notices.
- **`Workspace::read` runs one `git ls-files` per call.** Membership in the project is a question the filesystem cannot answer, so it is asked of git on the read path. Over a handful of reads per attempt this is nothing; over a repository with tens of thousands of tracked files and a model that reads freely it is a subprocess and a full listing each time. Caching it would need an invalidation story, because the model creates files as it goes.

Origin: implementation (epic fiddle-y1w6, bean fiddle-93cj — recorded while fixing the changed-file derivation, not deferred from it)
Tags: #debt #risk

### 2026-08-09 — The outer attempt bound's absence is now a decision, and this is what closes it
Supersedes the *action* of **2026-08-09 — `agent.max_capability_attempts` has no consumer** above; that entry's finding stands and its text is left alone, because this file is append-only. The decision is recorded in `docs/technical/decisions/013-one-attempt-bound-not-two.md`, which prices the change rather than deferring it again: `RunOutcome::Retryable` has four producers of which only one is "the capability tried and lost", so a retry loop needs a taxonomy the outcome type does not carry; and both placements for the loop move something committed — inside `run` it changes the shape of `capability_executions` and `progress` that every bundle consumer has seen, inside `attempt` it breaks the one-process-one-attempt-id premise that `fresh_invocation.rs` and `m0_skeleton.rs` both read. Taking it up means points 1–4 of that ADR in order, taxonomy first.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-9v2d)
Tags: #debt #process

### 2026-08-09 — What creating a directory on the model's behalf does not close
`Workspace::resolve` now walks to the deepest existing ancestor and `Workspace::write` makes the intervening directories, each proven inside the workspace by the same canonicalize-and-compare the leaf gets, before and again after creation. Three residues, none of them regressions and each a deliberate stop.

- **The check-to-write window is unchanged, not closed.** Nothing stops another process replacing a resolved component with a symlink between the containment check and the write; the fix re-canonicalizes the parent after `create_dir_all` and rebuilds the leaf on it, which narrows the window that creation opened but does not remove the one `std::fs::write` always had. Inside a per-attempt worktree the only other writer is the operator's own `run_check` program. Closing it properly means `openat`-style resolution against a directory handle — `cap-std` is the obvious candidate — which is a dependency decision, not a patch.
- **An empty directory a failed write left behind is invisible to the evidence.** git tracks files, so a `write_file` that resolved, made `src/newmod/`, and then failed to write leaves a directory that `changed_files()` will never name. It costs nothing and hides nothing that matters — no content is in it — but "the workspace is as the attempt left it" is one directory weaker than the changed-file set says.
- **Nothing bounds how deep a model may build.** `max_changed_files` caps the files; the directories on the way to them are uncounted and uncapped. Over M1's fixture this is invisible. It becomes a question at the same time as the uncapped `read_file`/`list_files` entry above.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-9v2d)
Tags: #debt #risk

### 2026-08-09 — Two claims above are corrected: the check's stderr *was* a leak, and the relativisation entry named only half its readers
Corrects, without editing, two entries dated the same day.

**2026-08-09 — `CheckFailed.stderr` is unbounded and reaches a published bundle** says "The path is already relativised, so this is a size problem rather than a leak." It was not relativised. `relativised` had exactly two call sites, both inside the `run_check` *tool*, so what was protected was the string handed to the **model**. `FixtureRepair::execute` calls `workspace.run(&config.check)` directly and puts `check.stderr` into `CapabilityError::CheckFailed`, which `orchestration::run` renders into `RunOutcome::Retryable.reason` and `ProgressEntry.summary` — so the absolute worktree path the model is protected from was published in `report.json` and printed on stdout. Both halves are now closed: relativisation moved into `Workspace::run`, the one place a `CommandResult` is constructed, so no reader of one can hold an unrelativised stream; and the size half is closed by `fiddle_core::Published`, the type of all four free-text bundle fields, whose only constructor bounds them to `PUBLISHED_TEXT_LIMIT` characters.

**2026-08-09 — Tool-output relativisation is a prefix rewrite, not a redactor** is right about what the function does and understated who reads it: "before the model sees them" and "the model cannot learn where this attempt is working" name one of two consumers. The published bundle is the other, and the one whose readers are not sandboxed. The residue that entry records is unchanged and still open — a child process printing a Nix store path, a `~/.cargo` checkout, or a path in a panic message is rewritten by nothing, and that is now true of the *bundle* as well as of the model's view.

**What remains open after this bean.** Three things, each a deliberate stop:
- `Published` bounds size and nothing else. It is not a redactor, and deliberately so: a denylist over content an adversary chooses is not a guarantee. The two channels that could carry a secret are handled where text *enters* — a provider response body is never quoted (`agent::provider_fault`), and a workspace command's output is relativised at construction — but a third such channel added later gets the bound and not the analysis.
- **A gateway that echoes a *fragment* of the credential is not covered by anything.** `provider_fault` withholds the whole body, so this is closed for the body; it is not closed for any future path that quotes provider text selectively. The general fix is a scrubber registered with the resolved credential at the one place it is read, which is a process-wide mutable registry and therefore an ADR rather than a patch.
- **`NextAction::Blocked.reason` is still a bare `String`** and is published. Its content is derived by `fiddle-core` from an observation, so it is host-authored and short by construction today — but it is the one published free-text field the bound does not reach, and the argument for that is a property of the current deriver rather than of the type.

Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-joen)
Tags: #debt #risk #security
