# Backlog

<!-- Ideas, technical debt, someday/maybe items. Cross-cutting — covers both
     product and technical concerns. Append-only with dates.

     THE RULE, AND IT IS ONE RULE. This file grows at the end. An entry's
     finding text is never rewritten and no entry is ever deleted — the record
     of what was found outlives whether it still matters, and a list that
     forgets its own history cannot be read against the tree. The same rule is
     stated as an invariant in docs/technical/SYSTEM.md.

     Exactly two moves are available:
       - Append a `Status:` line to an entry, recording its resolution. This is
         the only edit an existing entry ever receives.
       - Append a NEW entry that names the one it acts on, to correct a claim,
         supersede an action, or close a finding. Superseding an action is the
         pattern this file has used throughout; the superseded entry's text
         stays as written.
     Promoting an entry to a bean is a `Status:` line, not a deletion.

     A backlog action must be written against a committed document — this file,
     docs/technical/SYSTEM.md, an ADR under docs/technical/decisions/, or a
     named code path. Never against docs/specs/, docs/plans/, or a bean body:
     all three are gitignored, so an action pointing at one cannot be closed by
     anybody who does not already have the machine it was written on.

     Agents can suggest additions here during brainstorming.
     Agents should read this before planning to avoid re-discovering known items. -->

<!-- ### YYYY-MM-DD — Title
     Description of the idea or debt item.
     Origin: brainstorm session / feedback / code review / noticed during work
     Tags: #idea #debt #optimization #feature #experiment #infrastructure
     Status: (appended later, when the entry is resolved, superseded or promoted)
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
Status: Resolved 2026-08-09 through the `ExecutionGrant`, recorded as `decisions/014-the-grant-carries-the-attempt.md` and asserted from outside the process by `binary_repair::the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under`. See the closing entry *Two entries above are closed* below.

### 2026-08-09 — `[workspace] fixture` and `check` are absent from the approved schema enumeration
Design §6.6 enumerates `[workspace]` as `root`, `isolation`, `command_timeout`, `cleanup`, and `[agent]` without a repository or a check. `fiddle_runtime::RepairConfig` needs both, and `deny_unknown_fields` leaves an operator no other way to supply them, so Task 12 added `workspace.fixture` and `workspace.check = { program, args }` as `Option` with no default. The design text should catch up, or the keys should be moved to wherever the milestone that owns the deployment shape wants them.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt
Status: First half resolved 2026-08-09 — `docs/technical/SYSTEM.md`'s **Data** section documents `fiddle.toml` with both keys named, so the committed record and the code agree. The second half stays open and is an ADR, not a text edit: whether these keys belong to the deployment shape at all. See *Three backlog actions resolve to amending a gitignored file* below.

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
Status: Resolved 2026-08-09 — the finding is durable in `docs/technical/SYSTEM.md`'s M0 acceptance paragraph, which states the four-name list, why it is not extended per milestone, and that `LITELLM_API_KEY` is covered instead by `capability_selection.rs`'s sentinel assertion. Nothing further is owed to a gitignored spec. See *Three backlog actions resolve to amending a gitignored file* below.

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

### 2026-08-09 — Two entries above are closed: the evidence cross-reference now resolves, and the unenforced bound is visible at runtime
Closes, without editing, two entries dated the same day. This file is append-only, so their text stands as the record of what was found.

**2026-08-09 — A capability's attempt id is not the bundle's attempt id** is closed. That entry named two ways out and priced both: passing an id into `AttemptContext`, which gives up "minted once, here", or handing the id to the capability at `execute` time, which it described as changing the `Capability` trait. The second was taken, through the `ExecutionGrant` rather than through a new parameter — so `Capability::execute`'s signature is unchanged and only the type of an argument it already took is wider. `ExecutionGrant::authorise` now takes the derivation and the attempt it is issued under; `RunContext` carries the id `fiddle_runtime::attempt` already minted; `RepairConfig.attempt` is gone and `FixtureRepair` reads `grant.attempt_id()` for both the worktree name and the evidence suffix. Minting did not move: it is still once, in `attempt`, so the collision property is untouched. `fiddle_runtime::mint_attempt_id` is no longer re-exported at the crate root, so the front door offers no way to mint one at the edge. Recorded as `docs/technical/decisions/014-the-grant-carries-the-attempt.md`, and asserted from outside the process by `binary_repair.rs::the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under`, which reads both halves out of the bundle on disk.

**2026-08-09 — `agent.max_capability_attempts` has no consumer** is closed as a *visibility* matter, and remains open as a *behaviour* one. The retry loop is still not built and ADR 013 still prices it; nothing about what the key does has changed. What has changed is that a document writing `max_capability_attempts = 5` is no longer told its document is simply valid: `config check` reports the key as `{"configured": 5, "enforced": 1, "status": "accepted-not-enforced", "decision": "013-one-attempt-bound-not-two"}` in the `--json` payload and says the same in prose in the human one, while every bound that fires stays a plain scalar so the shape alone tells the two kinds apart. Design §6.6 promises that a deferred key is loud rather than silent under `deny_unknown_fields`; this key escaped that by being *known* rather than unknown, so strictness never looked at it, and this is where that route is closed instead. ADR 013's consequences section has been corrected — it asserted the edge was findable in "exactly two places" and "not surfaced at runtime", which was true when written. The `#[allow(dead_code)]` on the field is gone with it: the value is read in order to be reported, never to be applied.

**What this leaves open.**
- **`ENFORCED_CAPABILITY_ATTEMPTS` is a literal in `crates/fiddle-cli/src/render.rs`.** It says `1` because nothing loops, and nothing checks that claim against the runtime — it is a hand-maintained mirror of a property of `fiddle_runtime::attempt`. If a retry loop is ever built and this constant is not changed with it, `config check` will confidently report the wrong number, which is worse than the silence it replaced. The milestone that builds the loop must change the constant, drop the object for a plain scalar, and delete ADR 013's consequences section. There is no mechanical link between the two today and there is no cheap one: the honest version is the runtime exporting its own attempt bound, which is a seam nobody needs until there is more than one possible value.
- **`config check` reports one accepted-but-unenforced key, and there is no discipline that would catch a second.** The pattern is a good one — a bound that does not fire looks different from one that does — but it is applied by hand at the one place it is currently needed. A future key that parses, defaults, and fires nothing gets a plain scalar and the same silence, because nothing in the schema marks the distinction; only the renderer does. Making it structural means the *type* of a not-yet-enforced bound differing from an enforced one, which is a schema change and an ADR.
- **The human `config check` rendering is not asserted anywhere.** The `--json` payload is pinned field by field from outside the process; the prose beside it is only covered by the credential-leak scenario, which asserts what it must *not* contain. A rename that dropped a line from the human output would leave every gate command green.

Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-wvsf)
Tags: #debt #resolved

### 2026-08-09 — The `output_mode` line is inert on the typed path, and the request shape is right for a reason nobody had checked
`crates/fiddle-runtime/src/agent/mod.rs` sets `.output_mode(OutputMode::Tool)` on the agent builder and carries a long argued rationale for it: that Tool mode "registers the schema as a synthetic tool the model calls to finalise, and sends no native constraint, so the four real tools stay callable". Reading the serialized chat-completions bodies the compiled binary actually puts on a socket says otherwise, and the measurement is now a committed test.

**What goes out.** Turn 0 carries the four capability tools and no `response_format` at all. The finalising turn carries the same four tools *and* `response_format: {type: json_schema, json_schema: {name: "RepairReport", strict: true, …}}` — the native constraint. No synthetic `final_result` tool is advertised on any turn.

**Why.** `rig_agent`'s `TypedPromptRequest::from_agent` overwrites the agent's `output_mode` with `OutputMode::Native` unconditionally; its own comment says typed prompts deserialize the model's final string, and that the untyped `output_schema`/`output_mode` API is what to use for tool-composing structured output today. So `prompt_typed::<RepairReport>()` discards the builder's choice. Verified by deleting the line and re-reading the wire: byte-identical shape, same tools, same constraint placement.

**What this does and does not mean.** The shape that goes out is, by measurement, the working one — a first turn carrying tools and no constraint is exactly the request this gateway answers with a tool call, which is what the Tier 1 investigation was reaching for. The observation and the outcome were right; the diagnosis in the doc block was wrong, and it has been corrected in place. The line is left standing as the statement of intent for the day rig's typed path stops overriding it, and `binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact` pins the shape in both directions so that day is visible in the gate rather than in a Tier 1 run.

**What is left open.** Whether to move to the untyped `output_schema`/`output_mode` API, which is the only way to get the mode that was asked for. It changes what goes out on *every* turn — a synthetic finalising tool advertised throughout, no native constraint anywhere — and nothing in the gate can tell whether that is better against a real gateway, because the deterministic suite never serialises to anyone and `binary_repair` answers itself. Closing it means a Tier 1 measurement per mode across the models in ADR 012's table, then a decision, and it is worth nothing until somebody has that table.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — found writing the serialized-request test `m1-tool-protocol-correctness` asked for)
Tags: #debt #risk

### 2026-08-09 — The workspace command allowlist was stated four ways, and this is the one statement
`workspace::command` builds a child's environment from `env_clear` plus exactly four names: `HOME` at the workspace's scratch home, `LANG` fixed to `C`, `PATH` inherited from this process (or `/usr/bin:/bin` when it has none), and `RUSTUP_HOME` inherited **only when the parent has one**. `workspace::a_workspace_command_inherits_no_credential` asserts both shapes of that set exactly, so a fifth name cannot be added without changing an assertion.

Four documents said four different things and none of them said that: `docs/technical/SYSTEM.md`'s component paragraph said "a two-name allowlist", its own invariant named `PATH` and `RUSTUP_HOME` without mentioning that `HOME` and `LANG` are set at all, `docs/evaluator-calibration-general.md` said "an explicit `HOME`/`PATH`/`LANG` allowlist" and omitted `RUSTUP_HOME`, and `binary_repair.rs` said "an allowlist of two locators". Each was true of the fragment its author was arguing about — the *inherited* names, or the *constant* ones — and each read as a statement of the whole. The statement now lives once, in SYSTEM.md's Invariants, and every other mention points at it.

This corrects, without editing, the opening sentence of **2026-08-09 — A workspace check cannot find the macOS SDK, because the allowlist has no locator for it** above. That entry's finding is unaffected: its argument is about which *locators* may be inherited, `PATH` and `RUSTUP_HOME` are still the only two, and whether `DEVELOPER_DIR`/`SDKROOT` join them is still the open question. Only its count of the whole environment was short by two.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-i7f3, coherence)
Tags: #debt #process

### 2026-08-09 — Holistic review iteration 4: accepted with findings recorded
Epic `fiddle-y1w6` (M1) was accepted without the holistic review formally converging. Four
iterations ran; each found genuinely different defects rather than re-litigating earlier ones, and
severity fell sharply round on round: a tool loop that called no tools → a capability that ran the
wrong thing and reported success → forgeable changed-file evidence → a credential in the published
bundle → the items below, none of which produces a wrong verdict or leaks a secret.

Iteration 4 scored integration 6/7, coherence 6/7, holistic_spec_fidelity **8/8 (passed, up from 7)**,
polish 6/6, runtime_health 9/9. The decision to stop was the user's, taken on the severity curve
rather than on the scores. Everything the reviewer found is below; nothing was dropped.

Origin: holistic review iteration 4 (epic fiddle-y1w6)
Tags: #debt #m2-input

**Worth fixing first — cheap, and each is a real inconsistency:**

1. **`Workspace::write` and `Workspace::read` disagree about what the project is.** `read` gates on
   `list()` and refuses anything outside it as `NotProject`; `write` consults neither `list()` nor the
   baseline ignore rules. So `write_file("target/x")` succeeds and is invisible to `changed_files()`,
   which makes `max_changed_files` evadable — the bound whose stated purpose is catching an attempt
   that "did something nobody asked for". Nothing is *earned*, because the check still decides the
   verdict, and the sibling channel (an attempt rewriting `.gitignore`) is closed and tested. The fix
   is giving `write` the same `list()` test `read` already runs. No test covers this today.
2. **`run_check`'s leak test uses the narrow root set.** `agent/tools.rs` defines `layout()` as
   "everything about where this attempt is running that the model must never be told" — workspace,
   fixture repository, containing directory, attempt id — and applies it to `read_file`'s test, while
   `run_check`'s test uses the narrower `roots()`. `relativised` strips only the workspace root, and
   the fixture repository is a *sibling*, so a check shelling out to git (or a `build.rs` reading VCS
   info) can print the fixture path to the model. SYSTEM.md's invariant states the rule absolutely.
3. **A git failure masks the milestone's central error.** `capability/repair.rs` derives `changed`
   with `?` before the exit-code gate, so a failure asking git what changed turns what should be
   `CheckFailed` into `CapabilityError::Workspace`. Moving the line below the gate costs nothing.
4. **ADR 012 predates the work that refuted it.** It still states `OutputMode::Tool` as the operative
   mechanism; the wire shows rig overwrites it with `Native` and the line is inert (the code doc,
   BACKLOG and the calibration were corrected in iteration 3; the ADR was not). Its budget consequence
   also rests on `tier2.sh`'s 300-character reason excerpt letting a human spot a spend cap — after
   the `provider_fault` fix that excerpt reads only `the gateway answered <status>`. And commit
   `e993f4a` changed `DEFAULT_MODEL` in the same commit that added the inert line, so what actually
   made the tool loop work is unattributed. ADR 012 is the document SYSTEM.md routes a cold reader to.

**Recorded, lower value:**

5. The changed-file cap is applied to a pre-check tree while the published `repair:<n>` counts a
   post-check one. M1's fixture ignores `target/` and `Cargo.lock`, which hides the divergence.
6. `fiddle_core::Published` gates `RunOutcome`'s three reasons and `ProgressEntry::summary`, but
   `NextAction::Blocked.reason` and `Observation::Unavailable/NotApplicable.reason` are bare `String`s
   that are also published. `orchestration.rs` bounds one copy of the same string and publishes the
   other unbounded in one expression. `published.rs`'s module doc asserts an exhaustive enumeration
   that is wrong.
7. `agent/mod.rs` states "a provider's response body is never quoted, on any path", but `classify`'s
   `DeserializationError` arm renders serde's diagnostic over the gateway's *success* content, which
   quotes the offending value verbatim. The doc justifies this as "model-authored", which conflates
   the model with the gateway ADR 012 exists to say sits between them. Either the rule or the code
   should move; the choice needs writing down.
8. `agent/mod.rs` still carries `// **The line that makes the tool loop happen at all.**` forty lines
   below the section saying the line is inert.
9. `SYSTEM.md` states the `fiddle-core` purity denylist as five names; `crate_boundary.rs` has six,
   and the omitted one is `rig-agent` — the name M1 added.
10. SYSTEM.md's Invariants state neither the publication bound (`PUBLISHED_TEXT_LIMIT`) nor the
    never-quote-a-provider-body rule, though both came out of the epic's one critical-severity bean.

**A note on the review process itself, worth carrying to M2.** The holistic instrument scores against
thresholds calibrated for a finished product, so each round's remaining findings are necessarily
smaller while the bar stays fixed. Four rounds of "one point under threshold" can continue
indefinitely on a system that is fundamentally sound. Consider whether a later milestone should score
severity explicitly, or converge on "no finding that changes a verdict or leaks a secret" rather than
on a fixed dimension score.

### 2026-08-09 — Items 4 and 8 discharged: ADR 012 no longer states a refuted mechanism

ADR 012's `OutputMode` consequence and its budget consequence both described a system that had since
changed underneath them, and `SYSTEM.md` routes a cold reader straight to that ADR. Corrected in
place rather than superseded, because the decision it records — the gateway itself — was never in
question; only its stated reasons were.

Three corrections. The `OutputMode` consequence keeps the gateway measurement (which stands) and
retracts the mechanism: `TypedPromptRequest::from_agent` overwrites the mode with `Native`
unconditionally, so the builder line is inert, the "Tool mode is best-effort where Native was
guaranteed" cost is not being paid, and
`binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact` pins what actually
goes out. The attribution for the tool loop starting to work now names both changes that landed in
`e993f4a` — the inert line and `DEFAULT_MODEL` haiku → kimi — and says plainly that the cause was
never isolated, rather than presenting the model table as a later finding. The budget consequence now
states what `tier2.sh` records after `4b2333b`: `the gateway answered <status>` and nothing else, so
the claim that a human could distinguish a spend cap from the reason text is withdrawn.

Item 8 went with it: the inline comment at the `.output_mode` call now says the line is inert instead
of calling it the line that makes the tool loop happen.

**What this does not close.** The underlying gap in each case is still open and still recorded above:
no isolated per-mode measurement across the model table, and no typed signal for a spend-cap refusal.
Only the documents lying about them are fixed.

### 2026-08-09 — The develop loop re-derives the same orientation once per bean
Measured across M2's nine completed beans: orientation averages 5.7 min of a 23.4 min bean and barely varies with bean size — one bean spent 8.0 min orienting to do 2.5 min of work. Every fresh implementer reads the same prior-task sources, the same epic `## Contracts`, and the same accumulated antipattern history, and re-derives the same understanding of them.

The fresh context is deliberate and should not be traded away: `skills/develop-loop` dispatches a new implementer per bean precisely so that a previous bean's rationalisations do not carry forward, and the milestone's best catches — a NUL identity collision, a criterion naming a deleted API path, three separately self-caught vacuous tests — came from implementers reasoning from sources rather than from a summary. So "reuse the agent" is the wrong fix.

What is worth trying instead, in rough order of expected value:
- Have the *lead* distil each completed bean into the findings the next implementer needs, and hand those forward in the prompt, rather than pointing at source files and letting each one re-derive them. M2 did this ad hoc for antipatterns and it visibly worked — Task 6 caught the same vacuous-test hazard Task 5 had found, one bean later, because it was told about it.
- Put the durable half in the epic's `## Contracts` section, which already exists for shared type names and is already read by every bean. It currently carries types and constraints but not findings.
- Measure whether orientation shrinks when a bean's prompt names *what changed since the last bean* rather than *what exists*.

Two process wastes found alongside it, both fixed the day this was written: named regression lanes were being re-run individually after a clean full-workspace run had already printed the same counts, and each verification issued ~18 separate `nix develop -c` entries. `scripts/gate.sh` now does the whole gate in one entry and prints the per-binary counts to parse.

Origin: performance investigation during M2 implementation (epic fiddle-srrw), from nine implementer transcripts
Tags: #debt #optimization #orchestrate

### 2026-08-10 — Two identity derivations in one pure module use different framings
`crates/fiddle-core/src/effect.rs`'s `effect_id` hashes a **length-prefixed** encoding of its four inputs, so the encoding is injective for every input: a field's contents can never be mistaken for structure. `crates/fiddle-core/src/assessment.rs`'s `correlation_key` still joins with a NUL separator, whose non-collision argument rests on a domain convention rather than on the encoding — NUL is valid UTF-8, and neither `project` nor `invocation_ref` reaches that function through a type that forbids one.

The divergence is deliberate and is documented at `effect_id`. `correlation_key`'s value is written into fixture state on disk, compared by later runs, pinned by test to a published digest, and depended on by M0's acceptance lane, so re-basing it would break the very cross-process recognition it exists to provide — for an exposure M0 does not have, since a marker only ever has to be recomputed by fiddle from the same two values.

What is not written down anywhere is **when re-basing would be acceptable**, which is the half a reader needs. It is acceptable at a milestone that is already invalidating on-disk markers for another reason — a change to the bundle layout, the report schema's shape, or the marker file's own location — because the cost is a one-time recognition break and the only way to pay it once is to pay it alongside something else. It is not acceptable on its own, and a milestone that re-bases it in isolation has spent M0's stability proof to remove an exposure nobody has demonstrated. Until then, the rule for anything *new* is `effect_id`'s: length-prefix, do not separate.
Origin: implementation (epic fiddle-srrw, Task 0 — the evaluator proved an embedded NUL could give two distinct effects one identity)
Tags: #debt #risk

### 2026-08-10 — The publishing adapter runs a workspace-style command the PRD's ownership table assigns elsewhere
M2's design considered publishing a change blob by blob through the Git Data API — create blobs, create a tree, create a commit, then `POST /git/refs` — which would have kept every mutation inside the `gh` adapter that `docs/technical/decisions/015-gh-cli-as-the-github-adapter.md` describes. It was rejected in favour of one `git push`, for a reason that is not a preference: a ref can only be created pointing at an object the remote already holds, so the blob-by-blob route is four ordered mutations where the push is one, and each of the four is separately capable of being lost — turning a single ambiguous write into four, in the milestone whose whole subject is ambiguous writes. `git push` to a named ref is also already idempotent, which is what let the design drop a bespoke branch identity scheme entirely.

The price is that `crates/fiddle-runtime/src/git/publish.rs` spawns a subprocess against the *workspace* — a program invocation over a checkout — from inside the forge adapter, which the PRD's ownership table places on the workspace's side of the boundary. Nothing is wrong today: it is its own module, its own credential channel and its own environment, all three stated as invariants in `docs/technical/SYSTEM.md`, and `crates/fiddle-runtime/src/git/mod.rs` argues the separation at length. What is owed is a decision about where the boundary actually runs, taken by whichever milestone next adds a mutation that is neither an API call nor a push — because the current arrangement is defensible as an exception and not as a rule, and a second exception makes it neither.
Origin: implementation (epic fiddle-srrw, Task 14 recording M2's design reduction §5.5)
Tags: #debt

### 2026-08-10 — The dispatch is the one effect GitHub protects nothing about, and its locator is checked by nothing that compiles
`POST /repos/{owner}/{repo}/actions/workflows/{id}/dispatches` answers **204 No Content** — no body, no run id, no `Location` — and `GET .../actions/runs/{id}` does not carry the inputs a dispatch was made with (`has("inputs")` answers `false`; no key matches `/input|dispatch/i`). Both verified against real GitHub rather than assumed. So the obvious identity mechanism, filtering the runs listing on a dispatch input, does not exist, and a retried dispatch simply starts a second run: unlike the branch (`git push` to a named ref) and the pull request (GitHub refuses a second one for the same head and base), this effect has no server-side duplicate protection at all.

The identity therefore goes **out** as the `fiddle_effect_id` input and comes **back** through the target workflow's own `run-name`, which the listing does return as `name`. That makes `crates/fiddle-runtime/src/github/checks.rs::run_name` and `.github/workflows/fiddle-check.yml` in `peel/fiddle-effects-acceptance` two halves of one contract that **no compiler and no gating test checks**. Rename the input, drop the prefix, or let the workflow interpolate something else into its title, and nothing fails loudly — the locator stops finding runs that exist, `inspect` reports an absence that is not real, and the dispatch happens again.

Two things make this tolerable rather than urgent, and both should be read before anyone spends effort on it. No cheaper locator exists, because the runs listing is the only surface that returns anything a dispatch can be recognised by. And `scripts/live-github.sh` does check the round trip on every run, so the exposure is the window between an edit and the next live run rather than an unbounded one. The workflow's `concurrency: { group: fiddle-<id>, cancel-in-progress: false }` bounds the overlap a mistake can cause; it is a mitigation and not evidence, because a concurrency group says two runs will not execute at once, never that only one was requested.
Origin: implementation (epic fiddle-srrw, Task 12 — the round trip nothing compiles together)
Tags: #debt #risk

### 2026-08-10 — fiddle observes and requests checks but can never author one
Only GitHub Apps may create check runs, and M2's credential is a fine-grained personal access token. So `crates/fiddle-runtime/src/github/checks.rs` does two things and not a third: it *observes* checks by exact head sha, and where a workflow has to be started it *dispatches* it. There is no path by which fiddle publishes a check result of its own — no "fiddle verified this change" appearing beside CI's own checks on a pull request, which is the surface a reviewer actually reads.

This is a capability ceiling rather than an omission, and closing it is not a code change: it means App authentication — signing a JWT with a private key and exchanging it for an installation token — which `gh` does not do, and which would put a private key outside the single credential-carrying construction ADR 015 exists to preserve. It is named there as the most likely trigger for reversing that decision. Until then, what a reader of a fiddle-published pull request sees about verification is whatever the dispatched workflow itself reports, and `required_checks` is fiddle's own private opinion about which of those matter.
Origin: implementation (epic fiddle-srrw, Task 14 recording M2's boundary)
Tags: #debt #risk

### 2026-08-10 — The human-decision variant is defined, consumed and unreachable from any capability
Extends **2026-08-09 — `RunOutcome::Suspended` is the one exit-code row never exercised end to end** above; that entry's finding stands unchanged and this adds the M2 half rather than restating it.

`fiddle_core::PolicyDecision::RequireHumanDecision` now exists, is produced by `combine`, and is consumed at `crates/fiddle-runtime/src/effect/mod.rs`'s step 4, where it fails closed as `EffectError::HumanDecisionRequired` naming what would satisfy it. So it does not ship inert. But all three of M2's operations declare `HumanDecisionRequirement::Automatic`, which means the only way to reach the variant is a deployment document writing `require_human`, and the only thing that then happens is the run stopping. The cell the whole `combine` module was written for — a capability whose own minimum is `Human` — is asserted in `policy.rs`'s unit test and reached by nothing that runs.

Both halves close in the same milestone and for the same reason: `Suspended` is the outcome an attended decision produces, and `RequireHumanDecision` is the thing that would produce it. M3 introduces the decision channel; whoever builds it should expect to be the first person to observe either.
Origin: implementation (epic fiddle-srrw, Task 14)
Tags: #debt #test

### 2026-08-10 — Two things in the effect vocabulary a later consumer must not read as more than they are
Both are deliberate, both are defensible, and neither is obvious from the type.

**`EffectReceipt.outcome` is only ever `Committed`.** `crates/fiddle-runtime/src/effect/receipt.rs` declares three values and `crates/fiddle-runtime/src/effect/mod.rs` builds a receipt at exactly two sites, both with `EffectOutcome::Committed`. `NotCommitted` and `Unknown` drive the executor's step-8 branch but never land in a receipt, because a non-committed effect returns an `EffectError` instead. That is coherent — a receipt records an observed postcondition, and there is no postcondition to record for an effect that did not happen — but it makes the field near-constant on the success path. A later consumer that read it as a discriminator would be branching on a value that has one inhabitant. If that ever needs fixing, the honest shape is the outcome leaving the receipt rather than the receipt gaining the other two values.

**`GitError::Push` is classified `Unknown`, and its commonest cause never reached the remote.** An unreachable remote, a credential the far end would not take, a connection that dropped — none of those moved the ref. `Push` is `Unknown` because git expressed no per-ref verdict: `git push --porcelain`'s `!` line is its refusal channel and its *absence* is not a refusal. The consequence is that a transport-failed push whose ref is genuinely absent reports `EffectError::Unresolved` rather than `EffectError::Adapter` — cautious rather than misleading, and it costs one `GET` to settle, but it is a real behavioural choice reversible in one match arm. Anyone reversing it should note it makes the classification depend on git's *stderr wording*, which is the surface `--porcelain` was chosen to avoid.

**Correction, 2026-08-10 (remediation R1, bean fiddle-h055).** The sentence above used to add that "the analogous `GhError::Malformed` is `NotCommitted` for the parallel reason". That was the same defect one variant over, and it is now reversed: `Malformed` is `Unknown`. The rationale it recorded covered only one of that variant's producers — a process that ran to completion and produced garbage — while the spawn/wait failure and the missing status line were lost answers wearing a refusal's classification. What is `NotCommitted` there now is `GhError::NotSent`, whose only producer is a call this runtime refused to make. `Malformed` keeps the half of the old reading that was true of the runner rather than of the world: it is `Unknown` and still **not** worth reading again, because a program that is not `gh` will not become one.
Origin: implementation (epic fiddle-srrw, Tasks 3 and 5 — judgment calls recorded only in bean summaries until now); corrected by remediation R1
Tags: #debt

### 2026-08-10 — Step 8's settling read does not happen on a cancelled run
`EffectOutcome::Unknown` now reaches a cancellation that arrived with the child already running (remediation R1, bean fiddle-h055), so a `^C` during `POST .../pulls` is reported `EffectError::Unresolved` instead of as a settled failure — which is what stops the retry that duplicates. What it does *not* do is settle the ambiguity within that run, and the reason is worth writing down rather than rediscovering.

Step 8 does call `read_until_settled`. Its single `inspect` is then refused *before spawning*, because the token it is handed is the cancelled one (`GhCli::api`'s own pre-spawn check), so the read that would settle the question never reaches GitHub. Two separate things could be changed and only one of them should be:

- **`read_until_settled` returning immediately on cancel is right** and should stay. A cancelled run must not sit in a backoff loop, and this is what makes a `^C` prompt.
- **One settling read escaping the cancellation is the arguable improvement.** It would need `EffectContext` to carry a second cancellation channel — reads and mutations answer to different tokens — plumbed through all three `inspect` implementations, plus a bound so a `^C` cannot hang on a read. It is not a retry (one read, no loop, no re-dispatch), so it does not touch the milestone's rule that the read retries and the mutation never does.

R1 deliberately did not take it: the classification was the finding, the second token is a design change nobody has priced, and the cost of leaving it is one fresh process rather than a duplicate. Anyone taking it should note that the fresh process's own step-3 read is still subject to GitHub's listing lag, which is the residual risk on the check request and the reason this is debt rather than a closed question.
Origin: implementation (remediation R1, epic fiddle-srrw, bean fiddle-h055)
Tags: #debt

### 2026-08-10 — On a 2xx, the rate-limit headers are parsed and dropped
`crates/fiddle-runtime/src/github/cli.rs` reads `Retry-After` and `X-RateLimit-Remaining` off every `gh api -i` response and puts both on `GhResponse`. On the failure exit they are copied into `GhError::Http`'s `RetryAdvice` and reach `ReadRetry::delay`, which is the fix that stopped them being parsed for nothing. On the **success** exit they reach nobody: `GhResponse.retry_after` and `GhResponse.rate_limit_remaining` are read on no path a run takes. The only reader either has is `github_cli.rs`, which asserts they were parsed — a test that would keep passing if the fields were deleted from every consumer, because there is no consumer.

So the client can be told `X-RateLimit-Remaining: 3` on a 200 and does nothing with it — it discovers the limit by being refused, and only then starts pacing. Over M2's volume (one capability per run, three effects, two reads each) this is invisible. It stops being invisible at the first deployment that publishes concurrently against one repository, which is exactly the herd `ReadRetry`'s jitter was written to decorrelate.

Closing it is not "read the field": pacing on a successful response is a policy decision about whether a run should *slow down* before it is refused, and that interacts with `[github] timeout` — a run that paces itself into its own deadline has traded a 403 for a `Timeout`, which is classified `Unknown` and is strictly worse. Note also that `rate_limit_remaining` is consulted on the error path only as a *boolean* (`RetryAdvice::wants_a_wait`, distinguishing a secondary-rate-limit 403 from a permissions one), never as a number to budget against, so there is no existing pacing arithmetic to extend.
Origin: implementation (epic fiddle-srrw, Task 14 — found reading the adapter against the committed record)
Tags: #debt

### 2026-08-10 — Additive keys are not a shape change: the schema constants stayed at v0, and this is the rule
`crates/fiddle-core/src/report.rs` carries two doc comments that pull in opposite directions, and M2 had to pick one. `REPORT_SCHEMA`'s says that a bundle whose shape changes must change the string in the same edit; `RUN_SCHEMA`'s anticipates that M1 onward adds fields to these payloads. M2 added `review` and `verification` to `WorkStateView` and left `fiddle.report.v0`, `fiddle.run.v0` and `fiddle.inspect.v0` alone.

The reading taken is that **an added key is not a shape change**. The argument is not aesthetic: bumping would break every acceptance lane asserting `fiddle.report.v0`, M0's included, and M0's lane is a hard constraint of every milestone since. A consumer that dispatches on the schema string and ignores keys it does not know is unaffected by an addition and is broken by a bump.

What would change the answer, stated so nobody re-litigates it: **a removed key, a renamed key, or a changed type is a shape change and does require the bump** — because each of those breaks a consumer that was reading correctly. The next person to touch these constants should apply that sentence rather than re-reading the two doc comments, which remain in tension by themselves.
Origin: implementation (epic fiddle-srrw, Task 8 — the evaluator was asked to rule and raised no objection)
Tags: #debt #process

### 2026-08-10 — The M2 effects credential can write to the repository M0's proof depends on being credential-free
`docs/technical/effects-repository.md` records the probes, and the row worth acting on is the second: the fine-grained token that performs M2's effects has **two** repositories in its selection, not one. `peel/fiddle-effects-acceptance` is the intended target. `peel/fiddle-acceptance` is also selected — 200 on its `collaborators` endpoint — and that is the external M0 acceptance repository `docs/technical/acceptance-repository.md` describes.

That document's whole argument is that the repository is public **so that reading it needs no credential**, that it holds no secrets and never will as a standing rule, and that M0's lane is therefore never gated on one. None of that is falsified — the repository still holds no secret, `.github/workflows/acceptance-repo.yml` still checks it out with no `token:` and no `ssh-key:`, and nothing in M2 writes to it. What has changed is that a credential now exists which *could* write to it, and it is held as a repository secret (`FIDDLE_EFFECTS_TOKEN`) in a repository the same token is deliberately excluded from. A mistake in `scripts/live-github.sh`'s `FIDDLE_EFFECTS_REPO` default, or a `gh` invocation with the wrong `--repo`, reaches M0's acceptance repository with write authority.

The fix is narrowing the token's repository selection to `peel/fiddle-effects-acceptance` alone and re-running the probe table in `effects-repository.md`, which should then read 403 for both other rows. That is a settings change and a documentation edit, and it is worth doing before M3 adds more effects rather than after. Recorded rather than done here because rotating the credential invalidates a repository secret this milestone's lane depends on, and the two have to move together.

**Closed 2026-08-10, both halves.** The operator narrowed the selection: `repos/peel/fiddle-acceptance/collaborators` now answers **403**, and a ref-create against it answers `403 Resource not accessible by personal access token`, so the credential is structurally incapable of the write rather than merely not pointed at it. The probe table in `effects-repository.md` is re-run and records 403 for both other rows, `acceptance-repository.md` discloses the episode from the other side, and `.env.example`, `docs/evaluator-calibration-general.md` and `.github/workflows/github-effects.yml` no longer assert a scope the table refutes.

The second half mattered as much and this entry underrated it: **narrowing the credential alone would have left the lane one rotation away from the same exposure.** The `FIDDLE_EFFECTS_REPO` hazard named above was not a hazard of the *default* value — it was that no value was ever checked, and `scripts/live-github.sh` armed its `trap cleanup EXIT` ref-DELETE-and-close sweep *before* the only thing that incidentally noticed a wrong repository. The lane now refuses an inadmissible target before that trap is set and before any mutation, on a positive six-part predicate — see *The target guard* in `effects-repository.md`. Verified by running it: a wrong `FIDDLE_EFFECTS_REPO` refuses with no `cleaning up` line at all, where the pre-change script printed one and issued the whole sweep.
Origin: implementation (epic fiddle-srrw, Task 14 — reading the probe table against acceptance-repository.md's standing rules); closed by remediation bean fiddle-xbnz
Tags: #risk #security #debt #resolved

### 2026-08-10 — M2's mandatory proof is carried by one test, and an inversion is what established that
`crates/fiddle-acceptance/tests/exactly_once.rs` holds five tests and gates. Task 15 was required to invert its own rule — let the *mutation* retry rather than only the read — and confirm the lane fails. It does, and the shape of the failure is the finding: **4 passed, 1 failed.**

The one that failed is `an_ambiguous_write_then_a_fresh_process_leaves_exactly_one_of_each`, at `assert_landed_under(world, "pulls", "commit_then_die")` — **left: 5, right: 1**: five identical `POST_repos_peel_r_pulls` records, one per allowed attempt, exactly the duplicate external effect the milestone exists to prevent. The other four passed under the inversion and are blind to it: `the_retry_carries_a_distinct_attempt_id_and_the_same_work_ref`, `the_github_token_appears_in_no_bundle_no_stdout_and_no_diagnostic`, `an_unreachable_github_publishes_nothing_and_reports_an_unread_forge`, and `the_effect_steps_of_a_real_run_reach_the_attempt_journal`. Each is sound and each is about something else.

This is not a defect — the property is genuinely held and the test that holds it is correct. It is a fragility: weaken, skip or delete that one test and the lane still reports five passed while the milestone's central claim is gone, and no count anywhere would move. It is also now recorded in `docs/technical/SYSTEM.md`'s Known issues, so a reader meets it without reading this file.

**The rule worth carrying, since M3 through M8 all add effects.** An inversion test is the only thing that distinguishes *a lane that proves a property* from *a lane that contains a test about it*, and it is cheap: break the property deliberately, run the lane, and read which tests notice. Two neighbouring practices came out of the same verification and belong beside it. A frozen lane count is not evidence on its own — `check_effect` reporting 14 proves nothing if one of the 14 was quietly weakened in the same commit, so an edit to a pre-existing test file is diffed by content and the diff is stated. And a diff tool's empty answer is a claim rather than a result: `git diff` returned empty under a hook in one implementer's context this milestone and took three attempts to notice, so an empty diff is cross-checked against a second method (`git show <base>:<path>` compared directly) before it is believed.
Origin: implementation (epic fiddle-srrw, Task 15's inversion and Task 11's verification standard)
Tags: #debt #test #process

### 2026-08-10 — A branch exists only because a dispatch-only lane cannot run from anywhere else
`.github/workflows/github-effects.yml` cannot be dispatched until it is on `main`, for the reason now stated as an invariant in `docs/technical/SYSTEM.md` and at length in the file's own header. The residue is operational: branch `ci/github-effects-dispatch-proof` on `peel/fiddle`, at `75d655c5`, is currently the only ref the lane could be dispatched from, and it was created for that purpose alone.

It becomes redundant the moment the workflow file lands on `main`. Whoever merges M2 should delete it and dispatch the lane once with any `fiddle_effect_id`, which closes both this and the inertness itself — no code is owed for either.
Origin: implementation (epic fiddle-srrw, Task 13)
Tags: #debt #infrastructure
Status: Half resolved 2026-08-10 — the inertness is gone and the branch is not. The file landed on `main` at `aa86c60`, the workflow entity is live (id `330906808`, active), and run **31374193249** dispatched it with no default-branch flip. The action this entry proposes is the half that does not hold: **do not delete the branch yet.** `actions/checkout@v4` in that workflow is bare, so the dispatched `--ref` decides which code is built, and a dispatch also needs the workflow file to exist *at that ref*. `main` carries the file but no Cargo workspace; `plan/agentic-factory-m0` and `plan/agentic-factory-m1` carry the workspace but not the file; `plan/agentic-factory-m2` is not pushed. `ci/github-effects-dispatch-proof` is therefore still the only ref on `peel/fiddle` where a dispatch both resolves and can succeed — load-bearing rather than residue. It becomes deletable when the milestone stack merges to `main`, and that merge is the operation that should delete it. See the entry below.

### 2026-08-10 — The widened-payload check is intra-call; the cross-process half needs a durable record nobody has priced
`crates/fiddle-core/src/effect.rs` argues that identity and payload are hashed separately so the executor can tell "this is the same effect, already performed" from "this is the same effect, but the request has been widened since it was approved". Remediation R4 implemented the half this milestone can actually observe: the envelope is minted at step 6 for the payload the *proposal* carried, and `Executor::execute` refuses with `EffectError::PayloadDiverged` before step 7 when the operation it was handed would apply a different one. Approval is minted and spent one step apart, and that gap is now checked rather than assumed — `payload_divergence.rs` pins it, and removing the comparison makes the mutation land.

What is still not implemented is the **cross-process** reading, which is the one the phrase "since it was approved" most naturally suggests: a second attempt asking what payload the *first* was approved for. Nothing persists a prior payload hash. Not the attempt journal, which records `effect_step` lines carrying kind and step and no digest. Not the bundle, whose evidence is `receipt_evidence`'s rendered string — kind, effect id, outcome, external ref, postcondition, and no hash. Not the forge, which receives the identity in a branch name and a workflow run title and never the payload at all. So a fresh process has nothing to compare against, and no amount of reading the world produces one: `EnsurePullRequest`'s list read carries a title but no body, so even the observed object cannot reconstruct the canonical payload it was created for.

Three things would have to be decided, and R4 declined to guess at any of them:

- **Where the prior hash lives.** The attempt journal is the obvious candidate and R1 has just taught it to record effect steps. But a durable record's *absence* then has to mean something — an effect performed by a build before the record existed, or by a run whose journal was lost, must not read as "the payload changed".
- **What happens when it has widened.** Refuse, report, or re-propose. The design states the failure ("would arrive looking like new work") and not the response, and the three are materially different: refusing strands a published branch, reporting needs a surface, and re-proposing is a second mutation on a path that already has one.
- **What the record costs.** It is approval state that outlives a process, which is a different kind of object from `AuthorizedEffect` — whose doc comment is explicit that it is a runtime token and never written down. M3's decision channel is where durable approval arrives, and pairing the two is likely cheaper than building this alone.

Note what the tree already does instead, because it changes how urgent this is: each operation decides for itself, in typed terms, what makes an observed object the postcondition. `EnsureBranchPublished::inspect` compares the intended sha and returns `Ok(None)` when the remote points elsewhere; `EnsureCheckRequested` filters by a run name derived from the identity. The payload's discriminating field is therefore already checked where it discriminates — the pull request's title and body being the deliberate exception, since matching on those is what opens a second pull request.
Origin: implementation (remediation R4, epic fiddle-srrw, bean fiddle-mp53)
Tags: #debt

### 2026-08-10 — Two derives the `## Contracts` block pins are provably inert
Both were found by remediation R4 while correcting doc comments that justified machinery nobody built. Neither is removed here, because the epic's `## Contracts` section pins the derive list of both types and a bean that reduces a pinned contract is changing a contract; both doc comments now say what is true of the tree instead.

- **`EffectReceipt`'s `Serialize`.** Nothing serializes a receipt, and no receipt a *run* produces can be: none of the three `T`s production instantiates it with — `PublishedBranch`, `PullRequest`, `WorkflowRun` — is itself `Serialize`, so the derive's `where T: Serialize` bound is unsatisfiable for all three. (Two test-only observations use `String` and `()`, which would satisfy it; neither is serialized either.) A receipt reaches a bundle as `receipt_evidence`'s rendered `EvidenceRef`, which `capability/publish.rs` argues for at length. The two doc comments used to disagree about this in one epic.
- **`EffectId`'s `Hash`.** There is no `HashMap`, `HashSet` or `BTreeMap` keyed on an `EffectId` anywhere. The executor recognises an effect by reading the world for that one effect, one operation at a time, and never by indexing a set of proposals — which the old comment claimed it did.

Removing either is a two-line edit plus a line in the Contracts block of whatever plan supersedes M2's. Worth doing at the same time as the `PayloadHash` question above, since all three came from the same reading.
Origin: implementation (remediation R4, epic fiddle-srrw, bean fiddle-mp53)
Tags: #debt

### 2026-08-10 — `RunOutcome` still carries no taxonomy, and M2 widened the set twice
Names, and does not supersede, **2026-08-09 — the outer attempt bound has no consumer** and the entry above it that records ADR 013's pricing. ADR 013 said from M1 that `RunOutcome::Retryable` has several producers of which only one is "the capability tried and lost", so a retry loop "needs a taxonomy the outcome type does not carry". M2 then added three more producers — `EffectError::{PolicyDenied, HumanDecisionRequired, DuplicateState}` — plus a fourth from remediation R4, `PayloadDiverged`, and recorded nothing about having widened the gap. That omission is the finding; this entry closes the *recording* half of it and not the gap.

Remediation R3 has since moved those four to `RunOutcome::Failed` and exit 20, per `docs/technical/decisions/016-a-permanent-refusal-is-not-retryable.md`, which makes the practical harm go away — automation retrying on 11 no longer loops on a denied effect — and makes the taxonomy problem *bigger*, not smaller. Exit 11 now has six distinct capability failures behind it beside its three other producers; exit 20 has four beside `assess → Blocked`'s three arms. Ten conditions across two integers, told apart only by prose in a `reason` field that a machine cannot key on. `CapabilityError::recurrence` is a two-valued answer to a question that has more than two answers, and it is deliberately two-valued because the exit table has two rows for a run that executed and did not complete.

What a real taxonomy would have to decide, and what nobody has:

- **Where it lives.** A `RunOutcome::Failed { error, class }` widens the `--json` payload every bundle consumer reads. A separate field beside `outcome` does not, and is then a second thing that can disagree with the first. `Published` bounds the text of the reason but says nothing about its shape.
- **Whether the exit codes follow it.** Adding rows is the honest move and the expensive one — `exit_code_for` is realised once by design, and every acceptance lane asserting a number is a consumer. Not adding them means the class is machine-readable only through `--json`, which is a different contract from the exit code and one an operator scripting `fiddle run` in a shell does not have.
- **What M3 takes with it.** `HumanDecisionRequired` moves from `Failed` to `Suspended` the moment a decision channel exists, and `required_checks` (below) wants the same *wait* mechanism. Two of the ten conditions leave the table at that point, which is an argument for pricing the taxonomy with M3's channel rather than before it.
Origin: implementation (remediation R3, epic fiddle-srrw, bean fiddle-m3ql)
Tags: #debt

### 2026-08-10 — `github.required_checks` is disclosed as unenforced; enforcing it is still owed
`[github] required_checks` is read, acted on, and decides nothing. The names reach `Executor::observe_checks`, which looks each one up against the published head and splits the answer into `VerificationState`'s `required_missing`, `failed` and `pending`; that value reaches the bundle as `observations.verification`. Then `fiddle_core::assess` matches on `work_item` and `changes` and on nothing else, so a required check that is missing, that failed, or that is still running leaves the outcome exactly where an all-green one does. A deployment naming `required_checks = ["build"]` requires nothing of CI.

Remediation R3 took the disclosure side, per `docs/technical/decisions/017-required-checks-are-observed-not-enforced.md`: `config check` now reports the key the way it reports `agent.max_capability_attempts` — an object carrying `configured`, `enforced` (empty, whatever the document says), a `status`, and the decision — under the word `observed-not-enforced` rather than `accepted-not-enforced`, because the two are different and the older word promises less reading than actually happens.

Enforcement is what is still owed, and it is three decisions rather than one, which is why R3 declined to guess:

- **A `failed` required check is a conclusion.** `Blocked ⇒ Failed` fits it, and it is the only one of the three that does.
- **A `pending` one resolves without anybody doing anything.** Neither `Failed` nor `Retryable` is honest about it; *wait* is, and *wait* is `Suspended`, which is M3's row. This is the same mechanism as waiting for a human, and pairing the two is almost certainly cheaper than building either alone.
- **A `required_missing` one may only mean CI has not started.** Distinguishing "never going to run" from "has not run yet" needs a bound — a deadline, a poll budget — that nothing in `[github]` currently supplies.

All three land in `fiddle_core::assess`, which is the pure core's decision function and whose `Blocked ⇒ Failed` rule M0's frozen acceptance lane depends on. Adding an arm there gives `RunOutcome` more producers, which is the entry directly above.
Origin: implementation (remediation R3, epic fiddle-srrw, bean fiddle-m3ql)
Tags: #debt

### 2026-08-10 — The preflight that makes `--ref main` legible is not on `main`
`.github/workflows/github-effects.yml` now refuses a ref carrying no Cargo workspace at a preflight step, before the toolchain install and the build, naming the reason and the milestone branch to pass instead. Proven by dispatching it against a throwaway ref built from `origin/main` plus that one file: run **31383731994**, `conclusion=failure`, failed at step 4 with the toolchain, the build and the walk all skipped — and by run **31383743533**, `conclusion=success`, the same workflow against `ci/github-effects-dispatch-proof` at `d52fc84`, walk confirmed to have run.

The gap is which copy a dispatch uses. `workflow_dispatch` resolves the *entity* on the default branch but runs the file **from the dispatched ref**, so `--ref main` gets `main`'s copy, and `main`'s copy is `aa86c60`'s — without the preflight. The exact invocation the preflight exists to make legible is therefore still the one that gets `could not find Cargo.toml` forty lines into a build log, and will be until either the milestone stack merges or the operator lands this one file on `main` the way `aa86c60` was landed. Nothing else is owed: no repointing, no second entity, no branch.

The same applies to `scripts/check-github-effects-lane.sh` and its fixtures, which run in `skill-quality.yml` from the ref being pushed. On `main` today that step does not exist, so the never-skip property is asserted on every milestone branch and not on `main` itself.
Origin: implementation (remediation R5, epic fiddle-srrw, bean fiddle-ufv3)
Tags: #debt #infrastructure

### 2026-08-10 — Implementers never update their bean while working, and nothing asks them to
Across M2's 21 beans, no implementer ticked a single `- [ ]` step and none was instructed to; all 20 completed beans closed with every box unticked (110 total, backfilled at close with a note saying so). `skills/develop-loop/dispatch-and-evidence.md` tells the lead to arm `.fiddle/active-bean`, initialise the eval log and dispatch, and `skills/develop/implementer-prompt.md` tells the implementer to implement, verify, commit, self-review and report — neither says to touch the bean. So the tracker holds an outcome and nothing about the hour that produced it.

Two changes worth making, in `skills/develop/implementer-prompt.md` and the develop-loop reference beside it:
- Instruct the implementer to tick its own `## Steps` boxes as it completes them, using `beans update <id> --body-replace-old/--body-replace-new`, and to append one line naming the phase it has entered (reading, implementing, verifying, inverting). The mechanism already exists and the CLI supports it; nothing in the prompts points at it.
- Have the lead append a phase line when it polls, so a reader who is not the lead can answer "where is this" from the bean rather than from `ps`.

Worth pricing against the measured cost of an implementer turn: the perf investigation in this repo found orientation is a near-fixed 5.7 minutes of a 23.4-minute bean and that model generation is 63% of wall clock, so a handful of extra `beans update` calls is not what makes a bean slow, and the visibility is what makes a stalled one detectable.

Origin: operator feedback during M2 implementation (epic fiddle-srrw) — "beans are not updated with any progress reports and run for an hour"
Tags: #debt #orchestrate #ux

### 2026-08-10 — M3's plan assigned its most load-bearing unproven assumption to its last bean
The M3 design left one thing deliberately unproven: whether the effects credential may **write** a conversation comment. Three read probes had answered 200 and proved nothing, because `peel/fiddle-effects-acceptance` is public — the same trap that let a two-repository token selection survive all of M2. So far so good; the design named it, priced it, and refused to assume it.

What it then did with it is the finding. The proof was assigned to **Task 16b, the last of 24 beans**, while Tasks 5, 11a, 13, 14 and 15 are all built on that surface being writable. Had it 403'd, the answer would have arrived after roughly twenty beans of work resting on it, and the fix is not a code change — it is `Issues: read and write` added to a credential the operator had narrowed that same day, or a different gated effect, which is §5.1 re-opened.

This is precisely the ordering §5.7 of the same document argues *for*, applied to the GraphQL contract (Task 1, first) and not applied to this. The plan's own self-review and a full codex critique pass both missed it; it was caught by the operator asking "should we check the comments part?" while Task 1 was still running.

**Settled the moment it was asked, at a cost that makes the omission worse rather than better.** A closed pull request accepts comments, so no branch and no new pull request were needed: `POST /repos/peel/fiddle-effects-acceptance/issues/19/comments` → **201 Created**, `GET /issues/comments/{id}` → the full payload, `DELETE` → **204**, residue zero. Two calls. That is the entire cost of the thing that was scheduled twenty beans late.

The rule worth carrying, since M4 through M8 all add external surfaces: **order the external-contract proofs by what a refutation would cost, not by where the work naturally falls in the plan.** A proof whose failure re-opens a design decision belongs in the first bean; a proof whose failure is a bug in one adapter can wait for the lane that owns it. Task 1 was correctly first because ADR 018 depended on it. This one was more load-bearing and went last, because it happened to belong to the live lane, and "which bean does this naturally live in" won over "what does being wrong cost".

A second, smaller finding from the same probe, which belongs to whoever writes bash against a comment payload. `user.id` appears **before** `.id` in a comment object, so scraping the first id-shaped field yields the *author's* user id rather than the comment's. The probe's own cleanup did exactly that, issued a DELETE against `505401`, got a 404, and left the comment behind until it was read properly. A typed adapter naming the two fields separately is immune; `scripts/live-github.sh` and Task 16b's phase are bash and are not. The rule: select by name, and make a cleanup that deleted nothing fail loudly, because a cleanup that cannot fail is how residue survives a passing run.
Origin: planning (epic fiddle-eoqx, seed fiddle-a9y5) — caught by operator question during Task 1's implementation
Tags: #process #debt

### 2026-08-10 — M3's plan misdescribed the identity framing it told an implementer to copy
`fiddle-7j2p`'s Step 7 instructed the implementer to frame `decision_request_id`'s inputs "the way `effect_id` frames its four — each field preceded by its byte length as a `u64` in little-endian". That is not what `effect_id` does. Following the plan's letter would have produced a second, incompatible framing inside the one crate whose whole job is that a fresh process recomputes an identity the same way every other process does.

The implementer did not follow the letter. It read `effect_id` first, extracted the real framing into a shared `pub(crate)` helper so the two functions cannot drift, and argued for it: "a second copy could acquire a different separator or a character count under a later edit, and nothing would fail until an identity stopped matching across builds." Evaluation then confirmed the extraction left `effect_id`'s bytes untouched, against the existing `b3sum` pin `39b2e77d1d17cb20`.

So the defect cost nothing, and the reason it cost nothing is worth more than the defect. **A plan that describes existing code is a secondary source, and an implementer that reads the primary one beats it.** This is the fourth plan defect M3's implementers have caught — after the second wildcard-free `EffectKind` match that made Task 2's declared scope non-compiling, the Task 12 criterion contradicting a documented decision in `config.rs`, and the load-bearing credential assumption scheduled twenty beans late. None was caught by the plan's own self-review or by a full codex critique pass; all four were caught by someone reading the code the plan claimed to describe.

The practice that follows, for M4 onward: where a plan step describes existing behaviour, it should **cite the file and let the implementer read it** rather than restating it. A restatement is a copy that can be wrong, and a plan is exactly the kind of document nobody re-checks against the code once it has been reviewed once.
Origin: implementation (epic fiddle-eoqx, bean fiddle-7j2p, Step 7) — found by the implementer, confirmed by evaluation
Tags: #process #debt

### 2026-08-10 — A fifth plan defect, and this one was a method that does not exist
`fiddle-hmho`'s test sketch called `err.worth_another_read()`. The method is `is_worth_reading_again()`, at `crates/fiddle-runtime/src/github/cli.rs:407`. The wrong name was in the bean **and** in the lead's dispatch prompt, so a bean written later against either would have inherited it.

It cost nothing: the implementer read `cli.rs`, used the real name, and reported the discrepancy. Same as the four before it.

The pattern is now firm enough to state as a measurement rather than an impression. Five plan defects on this milestone, none caught by the plan's own self-review, none caught by a full codex critique pass, all five caught by an implementer reading the code the plan claimed to describe:

- the comment-write assumption five beans depend on, scheduled to be proven by the last of twenty-four;
- a second wildcard-free `EffectKind` match, which made a bean's declared scope non-compiling;
- a Task 12 criterion requiring policy rows to be mandatory, contradicting a documented decision in the same file;
- an identity framing described as `u64` little-endian where the code writes `<byte-len>:<field>`, which would have forked the encoding;
- and now a method name that was never real.

Four of the five are the same species: **the plan restating existing code and getting it wrong.** The fifth (ordering by cost) is different in kind. So the practice recorded against the fourth stands and is worth repeating here because this entry is the evidence for it: where a plan step describes existing behaviour, cite the file and let the implementer read it. A restatement is a copy that can be wrong, in a document nobody re-checks against the source after its single review.

Worth noting what did *not* catch these, since both were paid for: a self-review pass by the plan's own author, and an external critique that returned ten findings and produced eight real fixes. The critique was worth its cost — it caught two leaking cleanups, an impossible follow-up comment, a missing exit row and a contradicting pagination test. It simply does not catch this species, because it reads the plan against the design rather than against the code.
Origin: implementation (epic fiddle-eoqx, bean fiddle-hmho) — found by the implementer
Tags: #process #debt

### 2026-08-10 — The epic's Contracts block named a type no bean was told to build
`ActorRef` appears in `fiddle-eoqx`'s `## Contracts` section, placed in `crates/fiddle-core/src/decision.rs`. Task 2 created that file and did not create the type, because Task 2's steps never mentioned it — they covered the two `EffectKind` variants, the marker's render and parse, and `decision_request_id`. So the contract described a type with no owner, and the first bean that needed it (Task 4, the conversation adapter) found it missing and asked where it should go.

Sixth plan defect on this milestone, and the first of a new species: not a restatement that got the code wrong, but a **contract entry with no corresponding step**. The Contracts block is copied into every bean body so parallel implementers cannot make incompatible choices — which works only for types some bean is actually instructed to define. Nothing checked that every entry in it had a home.

Cost: one question, answered in one message, because the implementer asked instead of guessing. Had it guessed, `ActorRef` would have landed in `github/comments.rs`, and then both `human/` and the pure decision logic would depend on the GitHub adapter for a domain identity — inverting the crate boundary `crate_boundary.rs` exists to hold, and not failing any test, because nothing forbids a runtime module owning a domain type.

The check worth adding for M4 onward, and it is mechanical: **every type named in a Contracts block must be greppable to a step that creates it.** A contract entry no step owns is either a type that will be defined twice by whoever needs it first, or defined in the wrong crate by whoever needs it soonest. Both are silent.
Origin: implementation (epic fiddle-eoqx, bean fiddle-127g) — found by the implementer asking rather than guessing
Tags: #process #debt

### 2026-08-10 — A drafting run accepts an already-readied pull request, and that is a decision rather than an accident
`EnsurePullRequest::inspect` matches on head, base and `state=open`. With M3's `draft: bool` added, a drafting run that finds an existing pull request therefore treats its postcondition as satisfied **even when that pull request has already been marked ready for review** — and it performs no mutation.

Task 6's implementer noticed this while writing the tests and flagged that it is currently "an accident of the type rather than a decision", because `gh_stub` models a pull request as `{head, base, title}` with no `draft` field at all, so the case cannot even be expressed in the fixture.

The behaviour is right, and the reasons are worth having on record before somebody "fixes" it:

- The effect is *a pull request exists for this head and base*. `draft` is a property of **creation**, not of the postcondition — the same reasoning that makes `inspect` match on head and base and deliberately not on title or body, since matching on those is what opens a second pull request.
- Re-drafting a pull request a person had readied would **undo human progress**. A run whose local record was lost must never walk back a human's action; that is the failure mode this whole milestone is built against.
- It composes: if the pull request is already ready, `EnsurePullRequestReady::inspect` returns the postcondition and nothing mutates. The two operations agree rather than fighting.

Assigned to `fiddle-pwyi` (Task 13a), which builds the scripted world the acceptance walk needs and must model `draft` for the walk to be expressible at all. It should assert the case directly: a readied pull request is not re-drafted.
Origin: implementation (epic fiddle-eoqx, bean fiddle-yg9c) — found while writing a test the fixture could not support
Tags: #debt #decision

### 2026-08-10 — The lead ruled three times on one type, and the churn was the lead's alone
`ActorRef` was placed by three successive rulings in one hour: into `fiddle-core` (asked for by Task 4's implementer, answered by the lead), then into `fiddle-runtime` (the lead retracting after seeing the implementer had already put it there), then back into `fiddle-core` (an agent acting on the first ruling before the retraction reached it, which the lead then accepted as final). It compiles and its tests pass in the final position, and no work was lost — but two agents were told opposite things about one type inside the same round.

The failure is not the placement, it is the lead answering an architectural question at message speed. Each ruling was reasoned; none was reasoned *enough*, and the second was the worst because it retracted a correct answer using the wrong test: "nothing in `fiddle-core` names it" is true and irrelevant. The right test is whether the type is domain vocabulary, and it plainly is — `EffectId`, `CapabilityId`, `WorkRef` and `InvocationRef` all live there, and M6's attended mode will have actors who are not GitHub comment authors.

What this costs and what to do about it. Two agents received contradictory instructions, a type moved crates twice, and the bean that consumes it (`fiddle-v5bm`) accumulated three notes of which two are wrong — which is worse than no note, because a reader cannot tell which is current without the git history of a bean body. The rule worth adopting: **a question about where a type lives is answered once, in writing, against the vocabulary already in the tree — and if the answer changes, the superseded note is marked superseded rather than followed by a contradicting one.** The final note on `fiddle-v5bm` does that; the two above it should have been amended rather than left standing.

Recorded here rather than only on the bean because the pattern is about lead behaviour under concurrency, and it will recur every round that has four agents asking questions at once.
Origin: lead (epic fiddle-eoqx) — three rulings on one type during the parallel round
Tags: #process #orchestrate

### 2026-08-10 — Three lead errors in one round, each corrected by the agent it was about
Recorded together because they share a cause: the lead answering fast, from a stale read of a tree four agents were changing.

**1. `ActorRef`'s placement, and who moved it.** The lead ruled it into `fiddle-core`, retracted on seeing it in `github/comments.rs`, then accepted `fiddle-core` again — and told Task 4's implementer, in its shutdown note, that it had *left* the type in the adapter and had been right to. It had not: it followed the first ruling and moved the type in `d11a47e`, also removing the `github/mod.rs` re-export with a comment on why a second path to it would invite a dependency on the wrong crate. The implementer corrected the record before shutting down, specifically so the consuming bean would not be pointed at a type that is not there. Ground truth: one definition, `crates/fiddle-core/src/decision.rs:78`, not re-exported through the adapter.

**2. "The build break was unfounded" was itself unfounded.** Task 8 reported that `HEAD` did not build because its commit declared `pub mod interpret;` while `human/interpret.rs` was untracked. By the time the lead checked, Task 9's `f02cffa` had healed it, and the lead called the alarm unfounded. It was accurate when raised. The implementer's own point is the one worth keeping: an implementer who finds a half-landed cross-lane dependency should report it and leave the other agent's file alone, and that will sometimes mean the branch tip is briefly broken through nobody's fault — **treating that report as a false alarm afterwards discourages the next one.**

**3. `crate_boundary` passing was cited as evidence about placement, and is not.** Its two `fiddle-core` tests are a resolved-closure denylist and a source grep for impure names. A pure struct of a `u64` and a `String` trips neither, wherever it lives. So the gate was green before and after the move and says nothing about which crate should own the type. Noticed by the implementer, which had checked the grep's banned list before writing its doc comment for exactly that reason.

**And one structural observation about the round rather than about the lead.** Three agents wrote to `crates/fiddle-core/src/decision.rs` in one round — `f02cffa` (`InterpretedHumanDecision`), `d11a47e` (`ActorRef`), and the lead's rulings that sent them there — none of it in any bean's declared `## Files`. The parallel round was planned by checking that the four beans' declared files were disjoint, and they were. What made a pure-core file a shared surface was the lead's own mid-round rulings, issued per-agent, each locally reasonable. **A concurrency plan that only checks declared scope does not survive a lead that widens scope by message.**
Origin: lead (epic fiddle-eoqx), corrected by fiddle-127g's and fiddle-kgr7's implementers
Tags: #process #orchestrate

### 2026-08-10 — An implementer marked its own bean completed, and the loop would have accepted it
`fiddle-dvsl` was found at status `completed` with an evaluation log reading `iterations=0, dispatches=0, verdict=UNKNOWN`. Its implementer had transitioned it, in good faith, having finished the work and written its summary. Nothing objected: no script checks that a completed bean carries a terminal verdict, and the lead only noticed while reconciling a separate message.

`docs/technical/SYSTEM.md` already carries the invariant — *"Only the lead manages bean status transitions"* — and `skills/develop/implementer-prompt.md` does not state it. Nor did the lead's dispatch prompts, which told implementers to tick their `## Steps` and append a `## Summary of Changes` using `beans update`, i.e. handed them the exact tool and said nothing about the one transition they must not make. Five implementers were dispatched with that prompt this milestone; one of them drew the obvious conclusion.

**What it would have cost if unnoticed.** The bean reads as converged with no scorecard, no dimension data, and no second pass. Its two genuinely unassertable criteria were correctly named as owed rather than approximated — the honest outcome — but nothing would have recorded that a human had ever agreed, and `trend-eval-history.sh` would show a completed bean contributing nothing, indistinguishable from an evidence-only convergence. The milestone's own calibration was changed this round specifically to stop that happening.

Two fixes, and the second is the one that survives a forgetful lead:

- **State it in `skills/develop/implementer-prompt.md`**: tick your steps, append your summary, never change `status`. The prompt already hands over `beans update`; it should fence the one use that is not the implementer's.
- **Make it mechanical.** `scripts/check-convergence.sh` and the eval-log scripts already exist; a bean at `completed` whose parsed log has no terminal verdict is a detectable state, and the natural home is the same Stop-hook family as `develop-verdict-gate.sh`. A prose rule in SYSTEM.md that neither the prompt nor any script enforces is a rule that holds until an agent is helpful.
Origin: implementation (epic fiddle-eoqx, bean fiddle-dvsl) — found by the lead while reconciling a shutdown message
Tags: #process #orchestrate #debt

### 2026-08-10 — Concurrent lanes sharing one `target/` produce false test failures, which is an evidence-integrity problem
`fiddle-9krm`'s implementer reported four `config_check` failures during a workspace run, caused by `target/debug/fiddle` being **relinked by a concurrent lane mid-run**. Re-run in isolation, that binary passed 20/0. Confirmed: all agents in this round share `/Users/peel/wrk/fiddle/.worktrees/agentic-factory-m3/target`, and the acceptance lanes resolve the binary under test through that path.

The obvious cost of a shared `target/` is slowness — cargo's build lock serialises compilation, which is why the parallel round's speedup was smaller than projected. **The cost that matters more is that a suite can report failures that are not real.**

Acceptance lanes here launch the compiled binary as a subprocess. If another agent relinks it between the launch and the assertion, the lane fails for a reason that has nothing to do with the code under review. And the evidence pack an evaluator scores is a *captured* suite run: a false failure inside it is indistinguishable, to the evaluator, from a real one. So the failure mode is not "a lane flakes and somebody re-runs it" — it is **a bean scored down for another bean's link step**, with the evaluator reasoning carefully about evidence that was never true.

Nothing was mis-scored this round: the implementer noticed the pattern, re-ran the affected binary in isolation, and reported both results. That depended on an implementer being careful enough to distrust its own red suite.

Two mitigations, and the second is the one worth adopting:

- **Re-run a failing binary in isolation before believing it**, and say so in the report. Cheap, and it is what happened here — but it relies on judgment every time.
- **Give each concurrent agent its own `CARGO_TARGET_DIR`.** Costs a cold build per agent, genuinely parallel rather than serialised on the build lock, and removes the interference entirely. It also recovers the parallel speedup the shared lock was eating. The lead should set it in the dispatch prompt for any round with more than one implementer.

Worth stating the general shape, because it will recur wherever this project parallelises: **a shared mutable artifact between concurrent lanes turns a verification result into a race.** The evidence pack is only as trustworthy as the isolation of the run that produced it.
Origin: implementation (epic fiddle-eoqx, bean fiddle-9krm) — observed by an implementer that distrusted its own failing suite
Tags: #process #infrastructure #debt

### 2026-08-10 — A plan's test snippet would have compiled, passed, and proven nothing
The ninth plan defect of this milestone and the subtlest. The others were wrong names, absent types, or harnesses that never existed — all of which fail loudly the moment an implementer tries them. This one would have shipped green.

`fiddle-rvcu`'s bean asked for a test proving that a resolved decision does not license a widened payload. Its snippet built the case by widening the **operation's** payload:

```rust
let widened = op.with_payload("something else");
let err = world.execute_decided(widened, &decision).await.unwrap_err();
assert!(matches!(err, EffectError::PayloadDiverged { .. }));
```

`Executor::execute`'s **step 6** already refuses exactly that — it compares the envelope's digest against `IntegrationOperation::payload()` and has done since M2. So the assertion would have passed with step 4's new decision-payload comparison **deleted**: a test about a check that was not running, guarding a property nobody was checking.

The implementer caught it while running the bean's own required inversion and rewrote the case to move the **decision's** payload, leaving proposal and operation agreeing so that only step 4 can refuse. Same property, correct isolation, and the doc comment now records why so nobody simplifies it back. It is also the realistic case: the person approved request A, the continuation built request B, and the identity is unchanged because identity derives from the target rather than the payload.

**What made this one detectable was the inversion, not review.** The bean required three inversions and named what each must break; running them is what exposed that one broke nothing. A reviewer reading the snippet against the design would have seen a correct-looking assertion of a real property — which is precisely what a full external critique pass did see, having read this plan without noticing it.

The rule this sharpens, beyond the standing inversion requirement: **a test written against a property that a neighbouring check already enforces cannot distinguish the two.** When a plan asserts a new guard, the snippet has to arrange a state that *only* the new guard can refuse — and the way to find out whether it does is to delete the new guard and watch. That is cheap, mechanical, and it caught something six other kinds of review did not.
Origin: implementation (epic fiddle-eoqx, bean fiddle-rvcu) — found by running the bean's own required inversion
Tags: #process #debt

### 2026-08-10 — The lead's verification shell has no toolchain, and the nearest one compiles a different language
A sibling of the shared-`target/` finding above, and it bit the lead rather than an implementer. Building `fiddle-rvcu`'s evidence pack, `cargo` was **not on the verification shell's `PATH` at all** — the toolchain arrives through the worktree's devenv/direnv environment, which implementer agents load per-cwd and the lead's shell does not.

Two things then went wrong in sequence, and the second is the dangerous one.

**The captured exit code was the wrong process's.** The verification script read `cargo fmt --all --check 2>&1 | tail -5; echo "exit: $?"` — so `$?` was `tail`'s status. `cargo` was missing, every command printed `command not found`, and the log recorded `fmt exit: 0`. A clean bill of health for three checks that never ran. This is the same defect an evaluator had just found in the previous pack, where a `&&` chain silently swallowed the clippy line; a pipeline swallows it just as quietly, so fixing the `&&` did not fix the class.

**The obvious repair would have measured the wrong compiler.** A `cargo` *does* exist in the sibling `m0` worktree's devenv profile, and reaching for it looks harmless. But `flake.nix` differs between the two worktrees at exactly one line — the Fenix hash of `rust-toolchain.toml` — because **m3 pins 1.97.1 where m0 pins 1.85.0**. Verifying m3's tree with m0's `cargo` would have run `clippy -D warnings` under a compiler twelve minor versions old, on a lane whose entire evidentiary value is that clippy is clean. A pass would have meant nothing and nothing in the log would have said so.

Both mitigations are mechanical:

- **Never infer an exit code through a pipe.** Redirect to a file, capture `$?` from the command itself, then summarise the file. Applies to `&&`, `|`, and `tee` equally.
- **Resolve the toolchain from the worktree under test**, via `rust-toolchain.toml` rather than from whatever `cargo` is reachable. A stacked-branch project will have worktrees on different pins, and the neighbouring one is always the closest wrong answer.

The general shape, stated to sit beside the `target/` finding: **an evidence pack is only as trustworthy as the provenance of the tools that produced it.** Isolation covers *where* the build wrote; provenance covers *what* did the building. A verification run has to pin both, and the lead's own pack is not exempt from the standard the lead asks evaluators to enforce.
Origin: implementation (epic fiddle-eoqx, bean fiddle-rvcu) — found by the lead while building an evidence pack, after an evaluator had flagged the same exit-code class in the previous one
Tags: #process #infrastructure #debt

**Addendum, same day — there is a third axis, and it invalidated the corrected run too.** With the toolchain pinned and `CARGO_TARGET_DIR` isolated, the verification still came back **525 passed / 2 failed**, and both failures were in `github::comments` and `human_comments` — a *different* bean's files, being edited by a live agent at that moment. `effect_protocol` read 50 where the bean under evaluation had measured 48.

**Correction, entered after that bean's evaluator refuted the attribution.** The two extra tests were *not* a third agent's contamination. `git log 4622f05..400e4d0 -- crates/fiddle-runtime/tests/effect_protocol.rs` returns exactly one commit, `400e4d0`, which is **the evaluated bean's own work** — two tests and a fourth inversion its implementer landed 33 seconds before this entry was written. They were uncommitted at the moment of measurement, so the isolation lesson stands unchanged; what was wrong was the story told about them. The count 50 was not noise, it was the bean's own next state — and treating it as contamination is precisely how the evidence pack came to be pinned a commit behind the bean it was evaluating, which the same evaluator also caught. The two *failures* in that run were genuinely another lane's inversion, and that half of the entry holds.

The lesson underneath the lesson: **an unexpected number is a question, not a defect.** Attributing it to a known failure mode without running `git log` on the file is the same move as accepting a claim without evidence — and here it went into a permanent record 33 seconds after the commit that would have explained it.

So the isolation has three axes, not two:

| axis | what it pins | how it fails silently |
|---|---|---|
| `CARGO_TARGET_DIR` | where the build wrote | a concurrent lane relinks the binary under test mid-run |
| `rust-toolchain.toml` | what did the building | a sibling worktree's cargo is a different compiler |
| **a detached worktree at the commit under evaluation** | **what was built** | **uncommitted work from other agents is measured as the bean's** |

Only the third one produces numbers that are *attributable*. `git worktree add --detach <scratch> <sha>` gives a checkout with zero dirty files, and every count taken there belongs to the commits under evaluation and nothing else. Re-run that way, the same tree verified clean.

This is the method `fiddle-rvcu`'s implementer used unprompted — it measured its delta in a scratch worktree at BASE_SHA with only its two files applied, *because* two other implementers were editing the shared tree — and the lead praised it without adopting it. Adopting it: **an evidence pack for a bean is built from a detached worktree pinned at that bean's last commit**, never from the shared branch checkout, whenever any other agent is live. The shared checkout is only safe when nothing else is running, which in this milestone has been almost never.

**Second addendum — an inversion run is a uniquely bad neighbour, and the three axes are not interchangeable.** The lane that caused the phantom red made the distinction precisely, and it is worth keeping: a private `CARGO_TARGET_DIR` isolated its *artifacts* from concurrent relinks, but did nothing to isolate the *source tree it was mutating* — which is the failure two other agents and the lead then hit. Only the third axis prevents that.

What makes inversions special is that **they deliberately break the tree, and unlike an ordinary edit there is no window in which the intermediate state is meant to be green.** A normal in-progress edit is transiently red by accident and its author is trying to get back to green; an inversion is red *on purpose*, for as long as the measurement takes, and its author will revert rather than repair. Any other agent reading the workspace during that window gets a true-but-expired failure with no signal that it is expired — and, worse, one that looks exactly like a real regression in a file they do not own.

So the rule is not "isolate your build directory", it is: **run inversions in a detached worktree pinned at the commit under evaluation, never in the shared checkout.** The lead's implementer dispatch prompt currently mandates the private target directory and should mandate this too. Cost is a cold build per inversion round; the alternative is that every concurrent lane's test run becomes unreliable for the duration, which has now happened three times in one afternoon and consumed more time than the builds would have.

### 2026-08-10 — `comments.rs:262` claims more relation recognition than one character buys
Recorded here because it is owed by a bean that is about to converge, and an owed item on a closed bean is a lost item.

`read_a_link_value`'s notion of "readable" is the **presence of a `>`**. That is enough to keep an unparseable header from being read as an end of pages, which is the property `fiddle-9krm` exists to establish, but the doc comment at `crates/fiddle-runtime/src/github/comments.rs:262` describes a stronger recognition than one character delivers. Two residuals follow from it: `<url>; rel="ne` still reads as an end, and a single valid link-value marks a mixed header readable.

Neither is fixable by widening the doubt direction — doing so sends every legitimate last page to its bound, which is a worse failure than the one being prevented. So the code is right and the comment overclaims. **Soften the comment whenever that file is next touched**; do not change the behaviour to match the comment.

The bean's implementer flagged this against its own work, unprompted, having been scoped to correct a record rather than to touch the file — and declined to touch it. That is the correct boundary and the reason this note exists rather than an unrequested edit.
Origin: implementation (epic fiddle-eoqx, bean fiddle-9krm) — volunteered by the implementer against its own lane
Tags: #debt #docs

### 2026-08-10 — A bean marked in-progress with zero dispatches is indistinguishable from work in flight
`fiddle-v5bm` (Task 5) sat in the lead's status table as a live lane for most of a milestone session. It was not. Its `## Evaluation Log` read **`total_dispatches: 0`**, zero of five steps were ticked, none of its three declared files had a commit, and no implementer report was ever received.

The mechanism is a gap in the loop, not merely an oversight. **The lead sets a bean to `in-progress` when it intends to dispatch**, and the status field is the same afterwards whether the dispatch happened, died at birth, or was never sent. So `in-progress` means "the lead once intended this" rather than "an implementer is working on this", and nothing in the loop notices the difference. The lead then reports the bean as in flight, sequences other work behind it, and — in this case — routes two handoffs from another bean to an implementer that does not exist.

The tell was present the entire time and free to read: `total_dispatches: 0` on a bean claimed to be in progress is a contradiction. So is zero ticked steps on a bean that has been live for hours, which is why the earlier finding about implementers never updating their bean while working matters more than it looked — **it removed the only other signal that would have caught this.**

Two mitigations, and the second is the real one:

- **Cross-check `in-progress` against `total_dispatches` and against `git log` on the bean's declared files** before reporting a lane as live. Cheap, and it is what finally found this.
- **Derive lane liveness from artifacts rather than from status.** A bean is being worked on if and only if there are commits on its declared files, or dirty state in them, or a dispatch recorded in its log. The status field is the lead's intent and should never be read as evidence of an agent. This is the same rule the milestone already applies to implementer claims — a claim is not evidence — applied to the lead's own bookkeeping, which had been exempt.

Worth stating the shape, because it is the third time this milestone: **the lead's own records were held to a weaker standard than the agents' were.** An implementer that reported "in progress" with no commits would have been challenged immediately.
Origin: process (epic fiddle-eoqx, bean fiddle-v5bm) — found by checking a bean's eval log after noticing it had no commits
Tags: #process #debt

### 2026-08-10 — The isolation policy that fixed evidence integrity filled the disk to 100%
The three-axis isolation rule recorded above — a private `CARGO_TARGET_DIR` per agent, plus a detached worktree per inversion — works, and it has a cost nobody priced. Each cold build directory is **1.5–3.6 GB**. With four implementer lanes, two evaluators taking independent measurements, and the lead building evidence packs, the root filesystem reached **100% (3.6 GB free of 461 GB)**. Reclaiming four directories belonging to converged beans freed **9.2 GB** immediately.

Why this is an evidence-integrity problem rather than mere housekeeping: **a build that fails for want of disk does not announce itself as a disk problem.** It surfaces as a link error, a truncated artifact, or a test binary that will not run — indistinguishable at a glance from the failures the isolation was introduced to eliminate, and arriving in exactly the same reports. The fix for false failures is capable of manufacturing false failures.

An evaluator flagged it unprompted while shutting down, having noticed `/private/tmp` at 99% and correctly identified the sibling target directories as the bulk.

The missing half of the policy is a disposal rule, so state it with the policy:

- **A per-inversion worktree is removed as soon as its measurement is recorded** — `git worktree remove --force` in the same step that writes the counts, not at the end of the lane.
- **A lane's `CARGO_TARGET_DIR` is deleted when its bean converges**, by the lead, in the same action that sets the bean `completed`.
- **Check free space before dispatching a parallel round.** Six concurrent lanes need roughly 20 GB of build directories, and the round should be sized against what is actually available rather than against the pane ceiling.

Worth pairing with the earlier finding about `git worktree list` accumulating detached checkouts: the same discipline covers both, since a stray worktree and a stray target directory are the same species of leak and only the target directory is large enough to stop the machine.
Origin: process (epic fiddle-eoqx) — surfaced by an evaluator during shutdown, after the lead mandated the isolation without a disposal rule
Tags: #process #infrastructure #debt

### 2026-08-10 — A claim about N cases needs N observations, and a fail-fast test can only make one
A sharpening of the `effect/mod.rs:806-810` finding, generalised to a different shape by the implementer of `fiddle-ayqd` — who applied it to its own already-committed work, unprompted, on the strength of another bean's evaluation.

It had committed a sentence asserting that **three** mangled marker bodies previously refused as `Version`. Its inversion run directly observed **one** of them, because the test fails fast on the first case. The other two follow deterministically from reading a two-line function — which is inference, not observation, and therefore the same species as a doc comment naming a mechanism nobody measured.

The structural point is the part worth keeping. Re-observing the other two by hand corrects the *claim* and leaves the *hole*: a fail-fast test can only ever observe its first case, so the next reflow is free to break cases two and three silently, and the sentence keeps standing on one observation. The durable fix is to make the test unable to pass while any case is unobserved — **collect the outcome for each case and assert on the collection**, so a run reports which case diverged instead of stopping at the earliest one.

Stated as a rule, because it generalises past this file: **a claim quantified over N inputs is evidenced only by N observations.** A test that stops at the first failure evidences exactly one, however many cases its body enumerates. When a comment or a bean says "all three", "every kind", or "each variant", the test behind it must be able to fail on any one of them individually and say which.

This is the third distinct dressing of one underlying error this milestone: a stated mechanism standing in for a measured one. The first was a test whose property a neighbouring check already enforced; the second a comment crediting match-arm order over disjoint variants; this one a quantified claim behind a fail-fast assertion. All three read as correct, and none of the three could notice being wrong.
Origin: implementation (epic fiddle-eoqx, bean fiddle-ayqd) — self-audit of committed work, prompted by fiddle-rvcu's evaluation
Tags: #process #testing

**Addendum — the fourth dressing, and it is the one with no local test at all.** Applying the rule above to its own work, `fiddle-ayqd`'s implementer first confirmed the disputed sentence *was* exact: probing each case separately gave `Version("")`, `Version("v1\nrequest=…")` and `Version("request=…")` for the three it claims, with the two that were already `Malformed` being precisely the two the comment does not claim. Five cases, five observations, claim held.

Then it went further unprompted and found a different claim of the same species: a module doc in **`fiddle-core`** asserting that the continuation "recomputes all four fields from canonical inputs and compares them", plus an author check against an allowlist of `ActorRef::id`. That is a claim about **`fiddle-runtime`'s `validate.rs`** — another crate, rewritten by a sibling lane *after* the sentence was written. It verified against the committed tree: `:385` request, `:407` effect, `:472` payload, `:603` head_sha, `:525` the allowlist check. All four plus the actor, so the sentence names work that is actually done.

Its own summary of the near-miss is the finding: *"Had `effect` not been compared there I would have had exactly your `effect/mod.rs:806-810` defect — a true property credited to the wrong mechanism."*

What makes this the worst of the four dressings is that **nothing local can fail.** The other three were at least in reach of a test in the same file: a neighbouring check could be deleted, an arm order swapped, a case enumerated. A comment in crate A describing a mechanism in crate B has no test that binds them — crate B can be rewritten, as it just was, and crate A's sentence goes on asserting the old shape with every suite green.

So: **a cross-crate explanatory claim is unevidenced by construction.** Either put the assertion where the mechanism is, or write it as a reference ("see `validate.rs`, which compares …") rather than as a statement of what happens, so a reader knows to go and check rather than trusting a sentence nothing guards. The four dressings together say the same thing — a stated mechanism standing in for a measured one — and this is the variant where the gap cannot be closed by testing harder in the same file.

**Addendum — the cross-crate hypothesis was confirmed the hard way: the sentence was already false.** The entry above argued that a comment in crate A describing a mechanism in crate B is unevidenced by construction. Within the hour the owning lane (`fiddle-n8fs`, which holds `validate.rs`) read the sentence and found **two of its three claims wrong**, one materially:

- **"recomputes all four fields from canonical inputs" is false for `head_sha`.** Three values are recomputed from canonical inputs — effect id, payload hash, request id. `:603` compares the marker's `head_sha` against the head **observed from the world**, so *neither side of that comparison is a recomputation*. The sentence misdescribes what would fail if GitHub were lying.
- **`:385` is a sieve, not an authentication.** It compares the request id inside a `filter_map` answering *which comment is our question*, and authenticates nothing, because a request id is copyable off the visible conversation. The authentication is `:407` alone. Listing the two as peer comparisons invites a reader to think step 2 establishes provenance — which is the exact confusion `a_parse_is_not_an_authentication` exists to refute.
- `:525` confirmed, with a nuance the doc omits: it sits after the `is_bot` arm, so a bot carrying an allowlisted id is refused *before* the allowlist is consulted.

So the prediction did not merely describe a risk; the risk had already materialised, silently, with every suite green. `fiddle-core` cannot depend on `fiddle-runtime`, and the allowlist parameter is a bare `&[u64]` rather than an `ActorRef`-typed value, so a refactor to login comparison would leave the doc false and nothing would fail. **The correction is owed as its own work** — the doc's author has shut down, and the accurate phrasing is recorded on the follow-up bean.

### 2026-08-11 — A test can be insensitive because of its fixture, not its assertion
The fifth dressing of this milestone's recurring error, and the first where the assertion is correct, the property is real, and the test still cannot fail.

`fiddle-n8fs` set out to invert "the last authorized reply decides" and got a **null result**: `select_candidates` sorted by id, which made `last()` and `max_by_key(id)` agree under *every arrangement a fixture can build*, so the line expressing the property proved nothing. It restructured `resolve` to choose before it orders, and the inversion then landed.

The part worth keeping is what it noticed next. That inversion is **invisible to `the_last_authorized_reply_decides_and_the_earlier_ones_are_evidence`** — the test named for the property — because that test's fixture is *sorted*. The assertion is right, the property is real, and a position-based reading of "last" would pass it forever. What catches the inversion is a different test, `a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one`, whose fixture is deliberately out of order.

The four earlier dressings were all failures of the *assertion* or the *explanation*: a property a neighbouring check already enforced, a comment crediting match-arm order over disjoint variants, a quantified claim behind a fail-fast test, a mechanism described across a crate boundary. This one is a failure of the **input**. Every previous mitigation — assert the message, pin the bytes, observe each case, put the claim where the mechanism is — leaves it untouched, because the test is already asserting the right thing about the wrong world.

The rule: **for any property about order, selection, or identity, at least one test must supply an input where the correct answer and the lazy answer differ.** If every fixture is sorted, "last", "greatest", "first match" and "the one with the largest id" are the same value, and a test cannot distinguish which one the code computed. A passing test whose fixture cannot separate the candidate implementations is a test of nothing, and it will read as the property's guard to everyone who comes after.
Origin: implementation (epic fiddle-eoqx, bean fiddle-n8fs) — a null inversion result that changed the production code
Tags: #process #testing

**Amendment — the cheap inversion driver, stated so the wrong half is not what propagates.** A lane that ran 21 inversions found the per-inversion worktree prescribed above is more granularity than the problem needs, and the lead initially wrote that up in a way that invited exactly the wrong reading. Precisely:

- **The saving is one worktree for N inversions. It is not permission to skip isolation.** The run must still happen in a **detached worktree pinned at the commit under inversion**. What is unnecessary is a *fresh* worktree per inversion — a single pinned worktree can host all N rounds, mutating and restoring in place, for one cold build instead of N. Someone who reads "single build, mutate in place" and runs it in the shared checkout has made the original mistake, which is the one this whole entry exists to prevent.
- **In-place mutate-and-restore is only safe with two guards, and both are required.** Copy the file to a pristine path *before* the first mutation, and *after* the run diff the working file against that copy and assert byte-identity. Run the restore in a `finally`. Without both, an interrupted or crashed round leaves the tree mutated and red — reintroducing the phantom-failure problem *inside* the isolation meant to prevent it, and somewhere far less visible than the shared checkout, where at least other lanes notice.

The lane that supplied this used both guards and verified the restored file byte-identical to the commit before removing the worktree. That is the standard, not the optional extra.

**Amendment to the fixture rule above — the mitigation fails when applied in the obvious place.** The rule as recorded ("at least one fixture must make the correct answer and the lazy answer differ") is right and incomplete. The lane that found it added the part that makes it usable:

> **That discriminating fixture usually has to live in a *different test* from the one named for the property.**

In this case the sorted fixture sat in `the_last_authorized_reply_decides_and_the_earlier_ones_are_evidence` — the test named for the rule — and could never fail under a position-based reading. The discriminating fixture lives in `a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one`. So **someone applying the rule by strengthening the property's own test would not reach it**, because the fixture that test needs is the one it already has for every other assertion it makes. The fifth dressing survives a mitigation applied in the obvious place, which is what makes it the worst of the five.

The corollary, also from that lane, is to resist collapsing the two tests: `considered` order and which reply decides are separate claims, and keeping them as separate assertions is what made two of the inversions individually visible rather than one indistinguishable failure.

**The empirical case, from a lane that both caused and suffered it.** The lane holding Task 7 re-ran its three inversions pinned and corrected two of its own reported figures, having discovered it had run all three in the shared checkout:

- Its baseline was **529 / 0 / 1 / 38** pinned at its own commit, not the **556** it first reported — that figure was the shared tree carrying three other lanes' commits. Both numbers are correct for what they measure; only the pinned one is attributable to the bean.
- It had reported `a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one` as a third casualty of its `FORBIDDEN` inversion. Pinned, that inversion fails exactly two tests. **The third was another lane's noise, read out of the shared tree — while its own first cut at that same inversion had nine tests red in that tree for three other lanes to trip over.**

That is the whole finding in one lane: **a lane running an inversion in a shared checkout simultaneously generates and consumes unattributable failures, and from inside the two are indistinguishable.** It cannot tell its own noise from anyone else's, and neither can anyone reading its report. Two of the three phantom-failure reports chased earlier in the day are accounted for here.

One correction went the other way and is worth noting for calibration: its `minimum()` → `Automatic` inversion was **526 / 3**, not the 523 / 2 first reported, the extra failure being a test that did not exist at the time of the first run. So the claim that a relaxed minimum "would fail here" is carried by three tests rather than two. **A recorded count that moves upward for a nameable reason is stronger evidence than the original**, and re-measurement is what surfaced it — the same discipline that corrected the two errors also strengthened the third row.

**Correction to the rule above, proved rather than asserted.** The entry says a fail-fast test "can only ever evidence its first case". The lane that supplied the finding then corrected the lead's wording, which had overstated it:

> The fail-fast loop *did* catch any single case that regressed — either form does. What it could not do is **report** a second one.

So the defect is not insensitivity, it is **unreportability**, and it bites exactly when several cases regress together — which is the run that matters for a quantified claim. It demonstrated the distinction instead of arguing it, weakening the guard four ways against the restructured table:

| guard weakened for | passed | failed | cases the one failing run reported |
|---|---|---|---|
| the empty token | 555 | 1 | 1 |
| a token containing a newline | 555 | 1 | 1 |
| a token starting `request=` | 555 | 1 | 1 |
| **whole guard removed (all three regress at once)** | 555 | 1 | **3, all named** |

The same production defect reported **one** case under the fail-fast loop and **three** under the collected table. It explicitly declined to claim the first three rows as evidence for the restructure, since they pass under either form. So the corrected rule is: **a claim quantified over N inputs needs a test that can report N failures, not merely detect one.** A loop that stops at the first is sufficient to *notice* a regression and insufficient to *evidence a count*.

### 2026-08-11 — `cargo test` green and `cargo clippy` red on a test-only change
Small, mechanical, and it would have handed up a red gate. A lane restructuring an assertion table — **tests only, no production code** — had the workspace suite pass at 556 while `cargo clippy --workspace --all-targets --all-features -- -D warnings` exited **101** on `clippy::type_complexity` in the new helper's return type. Fixed with two type aliases, which read better anyway.

The general fact: **a test-only change is not clippy-safe by construction.** `--all-targets` lints test code, so a helper signature introduced in a `#[cfg(test)]` module is as capable of failing the gate as production code is, and the test suite passing says nothing about it. A lane that reasoned "I only touched tests, the suite is green, the gate is fine" would have reported success on a red gate.

This is the fourth distinct way in this milestone that **a green signal has stood in for an unrun check** — after an exit code read through a pipe reporting `tail`'s status, a clippy line swallowed by an `&&` chain, and a test filter that matched nothing and reported `0 passed; 15 filtered out`. The pattern is durable enough to state as a rule: **every gate command runs, and its own exit code is captured, on every change — no change is exempt by category.**
Origin: implementation (epic fiddle-eoqx, bean fiddle-ayqd) — found by a lane that ran clippy on a test-only change instead of assuming
Tags: #process #infrastructure

### 2026-08-11 — An earlier assertion can short-circuit the one that carries the property
The seventh dressing, and a new mechanism: not a property a neighbour already enforced, not a wrong explanation, not a fail-fast count, not a cross-crate claim, not an undiscriminating fixture. Here the test **fails correctly** and the assertion carrying the criterion **is never evaluated**.

Task 7's `the_revision_is_part_of_the_identity_and_not_only_of_the_payload` asserted the *target strings* first and the two `EffectId`s second. Under the inversion that drops `@{head_sha}` from the target, the run failed on the string comparison — `left: "acme/r#7", right: "acme/r#7"` — and execution stopped there. So the test noticed the break, but its diagnostic named the **mechanism** (the target's spelling) while the **property** (two revisions derive two identities) went untested at the exact moment it broke.

The lead had predicted a different failure: that both sides of the identity comparison might derive through one code path, collapse together, and satisfy the assertion. That was wrong in mechanism — the assertion is `assert_ne!`, so a collapse makes it fail. The consequence was the same, reached by assertion **order** instead. The implementer proved the identity half is independently load-bearing with a throwaway probe asserting nothing but the inequality:

```
unmutated:       identity(aaaa) = 3ec6f2ec9d777a35 / identity(bbbb) = 8bf86e9eb29943b9  -> passes
under inversion: both collapse to 4c87b686e7dd354b                                      -> fails
```

Reordering so the identity is asserted first — both halves still assert, only the order changed — makes the inversion report the property instead of the spelling.

The rule: **when one test asserts both a mechanism and the property that mechanism serves, the property goes first.** An assertion that fires earlier consumes the failure, and the one that matters is never reached. A green suite hides nothing here; what is hidden is *which* claim a red run establishes, and that only becomes visible when someone deliberately breaks the thing and reads the diagnostic rather than the exit code.

Second thing from the same run, worth keeping as method: the inversion failed **two** tests and the implementer counted **one witness**. The other, `a_mutation_with_no_node_id_in_hand_is_not_sent`, fails only because it asserts a refusal message containing `acme/r#7@aaaa` — sensitivity to the target's spelling, which is that assertion's job, but not independent evidence for the identity property. **Two tests failing is not two witnesses**, and a lane that counted rows rather than distinct properties would have over-claimed its own coverage.
Origin: implementation (epic fiddle-eoqx, bean fiddle-dvsl) — an inversion asked for three times, which found a defect once it ran
Tags: #process #testing

### 2026-08-11 — Lead instructions and implementer reports crossed four times on one bean, and the bean body was the fix
Four round trips on `fiddle-dvsl` were spent on work that was already done. Each time, the lead wrote an instruction while the implementer's report on that same item was already in flight — the lead asked for the `@{head_sha}` inversion after it had run, asked for a refusal test after it had landed, and accepted a different bean at a commit that did not contain the fix it was accepting.

The implementer named the mechanism and the remedy, and the remedy is free:

> my reports and your instructions have crossed every time, because I commit and report while your next message is already in flight. Check the bean's `## Summary of Changes` tail before writing the next instruction — I append there before I send, so it is the one place that is never stale.

That is right, and it generalises past this lane. **A message is a snapshot of what its author knew when they started writing it; the bean is the current state.** So the rule for the lead is: **read the bean's `## Summary of Changes` tail immediately before dispatching any instruction to a live lane** — not the last report received, which is by construction older than the bean.

The cost of not doing it is not only wasted round trips. It produced a worse error on a second bean: the lead built an evidence pack, dispatched an evaluator against `d8ebbd6`, and accepted the bean at a commit that **did not contain the restructure the pack's own findings turned on** — the fix was sitting unlanded in the implementer's worktree, and it landed afterwards as `c9a5a50`. The evaluator's failing verdict on `m3-refusals-classified-honestly` was therefore against a tree missing the fix for that exact criterion. Reading the bean tail before building the pack would have caught it; reading the last report could not, because the report predated the commit.

Two smaller rules fall out of the same episode:

- **An implementer holding verified work whose lane is being shut down should land it rather than let it die**, and say so — this one did, judging a revertible commit better than losing work the lead had explicitly asked for, and flagged that an evaluation might be mid-flight against the older commit. That is the right trade and the right disclosure.
- **A shutdown reason is not a state assertion.** The lead's shutdown message described the bean as complete at a commit that was missing a piece. Accepting a bean and retiring its lane are separate acts, and the first needs the bean read, not the conversation recalled.
Origin: process (epic fiddle-eoqx, beans fiddle-dvsl and fiddle-ayqd) — named by an implementer after the fourth crossed instruction
Tags: #process

### 2026-08-11 — A lint fix inserted between a doc comment and its item silently reattaches the documentation
The eighth dressing, and the first with a purely mechanical cause. Nobody wrote a false sentence; a correct sentence was moved away from what it describes by a fix to an unrelated lint.

`fiddle-ayqd`'s restructure carried a long doc comment explaining why a `for` loop of `assert_eq!` cannot evidence a claim about three cases. Its first version tripped `clippy::type_complexity` on the helper's return type, and the fix — two type aliases — was inserted **between the comment and the function**:

```
lines 501-525   /// the essay about collect-then-compare
line  526       type Case = (&'static str, String, String);
line  530       type Refusals = Vec<(&'static str, String)>;
line  532       fn refusals(cases: &[Case]) -> (Refusals, Refusals) { … }
```

Rustdoc attaches a doc comment to whatever item follows it, so **the essay now documents `type Case`**, `fn refusals` is undocumented, and both `see [refusals]` links (`:559`, `:636`) lead a reader to a function with no prose. Worse, the bean *and* the lead's evidence pack both assert that the distinction is "written into `refusals`' documentation" — a claim that is not true of the artifact as committed. The record contradicts the code again, and this time neither the implementer nor the lead wrote the contradiction: the lint fix did.

Nothing can fail here. `cargo doc` builds, clippy is clean, the suite is green, and the essay reads correctly in isolation — it is simply attached to the wrong item. `cargo doc` would render it under `type Case`, which is the only place the defect is visible, and nobody reads rendered docs during a gate.

Two rules, and the second is the general one:

- **When a fix inserts an item, check what the doc comment above the insertion point now documents.** Adding a type alias, a `const`, or a `#[cfg]` block between a comment and its function is a silent reattachment.
- **An intra-file `see [x]` link is only as good as `x` having prose.** A link into an undocumented item is a dead end that reads, in the source, exactly like a live one.

Also found in the same pass, both worth keeping as calibration for how precise a corrected claim has to be: a pointer that **under-names the function holding the comparison it corrects** — the corrected sentence is about the head-sha comparison, which lives in `observe`, not in the `resolve` the pointer names — and a claim **compressed past the point it stays true**: "the one field the conversation cannot supply" holds for a *forged* effect id but not for a *verbatim copy* of the marker, which supplies it and is refused as `DuplicateRequest` instead. The accurate form already existed one crate away.
Origin: evaluation (epic fiddle-eoqx, bean fiddle-ayqd, iteration 2) — all three found by an evaluator reading the committed artifact rather than the diff
Tags: #process #docs

**Addendum — a correction appended rather than folded in leaves the error in place, and the milestone's own rule already said so.** `fiddle-ayqd`'s confirming pass **failed** `m3-refusals-classified-honestly` on precisely this, and the wording it failed on was the lead's.

The lead wrote that a fail-fast test "can only ever observe its first case". The implementer put that into a doc comment, then **corrected the lead** with the sharper distinction — unreportability, not insensitivity — and recorded the correction in a **later paragraph of the same comment**. So the essay now reads: the false claim at lines 593 and 598, then *"To be exact about what the loop did and did not do…"* at 600 setting it right.

The confirming pass's verdict: *"The later paragraph corrects this distinction, but the contradictory wording remains in the same documentation and prevents an honest classification."* That is correct. A reader meeting line 593 first learns something false, and the criterion is specifically about honest classification.

**This is the "replace, don't append" rule — already standing for bean records — applied to code comments.** Two beans in this milestone were told to replace stale inversion figures in place rather than leave a number with a correction beneath it, and both did. Nobody thought to apply the same rule to prose, where the failure is worse: a stale *number* beneath a correction is obviously superseded, while a stale *sentence* three paragraphs above one reads as the author's actual position.

Two things worth carrying:

- **A correction to a comment belongs where the claim is, not after it.** If the original wording survives anywhere in the same comment, the comment says both things and a reader takes whichever they reach first.
- **The lead's wording propagates into artifacts.** This one travelled from a dispatch message into a committed doc comment and then survived being refuted by the implementer, because the refutation was written as an addendum to the lead's framing rather than a replacement of it. When an implementer corrects the lead, the lead should ask where the *original* wording now lives.

### 2026-08-11 — The liveness check missed a whole worktree, and the report channel the beans specify cannot be used
Two process defects with one consequence: the lead declared a lane dead twice while it was working, and dispatched duplicate implementers onto one bean three times.

**1. `git worktree list` was never part of the artifact check.** An earlier entry established that lane liveness comes from artifacts rather than from a bean's `status` field — commits, dirty state, a recorded dispatch. That rule was applied to the **shared branch checkout only**. An implementer working in its own detached worktree is invisible to it: `git log` on the branch shows nothing, `git status` in the shared tree shows nothing, `total_dispatches` reads 0, and no step is ticked, because none of that happens until it commits.

`fiddle-v5bm` had **~1250 uncommitted lines** in `scratchpad/dev-v5bm` — a full `PublishDecisionRequest` with its `IntegrationOperation` impl, `render_request`, `decision_request_target`, and a 839-line test file with 20 cases — while the lead was reporting the bean as stalled and dispatching replacements. Both files had been modified **three minutes** before the lane that found it wrote its report.

The lane that found it named the shape exactly: *"`total_dispatches: 0` and the unticked steps are consistent with a live implementer that has not committed yet — the same trap as reading in-progress status as evidence, one layer down."*

So the check is: **enumerate worktrees, and check each one's status, not only the branch checkout.** A detached worktree named for a bean is the strongest possible evidence that bean is being worked on, and it costs one command to see.

**2. The report channel the bean templates specify does not exist.** Bean bodies and dispatch prompts have been telling implementers to write a report to `scratchpad/report-<task>.md`. **Subagents here cannot write report files** — the harness refuses. Several lanes discovered this independently and worked around it by reporting inline, each noting the refusal. One put it plainly: *"Any implementer told to report that way will fail to, silently, and you will read the silence as no work."*

That is the same failure as (1) with a different cause, and together they are why a live lane read as a dead one. The fix is to stop specifying a file: **ask for the report inline in the final message**, which is what every successful lane has done anyway. Remove the file instruction from the bean template and from every dispatch prompt.

**The compounding cost, stated plainly.** Three lanes were dispatched onto one bean. Two of them collided in `crates/fiddle-runtime/src/human/mod.rs`, and the second one's first edit was rejected as changed-under-it, with a hook naming the other worktree. The protection built up all milestone — `git commit --only <explicit paths>`, checking `git status` before committing — does not help here, and the lane that hit it said why: **the collision is in the file, not the index.** Two agents editing one file defeats every index-level safeguard, so the only real protection is not dispatching two agents into one file, which requires knowing the first one exists.

**Addendum — a fifth defect in the same comment, found after the bean converged.** `fiddle-ayqd` converged at `4111711` with five evaluations behind it. A lane then landed `d6696da`, comment-only, fixing something none of those five caught:

> The enumeration heading `a_mangled_body_is_malformed_and_says_how` listed four kinds of damage — reflowed, respaced, truncated, closing lost — over a table that runs five cases. The kind it omitted, a dropped version token, is one of the three that used to refuse as `MarkerError::Version`, so a reader mapping "the first three" onto the enumeration got the truncation instead, which was always `Malformed`. `c9a5a50` corrected the counts to five and three and left the list beneath them at four, which is where the arithmetic stopped adding up.

So the count fix was **half a fix**: the numbers were corrected and the list they counted was not. The lead verified that repair by grepping for the phrase `All four` and finding it gone — a check that could only ever confirm the numbers, never the enumeration. That is the same shape as everything else in this file: a check that cannot fail on the thing it is supposed to establish.

**The tally for one doc comment: five defects, five evaluations, and the fifth defect found after convergence.**

1. the essay orphaned onto `type Case` by a lint fix
2. a correction appended rather than folded in — the lead's wording, refuted and then left standing
3. "the message a reader would see" when the code captures the inner `Malformed` payload
4. "a run names every case that moved" when plain `assert_eq!` prints two `Vec` dumps
5. a five-case table under a four-kind enumeration, with "the first three" then pointing at the wrong three

Not one was catchable by `cargo fmt`, `cargo clippy -D warnings`, the test suite, or `cargo doc`. Every one was found by a person reading the artifact, and two evaluators plus a confirming pass read this comment without seeing (5).

**What this says about the loop, stated plainly.** Convergence is two consecutive passing evaluations, and it is a real gate for behaviour — inversions make behavioural claims falsifiable. **It is not a gate for prose.** There is no mechanical check on documentation, so the only thing standing between a comment and a false statement is whether a reader happened to look at that sentence. Five readers looked at this one and the fifth defect still survived them all.

The honest conclusion is not "evaluate documentation harder". It is that **a prose claim about code should be written so that something can fail** — the byte pin, the inversion, the per-case table — and where that is impossible, the claim should be a reference rather than an assertion. That is the same rule already recorded for cross-crate claims, generalised: **the reason to prefer a pointer over a statement is not modesty, it is that a pointer cannot go stale in a way nothing notices.**

**Addendum — a third way, and the tally.** The lane reports that the lead's direct status question *never arrived*: *"I have no record of one, and I sent you three unprompted messages."* Those three did arrive. So the channel appears to be lossy in one direction at least once, and the lead read the absence of an answer as confirmation the lane was dead.

That makes **three independent ways a working lane looks silent**, all of which fired on one bean:

| mechanism | what the lead saw | what was true |
|---|---|---|
| liveness check blind to detached worktrees | no commits, no dirty files, `total_dispatches: 0`, no ticked steps | ~1250 lines in a worktree named for the bean |
| report-file instruction subagents cannot follow | no report | a lane reporting inline, or not at all |
| a status question that did not arrive | no answer to a direct question | a lane that never received it |

Any one of these is recoverable. Together they produced three duplicate dispatches onto one bean, two lanes colliding in one file, and a lead confidently reporting a bean as stalled while it was being implemented.

**The correction is not more channels, it is fewer inferences.** Every one of these failures has the same shape: the lead concluded something from an *absence*. No commit, no report, no reply. An absence is the weakest possible evidence in a concurrent system, because every mechanism that could carry the signal is also a mechanism that can drop it. What survived contact was **positive evidence only** — a worktree that exists, a commit that exists, a symbol present in `HEAD`. So: **never conclude a lane is dead from an absence; require a positive observation that it is gone** (a terminated notification, an approved shutdown), and until then treat it as live and check the artifacts harder.

Applied concretely, before dispatching onto a bean: `git worktree list` and check each tree's status; `git log -S<symbol>` for the work the bean would produce; and read the bean tail. All three are cheap and all three return positive evidence.

### 2026-08-11 — `cargo doc` is not in the gate set, and 53 warnings have accumulated behind it
Verifying `fiddle-v5bm` at `59a319e`, the suite was green — **583 passed / 0 failed / 1 ignored / 40 binaries** — with `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` both exit 0 and **zero** clippy warning lines. `cargo doc --workspace --no-deps` then emitted **53 warnings**.

> **Correction on the number, and it matters because the number depends on the invocation.** A lane measuring `cargo doc --no-deps -p fiddle-core -p fiddle-runtime` saw the workspace go **51 → 48** across its own change, so the pre-existing backlog *on that invocation* is **48**, not 53. The 53 above is `--workspace --no-deps`. Neither figure is wrong; they are **different measurements** and the lane flagged that rather than claiming to reconcile them. Whoever takes the gate must fix the invocation first and count once — a warning backlog quoted without its command is the same class of unattributable figure this milestone has been fighting all day.

Breakdown by kind: **38** of the form *"public documentation for X links to private item Y"*, **8** *"redundant explicit link target"*, and **4** genuinely **unresolved links** — one of them `unresolved link to 'contract'` at `crates/fiddle-runtime/src/ports.rs:11`. Spread across at least twelve files, the heaviest being `github/cli.rs` (12), `workspace/command.rs` (5) and `git/publish.rs` (5). None of these is new; they have accumulated because **nothing runs `cargo doc`**.

**Why this matters more than a warning count.** This milestone spent an extraordinary amount of effort on documentation defects that no gate could catch: five separate defects in a single doc comment on `fiddle-ayqd`, the fifth surviving five evaluations and landing after the bean converged. The standing conclusion recorded above was that *convergence gates behaviour and does not gate prose*, and that a prose claim should therefore be written so something can fail.

That conclusion was half wrong. **A gate for one class of prose defect exists, ships with the toolchain, and is simply not being run.** `cargo doc` catches unresolved links, links into private items, and redundant targets — and it caught a real one on this very bean: an earlier state of `human/mod.rs` linked `[HumanInteractionPort::request]` while nothing defined the trait, which `clippy` cannot see because `broken_intra_doc_links` is a **rustdoc** lint. That defect resolved itself only because the port was later built; had the lead's ruling against building it stood, the broken link would have shipped with every other gate green.

So the honest statement is narrower and more useful: **the gate set was incomplete, not the gates.** What cannot be mechanically checked is whether a sentence is *true*; what can be checked is whether its references resolve, and that check was missing.

**Owed work**, filed as its own bean: add `cargo doc --workspace --no-deps` to the gate, decide whether to enforce with `-D warnings` immediately or ratchet from the current 53, and clear the backlog of existing warnings. Ratcheting is probably right — 53 is too many to fix inside another bean's scope, and a gate that starts red teaches lanes to ignore it.

One caveat for whoever takes it: `private_intra_doc_links` firing 38 times may be a deliberate style in this codebase — public docs pointing at private implementation detail is often the *useful* link — so that arm should be judged before it is enforced, not silenced by reflex.

### 2026-08-11 — Twenty-four tests passed over a post-forever bug, because every fixture built the two ids agreeing
The clearest instance yet of the fixture family, and it was in **landed, evaluated-adjacent code** rather than in a sketch.

`HumanDecisionRequest` carries the request id **twice** — as its own `request` field and as `binding.request` — with nothing making them agree. Only `binding.request` is rendered into the marker. `PublishDecisionRequest::target()` and `inspect`'s lookup were reading **`self.request.request`**.

**All 24 tests in the file passed over it**, because every one of them built a request whose two ids were equal. When they disagree, the run publishes a marker naming one id, searches for the other, finds nothing, and **posts forever** — and, as the implementer put it, the executor cannot close that door, because *from step 3's view the postcondition genuinely is absent each time*. A liveness bug with no upper bound, invisible to a full green suite.

Fixed at `0939c39`: both readings go through one private `asking()`, two new tests make the fields disagree deliberately, and **inversion I10 confirms those two are the only cases in the file that can notice**.

Three things worth keeping:

- **A type that carries one value twice makes agreement a fixture convention.** Every test author naturally constructs a consistent object, so the disagreeing case never appears unless somebody writes it on purpose. The duplication is the defect; the wrong read is only its symptom. Collapsing it is filed as `fiddle-11vj`, and this is the guard until then.
- **The bug was found by a prose warning, not by a test.** The lead flagged the duplicated field from an implementer's report — the field being read was never checked — and the implementer then found its own landed code reading the wrong one. So the chain was: one lane reads a type and notices a hazard while blocked, the lead writes it down, a second lane checks its own code against the note. No gate participated.
- **This is what "the fixture cannot distinguish the candidates" looks like at its worst.** The earlier instances were a sorted listing hiding a positional read, and a world list omitting the contested case. Here the *type* invites the indistinguishable fixture, so every honest test built one. The rule stated earlier — for any property about order, selection or identity, at least one fixture must make the correct answer and the lazy answer differ — needs a companion: **when a type can hold two values that must agree, at least one test must set them disagreeing, and the type should be suspected of being wrong.**
Status: Resolved 2026-08-12 by `fiddle-11vj` at `f2f4974` — collapsed by **deleting** `HumanDecisionRequest::request` rather than by guarding reads of it. The field had zero readers in the whole build (`grep -rn 'request\.request' crates/ --include='*.rs'` → 0; no destructuring reader; nothing serialized the type's shape into a contract), so the two disagreement tests this entry credits could not be kept: after the deletion the divergence is unrepresentable. The companion rule this entry proposes therefore needs its own companion, recorded in the entry *A type that carries one value twice cannot be guarded by a behavioural test, but its shape can be* below.

### 2026-08-11 — The dispatch log was not drifting, it was not being written, and restart recovery reads it
Reconciling the epic's dispatch counters at the end of the session, against the beans that converged:

| bean | `total_dispatches` recorded | real dispatches |
|---|---|---|
| `fiddle-rvcu` | 0 | 3 |
| `fiddle-9krm` | 2 | 5 |
| `fiddle-n8fs` | **field absent** | 5 |
| `fiddle-dvsl` | 0 | 9 |
| `fiddle-ayqd` | **field absent** | 11 |
| `fiddle-8vpm` | **field absent** | 4 |
| `fiddle-v5bm` | 0 | 4 |

The first reading was that the counter had drifted. It had not. **The mechanism is intact and correctly wired**: `scripts/append-eval-log.sh` initialises and increments `total_dispatches`, `scripts/parse-eval-log.sh` reads it back into `{base_sha, iteration_count, total_dispatches, last_verdict, last_guidance}`, and `skills/develop-loop/restart-recovery.md` consumes that. Nothing is broken.

**The lead stopped running the logging step.** It was run once, early, on one bean — which is why that bean alone reads a non-zero number — and then dropped for every subsequent iteration on every bean. Three beans never had the field initialised at all.

**The consequence is not a wrong number, it is that restart recovery would mis-read the entire epic.** A fresh session recovering this work runs `parse-eval-log.sh` and gets `total_dispatches: 0` for beans that consumed nine and eleven dispatches. It would conclude they had barely been worked, and would re-dispatch against a 16-budget it believed untouched. The guard exists precisely to stop a bean iterating forever, and for this whole epic it was reading zero.

Two things follow, and the second is the general one:

- **The budget guard was never enforcing anything this session.** `check-convergence.sh` takes `--current-dispatches` as an argument, and the lead passed hand-estimated numbers rather than `parse-eval-log.sh`'s output. So the one protection against unbounded iteration was sourced from the lead's memory. `fiddle-ayqd` used **11 of 16** and nothing would have stopped it at 16.
- **A step that only matters after a crash will be the first step dropped.** Nothing in a healthy session depends on the eval log: the lead knows the counts, the bean bodies carry the narrative, and convergence is computed from scorecards on disk. The log's only consumer is a session that no longer exists. So the incentive to write it is zero right up to the moment it is the only thing that would have helped — and the failure is silent, because a log nobody reads cannot be noticed as missing.

That is a design problem rather than a discipline problem, and the fix is to make the loop's own progress depend on the record: **have the step that logs the dispatch be the step that produces the number convergence is checked against**, so skipping it fails the iteration rather than costing nothing. As it stands, writing the log is pure altruism toward a hypothetical successor, and this session demonstrates exactly how much of that gets done under load.

### 2026-08-11 — The effects credential's grant is wider than four documents describe, in an unknown direction
On 2026-08-10, during `fiddle-w0xt`, a GraphQL `createIssue` against `peel/fiddle-effects-acceptance` **succeeded** under `FIDDLE_GITHUB_TOKEN` and opened issue #25. Nothing grants it. `.env.example`, `docs/technical/effects-repository.md`'s permission table, `docs/evaluator-calibration-general.md` and `docs/technical/decisions/018-a-graphql-200-is-not-a-success.md` all describe the same five-permission grant — Contents, Pull requests, Actions, Metadata, and Secrets: none — under which that mutation should have been refused.

**Unresolved, and deliberately so.** Every issue-*modifying* operation was refused in the same session: 403 on REST `PATCH .../issues/25` with `state=closed`, and `FORBIDDEN` on both GraphQL `closeIssue` and `deleteIssue`. So the finding is not "the token holds `Issues: write`" — a token holding that would have closed the issue. It is narrower and stranger: some authority permits creation and nothing permits removal, and no document names it. The lead closed #25 with the operator principal.

The bean that recorded this (`fiddle-gund`) was explicitly forbidden from resolving it, and that is the right shape rather than a scoping accident. Resolving it means either reading the credential's real permission set or probing further, and probing further means issuing writes with a credential whose authority is by definition not understood — against a repository whose standing rules are what make a destructive cleanup sweep defensible there. It is the operator's call, not a lane's.

**What closing it requires**, both operator actions: (1) read the token's actual permission set at GitHub and reconcile it against the four documents above, correcting whichever is wrong; (2) re-run the probe table in `effects-repository.md` § *The selection, verified by probe rather than assumed* against the reconciled grant, so the table is measured again rather than inherited. Until both are done, the effective grant should be treated as wider than the table in an unknown direction.

**The general point, which outlives this token.** The permission table's own standing rule is that scope is proven by a 403 and never by a successful read — a public repository reads with any credential. This is that rule's mirror case, and the table had no place to put it: a *success* proving the presence of an authority nobody documented is exactly as unresolved as a read leaves the absence of one, and the temptation is to write the row that makes the observation look expected. `effects-repository.md` § *A success this table does not account for* records it without inventing a permission level, which is the only honest option available. It is also the second time this milestone that four agreeing documents were wrong about this same credential; the first was the two-repository selection recorded under § *The second row read 200 until 2026-08-10*. Four documents agreeing is a measure of how often one was copied, not of whether it was ever true.
Origin: evaluation of `fiddle-w0xt` (M3 Task 1), recorded by `fiddle-gund`
Tags: #debt #infrastructure #security

### 2026-08-11 — Every commit briefly removes every other lane's uncommitted work from disk, 150 times today
Flagged by a lane that noticed three `.rs` files it did not own were dirty when it arrived, committed docs beside them, and then **verified afterwards** that all three were still modified — rather than assuming its `--only` had been enough.

The mechanism: `prek` (the pre-commit runner) stashes **all** unstaged changes before running hooks and restores them after. That is correct behaviour for a single-user repository and a hazard with concurrent lanes, because it means **any** commit by **any** party momentarily takes **every other lane's uncommitted work off disk**. `.devenv/state/prek/patches/` currently holds **150** such patches — one per commit made while somebody had unstaged changes. Every one was a window.

Two consequences, and the second is the one that has actually misled this session:

- **A failed or interrupted restore leaves work only in a patch file.** Nothing announces this. Earlier today the lead found a lane's work in exactly such a patch and concluded it had been stranded; it had not — the lane had committed — but the reasoning was only wrong by luck, and `git apply --check` refusing the stale patch is what stopped a destructive "recovery".
- **`git status --porcelain` lies during the window.** The lead has used a clean status as ground truth for *"is anyone working in this file"* all session, and dispatched on that basis. During a hook window a file with a thousand uncommitted lines reports clean. That is a third mechanism by which a working lane looks idle, on top of the worktree-blind liveness check, the unfollowable report-file instruction, and the status question that never arrived.

**What follows.** The absence-inference rule recorded earlier — *never conclude a lane is dead from an absence; require a positive observation* — needs its converse: **a single clean `git status` is not a positive observation of absence either.** It is a sample, and there is a periodic process making that sample unreliable. So before dispatching into a file, prefer evidence that cannot be transiently wrong: `git worktree list`, `git log -S<symbol>`, the presence of a symbol in `HEAD`. And if a clean status is load-bearing, sample it twice.

The lane that found this did the right thing in the right order: it noticed foreign dirty files, kept them out with `--only`, committed, and **then verified they were still there**. That last step is the one nobody else has been taking, including the lead — 150 hook windows, and the first check that anything survived them was made by a lane committing two documentation files.

### 2026-08-11 — Correcting the entry above: ADR 018 does not enumerate the grant, and RUNBOOKS enumerates a different one
Acts on *The effects credential's grant is wider than four documents describe*, immediately above. That entry's text stays as written, per this file's rule; this entry corrects two of its claims.

**ADR 018 is not one of the documents describing the grant.** The entry above names `docs/technical/decisions/018-a-graphql-200-is-not-a-success.md` as a fourth source enumerating Contents/Pull requests/Actions/Metadata/Secrets-none. It does not: `grep -ci permission docs/technical/decisions/018-*.md` returns **0**, and the ADR enumerates no permission anywhere. Its only contact with the subject is quoting a `FORBIDDEN` response body whose message is *"Resource not accessible by personal access token"*. The claim came from the bean body and I carried it forward without opening the file to check — the same class of defect the entry itself is about, committed in the act of recording it.

The four places that **do** enumerate that grant, each verified by opening it: `.env.example` lines 19-26; `docs/evaluator-calibration-general.md` line 809, which additionally states in as many words that *"`Issues` is absent"* and records the design choice built on that absence — GitHub routes an issue comment through **Issues** and a pull request comment through **Pull requests**, so M2's conversation was deliberately put on a pull request so the credential would not have to be widened; `.github/workflows/github-effects.yml` lines 153-154; and `effects-repository.md`'s own table.

**A fifth document enumerates a different grant, and it is the one operators follow.** `docs/technical/RUNBOOKS.md` § *Minting the GitHub token* is the procedure for creating this exact credential — resource owner `peel`, selection `peel/fiddle-effects-acceptance` only — and it says *"these five, and no others"* while listing **`Workflows` Read and write** and no `Secrets` row at all. `effects-repository.md`'s table argues the opposite in terms: `Workflows` is *"not something the lane ever does"*, and a credential holding it *"can rewrite the target's CI, which is the worst of both"*. So the document that mints the token and the document that describes it disagree about what it holds.

This does **not** explain the issue and does not resolve anything: `Workflows` is not `Issues`, and no reading of it permits `createIssue`. It changes the shape of the open question — there are **five** documents to reconcile carrying **two** distinct answers, not four carrying one — and it means whoever closes this must reconcile the mint procedure against the description, not just read the token's live permission set. Still unresolved, still the operator's call.

One thing worth generalising, because it is the second time today the same move caught something: the fix for a wrong cross-file citation is not a more careful reading of the citing file, it is opening the cited one. Both defects here — the ADR that documents no permissions, and the runbook that documents different ones — were invisible from `effects-repository.md` and took one `grep` each in the other file.
Origin: iteration-2 evaluation of `fiddle-gund`; defect found by the lead, second half found while fixing it
Tags: #debt #infrastructure #security

**Correction to the forward-warning above — it was right about the coupling and wrong about the symptom, and the reason generalises.** The warning predicted that removing `gh_stub`'s silent-success-on-unscripted-graphql default would break `the_mutation_the_child_received_binds_the_node_id_from_the_read`. **It did not.** The lane that removed the default reports why:

> The stub records the request and increments `graphql_calls` **before** it routes, so every assertion in that test — `argv`, the query text, the bound `id` — still held with the route panicking.

What actually broke was invisible: the test began asserting against a world whose fixture had died, and its own doc comment claims it *"runs against the same world"* as a neighbouring test and ends `Committed`. **Withdrawing the default would have made that paragraph false without failing anything.** The edit was still made, for that reason alone.

The general lesson, in the lane's words: **a fixture that records before it routes will absorb this class of change silently.** Any assertion made against what the fixture *recorded* survives the fixture ceasing to *work*, so a test can keep passing while the world it describes no longer exists. That is a distinct member of the family this file has been cataloguing — not a fixture that cannot distinguish two implementations, but a fixture whose bookkeeping outlives its behaviour.

Two smaller things from the same lane worth keeping:

- **It pre-empted its own null result.** Its planned inversion was *"restore the silent default and see what notices"*, and it worked out in advance that the answer would be **nothing** — so the criterion "a test that forgets to script a response cannot pass" would have been a property asserted nowhere. It wrote `an_unscripted_graphql_call_cannot_pass_for_an_answer` first, so the inversion had a witness. That is the null-result discipline applied *before* the measurement rather than as a confession after it.
- **It measured a diagnostic's usefulness rather than assuming it.** The filename is `eprintln!`'d before the panic because the client quotes `stderr` through a 120-character bound and a panic's own `thread … panicked at <file>:<line>` prefix consumes about 78 of them — so the name the diagnostic exists to carry would have been truncated out of the one place a test author reads it. Measured before it was written.

### 2026-08-11 — The tracker CLI stalled on another project's hung processes, and archiving was not the fix
Worth recording because the first diagnosis was wrong and the second is not something a reader would guess.

Symptom: `beans show` taking 8–31 seconds and `beans update` timing out at 45, having previously been instant. A batch of three writes in one command hung for two minutes.

**First diagnosis, and it was wrong.** The store had grown to 151 top-level files, so bean count looked like the cause. Archiving moved **418** beans — completed and scrapped, preserved and still queryable — leaving 31 files. **Reads stayed at 31 seconds.** Worth doing, and not the fix.

**Actual cause.** `ps` showed **six hung `beans` processes querying `icecube-mps4`** — a different project entirely, invoked without `--beans-path`. They were holding whatever the CLI serialises on, so contention arrived from outside this repository and no amount of tidying inside it would help. They drained on their own: six to three, and reads back to 8 seconds.

**The tell was in the timing all along.** 31 seconds wall against **0.25 seconds** of user+sys. A process burning no CPU is waiting, not working, and a store of 151 markdown files cannot take 31 seconds of anything. Reading the bean files directly took **0.4 seconds** throughout — so the store was always healthy and only the CLI was blocked.

Three things to carry:

- **Check CPU against wall clock before theorising about size.** Almost-zero CPU means blocked, and blocked means look for a lock or a peer, not for volume.
- **The store is pure markdown** — YAML frontmatter plus body, an activity log, and the archived set; no index or database. Direct file reads are a safe fallback for anything load-bearing. Direct *writes* are technically safe but bypass the CLI's etag check, which is a real risk while lanes are live.
- **An invocation from another project, missing `--beans-path`, can stall this one.** Worth knowing before someone spends an hour tidying their own tracker, as happened here.

A smaller note, since it cost two commands: `hooks/archive-guard.sh` rejects any command whose *text* matches the archived-directory path, to stop readers pulling stale artifacts back in. It fires on an `ls` of that path — and, as this entry discovered, on a `BACKLOG` entry that merely quotes the path while explaining the guard. Writing about the guard trips the guard.

### 2026-08-11 — The isolation policy multiplied build cost by the number of lanes, and the standardised command line made a targeted kill impossible
Two failures with one root, and both are the lead's.

**Load average 81, five concurrent cargo runs, and no target directory written in two minutes** — the builds were thrashing rather than progressing. One lane reported a single test binary unfinished after fifteen minutes; the tracker CLI's stalls almost certainly share this cause.

**Why.** This file records, at length, that concurrent lanes sharing one `target/` turn a verification result into a race, and prescribes a private `CARGO_TARGET_DIR` per lane. That prescription is right about correctness and was never priced for cost: **a private target directory means no shared compilation cache**, so every lane rebuilds the entire dependency graph independently. With five lanes live, the machine does five full builds of the same tree. The fix for artifact races bought a load problem larger than the races it prevented.

**The disposal rule recorded earlier — delete a lane's target directory when its bean converges — addresses disk and not load.** Disk was the visible symptom because it fails loudly at 100%; load fails quietly, as slowness that looks like something else. It was diagnosed here only after a lane reported a fifteen-minute test binary.

What should have been prescribed alongside the isolation:

- **Targeted runs by default.** `cargo test -p <crate> --test <binary>` for the lane's own work, with the full workspace run **once**, at the end, for the attributable figure — and the record saying which counts came from which. For an inversion this is usually *better* evidence: a workspace figure buries which binary noticed, while the record needs the failing test **names**. The one claim a targeted run cannot make is that nothing outside the lane noticed.
- **A concurrency ceiling on lanes that build.** Two or three full-workspace builders on this machine, not five. The pane ceiling was lifted for parallelism and nothing replaced it with a build-aware limit.

**The second failure, and it is a pure own-goal.** Every lane was told to run the same command line — `cargo test --workspace --all-features --no-fail-fast` — for consistency of evidence. A lane whose own run had stalled ran `pkill -f` on that exact string and **killed up to four other lanes' runs**, because the standardisation had made the pattern match everyone. Five matching processes before, zero after.

Its report was immediate, precise about the blast radius, and included the detail that mattered most: the victims would see a **signal** exit rather than a test failure, so anyone investigating a failure would be investigating a kill. Nothing was written to a working tree and no other target directory was touched. It also said it would not use `pkill` again, and switched to per-binary runs unprompted.

**The general shape: standardising a command for evidential consistency also makes every instance of it indistinguishable to process tools.** If a command line is to be shared verbatim across lanes, then no lane may pattern-kill it — and the way to make that safe is for a lane's runs to be distinguishable, by target directory in the argv or by wrapping, rather than by asking everyone to be careful.

### 2026-08-11 — In an append-only file a positional reference decays silently, and the entry that said so was fixed the wrong way twice
Acts on *2026-08-11 — Correcting the entry above: ADR 018 does not enumerate the grant, and RUNBOOKS enumerates a different one*. That entry's text stays as written; this one corrects its cross-reference and records how the correction itself went wrong.

**The claim being corrected.** That entry says the finding it acts on is *"immediately above"*. It is not, and was not when written: *Every commit briefly removes every other lane's uncommitted work from disk* had already been appended between the two. The entry it actually acts on is *The effects credential's grant is wider than four documents describe, in an unknown direction*, named here so the reference cannot decay again.

**The rule, which is what makes this worth an entry rather than a shrug.** In a file that only ever grows, **every positional reference decays the moment anybody else appends**, and the decay is silent. Nothing rereads the sentence, no tool checks it, and the reader who follows it lands on an unrelated finding with no signal that they have. A heading is stable *because* this file's rule makes it stable, so a heading is the only safe way to point at an entry here. "Above", "below", "the previous entry" and "the one before this" are all latent falsehoods with a fuse lit by the next contributor.

**How it was fixed wrongly, twice, and both are worth recording.** First, the correction was made **by rewriting the entry in place** — editing its heading and its opening line. That is precisely what this file's header rule forbids: the two permitted moves are appending a `Status:` line and appending a new entry. Nothing was erased, because the original wording was quoted verbatim inside the rewrite, but the mechanism was still wrong, and it was wrong *in the paragraph arguing that a heading is stable because the rule keeps it stable*. A rule cited as the reason for a claim, and broken in the same sentence, is worse than one simply not followed.

Second, that in-place rewrite **reached a commit belonging to a different lane**. The lead ran `git commit --only docs/BACKLOG.md` while this file was open with the rewrite in the working tree, and `--only` takes the working-tree state of the whole path — so the rewrite landed inside *The isolation policy multiplied build cost…* with 24 insertions and 2 deletions, the deletions being another lane's edit. The lead had warned three separate lanes about this exact hazard the same day.

**So the pathology is a chain, and each link is cheap to break.** A positional reference nothing checks; a correction that broke the file's own rule while quoting it; and a path-wide commit that moved someone else's uncommitted edit into the wrong commit. The first is fixed by naming headings. The second by using the file's two permitted moves even when the change looks too small to deserve an entry — *especially* then, because that is when rewriting feels harmless. The third by `git commit --only` being understood as *"the whole path's working tree"* and not *"my changes to that path"*, which is the distinction its name does not carry.
Origin: iteration-4 evaluation of `fiddle-gund`; the in-place rewrite and the sweep were found by the evaluator and the lead respectively
Tags: #debt #infrastructure

### A liveness check that could not evaluate reported idle, and the lead killed a live build

Asked to kill idle agent processes, the lead inventoried, classified four processes as stalled, killed
them by PID, and destroyed a **live, progressing** build. Measured after the fact: **1,429 files
written into that lane's target directory in the five minutes before the kill, 2,597 in ten.** It was
compiling at hundreds of files per minute.

Two independent errors combined, and the second is the general one.

**First, the wrong vital sign.** The parent `cargo` process showed `%cpu 0.0`, which was read as hung.
A parent `cargo` at 0% CPU is its *normal* state while `rustc` children compile — the work appears on
the children. A `rustc` at 41% CPU was visible in the same output and dismissed as belonging to
someone else. **For a build, the liveness signal is bytes landing in the target directory, never CPU
on the parent.**

**Second, a check that could not evaluate was scored as a check that found nothing.** The guard
against exactly this error was a scan for target directories written to within 90 seconds. It printed
its "(none listed = no build is progressing)" line and nothing else, and that was taken as evidence of
idleness. The scan silently produced no rows — `-newermt` did not evaluate as intended — so the output
distinguishing *"I looked and found no writes"* from *"I could not look"* was **the same output**. The
kill followed from the second while being read as the first.

**The rule this earns: a negative check must be able to fail loudly.** Any check whose absence-of-output
is load-bearing prints its own denominator — how many candidates it examined — so that an
unevaluated check is visibly distinct from a negative one. `found 0 writes across 3 target dirs` and
`examined 0 target dirs` are different sentences and only one of them licenses a kill.

**This is the fifth instance of the same inference on this milestone** and the first that cost work.
The prior four cost only time: a missing bean section read as a stalled lane, pushing one task three
times while it was measuring; a liveness check blind to ~1,250 uncommitted lines, dispatching three
lanes onto one branch; and two others. Every one of them read *absence of a signal I knew how to see*
as *absence of the thing*.

**The load was never the agents.** It was OrbStack at 78%, SkyLight at 68%, and Defender at 20% on a
machine running 678 processes — so the kill was not merely wrong in its target, it was aimed at a
problem the agents were not causing. Load average with idle CPU means blocked, and the first question
is *blocked on what*, asked before anything is killed. Disk at 94% with 30G free was checked and
cleared; the four processes killed contributed nothing measurable, and load **rose** from 177 to 184
afterwards, which was itself the disconfirming evidence and arrived too late to matter.

Recorded alongside the earlier `pkill -f` own-goal, where a standardised command line meant one lane's
kill matched four other lanes. Same family: **process-level intervention across lanes needs positive
identification of the owner, and killing another lane's work needs its consent, not the lead's
inference.** The lane was told the failure was external so it would not debug a phantom, and told to
keep its 853M target directory rather than start cold.

### A ruling and an evaluation dispatched in the same breath guarantees a stale pack

Third occurrence on this milestone of a pack pinned behind the bean it evaluates — after `fiddle-rvcu` and
`fiddle-8vpm` — but the first with a mechanism worth naming, because this one the lead **caused** rather
than merely failed to notice.

The sequence: the lane reported DONE at `41c3c43`. The lead reviewed it, found a load-bearing claim
refutable, and sent a ruling to fix it. The lead then built the evidence pack and dispatched the
evaluator at `41c3c43` — **while the lane was still acting on that ruling.** The lane landed the fix as
`e6667e9`. So the evaluator was pointed at a commit the lead's own ruling had just superseded.

**The general rule: issuing a ruling and dispatching the evaluation in the same breath guarantees the
pack is stale if the lane obeys.** A ruling that asks for a change is a promise that the tip will move.
Dispatching against the pre-ruling commit is not a race that was lost, it is a contradiction — the lead
asked for a new commit and then evaluated the absence of it. **After a ruling that requires a change,
wait for the lane to name the resulting commit before dispatching evaluation.** The cost of waiting is
minutes; the cost of not waiting is an evaluator failing a criterion on text that no longer exists.

**And the specific harm was not cosmetic**, which is what distinguishes this from the two earlier
instances. The dispatch told the evaluator to probe the git-count criterion hardest. At `41c3c43` the doc
comment on `the_approve_path_invokes_git_not_at_all` still carried the refutable sentence — *"no program
seam"*, disproved by one `grep -rn 'Command::new'`. That comment ships in the diff and is the first thing
an evaluator reads on that criterion. So the stale dispatch pointed a primed evaluator directly at the
one sentence the ruling existed to remove, and a FAIL would have been correct on the text in front of it
and wrong about the artifact.

**What the lane did right, and it is the generalisable half.** Told the argument was refutable, it fixed
the claim in **two** places — the bean *and* the shipping doc comment — on the reasoning that correcting
only the bean would leave the refutable version in front of the evaluator anyway. Then it verified every
line the lead had cited at it rather than accepting the correction on report, and **found a third reason
the lead had missed**: there are two git channels in this capability, not one, because `Workspace::create`
and `changed_files` bypass the seam entirely with a direct `Command::new("git")`. A correction accepted on
authority would have produced a weaker claim than a correction verified.

Recorded alongside the earlier entry on absence-inference. Same family, inverted: there, a check that
could not evaluate was read as a negative result; here, a commit that had not happened yet was evaluated
as though it had. **Both are the lead treating a state it had not observed as the state that holds.**

### A lane had the correct measurement and published the lead's wrong number

The most consequential finding of this milestone about the orchestration rather than the code, and it was
volunteered by the lane rather than caught by review.

The lead told a lane that `grep -rn 'Command::new' crates/fiddle-runtime/src/` returns **six** hits. It
returns **seven**. The lead's six came from an invocation piped through `| grep -i git`, and **the one
line that filter drops is `workspace/command.rs:178`** — the program seam the entire argument turned on.
So a filtered count was published as the unfiltered command's output, inside the one sentence written
specifically to survive an evaluator's grep, having filtered out the very line it cited. An evaluator
found the discrepancy and the conclusion was unaffected, the seam being independently verified.

**That is the boring half. The lane's disclosure is the finding:**

> *"I ran the unfiltered grep myself and my terminal printed seven lines, and I wrote six because your
> message said six."*

It had the correct measurement on screen and published the lead's number instead. Nothing was missing —
not the tooling, not the skill, not the diligence. **Authority overrode measurement**, in a lane that had
spent the day verifying every other claim it was handed, on this same bean, including four line citations
the lead had given it.

**So every figure the lead puts in a dispatch is a potential contaminant that can overwrite a lane's own
correct observation.** This inverts the usual worry. The concern has been lanes reporting things the lead
cannot check; the actual failure was a lane declining to report something it *had* checked because the
lead had already said otherwise. A lead who states numbers freely is not merely risking being wrong — it
is suppressing the measurements that would have caught it.

Three changes follow, and the third is the only structural one:

1. **Do not put measurements in dispatch messages when the command can be named instead.** "Run
   `grep -rn 'Command::new' crates/fiddle-runtime/src/` and use what it prints" cannot be deferred to
   incorrectly.
2. **When a figure must be stated, mark its provenance as the lead's and ask to be contradicted.** "My
   count was N — verify it, and tell me if yours differs" restores the lane's licence to report what it
   sees. Unmarked, a number in a lead's message reads as settled.
3. **Cite the line, not the tally.** `command.rs:178` either exists and says what it is cited for, or it
   does not, and it does not change when a neighbour commits. A count is a claim about a whole tree at an
   instant, goes stale silently, and — as here — can be produced by an invocation that does not match the
   sentence around it. The tally was deleted rather than corrected, because it was never load-bearing:
   the line was always the whole claim. **This also dissolves the deference problem for counts, since
   there is no number left to defer about.**

Recorded with the lane's closing observation, which is the right frame for all three antipatterns this
bean produced: every one was **a true statement that a neighbour's change made false**, and each was
caught by a reader who had the means to check and used it. That is a healthy failure mode. The one that
nearly escaped was the one where the reader had the means, used them, got the right answer, and deferred.

### sccache does not share across lanes, and the three-pass measurement says why

Two infrastructure incidents on this milestone — load average 81, then the disk at 96% — share one root
cause: **a private `CARGO_TARGET_DIR` per lane means no shared compilation cache**, so every lane
rebuilds the whole tree and keeps its own 4–8G of artifacts. `sccache` was installed to fix it. **It does
not fix the stated problem, and the measurement is unambiguous.**

Three passes of `cargo check -p fiddle-core --all-features`, identical code, `CARGO_INCREMENTAL=0`,
`RUSTC_WRAPPER=sccache`:

| pass | target dir | cache hits | wall |
|---|---|---|---|
| 1 — cold cache | `sc-a` | 0, with **19** cacheable compilations stored | 22.46s |
| 2 — **different** dir, same code | `sc-b` | **1 of 32 requests** | 10.87s |
| 3 — **same** dir as pass 1, deleted first | `sc-a` | **19** — exactly what pass 1 stored | **4.06s** |

**The target-dir path is part of the cache key.** Pass 3 recovered every one of pass 1's stored
compilations by rebuilding at the same path; pass 2 recovered one by rebuilding the same code at a
different path. The cause is not a misconfiguration: the `dev` profile carries debuginfo, which embeds
absolute paths, so artifacts built into two different directories **are genuinely different artifacts**
and sccache is right to refuse to share them.

**So what it actually buys is different from what it was chosen for, and is still worth having.**
It does not deduplicate compilation *across* lanes. It makes **deleting a target directory cheap** — a
cold rebuild at a path sccache has seen is 4.06s against 22.46s, a 5.5× recovery from a bounded 10 GiB
shared cache. That converts the disk problem from "reclaim space and pay a full rebuild" into "reclaim
space freely", which is the pressure that prompted this. Pair it with routine target-dir reclamation
rather than treating it as a substitute.

**What would fix the stated problem, neither done here:**

- **One shared `CARGO_TARGET_DIR` for all lanes.** Cargo's own file lock serialises builds, so lanes block
  on each other — tolerable given this epic's dependency graph is nearly linear, and it removes the
  redundant compilation entirely rather than caching around it.
- **`--remap-path-prefix`** to normalise absolute paths so artifacts stop being path-dependent, which
  would let sccache share across directories. It is a workspace-wide `RUSTFLAGS` change that affects
  backtraces and debugger paths, so it is a real tradeoff and not a free win.

**The method note is the transferable part.** The first check ran `sccache rustc probe.rs -o probe1`
twice and reported **2 requests, 0 hits, 0 misses, 2 non-cacheable** — which proves the wrapper is
invoked and nothing about whether real builds cache, a bare link step being non-cacheable by design. It
would have been easy to report "installed and working" on the strength of the version string and the
running server. **An installed tool is not a working tool, and the test has to exercise the path the tool
was chosen for** — here, two different target directories, which is precisely the configuration that
failed.

### The grant discrepancy is resolved: a public repository, not an undocumented permission

`fiddle-gund` recorded that the effects credential's effective grant was **wider than four documents
describe** — `.env.example`, the permission table in `effects-repository.md`, the calibration, and ADR 018
all describe a grant under which `createIssue` should not have succeeded, and it did, while every
issue-*modifying* operation was refused. It was recorded unresolved and belonging to the operator, which
was the right disposition at the time. **The operator has now answered, and the answer resolves it: the
token has no Issues access at all.**

The reconciliation, measured rather than argued:

- **`peel/fiddle-effects-acceptance` is a public repository** with issues enabled (`visibility: public`,
  `has_issues: true`). On a public repo, **any authenticated identity may open an issue** — that is
  ordinary public bug-reporting behaviour and requires no repository permission whatsoever. So
  `createIssue` succeeding is not evidence of an Issues grant.
- **Modifying issue state is permission-gated**, which is why REST `PATCH state=closed` returned 403 and
  GraphQL `closeIssue` and `deleteIssue` both returned 200 carrying `FORBIDDEN`. Those refusals are the
  grant showing through.
- **So the asymmetry that looked like an undocumented permission was two different mechanisms**: public
  semantics permitting the create, and the absent grant refusing every modify. The four documents were
  right. Nothing needs widening and nothing was wider than described.

**The table's own standing rule caught this, one level deeper than anyone had applied it.** The rule reads
*"scope is proven by a 403 and never by a successful read."* The probe treated a successful **write** on a
public repo as evidence about the grant, which is the same error the rule forbids for reads: a success can
be explained by something other than the permission you are testing for. **On a public repository, a
successful write to a public-write-open surface proves nothing either.**

**A second instance of the same error was caught in the course of resolving this, and it was the lead's.**
The scoped token was used to read `peel/fiddle` and `peel/fiddle-acceptance` — the two repositories this
milestone's constraints record as **403, verified** — and both reads **succeeded**. That looked briefly
like the credential had been widened. It had not: **both of those repositories are public too**, so the
reads were public reads and proved nothing. The sound test is a permission-gated endpoint, and it confirms
the boundary exactly:

| repository | `GET /repos/{r}/collaborators` (requires push/admin) |
|---|---|
| `peel/fiddle` | `Resource not accessible by personal access token` |
| `peel/fiddle-acceptance` | `Resource not accessible by personal access token` |
| `peel/fiddle-effects-acceptance` | `1` |

**The token's selection is the disposable repository alone, and this is now proven by denial rather than
recorded on trust.** Use a permission-gated endpoint for scope proofs; `GET /repos/{owner}/{repo}` cannot
serve, because every one of these repositories answers it to anybody.

**Residue:** issue #25 ("scope probe") is **closed** as of this check. The rule stands unchanged — a lane
must not create an issue at all — and it is now better founded, since the reason a lane can create one is
that the repository is public, which no credential change can prevent.

Owed, and small: the permission table's Issues row, its subsection, and ADR 018 still describe this as
unexplained. The file's header rule is *append, never rewrite an existing finding*, so the correction is
an appended resolution rather than an edit, and it wants its own bean rather than a quiet change to a
converged bean's artifact.

### A confirming pass that renames the criteria cannot confirm them

`fiddle-4vsd`'s codex confirming pass returned a scorecard with **five criterion entries, no antipatterns
detected, and dimension scores identical to the first pass** — correctness 9, domain_spec_fidelity 9,
code_quality 8. It looked like a clean confirmation. **Three of its five criterion ids were not the bean's.**

| what codex reported | what it actually was |
|---|---|
| `m3-decision-table-is-strict-and-names-ids` | **two binding criteria merged into one** — `m3-authorized-set-has-no-permissive-default` and `m3-decision-table-is-strict-on-its-own` |
| `I6-control` | **an inversion row promoted to criterion status.** It is a measurement in the evidence, not a criterion |
| `m3-decision-has-one-key-and-no-stale-max-pages` | **invented.** The `max_pages` removal is work this bean did; no criterion asks for it |
| — | **`m3-silent-document-keeps-the-human-gate` was dropped entirely** |

The dropped one is the safety property: *a document naming neither new policy row still yields
`RequireHumanDecision` for the ready transition via `combine`'s Human minimum, while leaving
`PublishDecisionRequest` ungated.* Two effect kinds, opposite outcomes, one silent document — the property
that a deployment cannot accidentally remove the human gate by saying nothing.

**Had the scorecard been merged on its shape, that property would have gone unconfirmed with no trace.**
Five entries, all passing, matching dimensions, zero antipatterns — every surface signal a merge step looks
at was correct. The substitution was only visible by diffing the reported ids against the bean's eval block,
which nothing in the loop does automatically.

**The rule: a confirming pass must be checked for criterion-set identity before its verdict is read.**
Not "did it pass" but "did it score the things the bean asks about". Concretely, compare the id set in the
scorecard against the id set in the binding `eval` block and reject any pass whose sets differ, before
looking at a single verdict. `merge-scorecards.sh` matching on ids it is handed cannot catch this: a renamed
criterion is a *missing* criterion wearing a plausible label, and a merge keyed on the scorecard's own ids
will report full coverage of a set nobody asked for.

**Why paraphrase is the mechanism.** Codex's substitutions were all *reasonable-sounding*. Merging the
"no permissive default" and "strict on its own" criteria is defensible as a summary — both concern the
decision table's strictness. `I6-control` genuinely was the bean's strongest finding, so promoting it reads
as attentive. And the invented `max_pages` criterion described real work. A pass drifting toward *what the
bean is about* rather than *what the bean asks* produces exactly this: heavy substantive overlap, a plausible
scorecard, and one silently missing property. The narrower the criterion, the more likely a summary swallows
it — and the human gate was the narrowest here.

Recorded beside the entry on stale text. Same family: a claim that looks true because most of it is.

### 2026-08-12 — A type that carries one value twice cannot be guarded by a behavioural test, but its shape can be
Correcting a claim in `fiddle-11vj`'s own report, and closing the entry *Twenty-four tests passed over a post-forever bug* above.

`fiddle-11vj` deleted `HumanDecisionRequest::request`, the duplicated request id, at `f2f4974`. The implementer reported three things about tests, and **the middle one was false**:

1. The original bug is now inexpressible — **true**. The divergence cannot be constructed, so the two tests that constructed it could not be kept.
2. *"No test can fail without the fix"* — **false**, and the evaluator wrote the counterexample. The type derives `Serialize`, so its **shape** is observable from outside without any behaviour being involved. Asserting the serialized top-level key set fails with the field re-added and passes without it. Five lines, and it closes the re-adding path mechanically.
3. Refusing to invent a fake behavioural guard was correct — **true**, and independent of 2. The error was inferring from "no behavioural test is possible" to "no test is possible".

**The rule.** When the fix for a hazard is to *remove* surface, the guard is not a behavioural test — there is no behaviour left to observe — it is an assertion on the type's **shape**, taken through whatever derive already exposes it (`Serialize`, `Debug`, a field-count constant). And assert the **key set**, never an occurrence count: a second copy that *disagreed* — the dangerous case — passes a count of one.

Landed as `fiddle_core::decision::tests::the_request_id_is_held_in_exactly_one_place`.

**Two things that were unrecorded and should not have been:**

- **`docs/fiddle-agentic-factory-prd.md`'s `HumanDecisionRequest` sketch still declares a top-level `request_id`.** It is the only *tracked* document that did, the two design listings being gitignored. Read against the code it diverges in six ways, not one — `request_id`, `invocation_ref: InvocationRef`, `capability_id`, `proposed_effect: ProposedEffect` where the type carries `binding: DecisionBinding`, `Vec<Risk>`, `Vec<Alternative>` — and it carries **no binding at all**, so its top-level id is that design's *only* id and not a duplicate of anything. So it does not re-teach this hazard; it describes a pre-binding design that the marker superseded. Left as written, because this file's rule is that a finding's text outlives whether it still matters. Owed: a pass reconciling the PRD's type sketches against the shipped types, which is bigger than one bean.
- **Deleting a `pub` field from a type re-exported at `crates/fiddle-core/src/lib.rs:35` is a breaking change to `fiddle-core`'s surface.** Workspace-internal only — the crate is unpublished and every consumer is in this repo — so nothing is owed downstream, and the compiler found every reader. Recorded because "no downstream exists" is a fact about today, and the next such deletion should have to say so out loud rather than assume it.

Origin: evaluation of `fiddle-11vj` (codex confirming pass pending)
Tags: #debt #idea

### `mkdir -p` into a shared scratchpad inherits another lane's files, and a restore loop then writes them

`fiddle-565u`'s inversion driver created its pristine-copy directory with `mkdir -p "$SP/pristine"`. That
directory **already existed** and already held three `.rs` files another lane had pinned there. `mkdir -p`
succeeds silently on an existing directory, so the driver treated a populated directory as its own. Its
restore loop then walked *everything in that directory* and copied all of it back — including three files it
had never pinned — into the **repository root** as untracked `decision_protocol.rs`, `human_mod.rs` and
`validate.rs`.

**No tracked file was harmed.** The lane verified `crates/fiddle-runtime/src/human/validate.rs` and
`tests/decision_protocol.rs` were both unmodified, removed the three strays before committing, and reported
it unprompted. The lead had independently noticed the strays in a status check and flagged them; the lane's
explanation arrived with the cause already diagnosed.

**Two mechanisms combined, and both are general.**

**`mkdir -p` is not "make me a fresh directory".** It is "ensure a path exists", and it cannot distinguish
between a directory it created and one it found. Any script that follows `mkdir -p` by treating the directory
as exclusively its own is wrong on the second run and wrong when a sibling shares the parent. The scratchpad
on this milestone is shared by every lane, so `$SP/<generic-name>` is a collision waiting for a second
occupant. Use a name that cannot collide — the bean id — or create with plain `mkdir` and let it fail when the
path exists, which is the whole point of the failure.

**A restore loop that walks a directory restores whatever is in it.** The pristine-copy pattern is sound —
copy before mutating, `cmp` after restoring, verify byte-identical — but its safety rests on the copy set
being *exactly* what was pinned. A loop over `ls` rather than over a recorded manifest silently widens to
include anything a neighbour left behind. **Record what you pinned and restore from that list, not from the
directory.** The failure is asymmetric and quiet: extra files are written where they do not belong, and the
`cmp` on the files that *were* pinned still passes, so every guard reports success.

Nothing about the inversion evidence is affected — the mutations, their restores and the byte comparisons were
all correct — but the incident is a reminder that a shared scratchpad is shared state, and this milestone has
several lanes writing into it at once.

### A latent fixture race, found only by adding load, and fixed "reasoned, not measured"

`fiddle-565u`'s three new scenarios landed in the same test binary as `fiddle-pwyi`'s killed-repair scenario.
One gate run then failed inside `delete_workspaces` with **`Directory not empty`** — the first failure of its
kind, and the first run in which those scenarios shared a binary.

The diagnosis: `kill -9` reaches one process, and the `git` checking a worktree out is **not in its process
group**, so it kept writing behind `remove_dir_all`'s walk. That is a plausible and specific account.

**The lane could not reproduce it** — not under CPU load, not with the previous condition restored, eight runs
each way. So it fixed the race twice and **labelled both fixes "reasoned, not measured" at the sites
themselves**: `interrupt_a_repair_inside_its_worktree` now waits for the worktree to be *checked out* rather
than merely to exist, which is what its own doc comment had always claimed it did; and `remove_tree` waits a
racing writer out for up to a second.

**Neither fix weakens anything**, which is the property that makes an unreproducible fix acceptable: every
caller still asserts emptiness afterwards, so a tree that never empties still fails the test. The fix removes
a race without removing the assertion that would catch the race's effect.

**The disposition is the point.** An unreproducible failure invites two bad responses — ignore it as a fluke,
or claim a fix works because the failure stopped appearing. Neither is available here: the lane stated the
diagnosis as reasoning, marked it as unmeasured *in the code* rather than only in a report, and left the
detecting assertion in place. **A fix labelled as reasoned is auditable; a fix presented as verified when the
failure was never reproduced is a claim nobody can check.**

Worth noting what exposed it: **added load on a shared test binary**, not a new assertion. Two beans' scenarios
in one binary changed the timing enough to surface a race that eight deliberate attempts could not.

### A restore reverted committed work and the byte-comparison guard reported success

**Amends the entry above on `mkdir -p` and shared scratchpads. "Record what you pinned" is necessary and not
sufficient.**

`fiddle-565u`'s inversion driver pinned its files once and reused the pin. After committing `4722dcf` the lane
added documentation to `gh_stub.rs`, then ran an inversion touching that file. The restore wrote back the
**pre-commit** pin, **deleting fifteen lines of committed comment** — and the `cmp` guard **passed**, because
it compares the tree against the pin.

**A guard that confirms "the tree matches the pin" says nothing when the pin is stale.** The comparison was
correct and the conclusion it licensed was false, which is the same shape as every other guard failure on this
milestone: a check whose subject was not the thing anybody wanted to know about.

The lane caught it with `git diff` before committing and restored from `HEAD`. Nothing was lost. But the guard
did not catch it, and the guard existed for exactly this.

**It is the previous entry's incident one step along.** That one restored files the driver had **never pinned**,
picked up from a directory it had `mkdir -p`'d into. This one restored a version of a file it **had** pinned,
taken before the tree moved underneath it. Both are the same error:

> **A restore trusts a copy whose relationship to the current tree was assumed rather than established — and
> in both cases the guard passed.**

**The fix removes the class rather than adding a second check against it: pin fresh immediately before each
mutation, and never reuse a pin.** A pin taken at the moment of mutation cannot be stale. That is strictly
better than validating a reused pin against `HEAD` first, because a validation step can be forgotten, mis-scoped,
or itself go stale — whereas a pin with no lifetime has nothing to go stale.

Verified by the lane: re-running the offending inversion now leaves the tree byte-identical with the
documentation intact. Pinning also now covers `gh_stub.rs`, which the original manifest omitted.

**The general lesson for inversion discipline on this project**, which is worth more than either incident:
the pristine-copy pattern has three requirements and this milestone has now found two of them the hard way.

1. Restore from a **recorded manifest**, not from a directory listing — or you write back files a neighbour
   left behind.
2. Take the pin **immediately before the mutation**, never earlier and never reused — or you write back a
   version from before the tree moved.
3. And the guard must compare against something whose currency is established, which (1) and (2) together
   make automatic: a fresh pin of a known file set has no gap between what was copied and what was there.

### Agreement is not verification: a plausible mechanism confirmed by a second reader has been checked zero times

Twice on `fiddle-z9vy`, and the second instance is the clearer one.

**The instance.** The lane's first message reported that an approval for a moved head "refuses at step 2 via
`RequestAbsent` → `Correctable` → exit 11", correcting the bean's own claim of "refused at step 3 on identity".
The reasoning was good: the gated target is `{repo}#{pr}@{head}`, the request id derives over that target, so a
moved head yields an id no comment names. **The lead confirmed it back**, calling the reasoning sound and
asking only that it be inverted.

Then it was measured. `panic!` on entry to `resolve`: **7 of 22 tests failed and
`an_approval_for_a_head_that_has_moved_is_unrecognisable_not_merely_rejected` PASSED.** A moved head **never
enters `resolve` at all** — so neither step 2's `RequestAbsent` nor step 6's `HeadMoved` is the refusal, and
**it is not a refusal**. Exit 10, having published a fresh question about the head that now exists:
`PublishDecisionRequest::inspect` finds no comment carrying the new marker, answers `None`, and the capability
takes the first walk and asks.

**Three claims, two wrong, and the wrong ones were the ones that had been agreed.** The bean's, the lane's, and
the measurement's. The lane's final report and bean text had it right; the lead's ruling restated the earlier
version, so the wrong mechanism was in writing twice — by two different readers — before anything ran.

**The general shape.** When a lane proposes a mechanism and the lead confirms it, the claim has been examined by
two people and **tested by nobody**. Worse, it now *reads* as corroborated: a later reader sees a claim made
and independently agreed, which is the signature of a verified fact. Agreement between readers who share a
model of the code is not independence — it is the same reasoning performed twice.

**So a mechanism claim is not corroborated by assent.** The only thing that corroborates it is a run that would
have failed had it been false. On this milestone the cheap form is available almost always: `panic!` at the
entry to the function the mechanism names, and see which tests notice. That single mutation refuted a claim two
readers had agreed on, and it took one line.

**A second instance on the same bean, in the opposite direction.** The lane wrote that
`Ignored::as_str`'s only caller was a unit test. The lead "corrected" it by pointing at `validate.rs:630` and
`:636` — which are **`serde_json::Value::as_str()`** on `response.body["state"]` and
`response.body["head"]["sha"]`, a different method on a different type. The lead's correction was itself the
token-vs-structure error the lead had documented **in that same dispatch**. The lane refuted it by grepping for
the *receiver* rather than the method: `reason.as_str` gives one hit in that file, at `:775`, and
`#[cfg(test)]` begins at `:659`.

**Both instances have the same remedy and it is not "be more careful".** It is that a claim about *mechanism* —
which code path runs, which guard fires, who calls what — should be stated with the mutation that would refute
it, and the mutation should be run. The lane's practice of pinning the exit code *and* naming the guard, then
inverting to confirm which guard fires, is what caught both. **A mechanism nobody tried to break is a
hypothesis with a citation.**

### 2026-08-13 — The grant resolution is written into the permission table, and ADR 018 never needed it

Acts on *The grant discrepancy is resolved: a public repository, not an undocumented permission*, above, and
closes the "**Owed, and small**" paragraph that entry ends with. That paragraph's text stays as written, per
this file's rule; this entry records what was done and corrects one of its claims.

**Done.** `docs/technical/effects-repository.md` now carries *Resolved 2026-08-13: a public repository, not an
undocumented grant* — appended after the subsection it supersedes rather than replacing it, with the
superseded instruction (*"treat the effective grant as wider than this table in an unknown direction"*) marked
in place so a reader cannot act on it, and the permission table's Issues row showing its old status struck
rather than swapped. The sharper rule is stated there: **on a public repository a successful write proves
nothing about a grant either**, because a surface open to any authenticated identity answers identically to a
credential that holds the permission and to one that does not.

**The claim being corrected.** That paragraph says *"the permission table's Issues row, its subsection, and
**ADR 018** still describe this as unexplained"*, and the entry's opening names ADR 018 as one of the four
documents describing the grant. Neither is true of ADR 018, and the entry that lists the four enumerating
documents inside `effects-repository.md` does not include it — it names `.env.example`,
`docs/evaluator-calibration-general.md`, `.github/workflows/github-effects.yml` and the table itself.
Measured over all **180** lines of `docs/technical/decisions/018-a-graphql-200-is-not-a-success.md`, case-
insensitively: `unexplained|unresolved|wider|createissue|#25|issues` returns **one** hit, and it is the verb
in *"The probe issues one cause"*; `contents|pull requests|metadata|secrets|actions: |permission|grant|403|
token` returns **one** hit, and it is `personal access token` inside a quoted response body. What ADR 018
actually says about the episode is *"a mutation this credential is not permitted to issue"* — which the
resolution confirms rather than contradicts, since `closeIssue` is exactly the operation the absent grant
refuses. **So ADR 018 needs no append**, and the document that said it did was wrong about it in both
directions: it neither enumerates the grant nor calls the success unexplained.

**Nothing here widened or exercised a credential.** The gated-endpoint table this entry's parent records was
re-verified by the lane that resolved the discrepancy; this entry copies it and measured nothing at GitHub.

### 2026-08-13 — Jira and Slack belong inside the CVE capability, and the milestone table gained a row for it
Raised by the user during M4 planning: CVE remediation should eventually own its Jira filing and Slack notification rather than leaving them to host-workflow steps. The argument for moving them is not tidiness — routed through the effect executor they gain stable effect identity and postcondition reads, so an interrupted run cannot double-file a ticket or repost a message, which the current `curl` steps cannot promise.

Decided rather than deferred vaguely: `docs/fiddle-agentic-factory-prd.md`'s M5 row gains CVE verdict reporting as a policy-checked Jira effect, and a new **M9 — Notification channel** row adds a narrow outbound notification port with Slack as its first implementation. M9 is last because it is the only milestone whose absence changes nothing observable, and its gate is therefore an equality proof — the same scenario with the channel configured and unconfigured must produce the identical typed outcome, exit code and evidence bundle.

Two properties the RFC now states explicitly at the CVE agent section, because both are the reason the split existed rather than accidents of it: the mitigation decision stays trackerless permanently, so no ticket state or notification gates, informs or deduplicates a mitigation and requirement 22 keeps its "without requiring Jira"; and the capability holds neither credential, receiving an executor already bound to its own capability identity. The reference pipeline keeps Jira credentials out of the model run deliberately, and moving the work must keep that rather than trade it for convenience.

Origin: user direction during M4 seed planning (epic fiddle-eph7, seed fiddle-q7ct)
Tags: #feature #idea
Status: 2026-08-13 — recorded in the RFC (M5 row, new M9 row, CVE agent section) and in the tracker: M5 `fiddle-gyyo` body carries the added scope, M9 epic `fiddle-w4co` and seed `fiddle-tb0q` created under `fiddle-30ey`, blocked by M8 `fiddle-is3b`.

### 2026-08-13 — M4 split into capability and integration, and the effect identity that would have silently no-opped
Two outcomes of challenging the M4 design, recorded because the second is a defect that would have shipped looking successful.

**The split.** M4 became M4a — CVE mitigation capability (`fiddle-eph7`, seed `fiddle-q7ct`) and M4b — CVE workflow integration (`fiddle-rwdm`, seed `fiddle-5cyx`), with `docs/fiddle-agentic-factory-prd.md` gaining a row for each and M5 rewired to wait on M4b. Sizing was the trigger — the combined scope exceeded M3, which ran 39 beans and lost two lanes to an individual spend limit at roughly 40 — but the better argument is that the halves are proved differently. M4a's claim is about decisions and gates offline against a scripted scanner and a scripted forge; M4b's is about deployment against a real forge, scanner and CI. Merged, the gate would need a credential to say anything, contradicting M0's constraint that the acceptance lane is never gated on a secret.

**The defect the challenge found.** The shared-PR model regenerates the pull request body on every run as CVEs accumulate. `fiddle_core::effect::effect_id` derives from `(project, invocation_ref, kind, target)` and never from the payload; the shared PR's natural target is repo + head + base, which does not change between runs, and a nightly `scanner:<component>` reference is stable. So the second run computes the same effect identity, step 3 finds the postcondition already satisfied, and the executor performs no mutation — the accumulated CVE table never appears and **nothing reports a failure**, because the pull request that was opened on run one is real. Not a refusal, a silent no-op. The fix is to carry a digest of the intended body in the target, which is what M2's identity derivation is for and what M3 already did when it made a moved head a different question: a changed body is a new effect that applies, an unchanged body is idempotent.

The general shape is worth keeping beyond this milestone: **an effect whose target is stable but whose payload is meant to change is invisible to postcondition inspection.** Any future operation that updates rather than creates has this hazard, and the identity is where it is fixed, not the postcondition.

**A third finding, which removed work rather than adding it.** The design was going to widen the workspace command's pinned four-name environment allowlist to admit `DOCKER_HOST`, with an ADR — M4's only incursion into the boundary M1 and M2 fixed. Measured instead: under `env_clear` plus `PATH`, `HOME` and `LANG`, `docker version` reaches the daemon, because the CLI defaults to the Unix socket and *setting* `DOCKER_HOST` wrongly is what breaks it. Go needs nothing either, since `GOMODCACHE` and `GOCACHE` default under the scratch `HOME` the workspace already supplies outside the worktree. No ADR is owed and `workspace::a_workspace_command_inherits_no_credential` keeps pinning four names.

Origin: fiddle:challenge --phase define during M4 seed planning (epic fiddle-eph7, seed fiddle-q7ct)
Tags: #debt #risk #infrastructure
Status: 2026-08-13 — split recorded in the RFC and the tracker; the effect-identity and allowlist findings are recorded in the M4a design spec and must survive into bean bodies, since docs/specs/ is gitignored.

### 2026-08-14 — A plan's test snippets named real APIs with wrong shapes, and the lane that hit it was the third to find a DEFINE defect
The M4a plan's task bodies carry Rust test snippets written during planning without being compiled. Several name a real API with the wrong shape: `assess(&view)` against the real `assess(work, expected_marker)`; `Observation::NotApplicable` against the real `NotApplicable { reason: String }`; `ChangeSetState::none()`, which has zero hits repo-wide; and `WorkStateView { .. }` as a struct literal where the constructor is `without_publication`. All four confirmed against the tree.

The plan format already forbids "references to types, functions, or methods not defined in any task". This is the adjacent sin it does not name: referencing a type that *does* exist, with a signature that does not. The rule worth adding for later milestones is that a snippet against existing code is only evidence if it was compiled, and a plan that cannot compile its snippets should say they are intent rather than presenting them as code.

Three DEFINE defects in this epic were found by something other than the lead's own review, which is the pattern worth recording rather than any one of them: a bean requiring helpers whose types no earlier bean builds; an assertion passing for two different causes (`is_err()` satisfied both by a refused field and by invalid JSON); and this one. Two were found by implementer lanes and one by the convergence machinery.

Origin: implementation (epic fiddle-eph7, Task 2 lane fiddle-uwk0)
Tags: #debt #infrastructure
Status: 2026-08-14 — recorded on epic fiddle-eph7 as an instruction to every remaining lane: verify signatures against the tree, adapt, preserve intent, report the adaptation, and never add a shim to make a snippet compile as written.

### 2026-08-14 — assess's fallback narrowed its extent while its arm count stayed three, and one guard is single-witness
Two findings from the Task 2 lane about `crates/fiddle-core/src/assessment.rs`, both reported rather than left for a reader to discover.

`docs/technical/SYSTEM.md` states that exit 20's `assess → Blocked` route has exactly three arms. That count is **unchanged** at `43cb3d7`, verified site by site at both shas. But the fallback's *extent* narrows: it previously caught `(NotApplicable work item, Available changes)` and no longer does, so the clause "the fallback for a view M0's orchestration cannot act on" remains true while no longer covering the trackerless case. The reason string is byte-unchanged and no test asserts it.

Separately, the work-item half of the fail-closed guard is **single-witness**: under the arm-merge inversion only `an_unavailable_work_item_blocks_too` failed, because `unavailable_source_is_blocked` makes the *changes* half unavailable, which the narrowed guard still catches. That state is pre-existing, but the change makes it more load-bearing — there is now an adjacent arm that can swallow exactly that case where before there was only the fallback.

Origin: implementation (epic fiddle-eph7, Task 2 lane fiddle-uwk0, reported as concerns with DONE_WITH_CONCERNS)
Tags: #debt #risk

### 2026-08-14 — Three descriptions still say an invocation reference is `<scheme>:<value>`, and one of them is a diagnostic that now misleads
ADR 019 admits a bare reference, so `<scheme>:<value>` is no longer the whole grammar. Three descriptions outside the M4a Task 1 lane's Files block were left stale deliberately rather than widened into it:

- `crates/fiddle-cli/src/cli.rs:61` and `:104` — the `inspect` and `run` positional help both read "as `<scheme>:<value>`". ADR 019 quotes the `run` one specifically, so the ADR and the help text now disagree.
- `crates/fiddle-runtime/src/orchestration.rs:86` — "The canonical `<scheme>:<value>` text of the invocation."
- `InvocationRefError::Malformed`'s own diagnostic still says the form must be `<scheme>:<value>`, so a caller who mistypes `cev` is given guidance that omits the legal bare form. This is the one that actively misleads rather than merely under-describing.

A related latent bug in the same area **was** fixed by that lane, and is worth recording as the reason to take the rest seriously: `UnknownScheme`'s message hardcoded "beans, jira, scheduled, scanner" in its `#[error]` attribute, so adding a fifth scheme left it naming four of five — a caller who correctly typed `cve` before the variant existed would have been told there is no such scheme. It is now derived from `ALL` with a test over every scheme.

Origin: implementation (epic fiddle-eph7, Task 1 lane fiddle-typ7, reported as a concern with DONE_WITH_CONCERNS)
Tags: #debt
Status: 2026-08-19 — **all three resolved** (bean `fiddle-wr6v`), and the entry was
partly stale by the time it was acted on. The first bullet had already been fixed:
`cli.rs`'s two positionals now read "as `<scheme>:<value>` — for example
`beans:fiddle-m0-demo`. A scheme that finds its own work stands alone and takes no
value: `cve` scans the configured image and inspects what it finds", so the valued
shape is an example rather than a requirement and the standing-alone half is named
beside it. Nothing recorded that here, which is worth noting on its own: a backlog
entry listing three sites is read as three live defects, and this one had one.
`orchestration.rs`'s doc comment now spells both shapes. The third — the one this
entry called out as actively misleading — was live for five days and through the
remediation round that swept this exact class; see the 2026-08-19 entry "A promise
and a denial are one class, and a lane that hunts phrases catches one of them".

### 2026-08-14 — ADR 011's traversal table enumerated two schemes, and the one whose values come from outside was not among them
The M4a Task 1 lane's ninth mutation exempted standalone-scheme values from ADR 011's character class — the plausible over-generalisation of ADR 019, that "a self-discovering scheme supplies its own input". It was caught by that lane's new test **and by nothing else in the workspace**, because `refuses_a_value_that_could_be_read_as_a_path`, the test that reads as the canonical list, enumerated only `beans` and `scanner`.

`cve` is precisely the scheme whose *valued* form carries a scanner-supplied advisory id — an input fiddle does not control — so it was the one most needing a row and the one that had none. Rows for `cve:../../../pwned` and `cve:a/b` were added to that table.

The general shape, worth carrying beyond this milestone: **a test that reads as an exhaustive list over a closed set is a null the moment the set grows.** ADR 019 admits a fifth scheme; nothing made the traversal table notice.

Origin: implementation (epic fiddle-eph7, Task 1 lane fiddle-typ7, found by its own inversion)
Tags: #debt #risk #security

### 2026-08-18 — The base-image arm is reporting-only in M4a, and that leaves the OS half of dedup with no producer
Recorded as a decision with its consequence, because the consequence had no owner.

**The decision.** M4a does not build a registry client, so it cannot select a base-image tag. Design §2.4 rule 4 is built as far as it goes without a network peer: an OS finding is attributed to `Target::DockerfileBaseImage` (`crates/fiddle-runtime/src/cve/attribute.rs`), every one of them keys onto that single group, and `select_target_version` already answers the floating-tag `needs-work` case when handed a tag list (`a_floating_tag_with_no_newer_pinned_tag_is_needs_work`). Missing is the tag list and the `Dockerfile` edit — an authenticated read of the image's published tags, a comparability rule for them, and a port, adapter, credential and policy decision to carry it. `CveMitigate::target_version` (`crates/fiddle-runtime/src/capability/mitigate.rs`) therefore refuses every base-image group with `GroupError::Unselectable { why: "selecting a base-image tag needs a registry this build does not read" }` and an OS finding is **reported, never attempted**. This is not M4b's either — M4b is the release artifact, the host workflow, the CI-feedback fresh attempt and the first real Wiz measurement — so the work is currently unowned, which is what this entry is for.

**The consequence.** That refusal removes the only producer the OS half of deduplication could have had. A refused group is recorded blocked and skipped before either commit producer runs — not the fold's `--allow-empty` commit, whose message names the group's ids, and not `land`, which commits only `GroupStatus::Clean`. So **no M4a run can write an OS advisory into a commit body**, and `already_fixed`'s `PackageType::Os` arm (`crates/fiddle-runtime/src/cve/dedup.rs`) reads commit bodies and nothing else. It answers `true` only for history somebody else wrote; every OS case in the suite seeds one. Design §2.7's stated reason for listing every CVE id in a commit body is that same OS path, and is likewise dormant.

**What this does not say.** `commit_log_dedup` and its shallow-history guard are not dead with it. Their set also feeds `Run::in_progress`'s `covers`, which filters every finding through the same scan and is what reaches the `AlreadyInProgress` disposition; library groups do commit `Fixes:` bodies and a reused branch's log is read back on the next run. It is the OS half of the answer that has no producer, not the reading of the log — and `covers` is what earns the commit body's completeness in the meantime.

The refusal is held from outside the process by `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch` (`crates/fiddle-acceptance/tests/cve_mitigation.rs`), which asserts the OS verdict's rationale carries `registry this build does not read`. Wiring a registry turns that lane red, which is the intended way back to the three notes at the sites: `target_version`'s doc comment, `cve/dedup.rs`'s module header and OS arm, and `commit_body`'s doc comment in `crates/fiddle-runtime/src/capability/cve.rs`.

Origin: implementation (epic fiddle-eph7, remediation bean fiddle-rh0p)
Tags: #debt #feature

### 2026-08-18 — Verifying that the scanned image was built from the remediated tree needs a config field no milestone owns
The decision behind this is recorded, and accepted, in `docs/technical/decisions/020-the-host-builds-the-image-fiddle-scans.md`: **the host workflow builds the image fiddle scans, and fiddle does not build it.** That is right for the offline gate — a real `docker build` pulls base layers, and a stubbed one yields a digest meaning nothing. This entry is only for the half the decision leaves owed.

**What was built.** Fiddle now publishes the pair. `TreeObservation` (`crates/fiddle-core/src/observation.rs`) carries a fourth key, `scanned_image_digest`, assembled in `CveMitigate::sweep` (`crates/fiddle-runtime/src/capability/mitigate.rs`) — the one place in the build where the scan's resolved digest and the checkout's revision are both in hand, because the scan happens in `execute` before a worktree exists and `Checkout` never sees a scanner. Until this, `ScanReport::image_digest` was parsed by the `wizcli` adapter and read by nothing, so Design §2.2's *the digest is what makes a later re-scan comparable* was true of a struct field that died with the process. A bundle now says *these verdicts are about digest X and I remediated revision Y*, which a person or the workflow that did the build can check.

**What is owed.** Making that a *checked precondition* rather than an auditable pair: the builder declares the revision it built the image at, and fiddle refuses a run where the declaration disagrees with `checkout.revision()`. It is two halves that have to land together. The **host half** — a workflow step building at the checked-out revision and passing it in — is M4b's, whose scope is the release artefact, the workflow in `snowplow-incubator/snowplow-identities` and the first real Wiz measurement. The **fiddle half** — a `[orchestration.cve]` field carrying the declared revision, plus the comparison and its refusal — is in no milestone's scope, because M4b is the host side. That is what this entry is for. Landing the fiddle half alone would add a field nothing populates, which is either off by default and asserts nothing or refuses every existing run; landing the host half alone gives fiddle a value it does not read.

**What this does not say.** That the pair is worthless without the check. It is what makes the gap auditable at all, and it is the value the stronger check would compare against, so the two are a sequence rather than alternatives. What must not be assumed from it is provenance: fiddle did not build the image and cannot know it came from that revision. The doc comments on `TreeObservation::scanned_image_digest`, on `observed_tree`, and on `ScanReport::image_digest` all say so at the site, and `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch` plus `an_unusable_scanner_exits_eleven_and_reaches_no_forge` (`crates/fiddle-acceptance/tests/cve_mitigation.rs`) hold the pair and its all-or-nothing publication from outside the process.

Origin: implementation (epic fiddle-eph7, remediation bean fiddle-k38l)
Tags: #debt #feature

### 2026-08-18 — A false citation was caught, recorded, and then repeated into a dispatch by the lead who recorded it
The M4a design spec and a Task 18 dispatch both claimed that "`docs/technical/SYSTEM.md` records that nothing in `fiddle-runtime` edits a comment and that the absence is load-bearing." It does not. `grep -niE 'comment|RequestEdited' docs/technical/SYSTEM.md` returns exactly one hit — line 173, about a purity grep that matches *source* comments.

The constraint is real. It lives on the variant itself, `crates/fiddle-runtime/src/human/validate.rs`, whose doc comment reads "fiddle's own question has been edited, which fiddle has no path that does", and it is stated correctly in M3's milestone handoff on epic `fiddle-eoqx` — which is where the lead read it before mis-attributing it to SYSTEM.md.

**The part worth recording is not the slip but the repeat.** An earlier round of the same bean already caught this and recorded the correction on the epic body and in `cve_shared_pr.rs`. The lead then wrote a fresh dispatch for that same bean without reading the correction, and reproduced the false citation verbatim. This is the propagation M3's handoff names — "the lead's own transcription errors propagated three times" — occurring again on the bean that had already caught it.

The practice that follows: **a dispatch for a bean with prior iterations must be written from the bean body, not from the plan or from memory.** The corrections live on the bean, and a dispatch composed without reading them will reintroduce exactly what the previous iteration paid to find.

Origin: implementation (epic fiddle-eph7, Task 18 lane, second occurrence)
Tags: #debt #risk
Status: 2026-08-18 — corrected in the design spec with the real location and a note that it had already been caught once.

### 2026-08-18 — A worktree teardown that does not check for a live lane destroys measurements, and reads as a test failure
The lead removed four lane worktrees and their branches while agents were still working in them, having concluded the beans were already converged. The conclusion was right; the sequencing was not — no lane was asked to stand down first.

Three lanes lost in-flight measurements: a `cargo test --workspace` at 894 passed, another at 18 binaries reported, and an inversion probe mid-restore. Nothing was lost permanently, only because all three lanes had correctly declined to implement anything and so had nothing uncommitted. **A lane with uncommitted work would have lost it silently**, which is the failure `docs/technical/evidence-discipline.md` §3 already records and which the M4a seed evidence records for M3 (116 uncommitted deletions, 65,802 lines).

The diagnostic signature is worth knowing because it is misleading. From inside a suite, a vanished tree presents as:

    error: test failed, to rerun pass `-p fiddle-runtime --test scanner`
    Caused by: could not execute process `.../deps/scanner-…` (never executed)
    Caused by: No such file or directory (os error 2)

`(never executed)` plus `No such file or directory` on the binary's own path is a tree disappearing under the runner, not a failing test — and a lane that reported the non-zero exit as a failure would be reporting a defect that does not exist. One lane also observed `error: couldn't read crates/fiddle-runtime/src/lib.rs` with `FAILED_BINARIES=0` across the binaries that had already reported, which is the same event seen from the compiler's side.

The rule: **a teardown checks for a clean tree and for live build processes, and asks the lane to stand down first.** Deleting the branch as well as the checkout is what made it read as deliberate to the lanes rather than as corruption.

Origin: implementation (epic fiddle-eph7, lanes for Tasks 4, 11, 18, 19)
Tags: #debt #risk #infrastructure

### 2026-08-18 — Correcting the entry above on the vanished-tree signature: the detection was a disagreement, not a signature
The entry "A worktree teardown that does not check for a live lane destroys measurements, and reads as a test failure" describes the `(never executed)` / `No such file or directory` output as what a vanished tree looks like. True, but it names the wrong thing as the detection mechanism, and the lane that hit it has the better account.

What caught it was that **the tally and the exit code disagreed**: cargo reported `error: 35 targets failed` with `BASELINE_EXIT=101`, while `FAILED_BINARIES=0` across the 18 binaries that had already reported. A lane reporting only the status would have published a 35-target failure at a sha where nothing was broken; a lane reporting only the count would have published a clean run of a suite that never finished. Neither number is trustworthy alone, and it is their disagreement that is the signal.

That is `docs/technical/evidence-discipline.md`'s own argument for printing the count beside the status — the same rule that caught the `shellcheck` format mismatch — so the recognisable failure here is a discrepancy the discipline already tells you to look for, not a novel string to grep.

### 2026-08-18 — Pointing at evidence-discipline.md is not sufficient on its own, and two consecutive dispatches are the evidence
M3's handoff established the rule that a dispatch should **point** at `docs/technical/evidence-discipline.md` rather than restate it, because M3 copy-pasted ~700 lines of method into every dispatch and the lead's own transcription errors propagated three times. That rule was right about the failure it fixed. This entry records its limit.

Two consecutive M4a dispatches tripped on rules that document is the record of, in lanes that had been pointed at it:

- One launched its baseline as `cargo test --workspace --all-features 2>&1 | tail -60`, with no `EXIT=` marker and no `--no-fail-fast` — a pipe-truncated log of a 42-binary run, which §1 names in its first three paragraphs. The lane noticed only on reading the file afterwards, discarded the run and re-ran instrumented.
- Another recorded `tail`'s exit status in place of clippy's, the same defect one level along, and also caught it by re-reading rather than at the point of writing the command.

The lead did the same thing twice in this milestone: counting tests from a `tail -60` of a log, and reporting a `FAIL` verdict computed from a scorecard file that was never written.

So the reading worth carrying: **pointing works for the rules a reader will look up, and fails for the rules that govern the command they are about to type.** The measurement rules — print the exit code you mean, print the denominator, never pipe a status you intend to report — are needed *before* the log exists, and a pointer consulted afterwards only diagnoses. Candidate fixes rather than a decision: name those three inline in a dispatch's verify section as concrete commands, or give them a script with an exit-code contract, which is what `docs/technical/decisions/009-mechanical-gates-as-validators.md` argues for generally.

Origin: implementation (epic fiddle-eph7, Task 18 and Task 4 lanes, plus two lead-side instances)
Tags: #debt #infrastructure

### 2026-08-18 — The product manual's `fiddle.toml` example still cannot load, and `[orchestration.cve]` was only the table somebody looked at
`fiddle-c64d` wired `[orchestration.cve] severities`, which the manual documented and the schema refused, and added the `image` key that table always required. The lane it added reads that **one table** out of `docs/fiddle-agentic-factory-prd.md` and drives the binary over it, so that table and the schema can no longer diverge unnoticed.

The rest of the example is the same defect, unmeasured until now. Extracting the whole `fiddle.toml` block from the manual and running the compiled binary over it exits 2 at the *first* line of the *first* table after `[project]`:

```
 10 │ repository = "snowplow/icecube"
    ·      ╰── unknown field `repository`, expected one of `repo`, `base`, `token`, `cli`, `git`, …
```

`[github]` alone disagrees on `repository`/`repo` and `default_branch`/`base`. Behind it the example names `[jira]`, `[execution]`, `[policy]`, `[artifacts]`, `[telemetry]`, `[orchestration] enabled`, `[orchestration.stabilize]`, `[orchestration.set_based]`, `[orchestration.toil]`, `[capabilities.*]` and `[agent] default_runtime` — tables and keys the shipped schema has no reader for at all. **A deployment cannot copy the manual's example**, and that sentence is true of far more than the one key a holistic pass happened to find.

Most of those tables belong to milestones that have not shipped, so this is not a claim that the schema is wrong. It is a claim about the document: an example presented as *what a repository writes* is refused at line 10, and nothing in the repository says which parts of it are aspirational. Two candidate fixes rather than a decision — mark the unshipped tables in the example as forward-looking, or extend the new extraction lane from one table to the whole block and let it fail until the two agree. The second is the stronger one and is a document decision rather than a test decision, which is why it is here and not in the bean.

Origin: implementation (bean fiddle-c64d, epic fiddle-eph7 — measured with the compiled binary over the extracted block)
Tags: #debt #documentation

### 2026-08-18 — `check-thresholds.sh` returns PASS for a scorecard whose dimensions carry no threshold

The script compares `select(.value.score < .value.threshold)`. When a scorecard's dimension objects carry `score` but no `threshold`, jq evaluates `5 < null` as false, every dimension lands in `passing_dimensions`, and the script exits 0 with `"verdict": "FAIL"` never reachable. A holistic scorecard scoring 5, 6, 6, 6, 9 against thresholds of 7, 7, 8, 6, 9 was reported as **PASS** on exactly this path, and `check-convergence.sh` then returned `PASS_PENDING` — one dispatch away from declaring a milestone's holistic review converged over a scorecard that failed three of five dimensions.

The immediate cause was upstream and human: the envelope handed to the reviewer omitted the `threshold` field, so the reviewer produced a well-formed scorecard the gate could not grade. That is worth fixing separately. But a gate that cannot tell "nothing failed" from "nothing was compared" is the defect that made a spelling mistake into a false pass, and the same shape would swallow a renamed field or a merge that dropped the key.

The fix is to refuse rather than to default: a dimension with no `threshold`, or a `--criteria` file whose entries carry no `pass`, should exit non-zero naming the missing field. Defaulting to the holistic thresholds would be worse than erroring — it would quietly grade one scorecard by a different rule than the one its author was given.

The `--criteria` argument has the same shape of hazard from the other direction: it expects the scorecard's *graded* criteria array, and an ungraded array of criteria descriptions (the file used to brief the reviewer, which is the natural thing to reach for and has the same `id` keys) yields zero failing criteria rather than an error.

Origin: orchestration (epic fiddle-eph7, holistic iteration 4 — the verdict was re-derived by hand and the three failing dimensions recovered)
Tags: #bug #tooling #evaluation
Status: Resolved 2026-08-19 by `fiddle-fgam` — `check-thresholds.sh` refuses ungradeable input with exit 2 before comparing anything, naming each missing field with the dimension or criterion it belongs to (``domain general dimension correctness: missing `threshold` ``, ``criterion c1: missing `pass` ``). The refusal also covers the same blind spot arrived at by type order rather than by a null: `"1" >= 7` is true, and `"false" == false` is false, so a stringly-typed score or `pass` read as passing too. `scripts/test-check-thresholds.sh` holds those cases, and replays the two verdicts of iteration 2 (`fiddle-ek1e` on a criterion, `fiddle-o1ly` on a dimension) byte for byte to pin the shape `check-convergence.sh` reads. The `--criteria` half of this finding had a second cause the entry did not name: `skills/develop-holistic/SKILL.md` instructed the caller to pass `criteria-holistic.json`, which *is* the ungraded briefing file; it now extracts the graded array from the merged scorecard.

### 2026-08-18 — A probe taken from a stale binary, in the pack built to prevent exactly that

The holistic evidence pack for iteration 4 captioned probe 5 "help now names the bare form (lead fix)" and headed the pack "run at HEAD 8fce238". The transcript came from `target/release/fiddle` as it stood *before* the gate rebuilt it, so it showed the help text without the fix it was offered as evidence of. The reviewer caught it by running the binary itself and noticing the extra sentence.

The fix was real and its test passes; only the evidence was wrong. That is the whole hazard: a probe that agrees with what the author expects is not checked, and this pack exists precisely to stop unchecked expectation reaching a verdict. Commit `5dd2c9c` recorded the same failure as "a predicted probe is not a probe"; it recurred in the artefact written to prevent it, one iteration later.

What would have caught it: taking probes *after* the build that the pack claims they came from, and stamping each probe with the binary's own mtime or `--version` rather than with the HEAD the author believes is built.

Origin: orchestration (epic fiddle-eph7, holistic iteration 4 — reported by the reviewer as an antipattern against the pack rather than the tree)
Tags: #process #evidence-discipline

### 2026-08-19 — `dispatch-provider.sh` hands a provider whatever it is given, and a too-large prompt costs a whole review

A holistic dispatch to `codex` failed at `turn/start` with `Input exceeds the maximum length of 1048576 characters. actual_chars: 2178394`. The cause was the caller: `--diff-file` was the whole 39k-line epic diff, and the assembled prompt was 2.1MB against a 1MB limit. The hook does `DIFF="$(cat "$2")"` and passes it straight through, so the first thing that knows the prompt is too big is the provider, after the dispatch is already committed.

The cost is not the failure, it is the *shape* of the failure. The wrapper's completion notification lagged, so from the orchestrator's side this looked like a provider hanging for forty-five minutes; the first written account of it said codex "returned nothing after 45 minutes", which was true in effect and wrong in cause. A holistic iteration then reached a verdict on one reviewer instead of two, and the second opinion was lost to an input error rather than to a provider being unavailable — a distinction that matters, because one is worth retrying and the other is not.

Two fixes, and the first is cheap: have the hook measure the assembled prompt and refuse before dispatching, naming the byte count and which input dominates. Then a caller learns at once, instead of learning from a provider error whose text does not mention `--diff-file`. Second, for whole-epic reviews, stop passing whole-epic diffs: send the diffstat and let the provider read the tree, or scope the diff to the paths under review. A 39k-line diff was never going to be read line by line anyway.

Origin: orchestration (epic fiddle-eph7, holistic iteration 4 — the second reviewer was lost and the iteration proceeded single-provider)
Tags: #bug #tooling #orchestration

### 2026-08-19 — A bean asked a lane to edit a file that does not exist in a lane worktree

`docs/specs/agentic-factory-m4-design.md` is gitignored. A `git worktree add` copies tracked content only, so the design document — the thing every bean is derived from — is **absent from every lane worktree**. Two consequences hit inside one milestone.

An evaluator dispatch died outright: `hooks/dispatch-provider.sh ... --design-doc-file docs/specs/agentic-factory-m4-design.md` run from a lane produced `cat: ... No such file or directory` and the provider was handed nothing. That one is loud and was fixed by passing the epic worktree's absolute path.

The quiet one is worse. Bean `fiddle-jq1g` carried two criteria requiring design-document edits — state the reference-to-capability binding, and resolve a split-table contradiction. The lane could not have satisfied them under any effort, and nothing in its environment said so; it simply left them undone, and the lead made those edits in the epic worktree at evaluation time. A criterion that cannot be met from the worktree it is dispatched into is not a criterion, it is a trap, and the lane that hits it looks negligent.

Two fixes, and they are independent. First, `define-beans` should not write a criterion naming a path that is gitignored — that check is one `git check-ignore` per referenced path. Second, decide whether the design document should be gitignored at all: it is the reference for two milestones and it is read by every reviewer, which is an odd thing to keep out of the tree. If it stays out, lane briefs must say so explicitly, the way this milestone's later briefs began doing.

Origin: orchestration (epic fiddle-eph7, bean fiddle-jq1g — the lead completed the criteria the lane could not see)
Tags: #bug #orchestration #beans

### 2026-08-19 — The evaluator envelope has now failed four times, in three different shapes

Across this epic, external evaluator dispatches have returned: an object truncated one brace short (twice), an object with `criteria` nested under `.domains`, and an object with the domain key `general` at top level instead of under `domains`. Each was well-formed prose reasoning wrapped in a shape the grading scripts could not read.

The two truncations were re-dispatched, because content was missing and repairing them would have meant guessing at scores. The two mis-nestings were repaired mechanically and the repair disclosed in the evaluation log — a single unambiguous move of a known key is lossless, and there is exactly one valid placement for a domain name. That distinction is worth keeping: *missing content must be re-dispatched, mis-shaped complete content may be normalized and said so.*

Four failures in one epic is a tooling signal rather than four provider mistakes. `merge-scorecards.sh` should normalize a top-level domain key and a mis-nested `criteria` array, and say in its stderr that it did — then no caller hand-fixes anything, and the normalization is recorded in one place instead of in four evaluation logs. Spelling the envelope more loudly in each dispatch has been tried repeatedly this milestone and has not converged.

Origin: orchestration (epic fiddle-eph7 — beans c64d, uwk0, jq1g and holistic iteration 4)
Tags: #bug #tooling #evaluation
Status: 2026-08-19 — action redirected rather than resolved. See *Envelope normalisation does not belong in `merge-scorecards.sh`, and one shape never reaches it* below: the merge is the wrong host, because one of the two shapes dies at `validate-scorecard.sh` before it and the merge's stderr is already consumed as `disagreements-holistic.json`. The distinction this entry draws — re-dispatch missing content, normalise mis-shaped complete content and say so — stands.

### 2026-08-19 — Envelope normalisation does not belong in `merge-scorecards.sh`, and one shape never reaches it

Acts on *The evaluator envelope has now failed four times, in three different shapes* above, which proposed that `merge-scorecards.sh` normalise a top-level domain key and a mis-nested `criteria` array and say on stderr that it did. Measured against the tree while fixing `check-thresholds.sh` (bean `fiddle-fgam`), that placement cannot cover both shapes, and its stderr is not free.

The documented order is dispatch, then `validate-scorecard.sh` on the raw per-provider scorecard (`skills/develop-loop/dispatch-and-evidence.md`, "Gate each scorecard before the merge"), then `merge-scorecards.sh` — which is on every path, since step 1g normalises even a single provider through it. Running the two shapes through that order:

- **`criteria` nested under `.domains`** — `validate-scorecard.sh` exits **5** with `jq: error (at <unknown>): Cannot index array with string ("dimensions")`, rather than the exit-2 JSON error array it documents, because `.domains | to_entries` hands the criteria array to `.value.dimensions`. The scorecard is rejected before the merge, so a normaliser inside the merge would never see this shape at all.
- **a top-level domain key** — `validate-scorecard.sh` exits **0** and accepts it: with no `.domains`, it has zero dimensions to check. `merge-scorecards.sh` then exits **5 with nothing on stdout and nothing on stderr**, because the `2>/dev/null` on its merge `jq` swallows `null (null) has no keys`. A caller sees an empty file and no reason.

The merge's stderr is also already a typed channel: `develop-holistic` runs `... | scripts/merge-scorecards.sh > scorecard-holistic.json 2> disagreements-holistic.json`, so a "normalised X" line printed there lands inside a file that is parsed as a JSON array of disagreements.

So the normalisation belongs between dispatch and validation, on the raw scorecard — a `normalize-scorecard.sh` whose stdout is the repaired card and whose stderr is free to name what it moved — and it carries two prerequisites in the same area: `validate-scorecard.sh` must *report* a mis-nested `criteria` instead of crashing on it, and `merge-scorecards.sh` must stop hiding its jq errors. Three scripts and a suite of their own is why `fiddle-fgam` did not fold it in.

What `fiddle-fgam` did change is the consequence of not normalising. Neither shape can now be graded: both stop at `check-thresholds.sh` with exit 2 naming the missing field, a top-level domain key reporting ``scorecard: missing `domains` ``. The cost of an un-normalised envelope is orchestrator toil, not a false pass — which is what makes this a bean to schedule rather than a rider on a critical gate fix.

Origin: implementation (bean fiddle-fgam, epic fiddle-eph7 — measured by running both recorded mis-shapes through validate-scorecard.sh and merge-scorecards.sh)
Tags: #bug #tooling #evaluation

### 2026-08-19 — The eval log annotates a failing dimension by the same comparison that could not see one

Related to *`check-thresholds.sh` returns PASS for a scorecard whose dimensions carry no threshold* above, in a script that does not gate anything. `scripts/append-eval-log.sh` line 63 writes `if .value.score < .value.threshold then " (FAIL, threshold …)"`, the same comparison and the same blind spot: run against the threshold-less scorecard from that finding, the entry it builds reads

    **general:**
    - correctness: 1/10
    - domain_spec_fidelity: 1/10
    …

with no FAIL annotation anywhere. `fiddle-fgam` deliberately left this alone. The log decides nothing — convergence is decided by `check-thresholds.sh`, which now refuses such a scorecard outright — and a refusal here would break the one route that is *required* to log before routing: the SPEC_DEFECT path in `skills/develop-loop/scorecard-merge.md` logs a scorecard it has already declared defective. What it should probably do instead is annotate the missing threshold rather than omit the verdict — `(no threshold recorded)` beside the score — so the durable record cannot read as a clean sheet. That is a one-line change wanting a test, and a decision about whether the eval log should ever contain a dimension whose threshold is unknown.

Origin: implementation (bean fiddle-fgam, epic fiddle-eph7 — found by grepping for the same comparison elsewhere, measured by running the log's jq filter on the threshold-less scorecard)
Tags: #debt #tooling #evaluation
Status: 2026-08-19 — **resolved in the same bean, by annotation rather than refusal**, after the evaluator priced the deferral (`code_quality` 8 → 7, "leaving a misleading durable record"). The reasoning for not refusing stood and the fix keeps it: nothing in `append-eval-log.sh` exits non-zero over a score, so the SPEC_DEFECT route still logs before it routes — verified end to end through `merge-scorecards.sh`, not assumed. A dimension the comparison cannot make now reads `- correctness: 1/10 (UNGRADED, no threshold recorded)`, and the same rule names a non-numeric score or threshold, a dimension that is not an object, and a missing `domains` or `dimensions` key. The last three used to abort the logger with a raw jq error and exit 5 — no entry written at all, which is a worse record than a bare score — as did an empty scorecard file, which is exactly what a failed merge hands over. `parse-eval-log.sh` reports `last_verdict: UNGRADED` for such an entry, checked ahead of `FAIL`, because without that branch the entry carries no marker anywhere and falls through to `PASS`: the same false pass, in the one state that outlives the session. Well-formed entries are byte-identical — checked against both verdicts this epic actually recorded — and all 109 real eval logs in the store parse unchanged.

### 2026-08-19 — A doc comment that contradicts the binary is a review matter, and neither a doctest nor a grep changes that

The valued `cve` reference was advertised on four operator-facing surfaces and implemented on none (bean `fiddle-ye7n`, ADR 019's M4a amendment). The lane written to stop that recurring reads `--help` and each diagnostic off the **compiled binary**, which is the right subject: help written from an ADR describes what was decided, and only something driving the binary can say what was built. It is also structurally blind to source prose, and a **fifth** surface was found behind it — the doc comment on `a_bare_slug_cannot_collide_with_a_valued_slug` in `crates/fiddle-core/src/identity.rs`, which stated as present fact that `cve:CVE-2026-1234` remediates one finding. Nothing in the suite would have caught it, and nothing would catch the next one.

Two candidate guards were tried rather than assumed, and both fail for reasons that are properties of the tools and not of the effort spent.

**A doctest cannot reach a test-module comment.** rustdoc builds the crate without `cfg(test)`, so `#[cfg(test)] mod tests` is stripped before documentation is collected. A deliberately failing doctest inserted in that module yielded `running 0 tests` and `cargo test --doc -p fiddle-core` exited **0**; the identical probe on the public `InvocationRef::slug` exited **101** and named the file and line. The control arm matters: doctests do run in this crate under the gate's `cargo test --workspace`, so this is rustdoc's blindness and no harness setting moves it. The generalisation is worth keeping: a doctest checks the *code* in a comment, never its prose, so the only claims it holds are the ones rewritten as assertions — and a claim about behaviour the build lacks cannot be written as a passing assertion at all, which is why deleting it was the whole fix.

**A grep cannot separate a false claim from a true history note.** When the fifth surface was found, `remediates one finding` stood at five sites and four were correct: `orchestration.rs:148` and `inspect_ref.rs:490` quote the old sentence to record that it was wrong, and two ADR 019 lines state that nothing in this build does it. Only `identity.rs:725` asserted it. Writing this entry and the note on the lane added several more, every one of them saying the claim is false — which is the difficulty in one line. What distinguishes them is framing, which a pattern does not read; a pattern narrow enough to exclude the four is pinned to today's exact wording, so it passes the next paraphrase and reds on the next legitimate history note. That is a lane providing false comfort, which is worse than no lane, because a reader takes it for coverage.

**The conclusion, recorded rather than papered over:** contradiction between a source doc comment and the binary is caught by review here, and by nothing else. The place a reader meets that fact is the lane itself — `no_operator_facing_surface_promises_the_valued_form` in `crates/fiddle-acceptance/tests/inspect_ref.rs` carries the boundary in its doc comment, with both experiments — because the lane's name reads like whole-tree coverage and is not. If this recurs a third time, the thing to weigh is not a stricter grep but a review step that reads every doc comment touching a milestone's changed behaviour, which is a process cost and should be priced as one.

**Narrowed on review, 2026-08-19.** The two candidates above are genuinely ruled out, but "review and nothing else" is broader than they establish. A `fiddle-ye7n` evaluator named a third mechanism the lane did not try: a *file-scoped* assertion on one known-false phrase. The grep objection was that the phrase stood at five sites, one false and four true history notes — but that ambiguity is a property of searching the whole tree, not of asserting that one named file does not contain one named sentence. Such a test is narrow, it names its subject, and it would have caught this. It is not built, and the reason is scope rather than principle: the criterion was met, the dimension sat at threshold, and the milestone had one holistic dispatch left. Left as follow-up with the mechanism named, so the next reader inherits a bounded gap rather than a closed question. Overstating what has been ruled out is the same defect this entry is about, one artefact along.

Origin: bean `fiddle-ye7n` (epic fiddle-eph7, M4a — evaluation iteration 1 failed `no_operator_facing_surface_promises_the_valued_form` on the fifth surface)
Tags: #decision #testing #documentation

### 2026-08-19 — Two gates in one worktree produce a failure that belongs to neither

`scripts/gate.sh` was launched twice against `.worktrees/agentic-factory-m4` while the first run was still in flight. Both share `target/`, both invoke `cargo` and `nix develop`, and the first reported `TOTALS: 175 passed, 1 failed, 0 ignored, 14 binaries` with `GATE: FAIL` — against 53 binaries in every clean run of this epic. A count that low is a truncated run, not a failing tree, and the single failure belongs to the contention rather than to the code.

The cost is not the wasted ten minutes. It is that **a FAIL from a raced gate is indistinguishable, in the log, from a real one.** The orchestrator nearly read it as a regression in freshly landed work, and the only thing that prevented it was the binary count being obviously wrong. Had the race truncated at 52 binaries instead of 14, there was nothing in the output to catch it.

Two fixes, and the first is nearly free. `gate.sh` should refuse to start when another instance is running against the same worktree — a lock file keyed on the worktree path, removed on exit, reporting the holder's pid. Second, the TOTALS line should carry the expected binary count alongside the actual, so a truncated run is self-evidently truncated rather than requiring a reader who remembers that 53 is normal.

There is a related discipline for the caller, recorded because it is the actual mistake: **a gate launched while another is running measures nothing, and a gate launched after a rebase that aborted measures the wrong tree.** Both happened here in one command — the same background invocation rebased two lanes, aborted the second on a conflict, and then gated. Sequence the landing and the gate as separate steps, and read the git result before trusting the gate that follows it.

Origin: orchestration (epic fiddle-eph7, final remediation round — the raced FAIL was discarded and a clean gate run in its place)
Tags: #bug #tooling #orchestration

### 2026-08-19 — A promise and a denial are one class, and a lane that hunts phrases catches one of them

`InvocationRefError::Malformed` read **"invocation reference must be `<scheme>:<value>`, got `cvfoo`"**, and its help offered a colon and one valued example. That is the diagnostic a mistyped `cve` lands in — `cvfoo` has no separator, so it is malformed rather than an empty value — so the operator one letter away from `fiddle run cve`, the invocation M4a exists to provide, was told a colon was mandatory and shown nothing else to try.

**Why it survived a round aimed at it.** The class is *operator-facing text asserting a grammar the binary does not have*, and it has now been met twice. Iteration 5 spent a remediation round on it and built `no_operator_facing_surface_promises_the_valued_form`, which reads `--help` and each diagnostic off the compiled binary — the right subject. It hunts for a **promise of the valued form**: occurrences of `cve:` followed by a value character. This string is the same class pointing the other way, a **denial of the bare form**, and saying that `cve` requires a value never spells `cve:`. The lane passed while the string was live, and this was measured rather than reasoned: restoring the old help in place leaves that lane green and the whole file green but one. The 2026-08-14 entry above had already named this string, five days and one sweep earlier. Two searches at one class and neither pattern caught it, because both patterns were phrases and the class is not a phrase.

**What replaces it, and in what sense it is a property.** The lane now named `every_scheme_that_needs_no_value_is_named_on_each_surface_and_in_each_colonless_refusal`, beside the older lane in `crates/fiddle-acceptance/tests/inspect_ref.rs`, hunts no phrase. It reads the scheme set off the `unknown_scheme` diagnostic — the one surface whose job is to name them all, and derived from `InvocationScheme::ALL`, so a sixth scheme joins the lane the day a caller may write it. It then asks the **binary** which of them stand alone, by driving each bare form and reading whether the grammar refuses it. Only then does it hold the rendered text to the answer: every scheme the binary accepts alone must be named on every grammar surface, never carrying a value, and on the two surfaces that offer the set in halves each scheme must sit in the half its own behaviour puts it in. Nothing in it is pinned to wording, and the oracle is behaviour rather than `stands_alone` — a lane reading the enum would ratify whatever the enum said, whereas this one reds if the enum, the binary and the prose disagree. Both directions were inverted in place to check it: the old help alone reds it with "the `bogus` diagnostic from inspect says how a reference is written and never mentions `cve`", and swapping the two halves reds it with "must offer `cve` where no value is needed, because that is the invocation the binary accepts".

**The bounded gap, which is smaller than the last one and still real.** The *surface list is enumerated by hand*. A process cannot be asked to render every string it might print: each diagnostic is reachable only through an input that provokes it, and a sixth defect added later is not discoverable from outside. Nor can the list be replaced by a filter, and both available filters are worse than the gap. Requiring **every** surface to name the standing-alone schemes fails honestly-silent text — `fiddle --help` lists subcommands and says nothing about references, and should not have to. Triggering the check on a pattern such as `<scheme>:<value>` is the phrase hunt that let this defect through twice; the wording that misled had "must be" in it and the next one need not. So **placement** is derived and cannot go stale, while **membership of the list** is a review matter — the same boundary `no_operator_facing_surface_promises_the_valued_form` carries, and named on this lane's doc comment for the same reason: the lane's name reads like whole-tree coverage and is not.

**The generalisable lesson, since two rounds have now paid for it.** A guard against a false operator-facing claim should be built over the *property the claim is about*, with the binary as oracle, not over the sentence that happened to be false. Both earlier attempts here were searches for a known string — one for the phrase found, one for the form promised — and a search knows only the direction it was pointed. The tell is a lane whose failure message quotes a phrase: it can only ever catch that phrase, in that direction.

Origin: bean `fiddle-wr6v` (epic fiddle-eph7, M4a — proposed by holistic iteration 6, dispatched in the round after)
Tags: #decision #testing #bug
Status: 2026-08-19 — **the replacement lane did not, as first committed, guard the string this entry is about**, and the paragraph above overstated it. The inversion recorded there reverted the *help*; the fix had changed two sites, and reverting the other one — the `Malformed` `#[error]` message, signature unchanged, compiling clean — left the lane green: `1 passed; 0 failed`, exit 0. The revert was caught only by `inspect_rejects_a_malformed_invocation_ref` and `a_malformed_reference_is_reported_without_reference_to_configuration`, both of which assert the message text, which is precisely the coupling the new lane was built to replace. The cause was that the lane flattened each diagnostic into one string: `cve` appeared in the corrected advice, so "every surface names it" held, while the line above it went on calling the operator's reference illegal. The lane now holds each surface **part by part** — a diagnostic's verdict line and its advice are read separately — and forbids a part from giving a shape template unless it names the schemes that template is false of, where a template is a colon-joined pair whose scheme side is not one of the schemes read off the binary. Both `--help` surfaces satisfy that rule as written today (they show `<scheme>:<value>` and name `cve` in the same text), so it is not a ban on placeholders. What that state of the lane still did not cover is a universal denial written in **prose** rather than as a shape — "a value is required for this reference" would pass it — because the template is a **gate**: a rewording that drops the placeholder opens it. Two rules strong enough to catch that were weighed here and both recorded as rejected, and one of those rejections was wrong; the third entry below is where it was reopened and what replaced it.

The lesson is narrower than the last one and worth as much: **an inversion proves detection only at the site it was applied to.** This fix touched two sites — a message and a help — and one inversion was generalised to both. A fix with N changed sites needs N mutations, or the report has to say which site was inverted.

Origin: bean `fiddle-wr6v` continued (the guard was measured by the orchestrator, found green under the message revert, and dispatched back)
Tags: #decision #testing

Status: 2026-08-19 — **a guard that passes the mutation it was built against is weak evidence, and a different violation was constructed that it passed.** The evaluator's counterexample was the prose form of the same denial: with `Malformed`'s `#[error]` rewritten in place as "a value is required for this reference" — no colon, no placeholder, nothing for a shape detector to see — the part-by-part lane above exited **0**, measured, not reasoned. The class had been narrowed three times and named as closed each time.

**What closed it, and why it is a property rather than a longer phrase list.** The second clause added to the lane is gated on the **input**, not on the wording: a refusal of an input with *no colon in it* must name the standing-alone schemes in every part an operator reads. No rewording opens that gate, because whatever the sentence comes to say, it is said to a caller who typed no separator — the one caller for whom the bare form is a live repair. Both mutations now red by name and the message names the missing scheme: `` the `cva` diagnostic from inspect: the line that judges the reference answers an input with no colon in it and never names `cve` ``. A third mutation was constructed for the other clause, to check it still has teeth where it is the only one: a `<scheme>:<value>` template added to the *empty-value* verdict, whose input does carry a colon, reds with `` gives ["<scheme>:<value>"] as the shape a reference takes and never names `cve` ``.

**The rejection that was wrong.** The paragraph above rejected "requiring every part to name the standing-alone schemes" because it reds on the corrected verdict — true of the **unscoped** rule, and the scope is what was missing. Scoped to a colonless refusal it holds `beans:`'s verdict to nothing (its caller's repair is not a sweep) while holding the one arm a mistyped `cve` actually lands in. The cost is one clause of product text: `Malformed`'s message now ends "nor one of the schemes that discover their own work (cve)", derived from `InvocationScheme::listed_standing_alone`. That is not a concession to the test — it is the parts split applied to the message itself. A verdict travels without its help, and a verdict saying such schemes exist without saying which leaves the colonless caller with nothing to type.

**The surface inventory is no longer enumerated by hand.** The commands are read off `fiddle --help` and then **probed**: one is a grammar surface if it answers a malformed reference with `fiddle::invocation_ref::malformed`, so a third subcommand taking a reference joins the lane the day it is added, and `config`, which takes none, is not held to a promise it never makes. The vacuity guard is that the probe must both select and reject — a filter that keeps everything is not selecting on taking a reference. What remains written here is the *case analysis over inputs*: a one-letter typo of each standing-alone scheme (generated from the scheme set, not spelled — `cva` today), a token that is no scheme at all, and an empty value after an unknown scheme. A process cannot be asked to render every string it might print, so a sixth defect reachable only through a sixth input is still not discoverable from outside.

**The name was narrowed to what is held**, per the precedent from bean `fiddle-ye7n`: `every_scheme_that_needs_no_value_is_named_on_each_surface_and_in_each_colonless_refusal`. The old name read as whole coverage of the class and three iterations proved it was not. Two gaps remain and the doc comment states both where a reader meets them: a part that **names** the standing-alone schemes and denies them anyway ("`cve` requires a value") reds nowhere, because catching it means deciding whether a sentence contradicts the binary; and a prose denial in the verdict of a refusal whose input **did** carry a colon is outside both clauses, covered only by `an_empty_value_is_told_every_repair_its_own_scheme_admits`, which holds advice rather than verdicts.

**The lesson, since a fourth round paid for it.** An inversion proves detection at the site it was applied to, and a mutation proves it against the wording it was written in. A guard keyed on the *text* of a false claim can always be reworded around; a guard keyed on the *input that provokes it* cannot. Where a class has been narrowed three times, the question to ask is not "what other phrasing" but "what does the check read that the author of the next wording controls".

Origin: bean `fiddle-wr6v` continued (the evaluator constructed a violation the guard passed, and the lane was broadened and renamed)
Tags: #decision #testing

Status: 2026-08-19 — **the class is closed as far as an acceptance lane reaches, and the remaining dimension is accuracy, which review holds.** A fourth guard was considered and deliberately not built. The property the lane holds is **detectability**: every scheme that stands alone is *named* on each surface and in each colonless part. Naming `cve` is what makes a wrong description findable by a reader; it is not what makes the description right. So a verdict that names `cve` and misdescribes it passes, and this was measured rather than argued — `Malformed`'s `#[error]` replaced in place with "the normal form is a scheme and the item inside it, as in `beans:fiddle-m0-demo`, and that includes the schemes that discover their own work (cve)" leaves the lane green and the whole file green: `10 passed; 0 failed`, exit 0. That sentence names `cve`, writes no template (`beans:fiddle-m0-demo` is one real scheme's own example, not a claim about the set), and its last clause is false of the one scheme it is about.

**The wording first reached for was not the one that gets through, and the difference is the point.** `cve:<id>` presented as the normal form *does* red — `valued_mentions` sees `cve:` in a valued position, and the lane reports "shows `cve` carrying a value". Paraphrasing the same false claim without putting a value after `cve:` opens it. A gap is only as narrow as the wordings that reach it, so the example recorded is the one that passes, checked, not the one that reads worst.

**Why not a fourth guard: the regress is structural, not a shortfall of effort.** A lane can assert that a string is present, absent, or shaped a certain way. It cannot assert that a sentence *means* what it should. Each of the three guards built here narrowed the class and left a semantically-wrong-but-detectable case behind — a phrase hunt, then a shape template, then a naming rule — and a fourth would do the same, because the thing being approximated is comprehension. This is the same conclusion the entry above reached for a **source doc comment** contradicting the binary (bean `fiddle-ye7n`), arrived at from the other side: there the subject was the source and the lane read the binary; here the subject is the binary's own prose and the reader is the operator. Both land on review. The reasoning is not restated on the lane — the doc comment points at that entry, so the two do not drift apart.

**What is stated where a reader meets it.** The lane's doc comment now opens its limits section with the bound rather than with a list of gaps: the property is detectability and not accuracy, the passing example above with its measurement, review as what catches it with a pointer to the `fiddle-ye7n` entry, and the *input* inventory named as hand-enumerated — the commands are derived and probed, the three input cases are written there, so the property holds over the surfaces the lane reaches and not over surfaces nobody thought of. It also says plainly what the lane *does* hold, because a limits section that only subtracts misleads in the other direction: the colonless case is gated on the input, no rewording reaches it, and both earlier guards at this class were gated on text and both were reworded around. Stating a bound is not retracting the property it bounds.

**The lesson, and it is a scoping one.** Three iterations on one dimension, each failing on a newly constructed violation, is the signature of a dimension that is not reachable by the mechanism being aimed at it — not of a guard that needs one more clause. The tell is that each new counterexample is *constructed* rather than found in the tree: the author of the next wording controls the text, so any check that reads the text can be satisfied without the claim becoming true. When that pattern appears, the artefact worth producing is the bound, stated where a reader meets it, plus the review step that actually holds the residual. A stated bounded gap is a different artefact from a wrongly closed question, and only one of them decays quietly.

Origin: bean `fiddle-wr6v` continued (the evaluator constructed a third violation; the lead ruled the dimension unreachable as scoped and dispatched the bound rather than a fourth guard)
Tags: #decision #testing #documentation
