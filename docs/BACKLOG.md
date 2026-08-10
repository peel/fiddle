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

### 2026-08-10 — The preflight that makes `--ref main` legible is not on `main`
`.github/workflows/github-effects.yml` now refuses a ref carrying no Cargo workspace at a preflight step, before the toolchain install and the build, naming the reason and the milestone branch to pass instead. Proven by dispatching it against a throwaway ref built from `origin/main` plus that one file: run **31383731994**, `conclusion=failure`, failed at step 4 with the toolchain, the build and the walk all skipped — and by run **31383743533**, `conclusion=success`, the same workflow against `ci/github-effects-dispatch-proof` at `d52fc84`, walk confirmed to have run.

The gap is which copy a dispatch uses. `workflow_dispatch` resolves the *entity* on the default branch but runs the file **from the dispatched ref**, so `--ref main` gets `main`'s copy, and `main`'s copy is `aa86c60`'s — without the preflight. The exact invocation the preflight exists to make legible is therefore still the one that gets `could not find Cargo.toml` forty lines into a build log, and will be until either the milestone stack merges or the operator lands this one file on `main` the way `aa86c60` was landed. Nothing else is owed: no repointing, no second entity, no branch.

The same applies to `scripts/check-github-effects-lane.sh` and its fixtures, which run in `skill-quality.yml` from the ref being pushed. On `main` today that step does not exist, so the never-skip property is asserted on every milestone branch and not on `main` itself.
Origin: implementation (remediation R5, epic fiddle-srrw, bean fiddle-ufv3)
Tags: #debt #infrastructure
