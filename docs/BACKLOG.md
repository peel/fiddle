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
The design spec predicted 12KB for develop-loop. The actual size was 20.5KB, 71% over. JSON examples, code blocks, and HARD-GATE blocks are denser than prose. Measure the source line range directly.
Origin: implementation (develop modularization epic fiddle-wdg0)
Tags: #debt #optimization

### 2026-07-28 — PR-review feedback channel into calibration/antipattern memory
Harvest human review comments from finish-branch pull requests. Write them into the calibration and antipattern files. Today only the attended gate feeds those files. Epic fiddle-sip9 deferred this.
Origin: brainstorm (fiddle-sip9), research: humanlayer/skills design-control-loop
Tags: #idea #feature

### 2026-07-28 — Scheduled antipattern-eradication maintenance loop
A scheduled CI workflow reads deliver's antipattern files. A script scans for occurrences. The workflow picks one per run and an agent fixes it. One pull request stays open for a human to merge. Epic fiddle-sip9 deferred this.
Origin: brainstorm (fiddle-sip9), research: humanlayer/skills design-control-loop
Tags: #idea #infrastructure #experiment

### 2026-07-29 — check-convergence.sh budget-count convention and double-pass headroom
`check-convergence.sh` checks the budget before it evaluates the verdict. Post-dispatch counts therefore flag `DISPATCHES_EXCEEDED` for a run that passed. A pass on iteration N can never confirm within budget N. Choose pre-dispatch counting. Set the defaults with double-pass headroom.
Origin: implementation (epic fiddle-sip9, hit at both per-task and holistic budgets)
Tags: #debt #infrastructure

### 2026-07-29 — Holistic scorecard shape vs merge-scorecards input
`holistic-scorecard-schema.md`'s example puts domain and dimensions at the top level. It carries no criteria array. `merge-scorecards.sh` expects a domains wrapper and requires criteria. Nobody specified the wrapping step. Specify the shape or add a wrapper.
Origin: implementation (epic fiddle-sip9 holistic phase, fails loud exit 2 since the criteria validation)
Tags: #debt

### 2026-07-29 — develop-loop 1f wording and per-domain selected-provider files
Step 1f says "the evaluator may interact with the running app". Reword it to match the interpret-only role. Name the selection output `selected-provider-{domain}.json`. A multi-domain PASS_PENDING reuse then reads the right provider.
Origin: code-review (holistic review of epic fiddle-sip9)
Tags: #debt

### 2026-07-29 — patsub_replacement mangles ampersands in dispatch payloads
Bash 5.2 and later can set `patsub_replacement`. `dispatch-provider.sh` then rewrites `&` to the placeholder text during PROMPT substitution. It does this in `--diff-file` and `--evidence-file` content. Quote the replacement, or use a temp-free `jq` substitution.
Origin: code-review (fiddle-im2e confirming evaluation; pre-existing, affects --diff-file too)
Tags: #bug #debt
Status: Resolved 2026-08-05 by literal marker splitting in `dispatch-provider.sh`.

### 2026-07-30 — Three debug reference files were pointed at but never written
`skills/debug/SKILL.md` referenced `root-cause-tracing.md`, `defense-in-depth.md`, and `condition-based-waiting.md`. None has ever existed in `skills/debug/`. The pointers are dropped and their substance folded into the surrounding prose. Backward call-stack tracing, layered validation, and condition-based waiting need writing if they deserve full treatments.
Origin: implementation (epic fiddle-85jh, Claude-5 skill slim-down, utilities family)
Tags: #debt #idea

### 2026-08-08 — Permission-injection tests no-op silently under a root identity
Three tests in `crates/fiddle-runtime/tests/attempt.rs` return early on `if record.published.is_some() { return; }`. The escape hatch exists for an identity that ignores permission bits. Under a root CI runner the three pass without asserting anything. They do not skip visibly, so the fail-closed guarantees go unverified.
Origin: holistic review iteration 2 (epic fiddle-7lmw, bean fiddle-9mgy)
Tags: #debt #test

### 2026-08-08 — Acceptance lane parity is maintained by hand
`docs/technical/acceptance-repository.md` covers the in-repo `m0_skeleton.rs` and the external `scenarios/m0_skeleton.sh`. It says they "assert the same properties by design". It warns that divergence makes one of them the weaker proof. Nothing checks the parity mechanically. The two have already drifted once: the in-repo lane lacked the fail-closed step and the non-empty `attempt_id` assertion. CI names the in-repo lane, and later milestone seeds inherit it.
Origin: holistic review iterations 1 and 2 (epic fiddle-7lmw, beans fiddle-nciw, fiddle-89lv)
Tags: #debt #test

### 2026-08-08 — ASCII-only invocation values may reject M1's external identifiers
ADR 011 limits an invocation reference value at the parse boundary. It admits ASCII letters, digits, `-`, `_`, and `:`. That is the safe direction for path derivation. M1's `jira`, `scheduled`, and `scanner` references come from external systems. Those identifiers may hold non-ASCII characters, and the parser rejects them with exit 2. Confirm the real identifier formats before those adapters land.
Origin: implementation (epic fiddle-7lmw, bean fiddle-1p8q)
Tags: #idea #risk

### 2026-08-08 — ReportBundle.work_ref is Option<WorkRef> but the design requires it
Design §4.7 models `work_ref` as a required `WorkRef`. `crates/fiddle-core/src/report.rs` declares `Option<crate::identity::WorkRef>`. The runtime always supplies `Some` and the bundle always carries it. The type still permits `None`, and tests construct it. Tighten the type or amend the design.
Origin: deliver drift analysis (epic fiddle-7lmw)
Tags: #debt

### 2026-08-09 — A capability's attempt id is not the bundle's attempt id
`RepairConfig.attempt` names the per-attempt worktree. It also ends the evidence reference `repair:<changed>:<attempt>`. `capability/repair.rs` says a reader can tie that reference back to the record of the same attempt. A reader cannot. `fiddle_runtime::attempt` mints the run's id itself, so no caller can collide two bundles on one path. The CLI constructs the capability before that call, so the capability mints its own. Both ids are unique and nothing on disk is malformed. The cross-reference is not real.

Closing this decides where an attempt id is minted. Passing one into `AttemptContext` gives up the minted-once property. Handing the id to the capability at `execute` time changes the `Capability` trait.
Origin: implementation (epic fiddle-y1w6, Task 12 wiring the capability selection)
Tags: #debt
Status: Resolved 2026-08-09 through the `ExecutionGrant`. Recorded as `decisions/014-the-grant-carries-the-attempt.md`. Asserted from outside the process by `binary_repair::the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under`.

### 2026-08-09 — `[workspace] fixture` and `check` are absent from the approved schema enumeration
Design §6.6 enumerates `[workspace]` as `root`, `isolation`, `command_timeout`, `cleanup`, and `[agent]`. It names no repository and no check. `fiddle_runtime::RepairConfig` needs both. `deny_unknown_fields` leaves no other way to supply them. Task 12 therefore added `workspace.fixture` and `workspace.check = { program, args }` as `Option` with no default. Update the design text, or move the keys to the milestone that owns the deployment shape.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt
Status: First half resolved 2026-08-09. `docs/technical/SYSTEM.md`'s **Data** section documents `fiddle.toml` with both keys named. The second half stays open and is an ADR: whether these keys belong to the deployment shape at all.

### 2026-08-09 — `agent.max_capability_attempts` has no consumer
The outer attempt bound parses and defaults to 3. Nothing reads it. `fiddle_runtime::attempt` runs one attempt. It reports `RunOutcome::Retryable` for a caller to repeat. The key carries the one remaining `#[allow(dead_code)]` in `fiddle-cli`. Reading it means a retry loop. That changes what every existing retryable outcome does, M0's included. The milestone that owns the durable lifecycle owns this.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt #resolved
Status: Closed 2026-08-21. The bound is enforced, and no retry loop was built. `docs/technical/decisions/037-the-attempt-bound-is-per-pull-request.md` counts attempts across processes in the pull request body, so the premise that reading the key means a loop was wrong. ADR 013 carries the amendment.

### 2026-08-09 — The second interrupt's exit path is untested
`fiddle run --capability fixture_repair` installs a `SIGINT` handler. The first interrupt cancels the token and the second exits 130. `capability_selection::an_interrupt_cancels_the_attempt_rather_than_killing_the_runner_under_it` pins the first. Nothing pins the second. A cancelled attempt concludes in tens of milliseconds, and no test races a second signal into that window reliably. A deterministic test needs a capability that hangs after cancellation.
Origin: implementation (epic fiddle-y1w6, Task 12)
Tags: #debt

### 2026-08-09 — What M1's isolation does not claim: egress, injection, hostile processes
The ephemeral worktree, the `env_clear` allowlist, and `WorkspacePath` containment bound two things. They bound what an attempt reaches on this filesystem. They bound what it sees of the host environment. Three things they do not bound, named in no other document.

Network egress is not sandboxed. The check command runs with a real `PATH`, and nothing stops a build script or a test opening a socket. Prompt injection from repository contents is untested. `read_file` returns whatever the fixture holds, and no scenario places adversarial instructions in a file. Hostile-process containment is out of scope. The process group and the timeout stop a hung child, not a determined one. A milestone that runs this capability over a repository it did not author must revisit all three.
Origin: implementation (epic fiddle-y1w6, M1 threat-model boundary)
Tags: #risk #debt

### 2026-08-09 — `RunOutcome::Suspended` is the one exit-code row never exercised end to end
A real scenario drives every other row of the exit-code table. `Suspended` maps to exit 10. It needs a human decision point that neither M0 nor M1 has. `main.rs` covers it in a unit test of the mapping function. No capability can be driven into producing it. The row stays untested in the table an operator reads first. The milestone that introduces an attended decision closes this.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt #test

### 2026-08-09 — Claude-family models finalise after one tool call through this gateway
Measured over the trivial Tier 1 fixture at reference bounds. `claude-haiku-4-5` makes one `list_files` call and stops. `claude-sonnet-5` does the same. It then fails its own report schema with `missing field changed_files at line 1 column 11`. `bedrock/moonshotai.kimi-k2.5`, `deepseek.v3.2`, and `zai.glm-5` each drive the full loop and earn the marker. Nothing pins the mechanism.

Sonnet's diagnostic suggests two things. It calls the synthetic output tool `OutputMode::Tool` registers, with the wrong arguments. The rig's re-prompt path does not recover from that. Both are a hypothesis. This is a property of the gateway's translation. The deterministic suite cannot see it (ADR 012). Both real-model tiers therefore default to kimi.
Origin: implementation (epic fiddle-y1w6, Task 14 Tier 1 measurement)
Tags: #risk #debt

### 2026-08-09 — `read_file` and `list_files` are uncapped
Neither tool bounds what it returns. `read_file` hands back a whole file at any size. `list_files` walks the whole worktree. Both results bill as input tokens. Over M1's trivial fixture this is invisible. Over a real repository one call exhausts the context window or the budget. `max_tokens` bounds the completion, not the prompt. A cap needs a decision about truncation. A model then acts on the truncated view, and the cap must say what that view looks like.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt #risk

### 2026-08-09 — `CheckFailed.stderr` is unbounded and reaches a published bundle
`CapabilityError::CheckFailed` carries the check's stderr verbatim, so an operator can see why the repair was refused. The rendered error reaches the evidence bundle. The check is an arbitrary operator-configured program. A failing `cargo test` over a real project emits kilobytes, and nothing truncates it. The path is already relativised, so this is a size problem and not a leak.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt

### 2026-08-09 — Receipts publish as a summary; durations are collected and unpublished
`ToolReceipt` carries `tool`, `outcome`, and `duration_ms`. The bundle receives `tools:<n>` plus per-tool outcome counts as `EvidenceRef` strings. `EvidenceRef` is a string, and the bundle's evidence is a list of them. The receipts therefore have no home in the report schema. Widening a published contract was out of scope for the task that added them. `duration_ms` is measured on every call and read by nobody. Giving receipts a typed home is a schema change.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt

### 2026-08-09 — `WorkspacePath` rejects `a:b/c.rs`, a legal Unix filename
`WorkspacePath::parse` refuses any path whose second character is `:`. That is the Windows drive-letter shape. It is a cheap syntactic rule that no race can defeat. It also rejects legal Unix filenames. `a:b/c.rs` names an ordinary file, and no model can read or write it through these tools. No M1 fixture is affected. A capability pointed at a real repository holding such a path refuses it. The diagnostic says the path escapes the workspace, which is not what happened.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt

### 2026-08-09 — The broken-fixture builder is duplicated in three places
Three places each build the same small broken crate: `crates/fiddle-runtime/tests/fixture.rs`, the unit tests in `crates/fiddle-runtime/src/capability/repair.rs`, and `crates/fiddle-cli/tests/smoke.rs`. Three copies exist for two reasons. A `src/` unit test cannot reach a `tests/` module. One package's `tests/` module cannot be reached from another package. Removing the duplication needs one of two things. A `#[cfg(feature = "test-fixtures")]` module in `fiddle-runtime`, or a fifth workspace member holding thirty lines of `std::fs::write`. Both are larger than the duplication they remove. The cost of keeping it is that a change to the fixture's shape must be made three times.
Origin: implementation (epic fiddle-y1w6, Tasks 13 and 14)
Tags: #debt #test

### 2026-08-09 — Tool-output relativisation is a prefix rewrite, not a redactor
`relativised` rewrites both spellings of the workspace root out of a check's stdout and stderr. That stops `cargo`'s `Compiling foo (/…/ws/<attempt>)` from handing over the operator's directory layout. It is a string replacement. A child process may print any other absolute path. It may print a toolchain in the Nix store, a registry checkout under `~/.cargo`, or a path in a panic message. The function guarantees the model cannot learn where this attempt is working. It does not guarantee that no host path reaches the model.
Origin: implementation (epic fiddle-y1w6)
Tags: #debt #risk

### 2026-08-09 — Single-provider plan critique is now the intended state, not a gap
`gemini` is removed from provider dispatch in `orchestrate.json`. It failed authentication with exit 41 in both the M0 and the M1 plan critique. Every multi-provider critique this project has run was therefore a single-provider one in a two-provider configuration. The missing second opinion looked like a degradation rather than a decision. The removal makes the configuration match reality. Restoring adversarial breadth means fixing gemini authentication, or adding a different second provider.
Origin: implementation (epic fiddle-y1w6, plan critique)
Tags: #debt #infrastructure

### 2026-08-09 — The design's credential-scrub requirement is stale, and the code is right
M1 design §6 item 4 states that "the acceptance lanes scrub [`LITELLM_API_KEY`] alongside the four M0 already removes". They do not, and should not. `support::CREDENTIAL_VARS` is four names: `GITHUB_TOKEN`, `GH_TOKEN`, `ANTHROPIC_API_KEY`, `JIRA_API_TOKEN`. An assertion inside `m0_skeleton.rs` pins that list, and `peel/fiddle-acceptance` mirrors it by hand. Extending it is therefore a two-repository change. It would also prove nothing. The M0 scenario runs `stub_mark`, which never reaches a model. `capability_selection.rs` proves the property more strongly. It sets `LITELLM_API_KEY` to a sentinel. It asserts the sentinel appears in no stdout, no diagnostic, and no published bundle. Close this by amending the design text.
Origin: implementation (epic fiddle-y1w6, Task 15 verification)
Tags: #debt
Status: Resolved 2026-08-09. `docs/technical/SYSTEM.md`'s M0 acceptance paragraph states the four-name list. It states why the list is not extended per milestone. It states that `capability_selection.rs`'s sentinel assertion covers `LITELLM_API_KEY`.

### 2026-08-09 — What the scripted-gateway acceptance lane proves, and what it does not
Nothing gated `build_capability`'s document-to-capability wiring. `crates/fiddle-acceptance/tests/binary_repair.rs` closes that gap. It binds a loopback port. It answers the OpenAI chat-completions requests the real gateway client sends. It drives the compiled binary through a repair that writes the fix, passes the configured check, and earns the correlation marker. It runs offline with a sentinel credential.

Three things it does not prove. First, only one bound travels from the document into `AgentBudget`. The paired scenario flips `max_turns` from 4 to 1 and watches the outcome change. `max_tokens`, `deadline`, `max_changed_files`, and `tool_timeout` are carried by nothing but the code reading right, so a swap of two of them leaves this lane green. Second, the model is scripted. The lane says nothing about whether a real model drives the loop. That is Tier 1's job, and Tier 1 never asserts it either. Third, its check compiles nothing. It greps for the repaired text rather than running the fixture crate's suite, for the reason recorded under *A workspace check cannot find the macOS SDK* below. Closing the first needs one scenario per bound whose configured value is what stops the run.
Origin: implementation (epic fiddle-y1w6, holistic remediation of the M1 seams)
Tags: #debt #test

### 2026-08-09 — `inspect --capability` names a capability `run` might refuse to build
`inspect` takes the same `--capability` flag as `run`. The two cannot disagree about which capability is next. They still differ about whether it can be run. `inspect` carries the id as far as `derive_next` and builds nothing from it. So `fiddle inspect beans:x --capability fixture_repair` reports `execute fixture_repair` over an M0 document with no `[agent]` table. `fiddle run` over the same document exits 2 and names the missing table.

This is deliberate. Validating the deployment means `inspect` resolving a credential, which ends its read-only, offline, credential-free contract. A caller who reads `inspect` as "this will work" reads more into it than it says. If that confusion becomes real, the fix is a configured field in the `inspect` payload. Derive that field from which tables are present, never from resolving anything.
Origin: implementation (epic fiddle-y1w6, holistic remediation of the M1 seams)
Tags: #debt

### 2026-08-09 — A workspace check cannot find the macOS SDK, because the allowlist has no locator for it
`workspace::command` builds a child's environment from `env_clear` plus two inherited locators, `PATH` and `RUSTUP_HOME`. The rule is that a locator may be inherited and an authority may not. On macOS under this project's Nix dev shell that list is one entry short for anything that links. The shell also exports `DEVELOPER_DIR`, `SDKROOT`, `MACOSX_DEPLOYMENT_TARGET`, `NIX_LDFLAGS`, and `NIX_CFLAGS_COMPILE`. Stripped of them, a nested `cargo test` prints `warning: failed running "xcrun" "--sdk" "macosx" "--show-sdk-path" to find MacOSX.sdk … unable to find sdk: 'macosx'`. It then links against whatever it can find.

The consequence is measured. The black-box repair lane ran with `check = cargo test --offline`. The test binary failed nine consecutive runs with `error: test failed`. The tree's source was verifiably repaired, and its tests pass under the shell's intact environment. The same binary then passed twenty-nine consecutive runs, eight of them under deliberate load, with no source change. `repair_protocol` gates the same check and is exposed to the same thing. Nobody has seen it fail, so this is a latent flake in the gate. `binary_repair` avoids it by pointing its check at a program that compiles nothing.

Closing this decides whether an SDK path is a locator in the module's sense. It says where a toolchain is and grants no authority, which is the `RUSTUP_HOME` argument. It also decides which Nix compiler variables belong on the list.
Origin: implementation (epic fiddle-y1w6, holistic remediation of the M1 seams)
Tags: #debt #test #risk

### 2026-08-09 — What M1's isolation does not claim, the other two: provider serialization and model refusal
M1's design named five boundaries. The entry *What M1's isolation does not claim: egress, injection, hostile processes* recorded three. It read as the whole list. The other two are here.

**Provider-specific serialization is not claimed.** Every deterministic assertion about the tool protocol is made against what our client builds. None is made against what an upstream provider receives. `MockCompletionModel` replaces the provider and serialises nothing. `binary_repair.rs` serialises a real OpenAI chat-completions request only to its own loopback socket. That proves our client speaks the wire format. It says nothing about LiteLLM's translation of it. The `OutputMode::Auto` defect in ADR 012 shows the boundary has teeth. A gateway reported `composes_native_output_with_tools()` truthfully about OpenAI and falsely about itself. The model then called no tools, and no deterministic test could see it. Revisiting needs a lane that inspects the request as the upstream provider received it. That needs a gateway that echoes its translated request, or a per-provider fixture corpus from real traffic. It also needs a decision about which providers are in scope, because the translation differs per upstream.

**Model refusal and truncation behaviour is not claimed.** The taxonomy covers the model producing the wrong shape. `AgentError::Protocol` covers a report that misses the schema, empty content, and an unregistered tool name. It does not cover a policy refusal. It does not cover a completion cut short by `max_tokens` mid-tool-call. It does not cover a long tool result truncated on the way back. All three arrive as a schema failure or an ordinary unrepaired fixture. The operator then reads "the model did not hold up its end" for a run where the model was stopped. Revisiting decides whether a refusal is a distinct outcome class or a `Retryable`. It interacts with the uncapped `read_file`/`list_files` entry. A truncation the runtime causes and one the provider causes must not look the same.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — design §6.8 named five boundaries, three were recorded)
Tags: #risk #debt

### 2026-08-09 — A spend-cap refusal is not distinguishable from any other provider failure
The gateway key carries a $100 hard cap. Requests will eventually fail on spend rather than on correctness. Every run from that moment reads as a broken capability. `AgentError` has four variants. `agent::classify` matches Rig's typed variants rather than its message text. A spend-cap refusal is an HTTP error with no typed variant. It lands in the wildcard arm as `Provider { reason }`, carrying Rig's rendering of the response body. `scripts/tier2.sh` records the outcome kind and the first 300 characters of that reason. Tier 1 does not classify at all. ADR 012 states this as an open consequence.

Nothing classifies it because the only signal is the gateway's error text, and the cap has never been reached. A classifier written against a guessed string fails open silently while claiming the coverage. No test can pin it. Closing this needs a gateway key minted with a token `max_budget`, spent deliberately, and the response captured. It then needs one of two things. A fifth `AgentError` variant in `crates/fiddle-runtime/src/agent/mod.rs`, or a typed field on `Provider` that `scripts/tier2.sh` can key on without parsing prose.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — ADR 012 consequence, deferred rather than asserted)
Tags: #debt #risk

### 2026-08-09 — Three backlog actions resolve to amending a gitignored file, and cannot be closed as written
`.gitignore` excludes `docs/plans/`, `docs/specs/`, and `.beans/` wholesale. A backlog action whose resolution is "amend the design" therefore names a document that exists on one machine. The stated durable fallback, the bean body, is gitignored too. This entry supersedes the actions of three entries above. Their findings stand.

- **2026-08-08 — `ReportBundle.work_ref is Option<WorkRef>`** ends "Either tighten the type or amend the design." Only the first half is closable. Change `crates/fiddle-core/src/report.rs` to a required `WorkRef`. Fix the tests that construct `None`. Add the guarantee to `docs/technical/SYSTEM.md`'s invariant list. If the `Option` is judged correct, say why in that same list.
- **2026-08-09 — `[workspace] fixture` and `check` are absent from the approved schema enumeration** asks the design text to catch up. `docs/technical/SYSTEM.md`'s **Data** section already documents both keys, so the entry is closable. The second half is an ADR: whether these keys belong to the deployment shape. Relocating them changes a document an operator writes by hand.
- **2026-08-09 — The design's credential-scrub requirement is stale, and the code is right** asks for an amended design text. `docs/technical/SYSTEM.md`'s M0 acceptance paragraph already states the four-name list. It states why the list is not extended per milestone. It states that `capability_selection.rs`'s sentinel assertion covers `LITELLM_API_KEY`. Mark it closed.

The rule this establishes. Write a backlog action against `docs/BACKLOG.md`, `docs/technical/SYSTEM.md`, an ADR under `docs/technical/decisions/`, or a named code path. Never against `docs/specs/`, `docs/plans/`, or a bean body.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — holistic_spec_fidelity)
Tags: #debt #process

### 2026-08-09 — What the committed-ignore-rule boundary still does not cover
Deriving the changed-file set under the project's committed ignore rules closes one case. That case is an attempt writing `.gitignore` to hide what it created. Three things stay open. Each is documented at `Workspace::baseline_ignore` in `crates/fiddle-runtime/src/workspace/mod.rs`.

- **A file written into a path the project already excludes is not counted.** `write_file("target/x")` lands where the committed rules exclude. `changed_files()` does not name it, and the cap does not see it. The alternatives are worse. Counting the whole `target/` tree drowns the evidence. Letting the worktree's own rules decide is the defect just fixed. The gap earns an attempt nothing: the check still decides the verdict, and a marker still requires a passing check. Closing it needs a rule about where a repair may write. A first step is refusing writes under a baseline-ignored directory, which `git ls-files --others --ignored --exclude-from --directory` can enumerate.
- **Ignore files in subdirectories are not honoured.** `--exclude-from` reads one flat list whose patterns are relative to the top. Concatenating nested files would change what they mean. A monorepo keeping its build-output rule in `crates/foo/.gitignore` has that output counted. The error is towards reporting more, which is the safe direction. No fixture here has a nested ignore file.
- **`Workspace::read` runs one `git ls-files` per call.** The filesystem cannot answer whether a file belongs to the project, so the read path asks git. Over a repository with tens of thousands of tracked files this is a subprocess and a full listing each time. A model that reads freely pays it repeatedly. Caching needs an invalidation story, because the model creates files as it goes.
Origin: implementation (epic fiddle-y1w6, bean fiddle-93cj — recorded while fixing the changed-file derivation, not deferred from it)
Tags: #debt #risk

### 2026-08-09 — The outer attempt bound's absence is now a decision, and this is what closes it
Supersedes the action of **2026-08-09 — `agent.max_capability_attempts` has no consumer**. `docs/technical/decisions/013-one-attempt-bound-not-two.md` prices the change. `RunOutcome::Retryable` has four producers, and only one means the capability tried and lost. A retry loop therefore needs a taxonomy the outcome type does not carry. Both placements move something committed. Inside `run` it changes the shape of `capability_executions` and `progress` that every bundle consumer has seen. Inside `attempt` it breaks the one-process-one-attempt-id premise that `fresh_invocation.rs` and `m0_skeleton.rs` both read. Taking it up means points 1 to 4 of that ADR in order, taxonomy first.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-9v2d)
Tags: #debt #process #resolved
Status: Closed 2026-08-21 by ADR 037, and points 1 to 4 were never taken. Both placements this entry priced were in-process. The bound is spent across processes instead, and the count lives in the pull request's body, so `Retryable` keeps its four producers and `attempt` still mints one id per process.

### 2026-08-09 — What creating a directory on the model's behalf does not close
`Workspace::resolve` walks to the deepest existing ancestor. `Workspace::write` makes the intervening directories. The same canonicalize-and-compare proves each one inside the workspace, before creation and again after. Three residues remain.

- **The check-to-write window is narrower, not closed.** Nothing stops another process replacing a resolved component with a symlink between the containment check and the write. Re-canonicalizing the parent after `create_dir_all` narrows the window creation opened. It does not remove the window `std::fs::write` always had. Inside a per-attempt worktree the only other writer is the operator's own `run_check` program. Closing it properly means `openat`-style resolution against a directory handle. `cap-std` is the obvious candidate, and that is a dependency decision.
- **An empty directory a failed write left behind is invisible to the evidence.** git tracks files. A `write_file` that resolved, made `src/newmod/`, and then failed to write leaves a directory `changed_files()` never names. No content is in it. "The workspace is as the attempt left it" is one directory weaker than the changed-file set says.
- **Nothing bounds how deep a model may build.** `max_changed_files` caps the files. The directories on the way are uncounted. It becomes a question with the uncapped `read_file`/`list_files` entry.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-9v2d)
Tags: #debt #risk

### 2026-08-09 — Two claims above are corrected: the check's stderr *was* a leak, and the relativisation entry named only half its readers
**2026-08-09 — `CheckFailed.stderr` is unbounded and reaches a published bundle** says "The path is already relativised, so this is a size problem rather than a leak." It was not relativised. `relativised` had two call sites, both inside the `run_check` tool. It protected the string handed to the model. `FixtureRepair::execute` calls `workspace.run(&config.check)` directly. It puts `check.stderr` into `CapabilityError::CheckFailed`. `orchestration::run` renders that into `RunOutcome::Retryable.reason` and `ProgressEntry.summary`. The absolute worktree path the model is protected from was published in `report.json` and printed on stdout.

Both halves are closed. Relativisation moved into `Workspace::run`, the one place a `CommandResult` is constructed. The size half is closed by `fiddle_core::Published`, the type of all four free-text bundle fields, which bounds them to `PUBLISHED_TEXT_LIMIT` characters.

**2026-08-09 — Tool-output relativisation is a prefix rewrite, not a redactor** is right about the function. It understates who reads it. "Before the model sees them" names one of two consumers. The published bundle is the other, and its readers are not sandboxed. That entry's residue is unchanged. Nothing rewrites a Nix store path, a `~/.cargo` checkout, or a path in a panic message. That is now true of the bundle as well.

Three things stay open:
- `Published` bounds size and nothing else. It is deliberately not a redactor, because a denylist over content an adversary chooses is not a guarantee. Two channels could carry a secret and both are handled where text enters. `agent::provider_fault` never quotes a provider response body. A workspace command's output is relativised at construction. A third such channel added later gets the bound and not the analysis.
- **A gateway that echoes a fragment of the credential is covered by nothing.** `provider_fault` withholds the whole body. Any future path quoting provider text selectively is uncovered. The general fix is a scrubber registered with the resolved credential at the one place it is read. That is a process-wide mutable registry, and therefore an ADR.
- **`NextAction::Blocked.reason` is still a bare `String`** and is published. `fiddle-core` derives its content from an observation, so it is host-authored and short today. The argument for that is a property of the current deriver rather than of the type.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-joen)
Tags: #debt #risk #security

### 2026-08-09 — Two entries above are closed: the evidence cross-reference now resolves, and the unenforced bound is visible at runtime
**2026-08-09 — A capability's attempt id is not the bundle's attempt id** is closed. That entry named two ways out and the second was taken. It was taken through the `ExecutionGrant` rather than a new parameter, so `Capability::execute`'s signature is unchanged and one argument's type is wider. `ExecutionGrant::authorise` takes the derivation and the attempt it is issued under. `RunContext` carries the id `fiddle_runtime::attempt` already minted. `RepairConfig.attempt` is gone. `FixtureRepair` reads `grant.attempt_id()` for both the worktree name and the evidence suffix. Minting did not move, so the collision property is untouched. `fiddle_runtime::mint_attempt_id` is no longer re-exported at the crate root. Recorded as `docs/technical/decisions/014-the-grant-carries-the-attempt.md`. Asserted from outside the process by `binary_repair.rs::the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under`.

**2026-08-09 — `agent.max_capability_attempts` has no consumer** is closed as a visibility matter. It stays open as a behaviour one. The retry loop is still not built, and ADR 013 still prices it. A document writing `max_capability_attempts = 5` is no longer told it is simply valid. `config check` reported `{"configured": 5, "enforced": 1, "status": "accepted-not-enforced", "decision": "013-one-attempt-bound-not-two"}` in `--json`, and said the same in prose. The behaviour half closed on 2026-08-21, and that payload is false at the head. ADR 013's M4b amendment holds what replaced it. Every bound that fires stays a plain scalar, so the shape tells the two kinds apart. Design §6.6 promises a deferred key is loud rather than silent under `deny_unknown_fields`. This key escaped that by being known rather than unknown. ADR 013's consequences section is corrected: it asserted the edge was findable in "exactly two places" and "not surfaced at runtime". The `#[allow(dead_code)]` is gone with it.

Three things stayed open, and two are now closed.
- **`ENFORCED_CAPABILITY_ATTEMPTS` is a literal in `crates/fiddle-cli/src/render.rs`.** It says `1` because nothing loops. Nothing checks that against the runtime. It is a hand-maintained mirror of a property of `fiddle_runtime::attempt`. Build a retry loop without changing this constant, and `config check` reports the wrong number confidently, which is worse than the silence it replaced. The milestone that builds the loop must change the constant, drop the object for a plain scalar, and delete ADR 013's consequences section. The honest version is the runtime exporting its own attempt bound. **Closed 2026-08-21, and the hazard it named happened.** The bound started firing in M4b and the constant was not changed, so `config check` reported `1` confidently for as long as it took four lanes to notice. The constant is deleted. `configured` is now the only number, because it is the bound.
- **`config check` reports one accepted-but-unenforced key, and no discipline would catch a second.** A future key that parses, defaults, and fires nothing gets a plain scalar and the same silence. Only the renderer marks the distinction. Making it structural means the type of a not-yet-enforced bound differing from an enforced one. That is a schema change and an ADR.
- **The human `config check` rendering is asserted nowhere.** The `--json` payload is pinned field by field from outside the process. The prose beside it is covered only by the credential-leak scenario, which asserts what it must not contain. A rename that dropped a line would leave every gate command green. **Narrowed 2026-08-21.** `config_check_reports_the_attempt_bound_it_enforces_and_where_the_count_lives` now reads the prose for the attempt bound's line, and the decision statuses stay asserted only in the payload. Every other key's prose line is still unasserted.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-wvsf)
Tags: #debt #resolved

### 2026-08-09 — The `output_mode` line is inert on the typed path, and the request shape is right for a reason nobody had checked
`crates/fiddle-runtime/src/agent/mod.rs` sets `.output_mode(OutputMode::Tool)`. Its rationale claims Tool mode "registers the schema as a synthetic tool the model calls to finalise, and sends no native constraint, so the four real tools stay callable". The serialized chat-completions bodies the compiled binary puts on a socket say otherwise. The measurement is a committed test.

**What goes out.** Turn 0 carries the four capability tools and no `response_format`. The finalising turn carries the same four tools. It also carries `response_format: {type: json_schema, json_schema: {name: "RepairReport", strict: true, …}}`. No synthetic `final_result` tool is advertised on any turn.

**Why.** `rig_agent`'s `TypedPromptRequest::from_agent` overwrites the agent's `output_mode` with `OutputMode::Native` unconditionally. Its own comment says typed prompts deserialize the model's final string. It says the untyped `output_schema`/`output_mode` API is what to use for tool-composing structured output. `prompt_typed::<RepairReport>()` therefore discards the builder's choice. Deleting the line gives a byte-identical shape.

The shape that goes out is, by measurement, the working one. A first turn carrying tools and no constraint is the request this gateway answers with a tool call. The observation and the outcome were right. The diagnosis in the doc block was wrong, and that block is corrected. The line stands as the statement of intent for the day rig's typed path stops overriding it. `binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact` pins the shape in both directions.

Open: whether to move to the untyped `output_schema`/`output_mode` API. That is the only way to get the mode that was asked for. It changes what goes out on every turn. Nothing in the gate can tell whether that is better against a real gateway. The deterministic suite never serialises to anybody, and `binary_repair` answers itself. Closing it needs a Tier 1 measurement per mode across the models in ADR 012's table.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — found writing the serialized-request test `m1-tool-protocol-correctness` asked for)
Tags: #debt #risk

### 2026-08-09 — The workspace command allowlist was stated four ways, and this is the one statement
`workspace::command` builds a child's environment from `env_clear` plus exactly four names. `HOME` points at the workspace's scratch home. `LANG` is fixed to `C`. `PATH` is inherited from this process, or set to `/usr/bin:/bin` when it has none. `RUSTUP_HOME` is inherited only when the parent has one. `workspace::a_workspace_command_inherits_no_credential` asserts both shapes exactly. A fifth name cannot be added without changing an assertion.

Four documents said four different things, and none said that. `docs/technical/SYSTEM.md`'s component paragraph said "a two-name allowlist". Its own invariant named `PATH` and `RUSTUP_HOME` and did not mention `HOME` or `LANG`. `docs/evaluator-calibration-general.md` said "an explicit `HOME`/`PATH`/`LANG` allowlist" and omitted `RUSTUP_HOME`. `binary_repair.rs` said "an allowlist of two locators". Each was true of the fragment its author was arguing about. Each read as a statement of the whole. The statement now lives once, in SYSTEM.md's Invariants.

This corrects the opening sentence of **2026-08-09 — A workspace check cannot find the macOS SDK, because the allowlist has no locator for it**. That entry's finding is unaffected. `PATH` and `RUSTUP_HOME` are still the only two locators. Whether `DEVELOPER_DIR` and `SDKROOT` join them is still open. Only its count of the whole environment was short by two.
Origin: implementation (epic fiddle-y1w6, holistic remediation iteration 1 — bean fiddle-i7f3, coherence)
Tags: #debt #process

### 2026-08-09 — Holistic review iteration 4: accepted with findings recorded
Epic `fiddle-y1w6` (M1) was accepted without the holistic review converging. Four iterations ran and each found different defects. Severity fell round on round. First a tool loop that called no tools. Then a capability that ran the wrong thing and reported success. Then forgeable changed-file evidence. Then a credential in the published bundle. Then the items below. None of these produces a wrong verdict or leaks a secret.

Iteration 4 scored integration 6/7, coherence 6/7, holistic_spec_fidelity 8/8 (passed, up from 7), polish 6/6, runtime_health 9/9. The user decided to stop, on the severity curve rather than on the scores.

**Worth fixing first:**

1. **`Workspace::write` and `Workspace::read` disagree about what the project is.** `read` gates on `list()` and refuses anything outside it as `NotProject`. `write` consults neither `list()` nor the baseline ignore rules. So `write_file("target/x")` succeeds and is invisible to `changed_files()`, which makes `max_changed_files` evadable. Nothing is earned. The check decides the verdict, and the `.gitignore` channel is closed and tested. The fix gives `write` the same `list()` test. No test covers this.
2. **`run_check`'s leak test uses the narrow root set.** `agent/tools.rs` defines `layout()` as everything about where this attempt runs that the model must never be told. That is the workspace, the fixture repository, the containing directory, and the attempt id. It applies `layout()` to `read_file`'s test and the narrower `roots()` to `run_check`'s. `relativised` strips only the workspace root, and the fixture repository is a sibling. A check that shells out to git can therefore print the fixture path to the model, as can a `build.rs` that reads VCS info. SYSTEM.md's invariant states the rule absolutely.
3. **A git failure masks the milestone's central error.** `capability/repair.rs` derives `changed` with `?` before the exit-code gate. A failure asking git what changed therefore turns what should be `CheckFailed` into `CapabilityError::Workspace`. Moving the line below the gate costs nothing.
4. **ADR 012 predates the work that refuted it.** It still states `OutputMode::Tool` as the operative mechanism. The wire shows rig overwrites it with `Native`, and the line is inert. Iteration 3 corrected the code doc, this file, and the calibration. Nobody corrected the ADR. Its budget consequence rests on `tier2.sh`'s 300-character reason excerpt letting a human spot a spend cap. After the `provider_fault` fix that excerpt reads only `the gateway answered <status>`. Commit `e993f4a` changed `DEFAULT_MODEL` in the same commit that added the inert line, so what made the tool loop work is unattributed. SYSTEM.md routes a cold reader to ADR 012.

**Recorded, lower value:**

5. The changed-file cap is applied to a pre-check tree. The published `repair:<n>` counts a post-check one. M1's fixture ignores `target/` and `Cargo.lock`, which hides the divergence.
6. `fiddle_core::Published` gates `RunOutcome`'s three reasons and `ProgressEntry::summary`. `NextAction::Blocked.reason` and `Observation::Unavailable/NotApplicable.reason` are bare `String`s, and are also published. `orchestration.rs` bounds one copy of the same string and publishes the other unbounded in one expression. `published.rs`'s module doc asserts an exhaustive enumeration that is wrong.
7. `agent/mod.rs` states "a provider's response body is never quoted, on any path". `classify`'s `DeserializationError` arm renders serde's diagnostic over the gateway's success content, quoting the offending value verbatim. The doc justifies this as model-authored. That conflates the model with the gateway ADR 012 exists to say sits between them. Either the rule or the code must move.
8. `agent/mod.rs` still carries `// **The line that makes the tool loop happen at all.**` forty lines below the section saying the line is inert.
9. `SYSTEM.md` states the `fiddle-core` purity denylist as five names. `crate_boundary.rs` has six. The omitted one is `rig-agent`, the name M1 added.
10. SYSTEM.md's Invariants state neither `PUBLISHED_TEXT_LIMIT` nor the never-quote-a-provider-body rule. Both came out of the epic's one critical-severity bean.

**A note for M2.** The holistic instrument scores against thresholds calibrated for a finished product. Each round's remaining findings are therefore smaller while the bar stays fixed. Four rounds of one point under threshold can continue indefinitely on a sound system. Consider scoring severity explicitly. Or converge on "no finding that changes a verdict or leaks a secret".
Origin: holistic review iteration 4 (epic fiddle-y1w6)
Tags: #debt #m2-input

### 2026-08-09 — Items 4 and 8 discharged: ADR 012 no longer states a refuted mechanism
ADR 012's `OutputMode` consequence and its budget consequence both described a system that had changed underneath them. `SYSTEM.md` routes a cold reader straight to that ADR. Both are corrected in place rather than superseded, because the decision the ADR records was never in question.

The `OutputMode` consequence keeps the gateway measurement and retracts the mechanism. `TypedPromptRequest::from_agent` overwrites the mode with `Native` unconditionally, so the builder line is inert. The "Tool mode is best-effort where Native was guaranteed" cost is not paid. `binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact` pins what goes out. The attribution now names both changes that landed in `e993f4a`: the inert line, and `DEFAULT_MODEL` moving from haiku to kimi. It says the cause was never isolated. The budget consequence states what `tier2.sh` records after `4b2333b`: `the gateway answered <status>` and nothing else. Item 8 went with it: the inline comment at the `.output_mode` call now says the line is inert.

This closes no underlying gap. No isolated per-mode measurement exists. No typed signal for a spend-cap refusal exists. Only the documents that lied about them are fixed.

### 2026-08-09 — The develop loop re-derives the same orientation once per bean
Measured across M2's nine completed beans. Orientation averages 5.7 minutes of a 23.4 minute bean. It barely varies with bean size. One bean spent 8.0 minutes orienting to do 2.5 minutes of work. Every fresh implementer reads the same prior-task sources, the same epic `## Contracts`, and the same antipattern history.

Do not trade the fresh context away. `skills/develop-loop` dispatches a new implementer per bean so a previous bean's rationalisations do not carry forward. The milestone's best catches came from implementers reasoning from sources. They caught a NUL identity collision, a criterion naming a deleted API path, and three separately self-caught vacuous tests.

Try instead, in order of expected value:
- Have the lead distil each completed bean into the findings the next implementer needs, and hand those forward in the prompt. M2 did this for antipatterns and it worked. Task 6 caught the same vacuous-test hazard Task 5 had found, because it was told.
- Put the durable half in the epic's `## Contracts` section, which every bean already reads. It carries types and constraints today, not findings.
- Measure whether orientation shrinks when a bean's prompt names what changed since the last bean rather than what exists.

Two process wastes found alongside this are fixed. Named regression lanes were re-run individually after a clean full-workspace run had printed the same counts. Each verification issued about 18 separate `nix develop -c` entries. `scripts/gate.sh` now runs the whole gate in one entry and prints the per-binary counts.
Origin: performance investigation during M2 implementation (epic fiddle-srrw), from nine implementer transcripts
Tags: #debt #optimization #orchestrate

### 2026-08-10 — Two identity derivations in one pure module use different framings
`crates/fiddle-core/src/effect.rs`'s `effect_id` hashes a length-prefixed encoding of its four inputs. The encoding is therefore injective, and a field's contents can never be mistaken for structure. `crates/fiddle-core/src/assessment.rs`'s `correlation_key` joins with a NUL separator. Its non-collision argument rests on a domain convention rather than on the encoding. NUL is valid UTF-8, and neither `project` nor `invocation_ref` reaches that function through a type that forbids one.

The divergence is deliberate and documented at `effect_id`. `correlation_key`'s value is written into fixture state on disk. Later runs compare it. A test pins it to a published digest. M0's acceptance lane depends on it. Re-basing it would break the cross-process recognition it provides, for an exposure M0 does not have.

No document says when re-basing would be acceptable. It is acceptable at a milestone that is already invalidating on-disk markers for another reason. That reason could be a change to the bundle layout, to the report schema's shape, or to the marker file's location. The cost is a one-time recognition break, paid alongside something else. It is not acceptable on its own. Until then the rule for anything new is `effect_id`'s: length-prefix, do not separate.
Origin: implementation (epic fiddle-srrw, Task 0 — the evaluator proved an embedded NUL could give two distinct effects one identity)
Tags: #debt #risk

### 2026-08-10 — The publishing adapter runs a workspace-style command the PRD's ownership table assigns elsewhere
M2's design considered publishing a change blob by blob through the Git Data API. That means create blobs, create a tree, create a commit, then `POST /git/refs`. It keeps every mutation inside the `gh` adapter `docs/technical/decisions/015-gh-cli-as-the-github-adapter.md` describes. The design rejected it for one `git push`. A ref can only be created pointing at an object the remote already holds. The blob-by-blob route is therefore four ordered mutations where the push is one, and each can be lost separately. That turns one ambiguous write into four, in the milestone whose subject is ambiguous writes. `git push` to a named ref is also idempotent, which let the design drop a bespoke branch identity scheme.

The price is that `crates/fiddle-runtime/src/git/publish.rs` spawns a subprocess against the workspace from inside the forge adapter. The PRD's ownership table places a program invocation over a checkout on the workspace's side. Nothing is wrong today. It is its own module, its own credential channel, and its own environment. All three are stated as invariants in `docs/technical/SYSTEM.md`, and `crates/fiddle-runtime/src/git/mod.rs` argues the separation. What is owed is a decision about where the boundary runs. The milestone that next adds a mutation which is neither an API call nor a push owns it. The arrangement is defensible as an exception, and a second exception makes it a rule.
Origin: implementation (epic fiddle-srrw, Task 14 recording M2's design reduction §5.5)
Tags: #debt

### 2026-08-10 — The dispatch is the one effect GitHub protects nothing about, and its locator is checked by nothing that compiles
`POST /repos/{owner}/{repo}/actions/workflows/{id}/dispatches` answers 204 No Content. No body, no run id, no `Location`. `GET .../actions/runs/{id}` does not carry the inputs a dispatch was made with. `has("inputs")` answers `false`, and no key matches `/input|dispatch/i`. Both facts are verified against real GitHub. Filtering the runs listing on a dispatch input therefore does not exist as an identity mechanism, and a retried dispatch starts a second run. The branch has `git push` to a named ref. The pull request has GitHub refusing a second one for the same head and base. This effect has no server-side duplicate protection.

The identity goes out as the `fiddle_effect_id` input. It comes back through the target workflow's `run-name`, which the listing returns as `name`. `crates/fiddle-runtime/src/github/checks.rs::run_name` and `.github/workflows/fiddle-check.yml` in `peel/fiddle-effects-acceptance` are therefore two halves of one contract. No compiler and no gating test checks it. Rename the input, drop the prefix, or let the workflow interpolate something else into its title, and nothing fails loudly. The locator stops finding runs that exist. `inspect` reports an absence that is not real. The dispatch happens again.

Two things make this tolerable rather than urgent. No cheaper locator exists, because the runs listing is the only surface returning anything a dispatch can be recognised by. And `scripts/live-github.sh` checks the round trip on every run, so the exposure is the window between an edit and the next live run. The workflow's `concurrency: { group: fiddle-<id>, cancel-in-progress: false }` is a mitigation and not evidence. A concurrency group says two runs will not execute at once. It never says only one was requested.
Origin: implementation (epic fiddle-srrw, Task 12 — the round trip nothing compiles together)
Tags: #debt #risk

### 2026-08-10 — fiddle observes and requests checks but can never author one
Only GitHub Apps may create check runs. M2's credential is a fine-grained personal access token. `crates/fiddle-runtime/src/github/checks.rs` therefore does two things. It observes checks by exact head sha. It dispatches a workflow that has to be started. No path publishes a check result of fiddle's own. No "fiddle verified this change" appears beside CI's own checks on a pull request, which is the surface a reviewer reads.

This is a capability ceiling, not an omission. Closing it means App authentication: signing a JWT with a private key and exchanging it for an installation token. `gh` does not do that. It would put a private key outside the single credential-carrying construction ADR 015 exists to preserve. ADR 015 names this as the most likely trigger for reversing that decision. Until then a reader sees whatever the dispatched workflow reports, and `required_checks` is fiddle's private opinion about which of those matter.
Origin: implementation (epic fiddle-srrw, Task 14 recording M2's boundary)
Tags: #debt #risk

### 2026-08-10 — The human-decision variant is defined, consumed and unreachable from any capability
Extends **2026-08-09 — `RunOutcome::Suspended` is the one exit-code row never exercised end to end** with the M2 half.

`fiddle_core::PolicyDecision::RequireHumanDecision` exists and is produced by `combine`. Step 4 of `crates/fiddle-runtime/src/effect/mod.rs` consumes it. There it fails closed as `EffectError::HumanDecisionRequired` and names what would satisfy it. It does not ship inert. All three of M2's operations declare `HumanDecisionRequirement::Automatic`. The only way to reach the variant is therefore a deployment document writing `require_human`, and the only result is the run stopping. The case the `combine` module was written for is a capability whose own minimum is `Human`. `policy.rs`'s unit test asserts it, and nothing that runs reaches it.

Both halves close in the same milestone. `Suspended` is the outcome an attended decision produces, and `RequireHumanDecision` is what would produce it. M3 introduces the decision channel.
Origin: implementation (epic fiddle-srrw, Task 14)
Tags: #debt #test

### 2026-08-10 — Two things in the effect vocabulary a later consumer must not read as more than they are
**`EffectReceipt.outcome` is only ever `Committed`.** `crates/fiddle-runtime/src/effect/receipt.rs` declares three values. `crates/fiddle-runtime/src/effect/mod.rs` builds a receipt at exactly two sites, both with `EffectOutcome::Committed`. `NotCommitted` and `Unknown` drive the executor's step-8 branch and never land in a receipt, because a non-committed effect returns an `EffectError`. That is coherent: a receipt records an observed postcondition. It also makes the field near-constant on the success path. A later consumer reading it as a discriminator would branch on a value with one inhabitant. The honest fix, if one is needed, is the outcome leaving the receipt rather than the receipt gaining the other two values.

**`GitError::Push` is classified `Unknown`, and its commonest cause never reached the remote.** An unreachable remote, a rejected credential, and a dropped connection all move no ref. `Push` is `Unknown` because git expressed no per-ref verdict. `git push --porcelain`'s `!` line is its refusal channel, and the absence of that line is not a refusal. A transport-failed push whose ref is genuinely absent therefore reports `EffectError::Unresolved` rather than `EffectError::Adapter`. That is cautious rather than misleading, and it costs one `GET` to settle. It is reversible in one match arm. Reversing it makes the classification depend on git's stderr wording, which is the surface `--porcelain` was chosen to avoid.

**Correction, 2026-08-10 (remediation R1, bean fiddle-h055).** The paragraph above used to add that "the analogous `GhError::Malformed` is `NotCommitted` for the parallel reason". That was the same defect one variant over, and it is reversed: `Malformed` is `Unknown`. The old rationale covered one producer, a process that ran to completion and produced garbage. The spawn or wait failure and the missing status line were lost answers wearing a refusal's classification. `GhError::NotSent` is `NotCommitted` now, and its only producer is a call this runtime refused to make. `Malformed` is `Unknown` and still not worth reading again. A program that is not `gh` will not become one.
Origin: implementation (epic fiddle-srrw, Tasks 3 and 5 — judgment calls recorded only in bean summaries until now); corrected by remediation R1
Tags: #debt

### 2026-08-10 — Step 8's settling read does not happen on a cancelled run
`EffectOutcome::Unknown` reaches a cancellation that arrived with the child already running (remediation R1, bean fiddle-h055). A `^C` during `POST .../pulls` is therefore reported `EffectError::Unresolved` rather than as a settled failure, which stops the retry that duplicates. It does not settle the ambiguity within that run. Step 8 does call `read_until_settled`. Its single `inspect` is then refused before spawning, because `GhCli::api`'s pre-spawn check is handed the cancelled token.

Two things could change and only one should. `read_until_settled` returning immediately on cancel is right and stays, because a cancelled run must not sit in a backoff loop. One settling read escaping the cancellation is the arguable improvement. It needs `EffectContext` to carry a second cancellation channel, so reads and mutations answer to different tokens, plumbed through all three `inspect` implementations. It also needs a bound so a `^C` cannot hang on a read. It is not a retry, so it does not touch the milestone's rule that the read retries and the mutation never does.

R1 did not take it. The classification was the finding. The second token is a design change nobody has priced. The cost of leaving it is one fresh process rather than a duplicate. The fresh process's own step-3 read is still subject to GitHub's listing lag, which is the residual risk on the check request.
Origin: implementation (remediation R1, epic fiddle-srrw, bean fiddle-h055)
Tags: #debt

### 2026-08-10 — On a 2xx, the rate-limit headers are parsed and dropped
`crates/fiddle-runtime/src/github/cli.rs` reads `Retry-After` and `X-RateLimit-Remaining` off every `gh api -i` response. It puts both on `GhResponse`. On the failure exit they are copied into `GhError::Http`'s `RetryAdvice` and reach `ReadRetry::delay`. On the success exit no path a run takes reads `GhResponse.retry_after` or `GhResponse.rate_limit_remaining`. Their only reader is `github_cli.rs`, which asserts they were parsed. That test would keep passing if the fields were deleted from every consumer.

The client can be told `X-RateLimit-Remaining: 3` on a 200 and do nothing with it. Over M2's volume this is invisible: one capability per run, three effects, two reads each. It stops being invisible at the first deployment that publishes concurrently against one repository. That is the herd `ReadRetry`'s jitter was written to decorrelate.

Closing it is not "read the field". Pacing on a successful response is a policy decision about whether a run should slow down before it is refused. It interacts with `[github] timeout`. A run that paces itself into its own deadline has traded a 403 for a `Timeout`, which is classified `Unknown` and is strictly worse. `rate_limit_remaining` is also consulted on the error path only as a boolean, through `RetryAdvice::wants_a_wait`. No pacing arithmetic exists to extend.
Origin: implementation (epic fiddle-srrw, Task 14 — found reading the adapter against the committed record)
Tags: #debt

### 2026-08-10 — Additive keys are not a shape change: the schema constants stayed at v0, and this is the rule
`crates/fiddle-core/src/report.rs` carries two doc comments that pull in opposite directions. `REPORT_SCHEMA`'s says a bundle whose shape changes must change the string in the same edit. `RUN_SCHEMA`'s anticipates that M1 onward adds fields. M2 added `review` and `verification` to `WorkStateView`. It left `fiddle.report.v0`, `fiddle.run.v0`, and `fiddle.inspect.v0` alone.

An added key is not a shape change. Bumping would break every acceptance lane asserting `fiddle.report.v0`, M0's included, and M0's lane is a hard constraint of every milestone since. A consumer that dispatches on the schema string and ignores keys it does not know is unaffected by an addition. A bump breaks it.

A removed key, a renamed key, or a changed type is a shape change and does require the bump. Each of those breaks a consumer that was reading correctly. Apply that sentence rather than re-reading the two doc comments, which remain in tension.
Origin: implementation (epic fiddle-srrw, Task 8 — the evaluator was asked to rule and raised no objection)
Tags: #debt #process

### 2026-08-10 — The M2 effects credential can write to the repository M0's proof depends on being credential-free
`docs/technical/effects-repository.md` records the probes. The second row is the one to act on. The fine-grained token that performs M2's effects has two repositories in its selection. `peel/fiddle-effects-acceptance` is the intended target. `peel/fiddle-acceptance` is also selected, answering 200 on its `collaborators` endpoint. That is the external M0 acceptance repository `docs/technical/acceptance-repository.md` describes.

That document argues the repository is public, so reading it needs no credential. It holds no secrets as a standing rule. It never gates M0's lane on one. Nothing falsifies that: the repository holds no secret, `.github/workflows/acceptance-repo.yml` checks it out with no `token:` and no `ssh-key:`, and nothing in M2 writes to it. What changed is that a credential now exists which could write to it. It is held as the repository secret `FIDDLE_EFFECTS_TOKEN`, in a repository the same token is deliberately excluded from. A mistake in `scripts/live-github.sh`'s `FIDDLE_EFFECTS_REPO` default reaches M0's acceptance repository with write authority. So does a `gh` invocation with the wrong `--repo`.

**Closed 2026-08-10, both halves.** The operator narrowed the selection. `repos/peel/fiddle-acceptance/collaborators` now answers 403. A ref-create against it answers `403 Resource not accessible by personal access token`, so the credential is structurally incapable of the write. The probe table records 403 for both other rows. `acceptance-repository.md` discloses the episode. `.env.example`, `docs/evaluator-calibration-general.md`, and `.github/workflows/github-effects.yml` no longer assert a scope the table refutes.

This entry underrated the second half. Narrowing the credential alone would have left the lane one rotation away from the same exposure. The `FIDDLE_EFFECTS_REPO` hazard was not the default value. No value was ever checked, and `scripts/live-github.sh` armed its `trap cleanup EXIT` ref-delete-and-close sweep before the only thing that incidentally noticed a wrong repository. The lane now refuses an inadmissible target before that trap is set and before any mutation, on a positive six-part predicate. See *The target guard* in `effects-repository.md`. Verified by running it. A wrong `FIDDLE_EFFECTS_REPO` refuses with no `cleaning up` line, where the pre-change script printed one and issued the whole sweep.
Origin: implementation (epic fiddle-srrw, Task 14 — reading the probe table against acceptance-repository.md's standing rules); closed by remediation bean fiddle-xbnz
Tags: #risk #security #debt #resolved

### 2026-08-10 — M2's mandatory proof is carried by one test, and an inversion is what established that
`crates/fiddle-acceptance/tests/exactly_once.rs` holds five tests and gates. Task 15 was required to invert its own rule and confirm the lane fails. The inversion lets the mutation retry rather than only the read. The lane does fail, at 4 passed and 1 failed, and the shape of the failure is the finding.

The one that failed is `an_ambiguous_write_then_a_fresh_process_leaves_exactly_one_of_each`. It failed at `assert_landed_under(world, "pulls", "commit_then_die")`, left 5 and right 1. Five identical `POST_repos_peel_r_pulls` records, one per allowed attempt. That is the duplicate external effect the milestone exists to prevent. The other four passed under the inversion and are blind to it: `the_retry_carries_a_distinct_attempt_id_and_the_same_work_ref`, `the_github_token_appears_in_no_bundle_no_stdout_and_no_diagnostic`, `an_unreachable_github_publishes_nothing_and_reports_an_unread_forge`, and `the_effect_steps_of_a_real_run_reach_the_attempt_journal`. Each is sound and each is about something else.

This is a fragility rather than a defect. Weaken, skip, or delete that one test and the lane still reports five passed, while the milestone's central claim is gone. No count moves. `docs/technical/SYSTEM.md`'s Known issues records it.

**The rule, because M3 through M8 all add effects.** An inversion test is the only thing that separates a lane that proves a property from a lane that contains a test about it. It is cheap: break the property deliberately, run the lane, read which tests notice. Two neighbouring practices came from the same verification. First, a frozen lane count is not evidence on its own. `check_effect` reporting 14 proves nothing if one of the 14 was quietly weakened in the same commit, so an edit to a pre-existing test file is diffed by content and the diff is stated. Second, a diff tool's empty answer is a claim rather than a result. `git diff` returned empty under a hook in one implementer's context and took three attempts to notice. An empty diff is therefore cross-checked against `git show <base>:<path>`.
Origin: implementation (epic fiddle-srrw, Task 15's inversion and Task 11's verification standard)
Tags: #debt #test #process

### 2026-08-10 — A branch exists only because a dispatch-only lane cannot run from anywhere else
`.github/workflows/github-effects.yml` cannot be dispatched until it is on `main`. `docs/technical/SYSTEM.md` states the reason as an invariant, and the file's own header argues it. Branch `ci/github-effects-dispatch-proof` on `peel/fiddle`, at `75d655c5`, is the only ref the lane can be dispatched from. It was created for that purpose alone. It becomes redundant when the workflow file lands on `main`. Whoever merges M2 should delete it and dispatch the lane once with any `fiddle_effect_id`. No code is owed.
Origin: implementation (epic fiddle-srrw, Task 13)
Tags: #debt #infrastructure
Status: Half resolved 2026-08-10. The inertness is gone and the branch is not. The file landed on `main` at `aa86c60`. The workflow entity is live, id `330906808`, active. Run **31374193249** dispatched it with no default-branch flip. **Do not delete the branch yet.** `actions/checkout@v4` in that workflow is bare, so the dispatched `--ref` decides which code is built. A dispatch also needs the workflow file to exist at that ref. `main` carries the file but no Cargo workspace. `plan/agentic-factory-m0` and `plan/agentic-factory-m1` carry the workspace but not the file. `plan/agentic-factory-m2` is not pushed. `ci/github-effects-dispatch-proof` is therefore still the only ref where a dispatch both resolves and can succeed. It becomes deletable when the milestone stack merges to `main`, and that merge is the operation that should delete it.

### 2026-08-10 — The widened-payload check is intra-call; the cross-process half needs a durable record nobody has priced
`crates/fiddle-core/src/effect.rs` hashes identity and payload separately. The executor can then tell "already performed" from "the request has been widened since it was approved". Remediation R4 implemented the observable half. The envelope is minted at step 6, for the payload the proposal carried. `Executor::execute` refuses with `EffectError::PayloadDiverged` before step 7, when the operation would apply a different one. `payload_divergence.rs` pins it, and removing the comparison makes the mutation land.

The cross-process reading is not implemented. That reading is a second attempt asking what payload the first was approved for. Nothing persists a prior payload hash. The attempt journal records `effect_step` lines carrying kind and step and no digest. The bundle's evidence is `receipt_evidence`'s rendered string. That string carries kind, effect id, outcome, external ref, and postcondition. The forge receives the identity in a branch name and a workflow run title, never the payload. Reading the world produces nothing either. `EnsurePullRequest`'s list read carries a title but no body.

Three things must be decided, and R4 declined to guess:
- **Where the prior hash lives.** The attempt journal is the obvious candidate, and R1 taught it to record effect steps. The absence of a record must then mean something. An effect performed before the record existed must not read as a changed payload. Nor must one performed by a run whose journal was lost.
- **What happens when the payload has widened.** Refusing strands a published branch. Reporting needs a surface. Re-proposing is a second mutation on a path that already has one. The design states the failure and not the response.
- **What the record costs.** It is approval state that outlives a process. That is a different object from `AuthorizedEffect`, whose doc comment says it is a runtime token and is never written down. M3's decision channel is where durable approval arrives, and pairing the two is cheaper than building this alone.

The urgency is lower because each operation already decides for itself what makes an observed object the postcondition. It decides in typed terms. `EnsureBranchPublished::inspect` compares the intended sha and returns `Ok(None)` when the remote points elsewhere. `EnsureCheckRequested` filters by a run name derived from the identity. The pull request's title and body are the deliberate exception, because matching on those is what opens a second pull request.
Origin: implementation (remediation R4, epic fiddle-srrw, bean fiddle-mp53)
Tags: #debt

### 2026-08-10 — Two derives the `## Contracts` block pins are provably inert
Neither derive is removed here. The epic's `## Contracts` section pins the derive list of both types, and a bean that reduces a pinned contract is changing a contract. Both doc comments now say what is true of the tree.

- **`EffectReceipt`'s `Serialize`.** Nothing serializes a receipt, and no receipt a run produces can be. Production instantiates it with `PublishedBranch`, `PullRequest`, or `WorkflowRun`. None of the three is itself `Serialize`, so the derive's `where T: Serialize` bound is unsatisfiable for all three. Two test-only observations use `String` and `()`, and neither is serialized. A receipt reaches a bundle as `receipt_evidence`'s rendered `EvidenceRef`, which `capability/publish.rs` argues for. The two doc comments used to disagree about this in one epic.
- **`EffectId`'s `Hash`.** No `HashMap`, `HashSet`, or `BTreeMap` is keyed on an `EffectId`. The executor recognises an effect by reading the world for that one effect, one operation at a time. The old comment claimed it indexed a set of proposals.

Removing either is a two-line edit, plus a line in the Contracts block of whatever plan supersedes M2's. Do it with the `PayloadHash` question above.
Origin: implementation (remediation R4, epic fiddle-srrw, bean fiddle-mp53)
Tags: #debt

### 2026-08-10 — `RunOutcome` still carries no taxonomy, and M2 widened the set twice
ADR 013 said from M1 that `RunOutcome::Retryable` has several producers. Only one means the capability tried and lost. A retry loop therefore "needs a taxonomy the outcome type does not carry". M2 added three more producers: `EffectError::{PolicyDenied, HumanDecisionRequired, DuplicateState}`. Remediation R4 added a fourth, `PayloadDiverged`. Nothing recorded that the gap had widened. This entry closes the recording half.

Remediation R3 moved those four to `RunOutcome::Failed` and exit 20, per `docs/technical/decisions/016-a-permanent-refusal-is-not-retryable.md`. That removes the practical harm, because automation retrying on 11 no longer loops on a denied effect. It makes the taxonomy problem bigger. Exit 11 now has six distinct capability failures behind it, beside its three other producers. Exit 20 has four, beside `assess → Blocked`'s three arms. That is ten conditions across two integers. Prose in a `reason` field tells them apart, and a machine cannot key on it. `CapabilityError::recurrence` is a two-valued answer to a question with more than two answers. It is two-valued because the exit table has two rows for a run that executed and did not complete.

A real taxonomy has to decide three things:
- **Where it lives.** A `RunOutcome::Failed { error, class }` widens the `--json` payload every bundle consumer reads. A separate field beside `outcome` does not. It is then a second thing that can disagree with the first.
- **Whether the exit codes follow it.** Adding rows is honest and expensive. `exit_code_for` is realised once by design, and every acceptance lane asserting a number is a consumer. Not adding them leaves the class readable only through `--json`. An operator scripting `fiddle run` in a shell does not have that.
- **What M3 takes with it.** `HumanDecisionRequired` moves from `Failed` to `Suspended` the moment a decision channel exists. `required_checks` below wants the same wait mechanism. Two of the ten leave the table then, which argues for pricing the taxonomy with M3's channel.
Origin: implementation (remediation R3, epic fiddle-srrw, bean fiddle-m3ql)
Tags: #debt

### 2026-08-10 — `github.required_checks` is disclosed as unenforced; enforcing it is still owed
`[github] required_checks` is read, acted on, and decides nothing. The names reach `Executor::observe_checks`. That looks each one up against the published head. It splits the answer into `VerificationState`'s `required_missing`, `failed`, and `pending`, which reaches the bundle as `observations.verification`. `fiddle_core::assess` then matches on `work_item` and `changes` and on nothing else. A required check that is missing, failed, or still running therefore leaves the outcome where an all-green one does. A deployment naming `required_checks = ["build"]` requires nothing of CI.

Remediation R3 took the disclosure side, per `docs/technical/decisions/017-required-checks-are-observed-not-enforced.md`. `config check` now reports the key as an object. It carries `configured`, `enforced` (empty, whatever the document says), a `status`, and the decision. The status word is `observed-not-enforced` rather than `accepted-not-enforced`, because the older word promises less reading than actually happens.

Enforcement is three decisions rather than one. R3 declined to guess:
- **A failed required check is a conclusion.** `Blocked ⇒ Failed` fits it, and it is the only one of the three that does.
- **A pending one resolves without anybody doing anything.** Waiting is the honest answer, and waiting is `Suspended`, which is M3's row. It is the same mechanism as waiting for a human.
- **A `required_missing` one may only mean CI has not started.** Telling "never going to run" from "has not run yet" needs a deadline or a poll budget. `[github]` supplies neither.

All three land in `fiddle_core::assess`, whose `Blocked ⇒ Failed` rule M0's frozen acceptance lane depends on. Adding an arm gives `RunOutcome` more producers, which is the entry above.
Origin: implementation (remediation R3, epic fiddle-srrw, bean fiddle-m3ql)
Tags: #debt

### 2026-08-10 — The preflight that makes `--ref main` legible is not on `main`
`.github/workflows/github-effects.yml` refuses a ref carrying no Cargo workspace. It refuses at a preflight step, before the toolchain install and the build, naming the reason and the milestone branch to pass instead. Two dispatches prove it. Run **31383731994** ran against a throwaway ref built from `origin/main` plus that one file. It gives `conclusion=failure` at step 4, with the toolchain, the build, and the walk skipped. Run **31383743533** ran against `ci/github-effects-dispatch-proof` at `d52fc84`. It gives `conclusion=success` with the walk confirmed.

The gap is which copy a dispatch uses. `workflow_dispatch` resolves the entity on the default branch and runs the file from the dispatched ref. `--ref main` therefore gets `main`'s copy, which is `aa86c60`'s, without the preflight. The invocation the preflight exists to make legible still gets `could not find Cargo.toml` forty lines into a build log. That holds until the milestone stack merges, or until the operator lands this one file on `main`. Nothing else is owed.

The same applies to `scripts/check-github-effects-lane.sh` and its fixtures. They run in `skill-quality.yml` from the ref being pushed. That step does not exist on `main`. The never-skip property is therefore asserted on every milestone branch and not on `main` itself.
Origin: implementation (remediation R5, epic fiddle-srrw, bean fiddle-ufv3)
Tags: #debt #infrastructure

### 2026-08-10 — Implementers never update their bean while working, and nothing asks them to
Across M2's 21 beans no implementer ticked a single `- [ ]` step. Nothing instructed one to. All 20 completed beans closed with every box unticked, 110 in total, backfilled at close. `skills/develop-loop/dispatch-and-evidence.md` tells the lead to arm `.fiddle/active-bean`, initialise the eval log, and dispatch. `skills/develop/implementer-prompt.md` tells the implementer to implement, verify, commit, self-review, and report. Neither says to touch the bean. The tracker therefore holds an outcome and nothing about the hour that produced it.

Two changes are worth making, in `skills/develop/implementer-prompt.md` and the develop-loop reference beside it. Instruct the implementer to tick its own `## Steps` boxes with `beans update <id> --body-replace-old/--body-replace-new`. Instruct it to append one line naming the phase it has entered: reading, implementing, verifying, or inverting. The CLI already supports this, and nothing in the prompts points at it. Have the lead append a phase line when it polls. A reader who is not the lead can then answer "where is this" from the bean rather than from `ps`.

The cost is small against a measured 5.7 minutes of fixed orientation in a 23.4-minute bean, with model generation at 63% of wall clock. A handful of extra `beans update` calls is not what makes a bean slow. The visibility is what makes a stalled one detectable.
Origin: operator feedback during M2 implementation (epic fiddle-srrw) — "beans are not updated with any progress reports and run for an hour"
Tags: #debt #orchestrate #ux

### 2026-08-10 — M3's plan assigned its most load-bearing unproven assumption to its last bean
The M3 design left one thing deliberately unproven: whether the effects credential may write a conversation comment. Three read probes answered 200 and proved nothing, because `peel/fiddle-effects-acceptance` is public. That is the same trap that let a two-repository token selection survive all of M2.

The proof was assigned to Task 16b, the last of 24 beans. Tasks 5, 11a, 13, 14, and 15 all rest on that surface being writable. A 403 would have arrived after roughly twenty beans of work. The fix is not a code change. It is `Issues: read and write` added to a credential the operator had narrowed that same day, or a different gated effect, which re-opens §5.1. §5.7 of the same document argues for this ordering. The plan applied it to the GraphQL contract in Task 1 and not to this. The plan's self-review and a full codex critique pass both missed it. The operator caught it by asking "should we check the comments part?" while Task 1 was still running.

The question settled the moment it was asked. A closed pull request accepts comments, so no branch and no new pull request were needed. `POST /repos/peel/fiddle-effects-acceptance/issues/19/comments` answered 201. `GET /issues/comments/{id}` returned the full payload. `DELETE` answered 204, with zero residue. Two calls, for the thing scheduled twenty beans late.

**The rule, because M4 through M8 all add external surfaces.** Order the external-contract proofs by what a refutation would cost, not by where the work falls in the plan. A proof whose failure re-opens a design decision belongs in the first bean. Task 1 was correctly first because ADR 018 depended on it.

A second, smaller finding from the same probe. `user.id` appears before `.id` in a comment object. Scraping the first id-shaped field therefore yields the author's user id rather than the comment's. The probe's cleanup did that. It issued a DELETE against `505401`, got a 404, and left the comment behind. A typed adapter naming the two fields separately is immune. `scripts/live-github.sh` and Task 16b's phase are bash and are not. Select by name, and make a cleanup that deleted nothing fail loudly.
Origin: planning (epic fiddle-eoqx, seed fiddle-a9y5) — caught by operator question during Task 1's implementation
Tags: #process #debt

### 2026-08-10 — M3's plan misdescribed the identity framing it told an implementer to copy
`fiddle-7j2p`'s Step 7 instructed the implementer to frame `decision_request_id`'s inputs "the way `effect_id` frames its four — each field preceded by its byte length as a `u64` in little-endian". `effect_id` does not do that. Following the plan's letter would have produced a second, incompatible framing. It would have landed inside the one crate whose job is that a fresh process recomputes an identity the same way every other process does.

The implementer did not follow the letter. It read `effect_id` first. It extracted the real framing into a shared `pub(crate)` helper so the two functions cannot drift. Its argument: "a second copy could acquire a different separator or a character count under a later edit, and nothing would fail until an identity stopped matching across builds." Evaluation confirmed the extraction left `effect_id`'s bytes untouched, against the existing `b3sum` pin `39b2e77d1d17cb20`.

The defect cost nothing, and the reason is worth more than the defect. A plan that describes existing code is a secondary source. An implementer that reads the primary one beats it. This is the fourth plan defect M3's implementers have caught. The others were a second wildcard-free `EffectKind` match that made Task 2's declared scope non-compiling, a Task 12 criterion contradicting a documented decision in `config.rs`, and the credential assumption scheduled twenty beans late. The plan's self-review and a full codex critique pass caught none of the four.

For M4 onward: where a plan step describes existing behaviour, cite the file and let the implementer read it. A restatement is a copy that can be wrong. Nobody re-checks a plan against the code once it has been reviewed.
Origin: implementation (epic fiddle-eoqx, bean fiddle-7j2p, Step 7) — found by the implementer, confirmed by evaluation
Tags: #process #debt

### 2026-08-10 — A fifth plan defect, and this one was a method that does not exist
`fiddle-hmho`'s test sketch called `err.worth_another_read()`. The method is `is_worth_reading_again()`, in `crates/fiddle-runtime/src/github/cli.rs`. The wrong name was in the bean and in the lead's dispatch prompt. A later bean written against either would have inherited it. It cost nothing. The implementer read `cli.rs`, used the real name, and reported the discrepancy.

Five plan defects on this milestone. The plan's own self-review caught none. A full codex critique pass caught none. An implementer reading the code the plan claimed to describe caught all five:

- the comment-write assumption five beans depend on, scheduled to be proven by the last of twenty-four;
- a second wildcard-free `EffectKind` match, which made a bean's declared scope non-compiling;
- a Task 12 criterion requiring policy rows to be mandatory, contradicting a documented decision in the same file;
- an identity framing described as `u64` little-endian where the code writes `<byte-len>:<field>`;
- a method name that was never real.

Four of the five are one species: the plan restating existing code and getting it wrong. The fifth, ordering by cost, is different in kind. Where a plan step describes existing behaviour, cite the file and let the implementer read it.

Two things did not catch these, and both were paid for. A self-review pass by the plan's own author. And an external critique that returned ten findings and produced eight real fixes. The critique was worth its cost. It caught two leaking cleanups, an impossible follow-up comment, a missing exit row, and a contradicting pagination test. It does not catch this species, because it reads the plan against the design rather than against the code.
Origin: implementation (epic fiddle-eoqx, bean fiddle-hmho) — found by the implementer
Tags: #process #debt

### 2026-08-10 — The epic's Contracts block named a type no bean was told to build
`ActorRef` appears in `fiddle-eoqx`'s `## Contracts` section, placed in `crates/fiddle-core/src/decision.rs`. Task 2 created that file and did not create the type. Task 2's steps never mentioned it: they covered the two `EffectKind` variants, the marker's render and parse, and `decision_request_id`. Task 4, the conversation adapter, was the first bean that needed it. It found the type missing and asked where it should go.

This is the sixth plan defect on this milestone, and a new species: a contract entry with no corresponding step. The Contracts block is copied into every bean body, so parallel implementers cannot make incompatible choices. That works only for types some bean is instructed to define. Nothing checked that every entry had a home.

Cost: one question, answered in one message, because the implementer asked instead of guessing. Guessing would have landed `ActorRef` in `github/comments.rs`. Both `human/` and the pure decision logic would then depend on the GitHub adapter for a domain identity. That inverts the crate boundary `crate_boundary.rs` exists to hold, and it fails no test.

The check for M4 onward is mechanical. Every type named in a Contracts block must be greppable to a step that creates it. A contract entry no step owns is defined twice by whoever needs it first, or defined in the wrong crate by whoever needs it soonest.
Origin: implementation (epic fiddle-eoqx, bean fiddle-127g) — found by the implementer asking rather than guessing
Tags: #process #debt

### 2026-08-10 — A drafting run accepts an already-readied pull request, and that is a decision rather than an accident
`EnsurePullRequest::inspect` matches on head, base, and `state=open`. M3 adds `draft: bool`. A drafting run that finds an existing pull request therefore treats its postcondition as satisfied. It does so even when a person has marked that pull request ready for review, and it performs no mutation. Task 6's implementer flagged this while writing tests. `gh_stub` models a pull request as `{head, base, title}` with no `draft` field, and cannot express the case.

The behaviour is right, for three reasons. The effect is that a pull request exists for this head and base, and `draft` is a property of creation rather than of the postcondition. That is the same reasoning that makes `inspect` match on head and base and not on title or body. Re-drafting a pull request a person had readied would undo human progress, which is the failure mode this milestone is built against. And it composes: if the pull request is already ready, `EnsurePullRequestReady::inspect` returns the postcondition and nothing mutates.

Assigned to `fiddle-pwyi` (Task 13a). That bean builds the scripted world the acceptance walk needs, and must model `draft`. It should assert the case directly: a readied pull request is not re-drafted.
Origin: implementation (epic fiddle-eoqx, bean fiddle-yg9c) — found while writing a test the fixture could not support
Tags: #debt #decision

### 2026-08-10 — The lead ruled three times on one type, and the churn was the lead's alone
Three rulings placed `ActorRef` in one hour. The lead answered `fiddle-core` to Task 4's implementer. It retracted to `fiddle-runtime` after seeing the implementer had already put it there. An agent acting on the first ruling put it back, and the lead accepted `fiddle-core` again. It compiles and its tests pass in the final position. Two agents were told opposite things about one type inside one round.

The failure is the lead answering an architectural question at message speed. The second ruling was the worst. It retracted a correct answer using the wrong test: "nothing in `fiddle-core` names it" is true and irrelevant. The right test is whether the type is domain vocabulary. It is. `EffectId`, `CapabilityId`, `WorkRef`, and `InvocationRef` all live there, and M6's attended mode will have actors who are not GitHub comment authors.

The cost. Two agents received contradictory instructions. A type moved crates twice. The consuming bean `fiddle-v5bm` accumulated three notes, of which two are wrong, which is worse than no note. Answer a question about where a type lives once, in writing, against the vocabulary already in the tree. If the answer changes, mark the superseded note superseded rather than adding a contradicting one.

Recorded here rather than only on the bean. The pattern is lead behaviour under concurrency, and it will recur in every round with four agents asking questions at once.
Origin: lead (epic fiddle-eoqx) — three rulings on one type during the parallel round
Tags: #process #orchestrate

### 2026-08-10 — Three lead errors in one round, each corrected by the agent it was about
They share a cause. The lead answered fast, from a stale read of a tree four agents were changing.

**1. `ActorRef`'s placement, and who moved it.** The lead's shutdown note told Task 4's implementer it had left the type in the adapter and had been right to. It had not. It followed the first ruling and moved the type in `d11a47e`. It also removed the `github/mod.rs` re-export, with a comment on why a second path would invite a dependency on the wrong crate. The implementer corrected the record before shutting down. Ground truth: one definition in `crates/fiddle-core/src/decision.rs`, not re-exported through the adapter.

**2. "The build break was unfounded" was itself unfounded.** Task 8 reported that `HEAD` did not build. Its commit declared `pub mod interpret;` while `human/interpret.rs` was untracked. Task 9's `f02cffa` had healed it by the time the lead checked, and the lead called the alarm unfounded. It was accurate when raised. An implementer who finds a half-landed cross-lane dependency should report it and leave the other agent's file alone. That sometimes means the branch tip is briefly broken through nobody's fault. Treating the report as a false alarm discourages the next one.

**3. `crate_boundary` passing was cited as evidence about placement, and is not.** Its two `fiddle-core` tests are a resolved-closure denylist and a source grep for impure names. A pure struct of a `u64` and a `String` trips neither, wherever it lives. The gate was green before and after the move.

**One structural observation.** Three agents wrote to `crates/fiddle-core/src/decision.rs` in one round. `f02cffa` added `InterpretedHumanDecision`. `d11a47e` added `ActorRef`. The lead's rulings sent them there. None of it appeared in any bean's declared `## Files`. The round was planned by checking that the four beans' declared files were disjoint, and they were. The lead's mid-round rulings made a pure-core file a shared surface. A concurrency plan that only checks declared scope does not survive a lead that widens scope by message.
Origin: lead (epic fiddle-eoqx), corrected by fiddle-127g's and fiddle-kgr7's implementers
Tags: #process #orchestrate

### 2026-08-10 — An implementer marked its own bean completed, and the loop would have accepted it
`fiddle-dvsl` was found at status `completed`. Its evaluation log read `iterations=0, dispatches=0, verdict=UNKNOWN`. Its implementer had transitioned it in good faith. No script checks that a completed bean carries a terminal verdict. The lead noticed only while reconciling a separate message.

`docs/technical/SYSTEM.md` carries the invariant "Only the lead manages bean status transitions". `skills/develop/implementer-prompt.md` does not state it. Nor did the lead's dispatch prompts, which told implementers to tick their `## Steps` and append a `## Summary of Changes` using `beans update`. The prompt handed over the exact tool and said nothing about the one transition an implementer must not make. Five implementers were dispatched with that prompt. One drew the obvious conclusion.

Unnoticed, the bean reads as converged with no scorecard, no dimension data, and no second pass. Nothing would have recorded that a human ever agreed. `trend-eval-history.sh` would show a completed bean contributing nothing, indistinguishable from an evidence-only convergence.

Two fixes, and the second survives a forgetful lead. State it in `skills/develop/implementer-prompt.md`: tick your steps, append your summary, never change `status`. And make it mechanical. `scripts/check-convergence.sh` and the eval-log scripts already exist. A bean at `completed` whose parsed log has no terminal verdict is a detectable state, and the natural home is the same Stop-hook family as `develop-verdict-gate.sh`. A prose rule in SYSTEM.md that neither the prompt nor any script enforces holds until an agent is helpful.
Origin: implementation (epic fiddle-eoqx, bean fiddle-dvsl) — found by the lead while reconciling a shutdown message
Tags: #process #orchestrate #debt

### 2026-08-10 — Concurrent lanes sharing one `target/` produce false test failures, which is an evidence-integrity problem
`fiddle-9krm`'s implementer reported four `config_check` failures during a workspace run. A concurrent lane had relinked `target/debug/fiddle` mid-run. Re-run in isolation, that binary passed 20/0. All agents in this round share `/Users/peel/wrk/fiddle/.worktrees/agentic-factory-m3/target`, and the acceptance lanes resolve the binary under test through that path.

The obvious cost is slowness, because cargo's build lock serialises compilation. The cost that matters more is that a suite can report failures that are not real. Acceptance lanes launch the compiled binary as a subprocess. A relink between the launch and the assertion fails the lane for a reason unrelated to the code under review. The evidence pack an evaluator scores is a captured suite run, and a false failure inside it is indistinguishable from a real one. The failure mode is a bean scored down for another bean's link step. The evaluator reasons carefully about evidence that was never true.

Nothing was mis-scored this round. The implementer noticed the pattern, re-ran the affected binary in isolation, and reported both results. That depended on an implementer distrusting its own red suite.

Two mitigations, and the second is worth adopting. Re-run a failing binary in isolation before believing it, and say so. That is cheap, and it relies on judgment every time. Give each concurrent agent its own `CARGO_TARGET_DIR`. It costs a cold build per agent. It runs genuinely parallel rather than serialised on the build lock. It removes the interference and recovers the speedup the shared lock was eating. The lead should set it in the dispatch prompt for any round with more than one implementer.

A shared mutable artifact between concurrent lanes turns a verification result into a race. The evidence pack is only as trustworthy as the isolation of the run that produced it.
Origin: implementation (epic fiddle-eoqx, bean fiddle-9krm) — observed by an implementer that distrusted its own failing suite
Tags: #process #infrastructure #debt

### 2026-08-10 — A plan's test snippet would have compiled, passed, and proven nothing
The ninth plan defect of this milestone, and the subtlest. The others were wrong names, absent types, or harnesses that never existed. All of those fail loudly the moment an implementer tries them. This one would have shipped green.

`fiddle-rvcu`'s bean asked for a test proving that a resolved decision does not license a widened payload. Its snippet built the case by widening the operation's payload:

```rust
let widened = op.with_payload("something else");
let err = world.execute_decided(widened, &decision).await.unwrap_err();
assert!(matches!(err, EffectError::PayloadDiverged { .. }));
```

`Executor::execute`'s step 6 already refuses exactly that. It compares the envelope's digest against `IntegrationOperation::payload()`, and it has since M2. The assertion would therefore have passed with step 4's new decision-payload comparison deleted. It is a test about a check that was not running.

The implementer caught it while running the bean's own required inversion. It rewrote the case to move the decision's payload, leaving proposal and operation agreeing so only step 4 can refuse. The doc comment records why. It is also the realistic case: the person approved request A, and the continuation built request B. The identity is unchanged, because identity derives from the target rather than the payload.

The inversion made this detectable, not review. A reviewer reading the snippet against the design would have seen a correct-looking assertion of a real property. A full external critique pass did see that. The rule, beyond the standing inversion requirement: a test written against a property that a neighbouring check already enforces cannot distinguish the two. When a plan asserts a new guard, the snippet has to arrange a state that only the new guard can refuse. Delete the new guard and watch.
Origin: implementation (epic fiddle-eoqx, bean fiddle-rvcu) — found by running the bean's own required inversion
Tags: #process #debt

### 2026-08-10 — The lead's verification shell has no toolchain, and the nearest one compiles a different language
A sibling of the shared-`target/` finding above, and it bit the lead. Building `fiddle-rvcu`'s evidence pack, `cargo` was not on the verification shell's `PATH` at all. The toolchain arrives through the worktree's devenv/direnv environment. Implementer agents load that per cwd, and the lead's shell does not.

**The captured exit code was the wrong process's.** The script read `cargo fmt --all --check 2>&1 | tail -5; echo "exit: $?"`. `$?` was therefore `tail`'s status. `cargo` was missing, every command printed `command not found`, and the log recorded `fmt exit: 0`. That is a clean bill of health for three checks that never ran. An evaluator had just found the same defect in the previous pack, where a `&&` chain swallowed the clippy line. Fixing the `&&` did not fix the class.

**The obvious repair would have measured the wrong compiler.** A `cargo` exists in the sibling m0 worktree's devenv profile. `flake.nix` differs between the two worktrees at exactly one line, the Fenix hash of `rust-toolchain.toml`. m3 pins 1.97.1 where m0 pins 1.85.0. Verifying m3's tree with m0's `cargo` would have run `clippy -D warnings` under a compiler twelve minor versions old. That is a lane whose evidentiary value is that clippy is clean.

Both mitigations are mechanical. Never infer an exit code through a pipe. Redirect to a file, capture `$?` from the command itself, then summarise the file. This applies to `&&`, `|`, and `tee` equally. Resolve the toolchain from the worktree under test, through `rust-toolchain.toml`. A stacked-branch project has worktrees on different pins, and the neighbouring one is the closest wrong answer.

An evidence pack is only as trustworthy as the provenance of the tools that produced it. Isolation covers where the build wrote. Provenance covers what did the building.

**Addendum, same day — there is a third axis, and it invalidated the corrected run too.** The toolchain was pinned and `CARGO_TARGET_DIR` isolated. The verification came back 525 passed and 2 failed. Both failures were in `github::comments` and `human_comments`, a different bean's files, being edited by a live agent. `effect_protocol` read 50 where the bean under evaluation had measured 48.

**Correction, after that bean's evaluator refuted the attribution.** The two extra tests were not contamination. `git log 4622f05..400e4d0 -- crates/fiddle-runtime/tests/effect_protocol.rs` returns exactly one commit, `400e4d0`. That is the evaluated bean's own work: two tests and a fourth inversion, landed 33 seconds before this entry was written. They were uncommitted at the moment of measurement, so the isolation lesson stands. The story told about them was wrong. Treating the count as contamination is how the evidence pack came to be pinned a commit behind the bean it was evaluating. The two failures were genuinely another lane's inversion.

An unexpected number is a question, not a defect. Attributing it to a known failure mode without running `git log` on the file is accepting a claim without evidence. Here it went into a permanent record 33 seconds after the commit that would have explained it.

The isolation has three axes:

| axis | what it pins | how it fails silently |
|---|---|---|
| `CARGO_TARGET_DIR` | where the build wrote | a concurrent lane relinks the binary under test mid-run |
| `rust-toolchain.toml` | what did the building | a sibling worktree's cargo is a different compiler |
| **a detached worktree at the commit under evaluation** | **what was built** | **uncommitted work from other agents is measured as the bean's** |

Only the third produces attributable numbers. `git worktree add --detach <scratch> <sha>` gives a checkout with zero dirty files. Re-run that way, the same tree verified clean. `fiddle-rvcu`'s implementer used this unprompted. It measured its delta in a scratch worktree at BASE_SHA, with only its two files applied, because two other implementers were editing the shared tree. The lead praised it without adopting it. Adopt it. Build an evidence pack for a bean from a detached worktree pinned at that bean's last commit, never from the shared branch checkout, whenever any other agent is live.

**Second addendum — an inversion run is a uniquely bad neighbour.** A private `CARGO_TARGET_DIR` isolated one lane's artifacts from concurrent relinks. It did nothing to isolate the source tree that lane was mutating, which is the failure two other agents and the lead then hit. Inversions deliberately break the tree. No window exists in which the intermediate state is meant to be green. A normal in-progress edit is transiently red by accident, and its author is trying to get back to green. An inversion is red on purpose, and its author will revert rather than repair. Any other agent reading the workspace during that window gets a true-but-expired failure. It looks like a real regression in a file they do not own.

So the rule is not "isolate your build directory". Run inversions in a detached worktree pinned at the commit under evaluation, never in the shared checkout. The lead's dispatch prompt mandates the private target directory and should mandate this too. The cost is a cold build per inversion round. The alternative made every concurrent lane's test run unreliable three times in one afternoon.
Origin: implementation (epic fiddle-eoqx, bean fiddle-rvcu) — found by the lead while building an evidence pack, after an evaluator had flagged the same exit-code class in the previous one
Tags: #process #infrastructure #debt

### 2026-08-10 — `comments.rs:262` claims more relation recognition than one character buys
Recorded here because a bean about to converge owes it. An owed item on a closed bean is a lost item.

`read_a_link_value`'s notion of readable is the presence of a `>`. That is enough to keep an unparseable header from being read as an end of pages, which is the property `fiddle-9krm` establishes. Its doc comment in `crates/fiddle-runtime/src/github/comments.rs` describes a stronger recognition than one character delivers. Two residuals follow. `<url>; rel="ne` still reads as an end. A single valid link-value marks a mixed header readable.

Widening the doubt direction sends every legitimate last page to its bound, which is worse than the failure being prevented. The code is right and the comment overclaims. Soften the comment when that file is next touched. Do not change the behaviour to match it.

The bean's implementer flagged this against its own work, unprompted. It had been scoped to correct a record rather than to touch the file, and it declined to touch it.
Origin: implementation (epic fiddle-eoqx, bean fiddle-9krm) — volunteered by the implementer against its own lane
Tags: #debt #docs

### 2026-08-10 — A bean marked in-progress with zero dispatches is indistinguishable from work in flight
`fiddle-v5bm` (Task 5) sat in the lead's status table as a live lane for most of a session. Its `## Evaluation Log` read `total_dispatches: 0`. Zero of five steps were ticked. None of its three declared files had a commit. No implementer report was ever received.

The mechanism is a gap in the loop. The lead sets a bean to `in-progress` when it intends to dispatch. The status field is the same afterwards whether the dispatch happened, died at birth, or was never sent. `in-progress` therefore means "the lead once intended this", not "an implementer is working on this". The lead reported the bean as in flight and sequenced other work behind it. It also routed two handoffs from another bean to an implementer that does not exist.

The tell was free to read. `total_dispatches: 0` on a bean claimed to be in progress is a contradiction. So is zero ticked steps on a bean live for hours. That is why the earlier finding about implementers never updating their bean matters more than it looked. It removed the only other signal.

Two mitigations, and the second is the real one. Cross-check `in-progress` against `total_dispatches` and against `git log` on the bean's declared files, before reporting a lane as live. And derive lane liveness from artifacts rather than from status. A bean is being worked on if and only if its declared files have commits, or dirty state, or a dispatch recorded in its log. The status field is the lead's intent, and is never evidence of an agent. The milestone already applies this rule to implementer claims. It had exempted the lead's own bookkeeping.
Origin: process (epic fiddle-eoqx, bean fiddle-v5bm) — found by checking a bean's eval log after noticing it had no commits
Tags: #process #debt

### 2026-08-10 — The isolation policy that fixed evidence integrity filled the disk to 100%
The three-axis isolation rule above works, and it has a cost nobody priced. Each cold build directory is 1.5 to 3.6 GB. Four implementer lanes, two evaluators, and the lead building evidence packs took the root filesystem to 100%, with 3.6 GB free of 461 GB. Reclaiming four directories belonging to converged beans freed 9.2 GB.

This is an evidence-integrity problem rather than housekeeping. A build that fails for want of disk does not announce itself as a disk problem. It surfaces as a link error, a truncated artifact, or a test binary that will not run. That is indistinguishable from the failures the isolation was introduced to eliminate, and it arrives in the same reports. The fix for false failures can manufacture false failures. An evaluator flagged it while shutting down, having noticed `/private/tmp` at 99%.

The missing half of the policy is a disposal rule. A per-inversion worktree is removed by `git worktree remove --force`, in the same step that writes the counts, not at the end of the lane. A lane's `CARGO_TARGET_DIR` is deleted by the lead, in the same action that sets the bean `completed`. And check free space before dispatching a parallel round. Six concurrent lanes need roughly 20 GB, so size the round against what is available rather than against the pane ceiling. One discipline covers this and the earlier finding about `git worktree list` accumulating detached checkouts. A stray worktree and a stray target directory are the same species of leak.
Origin: process (epic fiddle-eoqx) — surfaced by an evaluator during shutdown, after the lead mandated the isolation without a disposal rule
Tags: #process #infrastructure #debt

### 2026-08-10 — A claim about N cases needs N observations, and a fail-fast test can only make one
A sharpening of the `effect/mod.rs` match-arm-order finding. `fiddle-ayqd`'s implementer applied it to its own committed work, unprompted.

It had committed a sentence asserting that three mangled marker bodies previously refused as `Version`. Its inversion run observed one of them, because the test fails fast on the first case. The other two follow deterministically from reading a two-line function. That is inference rather than observation. It is therefore the same species as a doc comment naming a mechanism nobody measured.

Re-observing the other two by hand corrects the claim and leaves the hole. A fail-fast test can only observe its first case, so the next reflow is free to break cases two and three silently. The durable fix makes the test unable to pass while any case is unobserved. Collect the outcome for each case and assert on the collection. A run then reports which case diverged.

As a rule: a claim quantified over N inputs is evidenced only by N observations. When a comment or a bean says "all three", "every kind", or "each variant", the test behind it must be able to fail on any one of them individually and say which.

This is the third dressing of one error this milestone: a stated mechanism standing in for a measured one. The first was a test whose property a neighbouring check already enforced. The second was a comment crediting match-arm order over disjoint variants. All three read as correct, and none could notice being wrong.

**Addendum — the fourth dressing, and it is the one with no local test at all.** `fiddle-ayqd`'s implementer first confirmed the disputed sentence was exact. Probing each case separately gave `Version("")`, `Version("v1\nrequest=…")`, and `Version("request=…")` for the three it claims. The two already `Malformed` are precisely the two the comment does not claim. Five cases, five observations, claim held.

It then found a claim of the same species. A module doc in `fiddle-core` asserts that the continuation "recomputes all four fields from canonical inputs and compares them". It also asserts an author check against an allowlist of `ActorRef::id`. That is a claim about `fiddle-runtime`'s `validate.rs`, another crate, rewritten by a sibling lane after the sentence was written. It verified against the committed tree and found all four comparisons plus the actor check. Its own summary of the near-miss: *"Had `effect` not been compared there I would have had exactly your `effect/mod.rs` defect — a true property credited to the wrong mechanism."*

This is the worst of the four, because nothing local can fail. The other three were in reach of a test in the same file. A neighbouring check could be deleted, an arm order swapped, a case enumerated. A comment in crate A describing a mechanism in crate B has no test that binds them. Crate B can be rewritten, as it just was, and crate A's sentence goes on asserting the old shape with every suite green. Either put the assertion where the mechanism is, or write it as a reference. "See `validate.rs`, which compares …" is a reference. A statement of what happens is not.

**Addendum — the cross-crate hypothesis was confirmed the hard way: the sentence was already false.** Within the hour the owning lane `fiddle-n8fs`, which holds `validate.rs`, found two of its three claims wrong, one materially:

- **"Recomputes all four fields from canonical inputs" is false for `head_sha`.** Three values are recomputed from canonical inputs: effect id, payload hash, request id. The `head_sha` comparison reads the head observed from the world, so neither side of it is a recomputation. The sentence misdescribes what would fail if GitHub were lying.
- **The request-id comparison is a sieve, not an authentication.** It runs inside a `filter_map` answering which comment is our question. It authenticates nothing, because a request id is copyable off the visible conversation. The effect-id comparison is the authentication. Listing the two as peer comparisons invites a reader to think the first establishes provenance. That is the confusion `a_parse_is_not_an_authentication` exists to refute.
- The allowlist check is confirmed, with a nuance the doc omits. It sits after the `is_bot` arm, so a bot carrying an allowlisted id is refused before the allowlist is consulted.

The risk had already materialised, silently, with every suite green. `fiddle-core` cannot depend on `fiddle-runtime`. The allowlist parameter is a bare `&[u64]` rather than an `ActorRef`-typed value, so a refactor to login comparison would leave the doc false and nothing would fail. The correction is owed as its own work, because the doc's author has shut down.
Origin: implementation (epic fiddle-eoqx, bean fiddle-ayqd) — self-audit of committed work, prompted by fiddle-rvcu's evaluation
Tags: #process #testing

### 2026-08-11 — A test can be insensitive because of its fixture, not its assertion
The fifth dressing of this milestone's recurring error. It is the first where the assertion is correct, the property is real, and the test still cannot fail.

`fiddle-n8fs` set out to invert "the last authorized reply decides" and got a null result. `select_candidates` sorted by id. `last()` and `max_by_key(id)` therefore agree under every arrangement a fixture can build. It restructured `resolve` to choose before it orders, and the inversion then landed.

That inversion is invisible to `the_last_authorized_reply_decides_and_the_earlier_ones_are_evidence`, the test named for the property. That test's fixture is sorted. The assertion is right, the property is real, and a position-based reading of "last" would pass it forever. `a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one` catches the inversion, because its fixture is deliberately out of order.

The four earlier dressings were failures of the assertion or the explanation. This one is a failure of the input. Every previous mitigation leaves it untouched: assert the message, pin the bytes, observe each case, put the claim where the mechanism is. The test already asserts the right thing about the wrong world.

The rule. For any property about order, selection, or identity, at least one test must supply an input where the correct answer and the lazy answer differ. If every fixture is sorted, four things are the same value: "last", "greatest", "first match", and "the one with the largest id".

**Amendment — the cheap inversion driver, stated so the wrong half is not what propagates.** A lane that ran 21 inversions found the per-inversion worktree prescribed above is more granularity than the problem needs. The saving is one worktree for N inversions. It is not permission to skip isolation. The run must still happen in a detached worktree pinned at the commit under inversion. One pinned worktree can host all N rounds, for one cold build instead of N. In-place mutate-and-restore is only safe with two guards, and both are required. Copy the file to a pristine path before the first mutation. After the run, diff the working file against that copy and assert byte-identity, with the restore in a `finally`. Without both, an interrupted round leaves the tree mutated and red, inside the isolation meant to prevent that. The lane that supplied this used both guards. It verified the restored file byte-identical before removing the worktree.

**Amendment to the fixture rule — the mitigation fails when applied in the obvious place.** The lane that found it added the part that makes the rule usable:

> That discriminating fixture usually has to live in a different test from the one named for the property.

The sorted fixture sat in the test named for the rule, and could never fail under a position-based reading. The discriminating fixture lives in `a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one`. Somebody applying the rule by strengthening the property's own test would not reach it. The fixture that test needs is the one it already has for every other assertion. The corollary: resist collapsing the two tests. `considered` order and which reply decides are separate claims. Keeping them separate made two of the inversions individually visible.

**The empirical case, from a lane that both caused and suffered it.** The lane holding Task 7 re-ran its three inversions pinned. It corrected two figures, having discovered it had run all three in the shared checkout. Its baseline was 529 / 0 / 1 / 38 pinned at its own commit, not the 556 it first reported. That 556 was the shared tree carrying three other lanes' commits. It had reported `a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one` as a third casualty of its `FORBIDDEN` inversion. Pinned, that inversion fails exactly two tests. The third was another lane's noise. Its own first cut at that inversion had nine tests red in that tree, for three other lanes to trip over.

A lane running an inversion in a shared checkout generates and consumes unattributable failures at the same time. From inside, the two are indistinguishable. Two of the three phantom-failure reports chased earlier in the day are accounted for here.

One correction went the other way. Its `minimum()` to `Automatic` inversion was 526 / 3, not the 523 / 2 first reported. The extra failure is a test that did not exist at the time of the first run. The claim that a relaxed minimum would fail here is therefore carried by three tests rather than two. A recorded count that moves upward for a nameable reason is stronger evidence than the original.

**Correction to the rule above, proved rather than asserted.** The entry says a fail-fast test "can only ever evidence its first case". The lane corrected the lead's wording:

> The fail-fast loop did catch any single case that regressed — either form does. What it could not do is report a second one.

The defect is unreportability, not insensitivity. It bites when several cases regress together. The lane demonstrated it, weakening the guard four ways against the restructured table:

| guard weakened for | passed | failed | cases the one failing run reported |
|---|---|---|---|
| the empty token | 555 | 1 | 1 |
| a token containing a newline | 555 | 1 | 1 |
| a token starting `request=` | 555 | 1 | 1 |
| **whole guard removed (all three regress at once)** | 555 | 1 | **3, all named** |

The same production defect reported one case under the fail-fast loop and three under the collected table. The lane declined to claim the first three rows as evidence for the restructure, because they pass under either form. The corrected rule: a claim quantified over N inputs needs a test that can report N failures, not merely detect one.
Origin: implementation (epic fiddle-eoqx, bean fiddle-n8fs) — a null inversion result that changed the production code
Tags: #process #testing

### 2026-08-11 — `cargo test` green and `cargo clippy` red on a test-only change
A lane restructured an assertion table. It touched tests only, and no production code. The workspace suite passed at 556. `cargo clippy --workspace --all-targets --all-features -- -D warnings` exited 101 on `clippy::type_complexity`, in the new helper's return type. Two type aliases fixed it.

A test-only change is not clippy-safe by construction. `--all-targets` lints test code. A helper signature in a `#[cfg(test)]` module can therefore fail the gate exactly as production code can. A lane that reasoned "I only touched tests, the suite is green, the gate is fine" would have reported success on a red gate.

This is the fourth distinct way in this milestone that a green signal has stood in for an unrun check. The others: an exit code read through a pipe reporting `tail`'s status; a clippy line swallowed by an `&&` chain; and a test filter that matched nothing and reported `0 passed; 15 filtered out`. Every gate command runs, and its own exit code is captured, on every change. No change is exempt by category.
Origin: implementation (epic fiddle-eoqx, bean fiddle-ayqd) — found by a lane that ran clippy on a test-only change instead of assuming
Tags: #process #infrastructure

### 2026-08-11 — An earlier assertion can short-circuit the one that carries the property
The seventh dressing, and a new mechanism. The test fails correctly, and the assertion carrying the criterion is never evaluated.

Task 7's `the_revision_is_part_of_the_identity_and_not_only_of_the_payload` asserted the target strings first and the two `EffectId`s second. Under the inversion that drops `@{head_sha}` from the target, the run failed on the string comparison. It reported `left: "acme/r#7", right: "acme/r#7"`, and stopped. The test noticed the break. Its diagnostic named the mechanism, the target's spelling. The property — two revisions derive two identities — went untested at the moment it broke.

The lead had predicted a different failure. Both sides of the identity comparison might derive through one code path and collapse together. That was wrong in mechanism, because the assertion is `assert_ne!` and a collapse makes it fail. The consequence was the same, reached by assertion order. The implementer proved the identity half is independently load-bearing, with a throwaway probe asserting nothing but the inequality:

```
unmutated:       identity(aaaa) = 3ec6f2ec9d777a35 / identity(bbbb) = 8bf86e9eb29943b9  -> passes
under inversion: both collapse to 4c87b686e7dd354b                                      -> fails
```

Reordering so the identity is asserted first makes the inversion report the property instead of the spelling. Both halves still assert.

The rule. When one test asserts both a mechanism and the property that mechanism serves, the property goes first. An assertion that fires earlier consumes the failure. What is hidden is which claim a red run establishes. That becomes visible only when somebody breaks the thing deliberately and reads the diagnostic rather than the exit code.

A second thing from the same run, worth keeping as method. The inversion failed two tests and the implementer counted one witness. The other is `a_mutation_with_no_node_id_in_hand_is_not_sent`. It fails only because it asserts a refusal message containing `acme/r#7@aaaa`. That is sensitivity to the target's spelling, and not independent evidence for the identity property. Two tests failing is not two witnesses.
Origin: implementation (epic fiddle-eoqx, bean fiddle-dvsl) — an inversion asked for three times, which found a defect once it ran
Tags: #process #testing

### 2026-08-11 — Lead instructions and implementer reports crossed four times on one bean, and the bean body was the fix
Four round trips on `fiddle-dvsl` were spent on work that was already done. The lead asked for the `@{head_sha}` inversion after it had run. It asked for a refusal test after it had landed. It accepted a different bean at a commit that did not contain the fix it was accepting. Each time it wrote an instruction while the implementer's report on that item was already in flight.

The implementer named the mechanism and the remedy:

> my reports and your instructions have crossed every time, because I commit and report while your next message is already in flight. Check the bean's `## Summary of Changes` tail before writing the next instruction — I append there before I send, so it is the one place that is never stale.

A message is a snapshot of what its author knew when they started writing it. The bean is the current state. Read the bean's `## Summary of Changes` tail immediately before dispatching any instruction to a live lane. Do not read the last report received, which is by construction older than the bean.

The cost is not only wasted round trips. The lead built an evidence pack and dispatched an evaluator against `d8ebbd6`. It accepted the bean at a commit that did not contain the restructure the pack's own findings turned on. The fix landed afterwards as `c9a5a50`. The evaluator's failing verdict on `m3-refusals-classified-honestly` was therefore against a tree missing the fix for that criterion. Reading the bean tail before building the pack would have caught it. Reading the last report could not.

Two smaller rules from the same episode. An implementer holding verified work whose lane is being shut down should land it rather than let it die, and say so. This one did. It judged a revertible commit better than losing work the lead had asked for, and flagged that an evaluation might be mid-flight against the older commit. And a shutdown reason is not a state assertion. Accepting a bean and retiring its lane are separate acts, and the first needs the bean read.
Origin: process (epic fiddle-eoqx, beans fiddle-dvsl and fiddle-ayqd) — named by an implementer after the fourth crossed instruction
Tags: #process

### 2026-08-11 — A lint fix inserted between a doc comment and its item silently reattaches the documentation
The eighth dressing, and the first with a purely mechanical cause. Nobody wrote a false sentence. A fix to an unrelated lint moved a correct sentence away from what it describes.

`fiddle-ayqd`'s restructure carried a long doc comment. It explained why a `for` loop of `assert_eq!` cannot evidence a claim about three cases. Its first version tripped `clippy::type_complexity`. The fix, two type aliases, was inserted between the comment and the function:

```
/// the essay about collect-then-compare
type Case = (&'static str, String, String);
type Refusals = Vec<(&'static str, String)>;
fn refusals(cases: &[Case]) -> (Refusals, Refusals) { … }
```

Rustdoc attaches a doc comment to whatever item follows it. The essay therefore documents `type Case`. `fn refusals` is undocumented, and both `see [refusals]` links lead to a function with no prose. The bean and the lead's evidence pack both assert that the distinction is "written into `refusals`' documentation". That is not true of the artifact as committed. The lint fix wrote the contradiction.

Nothing can fail here. `cargo doc` builds, clippy is clean, the suite is green, and the essay reads correctly in isolation. `cargo doc` renders it under `type Case`, and nobody reads rendered docs during a gate.

Two rules. When a fix inserts an item, check what the doc comment above the insertion point now documents. A type alias, a `const`, or a `#[cfg]` block added between a comment and its function is a silent reattachment. And an intra-file `see [x]` link is only as good as `x` having prose. A link into an undocumented item reads in the source exactly like a live one.

Two more findings from the same pass, as calibration for how precise a corrected claim must be. A pointer under-names the function holding the comparison it corrects. The corrected sentence is about the head-sha comparison, which lives in `observe` and not in the `resolve` the pointer names. And a claim is compressed past the point it stays true. "The one field the conversation cannot supply" holds for a forged effect id. It does not hold for a verbatim copy of the marker, which supplies it and is refused as `DuplicateRequest`.

**Addendum — a correction appended rather than folded in leaves the error in place.** `fiddle-ayqd`'s confirming pass failed `m3-refusals-classified-honestly` on this. The wording it failed on was the lead's. The lead wrote that a fail-fast test "can only ever observe its first case". The implementer put that into a doc comment. It then corrected the lead with the sharper distinction, unreportability rather than insensitivity, in a later paragraph of the same comment. The verdict: *"The later paragraph corrects this distinction, but the contradictory wording remains in the same documentation and prevents an honest classification."* A reader who meets the first paragraph learns something false.

This is the replace-do-not-append rule, already standing for bean records, applied to code comments. Two beans in this milestone were told to replace stale inversion figures in place, rather than leave a number with a correction beneath it. Both did. Nobody applied the same rule to prose, where the failure is worse. A stale number beneath a correction is obviously superseded. A stale sentence three paragraphs above one reads as the author's position.

A correction to a comment belongs where the claim is, not after it. And the lead's wording propagates into artifacts. This one travelled from a dispatch message into a committed doc comment. It survived being refuted, because the refutation was written as an addendum to the lead's framing rather than a replacement. When an implementer corrects the lead, the lead should ask where the original wording now lives.
Origin: evaluation (epic fiddle-eoqx, bean fiddle-ayqd, iteration 2) — all three found by an evaluator reading the committed artifact rather than the diff
Tags: #process #docs

### 2026-08-11 — The liveness check missed a whole worktree, and the report channel the beans specify cannot be used
Two process defects with one consequence. The lead declared a lane dead twice while it was working. It dispatched duplicate implementers onto one bean three times.

**1. `git worktree list` was never part of the artifact check.** An earlier entry established that lane liveness comes from artifacts rather than from a bean's `status` field. That rule was applied to the shared branch checkout only. An implementer working in its own detached worktree is invisible to it. `git log` on the branch shows nothing. `git status` in the shared tree shows nothing. `total_dispatches` reads 0, and no step is ticked, because none of that happens until it commits.

`fiddle-v5bm` had about 1250 uncommitted lines in `scratchpad/dev-v5bm`. That was a full `PublishDecisionRequest` with its `IntegrationOperation` impl, `render_request`, `decision_request_target`, and an 839-line test file with 20 cases. The lead was reporting the bean as stalled and dispatching replacements. Both files had been modified three minutes before the lane that found it wrote its report. It named the shape: *"`total_dispatches: 0` and the unticked steps are consistent with a live implementer that has not committed yet — the same trap as reading in-progress status as evidence, one layer down."*

Enumerate worktrees and check each one's status, not only the branch checkout. A detached worktree named for a bean is the strongest evidence that bean is being worked on, and it costs one command to see.

**2. The report channel the bean templates specify does not exist.** Bean bodies and dispatch prompts have been telling implementers to write a report to `scratchpad/report-<task>.md`. Subagents here cannot write report files, because the harness refuses. Several lanes discovered this independently and reported inline instead, each noting the refusal. One put it plainly: *"Any implementer told to report that way will fail to, silently, and you will read the silence as no work."* Stop specifying a file. Ask for the report inline in the final message. Remove the file instruction from the bean template and every dispatch prompt.

**The compounding cost.** Three lanes were dispatched onto one bean. Two collided in `crates/fiddle-runtime/src/human/mod.rs`. The second one's first edit was rejected as changed-under-it, with a hook naming the other worktree. The protection built up all milestone does not help. That protection is `git commit --only <explicit paths>` and checking `git status` before committing. The collision is in the file, not the index. Two agents editing one file defeats every index-level safeguard. The only real protection is not dispatching two agents into one file, which requires knowing the first one exists.

**Addendum — a fifth defect in the same comment, found after the bean converged.** `fiddle-ayqd` converged at `4111711` with five evaluations behind it. A lane then landed `d6696da`, comment-only, fixing something none of those five caught:

> The enumeration heading `a_mangled_body_is_malformed_and_says_how` listed four kinds of damage — reflowed, respaced, truncated, closing lost — over a table that runs five cases. The kind it omitted, a dropped version token, is one of the three that used to refuse as `MarkerError::Version`, so a reader mapping "the first three" onto the enumeration got the truncation instead, which was always `Malformed`. `c9a5a50` corrected the counts to five and three and left the list beneath them at four.

The count fix was half a fix. The lead verified that repair by grepping for the phrase `All four` and finding it gone. That check could only confirm the numbers, never the enumeration.

The tally for one doc comment is five defects, five evaluations, and the fifth found after convergence:

1. the essay orphaned onto `type Case` by a lint fix;
2. a correction appended rather than folded in;
3. "the message a reader would see" when the code captures the inner `Malformed` payload;
4. "a run names every case that moved" when plain `assert_eq!` prints two `Vec` dumps;
5. a five-case table under a four-kind enumeration, with "the first three" pointing at the wrong three.

None was catchable by `cargo fmt`, `cargo clippy -D warnings`, the test suite, or `cargo doc`. A person reading the artifact found every one. Two evaluators plus a confirming pass read this comment without seeing the fifth.

Convergence is two consecutive passing evaluations. It is a real gate for behaviour, because inversions make behavioural claims falsifiable. It is not a gate for prose. A prose claim about code should be written so that something can fail. The byte pin, the inversion, and the per-case table all do that. Where that is impossible, the claim should be a reference rather than an assertion. A pointer cannot go stale in a way nothing notices.

**Addendum — a third way, and the tally.** The lane reports that the lead's direct status question never arrived: *"I have no record of one, and I sent you three unprompted messages."* Those three did arrive. The channel is therefore lossy in one direction at least once. The lead read the absence of an answer as confirmation the lane was dead.

Three independent ways a working lane looks silent, all of which fired on one bean:

| mechanism | what the lead saw | what was true |
|---|---|---|
| liveness check blind to detached worktrees | no commits, no dirty files, `total_dispatches: 0`, no ticked steps | about 1250 lines in a worktree named for the bean |
| report-file instruction subagents cannot follow | no report | a lane reporting inline, or not at all |
| a status question that did not arrive | no answer to a direct question | a lane that never received it |

Together they produced three duplicate dispatches onto one bean, two lanes colliding in one file, and a lead confidently reporting a bean as stalled while it was being implemented.

The correction is fewer inferences, not more channels. Every one of these failures has one shape: the lead concluded something from an absence. An absence is the weakest possible evidence in a concurrent system. Every mechanism that could carry the signal can also drop it. Never conclude a lane is dead from an absence. Require a positive observation that it is gone, such as a terminated notification or an approved shutdown. Before dispatching onto a bean, run `git worktree list` and check each tree's status. Run `git log -S<symbol>` for the work the bean would produce. Read the bean tail.

### 2026-08-11 — `cargo doc` is not in the gate set, and 53 warnings have accumulated behind it
Verifying `fiddle-v5bm` at `59a319e`, the suite was green at 583 passed, 0 failed, 1 ignored, 40 binaries. `cargo fmt --all --check` exited 0. `cargo clippy --workspace --all-targets --all-features -- -D warnings` exited 0 with zero warning lines. `cargo doc --workspace --no-deps` then emitted 53 warnings.

> **Correction on the number, because the number depends on the invocation.** A lane measuring `cargo doc --no-deps -p fiddle-core -p fiddle-runtime` saw the workspace go from 51 to 48 across its own change. The pre-existing backlog on that invocation is therefore 48. The 53 above is `--workspace --no-deps`. They are different measurements, and the lane flagged that rather than reconciling them. Whoever takes the gate must fix the invocation first and count once. A warning backlog quoted without its command is the same class of unattributable figure this milestone has been fighting all day.

Breakdown by kind. 38 read "public documentation for X links to private item Y". 8 read "redundant explicit link target". 4 are genuinely unresolved links, one of them `unresolved link to 'contract'` in `crates/fiddle-runtime/src/ports.rs`. They spread across at least twelve files. The heaviest are `github/cli.rs` with 12, `workspace/command.rs` with 5, and `git/publish.rs` with 5. None is new. They accumulated because nothing runs `cargo doc`.

This milestone spent extraordinary effort on documentation defects no gate could catch. The standing conclusion was that convergence gates behaviour and does not gate prose. That conclusion was half wrong. A gate for one class of prose defect ships with the toolchain and is not being run. `cargo doc` catches unresolved links, links into private items, and redundant targets. It caught a real one on this bean. An earlier state of `human/mod.rs` linked `[HumanInteractionPort::request]` while nothing defined the trait. `clippy` cannot see that, because `broken_intra_doc_links` is a rustdoc lint. The defect resolved itself only because the port was later built. The gate set was incomplete, not the gates. What cannot be mechanically checked is whether a sentence is true. What can be checked is whether its references resolve.

Owed work, filed as its own bean. Add `cargo doc --workspace --no-deps` to the gate. Decide whether to enforce with `-D warnings` immediately or ratchet. Clear the backlog. Ratcheting is probably right: 53 is too many to fix inside another bean's scope, and a gate that starts red teaches lanes to ignore it. One caveat. `private_intra_doc_links` firing 38 times may be a deliberate style here, because public docs pointing at private implementation detail is often the useful link. Judge that arm before enforcing it.

### 2026-08-11 — Twenty-four tests passed over a post-forever bug, because every fixture built the two ids agreeing
The clearest instance yet of the fixture family, and it was in landed code rather than in a sketch.

`HumanDecisionRequest` carries the request id twice. It carries its own `request` field and `binding.request`, with nothing making them agree. Only `binding.request` is rendered into the marker. `PublishDecisionRequest::target()` and `inspect`'s lookup were reading `self.request.request`.

All 24 tests in the file passed over it, because every one built a request whose two ids were equal. When they disagree, the run publishes a marker naming one id, searches for the other, finds nothing, and posts forever. As the implementer put it, the executor cannot close that door, because *from step 3's view the postcondition genuinely is absent each time*. It is a liveness bug with no upper bound, invisible to a full green suite.

Fixed at `0939c39`. Both readings go through one private `asking()`. Two new tests make the fields disagree deliberately. Inversion I10 confirms those two are the only cases in the file that can notice.

Three things worth keeping:

- **A type that carries one value twice makes agreement a fixture convention.** Every test author naturally constructs a consistent object. The disagreeing case never appears unless somebody writes it on purpose. The duplication is the defect, and the wrong read is its symptom. Collapsing it is filed as `fiddle-11vj`.
- **The bug was found by a prose warning, not by a test.** The lead flagged the duplicated field from an implementer's report, which said the field being read was never checked. The implementer then found its own landed code reading the wrong one. No gate participated.
- **This is the fixture problem at its worst.** The earlier instances were a sorted listing hiding a positional read, and a world list omitting the contested case. Here the type invites the indistinguishable fixture, so every honest test built one. The earlier rule needs a companion. When a type can hold two values that must agree, at least one test must set them disagreeing, and the type should be suspected of being wrong.
Status: Resolved 2026-08-12 by `fiddle-11vj` at `f2f4974`. It collapsed the duplication by **deleting** `HumanDecisionRequest::request`, rather than by guarding reads of it. The field had zero readers in the whole build: `grep -rn 'request\.request' crates/ --include='*.rs'` returned 0, no destructuring reader existed, and nothing serialized the type's shape into a contract. The two disagreement tests this entry credits could therefore not be kept. After the deletion the divergence is unrepresentable. The companion rule this entry proposes needs its own companion, recorded in the entry *A type that carries one value twice cannot be guarded by a behavioural test, but its shape can be* below.

### 2026-08-11 — The dispatch log was not drifting, it was not being written, and restart recovery reads it
Reconciling the epic's dispatch counters against the beans that converged:

| bean | `total_dispatches` recorded | real dispatches |
|---|---|---|
| `fiddle-rvcu` | 0 | 3 |
| `fiddle-9krm` | 2 | 5 |
| `fiddle-n8fs` | **field absent** | 5 |
| `fiddle-dvsl` | 0 | 9 |
| `fiddle-ayqd` | **field absent** | 11 |
| `fiddle-8vpm` | **field absent** | 4 |
| `fiddle-v5bm` | 0 | 4 |

The counter had not drifted. The mechanism is intact and correctly wired. `scripts/append-eval-log.sh` initialises and increments `total_dispatches`. `scripts/parse-eval-log.sh` reads it back into `{base_sha, iteration_count, total_dispatches, last_verdict, last_guidance}`. `skills/develop-loop/restart-recovery.md` consumes that. The lead stopped running the logging step. It ran once, early, on one bean. It was dropped for every subsequent iteration on every bean, and three beans never had the field initialised.

The consequence is that restart recovery would mis-read the entire epic. A fresh session runs `parse-eval-log.sh` and gets `total_dispatches: 0` for beans that consumed nine and eleven dispatches. It re-dispatches against a 16-budget it believes untouched.

Two things follow. **The budget guard was never enforcing anything this session.** `check-convergence.sh` takes `--current-dispatches` as an argument, and the lead passed hand-estimated numbers rather than `parse-eval-log.sh`'s output. The one protection against unbounded iteration came from the lead's memory. `fiddle-ayqd` used 11 of 16, and nothing would have stopped it at 16. **And a step that only matters after a crash will be the first step dropped.** Nothing in a healthy session depends on the eval log. The lead knows the counts. The bean bodies carry the narrative. Convergence is computed from scorecards on disk. The log's only consumer is a session that no longer exists, and the failure is silent, because a log nobody reads cannot be noticed as missing.

That is a design problem rather than a discipline problem. Make the step that logs the dispatch be the step that produces the number convergence is checked against. Skipping it then fails the iteration rather than costing nothing.

### 2026-08-11 — The effects credential's grant is wider than four documents describe, in an unknown direction
On 2026-08-10, during `fiddle-w0xt`, a GraphQL `createIssue` against `peel/fiddle-effects-acceptance` succeeded under `FIDDLE_GITHUB_TOKEN`. It opened issue #25. Nothing grants it. Four documents describe the same five-permission grant: `.env.example`, `docs/technical/effects-repository.md`'s permission table, `docs/evaluator-calibration-general.md`, and `docs/technical/decisions/018-a-graphql-200-is-not-a-success.md`. That grant is Contents, Pull requests, Actions, Metadata, and Secrets: none. Under it, that mutation should have been refused.

It is unresolved, deliberately. Every issue-modifying operation was refused in the same session. REST `PATCH .../issues/25` with `state=closed` answered 403. GraphQL `closeIssue` and `deleteIssue` both answered `FORBIDDEN`. The finding is not that the token holds `Issues: write`, because a token holding that would have closed the issue. Some authority permits creation, nothing permits removal, and no document names it. The lead closed #25 with the operator principal.

`fiddle-gund` was explicitly forbidden from resolving it, and that is the right shape. Resolving it means reading the credential's real permission set, or probing further. Probing further means issuing writes with a credential whose authority is not understood, against a repository whose standing rules are what make a destructive cleanup sweep defensible there. It is the operator's call.

Closing it requires two operator actions. Read the token's actual permission set at GitHub and reconcile it against the four documents, correcting whichever is wrong. Then re-run the probe table in `effects-repository.md` § *The selection, verified by probe rather than assumed* against the reconciled grant. Until both are done, treat the effective grant as wider than the table in an unknown direction.

**The general point, which outlives this token.** The permission table's standing rule is that scope is proven by a 403 and never by a successful read. A public repository reads with any credential. This is that rule's mirror case, and the table had no place to put it. A success proving the presence of an authority nobody documented is exactly as unresolved as a read leaves the absence of one. The temptation is to write the row that makes the observation look expected. `effects-repository.md` § *A success this table does not account for* records it without inventing a permission level. It is also the second time this milestone that four agreeing documents were wrong about this credential. The first was the two-repository selection under § *The second row read 200 until 2026-08-10*. Four documents agreeing measures how often one was copied.
Origin: evaluation of `fiddle-w0xt` (M3 Task 1), recorded by `fiddle-gund`
Tags: #debt #infrastructure #security

### 2026-08-11 — Every commit briefly removes every other lane's uncommitted work from disk, 150 times today
Flagged by a lane that noticed three `.rs` files it did not own were dirty when it arrived. It committed docs beside them, then verified afterwards that all three were still modified.

`prek`, the pre-commit runner, stashes all unstaged changes before running hooks and restores them after. That is correct for a single-user repository and a hazard with concurrent lanes. Any commit by any party momentarily takes every other lane's uncommitted work off disk. `.devenv/state/prek/patches/` holds 150 such patches, one per commit made while somebody had unstaged changes. Every one was a window.

Two consequences, and the second has misled this session. **A failed or interrupted restore leaves work only in a patch file**, and nothing announces it. Earlier today the lead found a lane's work in such a patch and concluded it had been stranded. The lane had committed. `git apply --check` refusing the stale patch is what stopped a destructive recovery. **And `git status --porcelain` lies during the window.** The lead has used a clean status as ground truth for "is anyone working in this file" all session, and dispatched on that basis. During a hook window a file with a thousand uncommitted lines reports clean. That is a third mechanism by which a working lane looks idle.

The absence-inference rule recorded earlier needs its converse. A single clean `git status` is not a positive observation of absence either. It is a sample, and a periodic process makes that sample unreliable. Prefer evidence that cannot be transiently wrong: `git worktree list`, `git log -S<symbol>`, the presence of a symbol in `HEAD`. If a clean status is load-bearing, sample it twice.

The lane that found this noticed foreign dirty files, kept them out with `--only`, committed, and then verified they were still there. Nobody else has been taking that last step, the lead included. There were 150 hook windows, and the first check that anything survived them was made by a lane committing two documentation files.

### 2026-08-11 — Correcting the entry above: ADR 018 does not enumerate the grant, and RUNBOOKS enumerates a different one
Acts on *The effects credential's grant is wider than four documents describe*, immediately above. That entry's text stays as written.

**ADR 018 is not one of the documents describing the grant.** That entry names `docs/technical/decisions/018-a-graphql-200-is-not-a-success.md` as a fourth source. `grep -ci permission docs/technical/decisions/018-*.md` returns 0. The ADR enumerates no permission anywhere. Its only contact with the subject is quoting a `FORBIDDEN` response body, whose message is "Resource not accessible by personal access token". The claim came from the bean body, and the lead carried it forward without opening the file. That is the same class of defect the entry is about, committed in the act of recording it.

Four places do enumerate that grant, each verified by opening it. `.env.example`'s permission block. `docs/evaluator-calibration-general.md`, which additionally states that "`Issues` is absent" and records the design choice built on that absence. GitHub routes an issue comment through Issues and a pull request comment through Pull requests, so M2's conversation was deliberately put on a pull request. `.github/workflows/github-effects.yml`'s permission comment. And `effects-repository.md`'s own table.

**A fifth document enumerates a different grant, and it is the one operators follow.** `docs/technical/RUNBOOKS.md` § *Minting the GitHub token* is the procedure for creating this credential. It sets resource owner `peel` and selection `peel/fiddle-effects-acceptance` only. It says "these five, and no others" while listing `Workflows` read and write, and no `Secrets` row. `effects-repository.md`'s table argues the opposite. `Workflows` is "not something the lane ever does", and a credential holding it "can rewrite the target's CI, which is the worst of both".

This explains nothing about the issue. `Workflows` is not `Issues`, and no reading of it permits `createIssue`. It changes the shape of the open question: five documents carrying two answers, not four carrying one. Whoever closes this must reconcile the mint procedure against the description, not only read the token's live permission set.

The fix for a wrong cross-file citation is not a more careful reading of the citing file. It is opening the cited one. Both defects here were invisible from `effects-repository.md`. Each took one `grep` in the other file.
Origin: iteration-2 evaluation of `fiddle-gund`; defect found by the lead, second half found while fixing it
Tags: #debt #infrastructure #security

**Correction to the forward-warning above — it was right about the coupling and wrong about the symptom.** The warning predicted that removing `gh_stub`'s silent-success-on-unscripted-graphql default would break `the_mutation_the_child_received_binds_the_node_id_from_the_read`. It did not. The lane that removed the default reports why:

> The stub records the request and increments `graphql_calls` **before** it routes, so every assertion in that test — `argv`, the query text, the bound `id` — still held with the route panicking.

What broke was invisible. The test began asserting against a world whose fixture had died. Its own doc comment claims it "runs against the same world" as a neighbouring test and ends `Committed`. Withdrawing the default would have made that paragraph false without failing anything. The edit was made for that reason alone.

A fixture that records before it routes will absorb this class of change silently. Any assertion made against what the fixture recorded survives the fixture ceasing to work. A test can therefore keep passing while the world it describes no longer exists. That is a distinct member of the family this file catalogues. It is not a fixture that cannot distinguish two implementations. It is a fixture whose bookkeeping outlives its behaviour.

Two smaller things from the same lane. **It pre-empted its own null result.** Its planned inversion was to restore the silent default and see what notices. It worked out in advance that the answer would be nothing. It therefore wrote `an_unscripted_graphql_call_cannot_pass_for_an_answer` first, and gave the inversion a witness. **And it measured a diagnostic's usefulness rather than assuming it.** The filename is `eprintln!`'d before the panic. The client quotes `stderr` through a 120-character bound, and a panic's own `thread … panicked at <file>:<line>` prefix consumes about 78 of them. The name the diagnostic exists to carry would have been truncated out of the one place a test author reads it.

### 2026-08-11 — The tracker CLI stalled on another project's hung processes, and archiving was not the fix
Symptom: `beans show` taking 8 to 31 seconds, and `beans update` timing out at 45. Both had previously been instant. A batch of three writes in one command hung for two minutes.

**First diagnosis, and it was wrong.** The store had grown to 151 top-level files, so bean count looked like the cause. Archiving moved 418 beans, preserved and still queryable, leaving 31 files. Reads stayed at 31 seconds.

**Actual cause.** `ps` showed six hung `beans` processes querying `icecube-mps4`, a different project, invoked without `--beans-path`. They were holding whatever the CLI serialises on. Contention therefore arrived from outside this repository. They drained on their own, six to three, and reads returned to 8 seconds.

**The tell was in the timing all along.** 31 seconds wall against 0.25 seconds of user and sys. A process burning no CPU is waiting, not working. A store of 151 markdown files cannot take 31 seconds of anything. Reading the bean files directly took 0.4 seconds throughout.

Three things to carry. Check CPU against wall clock before theorising about size: almost-zero CPU means blocked, and blocked means look for a lock or a peer. The store is pure markdown — YAML frontmatter plus body, an activity log, and the archived set, with no index and no database — so direct file reads are a safe fallback, while direct writes bypass the CLI's etag check and are a real risk while lanes are live. And an invocation from another project, missing `--beans-path`, can stall this one.

A smaller note, since it cost two commands. `hooks/archive-guard.sh` rejects any command whose text matches the archived-directory path. That stops readers pulling stale artifacts back in. It fires on an `ls` of that path. It also fires on a BACKLOG entry that merely quotes the path while explaining the guard.

### 2026-08-11 — The isolation policy multiplied build cost by the number of lanes, and the standardised command line made a targeted kill impossible
Two failures with one root, and both are the lead's.

Load average reached 81 with five concurrent cargo runs and no target directory written in two minutes. The builds were thrashing rather than progressing. One lane reported a single test binary unfinished after fifteen minutes. The tracker CLI's stalls almost certainly share this cause.

This file prescribes a private `CARGO_TARGET_DIR` per lane. That is right about correctness and was never priced for cost. It means no shared compilation cache, so with five lanes live the machine does five full builds of the same tree. The fix for artifact races bought a load problem larger than the races it prevented. The disposal rule recorded earlier addresses disk and not load. Disk was the visible symptom, because it fails loudly at 100% while load fails quietly.

What should have been prescribed alongside the isolation. **Targeted runs by default:** `cargo test -p <crate> --test <binary>` for the lane's own work, with one full workspace run at the end for the attributable figure, and the record saying which counts came from which. For an inversion this is usually better evidence. A workspace figure buries which binary noticed, while the record needs the failing test names. The one claim a targeted run cannot make is that nothing outside the lane noticed. **And a concurrency ceiling on lanes that build:** two or three full-workspace builders on this machine, not five.

**The second failure is a pure own-goal.** Every lane was told to run the same command line, `cargo test --workspace --all-features --no-fail-fast`, for consistency of evidence. A lane whose own run had stalled ran `pkill -f` on that exact string. It killed up to four other lanes' runs, because the standardisation had made the pattern match everybody. Five matching processes before, zero after. Its report was immediate and precise about the blast radius. It included the detail that mattered most: the victims would see a signal exit rather than a test failure, so anybody investigating a failure would be investigating a kill. Nothing was written to a working tree, and no other target directory was touched. It switched to per-binary runs unprompted.

Standardising a command for evidential consistency also makes every instance of it indistinguishable to process tools. If a command line is shared verbatim across lanes then no lane may pattern-kill it. The way to make that safe is for a lane's runs to be distinguishable, by target directory in the argv or by wrapping.

### 2026-08-11 — In an append-only file a positional reference decays silently, and the entry that said so was fixed the wrong way twice
Acts on *2026-08-11 — Correcting the entry above: ADR 018 does not enumerate the grant, and RUNBOOKS enumerates a different one*, whose text stays as written.

**The claim being corrected.** That entry says the finding it acts on is "immediately above". It is not, and was not when written. *Every commit briefly removes every other lane's uncommitted work from disk* had already been appended between the two. The entry it acts on is *The effects credential's grant is wider than four documents describe, in an unknown direction*. It is named here so the reference cannot decay again.

**The rule.** In a file that only ever grows, every positional reference decays the moment anybody else appends. The decay is silent. Nothing rereads the sentence, no tool checks it, and the reader who follows it lands on an unrelated finding with no signal. A heading is stable because this file's rule makes it stable. A heading is therefore the only safe way to point at an entry here. "Above", "below", "the previous entry", and "the one before this" are latent falsehoods with a fuse lit by the next contributor.

**How it was fixed wrongly, twice.** First, the correction was made by rewriting the entry in place, editing its heading and its opening line. This file's header rule forbids that. The two permitted moves are appending a `Status:` line and appending a new entry. Nothing was erased, because the original wording was quoted verbatim inside the rewrite. The mechanism was still wrong, and it was wrong in the paragraph arguing that a heading is stable because the rule keeps it stable.

Second, that in-place rewrite reached a commit belonging to a different lane. The lead ran `git commit --only docs/BACKLOG.md` while this file was open with the rewrite in the working tree. `--only` takes the working-tree state of the whole path. The rewrite therefore landed inside *The isolation policy multiplied build cost…* with 24 insertions and 2 deletions. The deletions were another lane's edit. The lead had warned three separate lanes about this hazard the same day.

Each link is cheap to break. Fix the first by naming headings. Fix the second by using the file's two permitted moves, even when the change looks too small to deserve an entry. Especially then, because that is when rewriting feels harmless. Fix the third by reading `git commit --only` as "the whole path's working tree" rather than "my changes to that path".
Origin: iteration-4 evaluation of `fiddle-gund`; the in-place rewrite and the sweep were found by the evaluator and the lead respectively
Tags: #debt #infrastructure

### A liveness check that could not evaluate reported idle, and the lead killed a live build
Asked to kill idle agent processes, the lead inventoried and classified four processes as stalled. It killed them by PID, and destroyed a live, progressing build. Measured after the fact: 1,429 files written into that lane's target directory in the five minutes before the kill, and 2,597 in ten.

**First, the wrong vital sign.** The parent `cargo` process showed `%cpu 0.0`, which was read as hung. A parent `cargo` at 0% CPU is its normal state while `rustc` children compile. A `rustc` at 41% CPU was visible in the same output, and was dismissed as belonging to somebody else. For a build, the liveness signal is bytes landing in the target directory, never CPU on the parent.

**Second, a check that could not evaluate was scored as a check that found nothing.** The guard was a scan for target directories written to within 90 seconds. It printed its "(none listed = no build is progressing)" line and nothing else, and that was taken as evidence of idleness. The scan silently produced no rows, because `-newermt` did not evaluate as intended. The output distinguishing "I looked and found no writes" from "I could not look" was therefore the same output.

**The rule: a negative check must be able to fail loudly.** Any check whose absence of output is load-bearing prints its own denominator, the number of candidates it examined. `found 0 writes across 3 target dirs` and `examined 0 target dirs` are different sentences. Only one licenses a kill.

This is the fifth instance of the same inference on this milestone, and the first that cost work. The prior four cost only time. A missing bean section read as a stalled lane, pushing one task three times while it was measuring. A liveness check blind to about 1,250 uncommitted lines, dispatching three lanes onto one branch. And two others. Every one read absence of a signal I knew how to see as absence of the thing.

**The load was never the agents.** It was OrbStack at 78%, SkyLight at 68%, and Defender at 20%, on a machine running 678 processes. Load average with idle CPU means blocked. The first question is what it is blocked on, asked before anything is killed. Disk at 94% with 30G free was checked and cleared. The four processes killed contributed nothing measurable, and load rose from 177 to 184 afterwards.

Recorded alongside the earlier `pkill -f` own-goal. Same family. Process-level intervention across lanes needs positive identification of the owner. Killing another lane's work needs its consent rather than the lead's inference. The lane was told the failure was external, so it would not debug a phantom. It was told to keep its 853M target directory.

### A ruling and an evaluation dispatched in the same breath guarantees a stale pack
The third occurrence on this milestone of a pack pinned behind the bean it evaluates, after `fiddle-rvcu` and `fiddle-8vpm`. It is the first the lead caused rather than merely failed to notice.

The lane reported DONE at `41c3c43`. The lead reviewed it, found a load-bearing claim refutable, and sent a ruling to fix it. It then built the evidence pack and dispatched the evaluator at `41c3c43`, while the lane was still acting on that ruling. The lane landed the fix as `e6667e9`.

Issuing a ruling and dispatching the evaluation in the same breath guarantees the pack is stale if the lane obeys. A ruling that asks for a change is a promise that the tip will move. Dispatching against the pre-ruling commit is therefore a contradiction rather than a lost race. After a ruling that requires a change, wait for the lane to name the resulting commit. Waiting costs minutes. Not waiting costs an evaluator failing a criterion on text that no longer exists.

The specific harm was not cosmetic. The dispatch told the evaluator to probe the git-count criterion hardest. At `41c3c43` the doc comment on `the_approve_path_invokes_git_not_at_all` still carried the refutable sentence, "no program seam", which one `grep -rn 'Command::new'` disproves. That comment ships in the diff, and it is the first thing an evaluator reads on that criterion. The stale dispatch therefore pointed a primed evaluator at the one sentence the ruling existed to remove. A FAIL would have been correct on the text and wrong about the artifact.

What the lane did right is the generalisable half. Told the argument was refutable, it fixed the claim in the bean and in the shipping doc comment. Correcting only the bean would leave the refutable version in front of the evaluator. It then verified every claim the lead had cited at it, rather than accepting the correction on report. It found a third reason the lead had missed. This capability has two git channels, not one, because `Workspace::create` and `changed_files` bypass the seam with a direct `Command::new("git")`.

Same family as the earlier entry on absence-inference, inverted. There, a check that could not evaluate was read as a negative result. Here, a commit that had not happened yet was evaluated as though it had. Both are the lead treating a state it had not observed as the state that holds.

### A lane had the correct measurement and published the lead's wrong number
The lead told a lane that `grep -rn 'Command::new' crates/fiddle-runtime/src/` returns six hits. It returns seven. The lead's six came from an invocation piped through `| grep -i git`. The line that filter drops is the `Command::new` call in `workspace/command.rs`, the program seam the argument turned on. A filtered count was therefore published as the unfiltered command's output. It was published inside the one sentence written to survive an evaluator's grep. An evaluator found the discrepancy. The conclusion was unaffected, because the seam was independently verified.

The lane's disclosure is the finding:

> *"I ran the unfiltered grep myself and my terminal printed seven lines, and I wrote six because your message said six."*

It had the correct measurement on screen and published the lead's number instead. Nothing was missing: not the tooling, not the skill, not the diligence. Authority overrode measurement. It did so in a lane that had spent the day verifying every other claim it was handed on this same bean, including four citations the lead had given it.

Every figure the lead puts in a dispatch is therefore a potential contaminant. It can overwrite a lane's own correct observation. This inverts the usual worry. The concern has been lanes reporting things the lead cannot check. The actual failure was a lane declining to report something it had checked. A lead who states numbers freely is not only risking being wrong. It is suppressing the measurements that would have caught it.

Three changes, and the third is the only structural one:

1. **Do not put measurements in dispatch messages when the command can be named instead.** "Run `grep -rn 'Command::new' crates/fiddle-runtime/src/` and use what it prints" cannot be deferred to incorrectly.
2. **When a figure must be stated, mark it as the lead's and ask to be contradicted.** "My count was N — verify it, and tell me if yours differs" restores the lane's licence to report what it sees. Unmarked, a number in a lead's message reads as settled.
3. **Cite the call site, not the tally.** The `Command::new` call in `workspace/command.rs` either exists and says what it is cited for, or it does not. It does not change when a neighbour commits. A count is a claim about a whole tree at an instant. It goes stale silently, and it can be produced by an invocation that does not match the sentence around it. The tally was deleted rather than corrected, because the call site was always the whole claim. That also dissolves the deference problem for counts.

The lane's closing observation frames all three antipatterns this bean produced. Every one was a true statement that a neighbour's change made false. Each was caught by a reader who had the means to check and used it. The one that nearly escaped was the one where the reader had the means, used them, got the right answer, and deferred.

### sccache does not share across lanes, and the three-pass measurement says why
Two infrastructure incidents on this milestone share one root cause: load average 81, then the disk at 96%. A private `CARGO_TARGET_DIR` per lane means no shared compilation cache. Every lane therefore rebuilds the whole tree and keeps its own 4 to 8G of artifacts. `sccache` was installed to fix it. It does not fix the stated problem.

Three passes of `cargo check -p fiddle-core --all-features`, identical code, `CARGO_INCREMENTAL=0`, `RUSTC_WRAPPER=sccache`:

| pass | target dir | cache hits | wall |
|---|---|---|---|
| 1 — cold cache | `sc-a` | 0, with **19** cacheable compilations stored | 22.46s |
| 2 — **different** dir, same code | `sc-b` | **1 of 32 requests** | 10.87s |
| 3 — **same** dir as pass 1, deleted first | `sc-a` | **19** — exactly what pass 1 stored | **4.06s** |

The target-dir path is part of the cache key. This is not a misconfiguration. The `dev` profile carries debuginfo, which embeds absolute paths. Artifacts built into two different directories are therefore genuinely different artifacts, and sccache is right to refuse to share them.

What it buys is still worth having. It does not deduplicate compilation across lanes. It makes deleting a target directory cheap. A cold rebuild at a path sccache has seen is 4.06s against 22.46s. That is a 5.5x recovery from a bounded 10 GiB shared cache. It turns the disk problem from "reclaim space and pay a full rebuild" into "reclaim space freely". Pair it with routine target-dir reclamation rather than treating it as a substitute.

Two things would fix the stated problem, and neither is done here. **One shared `CARGO_TARGET_DIR` for all lanes.** Cargo's file lock serialises builds, so lanes block on each other. That is tolerable while this epic's dependency graph is nearly linear, and it removes the redundant compilation rather than caching around it. **Or `--remap-path-prefix`** to normalise absolute paths, so artifacts stop being path-dependent and sccache can share across directories. That is a workspace-wide `RUSTFLAGS` change, and it affects backtraces and debugger paths.

The method note is the transferable part. The first check ran `sccache rustc probe.rs -o probe1` twice. It reported 2 requests, 0 hits, 0 misses, 2 non-cacheable. That proves the wrapper is invoked. It says nothing about whether real builds cache, because a bare link step is non-cacheable by design. An installed tool is not a working tool. The test has to exercise the path the tool was chosen for, which here is two different target directories.

### The grant discrepancy is resolved: a public repository, not an undocumented permission
`fiddle-gund` recorded that the effects credential's effective grant was wider than four documents describe. `createIssue` succeeded where the described grant should have refused it, while every issue-modifying operation was refused. It was recorded unresolved and belonging to the operator. The operator has answered. The token has no Issues access at all.

- **`peel/fiddle-effects-acceptance` is a public repository** with issues enabled: `visibility: public`, `has_issues: true`. On a public repository any authenticated identity may open an issue. That requires no repository permission. `createIssue` succeeding is therefore not evidence of an Issues grant.
- **Modifying issue state is permission-gated.** REST `PATCH state=closed` returned 403. GraphQL `closeIssue` and `deleteIssue` both returned 200 carrying `FORBIDDEN`. Those refusals are the grant showing through.
- **The asymmetry was two different mechanisms.** Public semantics permitted the create. The absent grant refused every modify. The four documents were right.

The table's own standing rule caught this, one level deeper than anybody had applied it. The rule reads "scope is proven by a 403 and never by a successful read." The probe treated a successful write on a public repository as evidence about the grant. That is the same error the rule forbids for reads: a success can be explained by something other than the permission under test.

A second instance was caught while resolving this, and it was the lead's. The scoped token read `peel/fiddle` and `peel/fiddle-acceptance`, the two repositories recorded as 403 verified. Both reads succeeded. Both are public too, so the reads were public reads. The sound test is a permission-gated endpoint:

| repository | `GET /repos/{r}/collaborators` (requires push/admin) |
|---|---|
| `peel/fiddle` | `Resource not accessible by personal access token` |
| `peel/fiddle-acceptance` | `Resource not accessible by personal access token` |
| `peel/fiddle-effects-acceptance` | `1` |

The token's selection is the disposable repository alone, proven by denial rather than recorded on trust. `GET /repos/{owner}/{repo}` cannot serve as a scope proof. Every one of these repositories answers it to anybody.

Residue: issue #25, "scope probe", is closed. The rule that a lane must not create an issue at all stands, and is better founded. The reason a lane can create one is that the repository is public, which no credential change can prevent.

Owed, and small. The permission table's Issues row, its subsection, and ADR 018 still describe this as unexplained. This file's rule is append, never rewrite, so the correction is an appended resolution and wants its own bean.

### A confirming pass that renames the criteria cannot confirm them
`fiddle-4vsd`'s codex confirming pass returned a scorecard with five criterion entries and no antipatterns detected. Its dimension scores were identical to the first pass: correctness 9, domain_spec_fidelity 9, code_quality 8. Three of its five criterion ids were not the bean's.

| what codex reported | what it actually was |
|---|---|
| `m3-decision-table-is-strict-and-names-ids` | **two binding criteria merged into one** — `m3-authorized-set-has-no-permissive-default` and `m3-decision-table-is-strict-on-its-own` |
| `I6-control` | **an inversion row promoted to criterion status.** It is a measurement in the evidence, not a criterion |
| `m3-decision-has-one-key-and-no-stale-max-pages` | **invented.** The `max_pages` removal is work this bean did; no criterion asks for it |
| — | **`m3-silent-document-keeps-the-human-gate` was dropped entirely** |

The dropped one is the safety property. A document naming neither new policy row still yields `RequireHumanDecision` for the ready transition, through `combine`'s Human minimum. It leaves `PublishDecisionRequest` ungated. Two effect kinds, opposite outcomes, one silent document. It is the property that a deployment cannot accidentally remove the human gate by saying nothing.

Merged on its shape, that scorecard would have left the property unconfirmed with no trace. Five entries, all passing, matching dimensions, zero antipatterns. Every surface signal a merge step looks at was correct. The substitution was visible only by diffing the reported ids against the bean's eval block, which nothing in the loop does.

Check a confirming pass for criterion-set identity before reading its verdict. Not "did it pass" but "did it score the things the bean asks about". Compare the id set in the scorecard against the id set in the binding `eval` block, and reject any pass whose sets differ. `merge-scorecards.sh` matching on ids it is handed cannot catch this. A renamed criterion is a missing criterion wearing a plausible label, and a merge keyed on the scorecard's own ids reports full coverage of a set nobody asked for.

Paraphrase is the mechanism, and the substitutions all sounded reasonable. Merging the two strictness criteria is defensible as a summary. `I6-control` genuinely was the bean's strongest finding. The invented `max_pages` criterion described real work. A pass that drifts toward what the bean is about rather than what the bean asks produces exactly this: heavy substantive overlap, a plausible scorecard, and one silently missing property. The narrower the criterion, the more likely a summary swallows it.

### 2026-08-12 — A type that carries one value twice cannot be guarded by a behavioural test, but its shape can be
Corrects a claim in `fiddle-11vj`'s own report. Closes *Twenty-four tests passed over a post-forever bug* above.

`fiddle-11vj` deleted `HumanDecisionRequest::request`, the duplicated request id, at `f2f4974`. The implementer reported three things about tests, and the middle one was false:

1. The original bug is now inexpressible. True. The divergence cannot be constructed, so the two tests that constructed it could not be kept.
2. "No test can fail without the fix." False, and the evaluator wrote the counterexample. The type derives `Serialize`, so its shape is observable from outside without any behaviour involved. Asserting the serialized top-level key set fails with the field re-added, and passes without it. Five lines, and it closes the re-adding path mechanically.
3. Refusing to invent a fake behavioural guard was correct. True, and independent of the second point. The error was inferring from "no behavioural test is possible" to "no test is possible".

When the fix for a hazard is to remove surface, the guard is an assertion on the type's shape. Take it through whatever derive already exposes it: `Serialize`, `Debug`, a field-count constant. No behaviour is left to observe. Assert the key set and never an occurrence count. A second copy that disagreed, the dangerous case, passes a count of one. Landed as `fiddle_core::decision::tests::the_request_id_is_held_in_exactly_one_place`.

Two things were unrecorded and should not have been:

- **`docs/fiddle-agentic-factory-prd.md`'s `HumanDecisionRequest` sketch still declares a top-level `request_id`.** It is the only tracked document that did, the two design listings being gitignored. Read against the code it diverges in six ways: `request_id`, `invocation_ref: InvocationRef`, `capability_id`, `proposed_effect: ProposedEffect` where the type carries `binding: DecisionBinding`, `Vec<Risk>`, and `Vec<Alternative>`. It carries no binding at all. Its top-level id is therefore that design's only id and not a duplicate. It describes a pre-binding design the marker superseded. Owed: a pass reconciling the PRD's type sketches against the shipped types, which is bigger than one bean.
- **Deleting a `pub` field from a type re-exported at the `fiddle-core` crate root is a breaking change to that crate's surface.** It is workspace-internal only. The crate is unpublished, every consumer is in this repository, and the compiler found every reader. Nothing is owed downstream. Recorded because "no downstream exists" is a fact about today. The next such deletion should have to say so out loud.
Origin: evaluation of `fiddle-11vj` (codex confirming pass pending)
Tags: #debt #idea

### `mkdir -p` into a shared scratchpad inherits another lane's files, and a restore loop then writes them
`fiddle-565u`'s inversion driver created its pristine-copy directory with `mkdir -p "$SP/pristine"`. That directory already existed. It already held three `.rs` files another lane had pinned there. `mkdir -p` succeeds silently on an existing directory, so the driver treated a populated directory as its own. Its restore loop then walked everything in it and copied all of it back. That included three files it had never pinned. They landed in the repository root as untracked `decision_protocol.rs`, `human_mod.rs`, and `validate.rs`.

No tracked file was harmed. The lane verified `crates/fiddle-runtime/src/human/validate.rs` and `tests/decision_protocol.rs` were unmodified. It removed the three strays before committing and reported it unprompted, with the cause already diagnosed.

**`mkdir -p` is not "make me a fresh directory".** It is "ensure a path exists". It cannot distinguish a directory it created from one it found. Any script that follows `mkdir -p` by treating the directory as exclusively its own is wrong on the second run. It is wrong again when a sibling shares the parent. The scratchpad on this milestone is shared by every lane, so `$SP/<generic-name>` is a collision waiting for a second occupant. Use a name that cannot collide, such as the bean id. Or create with plain `mkdir` and let it fail.

**A restore loop that walks a directory restores whatever is in it.** The pristine-copy pattern is sound: copy before mutating, `cmp` after restoring, verify byte-identical. Its safety rests on the copy set being exactly what was pinned. A loop over `ls` rather than over a recorded manifest silently widens. It then includes anything a neighbour left behind. Record what you pinned and restore from that list. The failure is asymmetric and quiet. Extra files are written where they do not belong, and the `cmp` on the files that were pinned still passes.

Nothing about the inversion evidence is affected. The mutations, restores, and byte comparisons were all correct.

### A latent fixture race, found only by adding load, and fixed "reasoned, not measured"
`fiddle-565u`'s three new scenarios landed in the same test binary as `fiddle-pwyi`'s killed-repair scenario. One gate run then failed inside `delete_workspaces` with `Directory not empty`. That is the first failure of its kind. It came in the first run where those scenarios shared a binary.

The diagnosis. `kill -9` reaches one process. The `git` checking a worktree out is not in its process group, so it kept writing behind `remove_dir_all`'s walk. That is a plausible and specific account.

The lane could not reproduce it. Not under CPU load, and not with the previous condition restored, eight runs each way. It fixed the race twice and labelled both fixes "reasoned, not measured" at the sites themselves. `interrupt_a_repair_inside_its_worktree` now waits for the worktree to be checked out, rather than merely to exist. That is what its own doc comment had always claimed it did. `remove_tree` waits a racing writer out for up to a second.

Neither fix weakens anything, which is what makes an unreproducible fix acceptable. Every caller still asserts emptiness afterwards, so a tree that never empties still fails the test. An unreproducible failure invites two bad responses: ignore it as a fluke, or claim a fix works because the failure stopped appearing. Neither is available here. The lane stated the diagnosis as reasoning. It marked it unmeasured in the code rather than only in a report. It left the detecting assertion in place. A fix labelled as reasoned is auditable. A fix presented as verified when the failure was never reproduced is a claim nobody can check.

Worth noting what exposed it: added load on a shared test binary, not a new assertion. Two beans' scenarios in one binary changed the timing enough to surface a race that eight deliberate attempts could not.

### A restore reverted committed work and the byte-comparison guard reported success
Amends the entry above on `mkdir -p` and shared scratchpads. "Record what you pinned" is necessary and not sufficient.

`fiddle-565u`'s inversion driver pinned its files once and reused the pin. After committing `4722dcf` the lane added documentation to `gh_stub.rs`. It then ran an inversion touching that file. The restore wrote back the pre-commit pin, deleting fifteen lines of committed comment. The `cmp` guard passed, because it compares the tree against the pin.

A guard that confirms the tree matches the pin says nothing when the pin is stale. The comparison was correct and the conclusion it licensed was false. That is the same shape as every other guard failure on this milestone: a check whose subject was not the thing anybody wanted to know about. The lane caught it with `git diff` before committing and restored from `HEAD`. Nothing was lost.

It is the previous entry's incident one step along. That one restored files the driver had never pinned. This one restored a version of a file it had pinned, taken before the tree moved underneath it.

> A restore trusts a copy whose relationship to the current tree was assumed rather than established, and in both cases the guard passed.

The fix removes the class rather than adding a second check against it. Pin fresh immediately before each mutation, and never reuse a pin. A pin taken at the moment of mutation cannot be stale. That is strictly better than validating a reused pin against `HEAD`. A validation step can be forgotten, mis-scoped, or itself go stale. Verified by the lane: re-running the offending inversion now leaves the tree byte-identical with the documentation intact. Pinning now also covers `gh_stub.rs`, which the original manifest omitted.

The pristine-copy pattern has three requirements. This milestone has found two of them the hard way.

1. Restore from a recorded manifest, not from a directory listing. Otherwise you write back files a neighbour left behind.
2. Take the pin immediately before the mutation, never earlier and never reused. Otherwise you write back a version from before the tree moved.
3. The guard must compare against something whose currency is established. The first two together make that automatic.

### Agreement is not verification: a plausible mechanism confirmed by a second reader has been checked zero times
Twice on `fiddle-z9vy`, and the second instance is the clearer one.

The lane's first message reported that an approval for a moved head "refuses at step 2 via `RequestAbsent` → `Correctable` → exit 11". That corrected the bean's own claim of a refusal at step 3 on identity. The reasoning was good. The gated target is `{repo}#{pr}@{head}`, and the request id derives over that target, so a moved head yields an id no comment names. The lead confirmed it back and asked only that it be inverted.

Then it was measured. With `panic!` on entry to `resolve`, 7 of 22 tests failed. `an_approval_for_a_head_that_has_moved_is_unrecognisable_not_merely_rejected` passed. A moved head never enters `resolve` at all. Neither step 2's `RequestAbsent` nor step 6's `HeadMoved` is the refusal, and it is not a refusal. It is exit 10, having published a fresh question about the head that now exists. `PublishDecisionRequest::inspect` finds no comment carrying the new marker, answers `None`, and the capability takes the first walk and asks.

Three claims, two wrong, and the wrong ones were the ones that had been agreed. The lane's final report and bean text had it right. The lead's ruling restated the earlier version. The wrong mechanism was therefore in writing twice, by two different readers, before anything ran.

When a lane proposes a mechanism and the lead confirms it, the claim has been examined by two people and tested by nobody. It now reads as corroborated. Agreement between readers who share a model of the code is not independence. It is the same reasoning performed twice. The only thing that corroborates a mechanism claim is a run that would have failed had it been false. The cheap form here is `panic!` at the entry to the function the mechanism names. That one mutation refuted a claim two readers had agreed on, and it took one line.

**A second instance on the same bean, in the opposite direction.** The lane wrote that `Ignored::as_str`'s only caller was a unit test. The lead "corrected" it by pointing at two calls in `validate.rs`. Those are `serde_json::Value::as_str()` on `response.body["state"]` and `response.body["head"]["sha"]` — a different method on a different type. The lead's correction was itself the token-versus-structure error the lead had documented in that same dispatch. The lane refuted it by grepping for the receiver rather than the method. `reason.as_str` gives one hit in that file, and it sits after the file's `#[cfg(test)]` boundary.

Both instances have the same remedy, and it is not "be more careful". A claim about mechanism should be stated with the mutation that would refute it, and the mutation should be run. Mechanism means which code path runs, which guard fires, who calls what. A mechanism nobody tried to break is a hypothesis with a citation.

### 2026-08-13 — The grant resolution is written into the permission table, and ADR 018 never needed it
Acts on *The grant discrepancy is resolved: a public repository, not an undocumented permission*, above. Closes the "Owed, and small" paragraph that entry ends with. That paragraph's text stays as written.

**Done.** `docs/technical/effects-repository.md` now carries *Resolved 2026-08-13: a public repository, not an undocumented grant*. It is appended after the subsection it supersedes. The superseded instruction is marked in place so a reader cannot act on it. That instruction read "treat the effective grant as wider than this table in an unknown direction". The permission table's Issues row shows its old status struck rather than swapped. The sharper rule is stated there. On a public repository a successful write proves nothing about a grant either. A surface open to any authenticated identity answers identically to a credential that holds the permission and to one that does not.

**The claim being corrected.** That paragraph says "the permission table's Issues row, its subsection, and ADR 018 still describe this as unexplained". The entry's opening also names ADR 018 as one of the four documents describing the grant. Neither is true. The entry listing the four enumerating documents inside `effects-repository.md` does not include it. It names `.env.example`, `docs/evaluator-calibration-general.md`, `.github/workflows/github-effects.yml`, and the table itself. Measured over all 180 lines of `docs/technical/decisions/018-a-graphql-200-is-not-a-success.md`, case-insensitively: `unexplained|unresolved|wider|createissue|#25|issues` returns one hit, the verb in "The probe issues one cause". `contents|pull requests|metadata|secrets|actions: |permission|grant|403|token` returns one hit, `personal access token` inside a quoted response body. What ADR 018 says about the episode is "a mutation this credential is not permitted to issue". The resolution confirms that, because `closeIssue` is the operation the absent grant refuses. ADR 018 needs no append. The document that said it did was wrong in both directions.

Nothing here widened or exercised a credential. The lane that resolved the discrepancy re-verified the gated-endpoint table. This entry copies it and measured nothing at GitHub.

### 2026-08-13 — Jira and Slack belong inside the CVE capability, and the milestone table gained a row for it
Raised by the user during M4 planning. CVE remediation should own its Jira filing and Slack notification, rather than leaving them to host-workflow steps. Routed through the effect executor they gain stable effect identity and postcondition reads. An interrupted run then cannot double-file a ticket or repost a message. The current `curl` steps cannot promise that.

`docs/fiddle-agentic-factory-prd.md`'s M5 row gains CVE verdict reporting as a policy-checked Jira effect. A new M9 row adds a narrow outbound notification port, with Slack as its first implementation. M9 is last because it is the only milestone whose absence changes nothing observable. Its gate is therefore an equality proof. The same scenario, with the channel configured and unconfigured, must produce the identical typed outcome, exit code, and evidence bundle.

The RFC states two properties explicitly at the CVE agent section, because both are why the split existed. The mitigation decision stays trackerless permanently. No ticket state or notification gates, informs, or deduplicates a mitigation, and requirement 22 keeps its "without requiring Jira". And the capability holds neither credential. It receives an executor already bound to its own capability identity. The reference pipeline keeps Jira credentials out of the model run deliberately, and moving the work must keep that.
Origin: user direction during M4 seed planning (epic fiddle-eph7, seed fiddle-q7ct)
Tags: #feature #idea
Status: 2026-08-13 — recorded in the RFC (M5 row, new M9 row, CVE agent section) and in the tracker. M5 `fiddle-gyyo` body carries the added scope. M9 epic `fiddle-w4co` and seed `fiddle-tb0q` are created under `fiddle-30ey`, blocked by M8 `fiddle-is3b`.

### 2026-08-13 — M4 split into capability and integration, and the effect identity that would have silently no-opped
**The split.** M4 became M4a, the CVE mitigation capability (`fiddle-eph7`, seed `fiddle-q7ct`), and M4b, CVE workflow integration (`fiddle-rwdm`, seed `fiddle-5cyx`). The PRD gains a row for each, and M5 is rewired to wait on M4b. Sizing was the trigger. The combined scope exceeded M3, which ran 39 beans and lost two lanes to an individual spend limit at roughly 40. The better argument is that the halves are proved differently. M4a's claim is about decisions and gates offline, against a scripted scanner and forge. M4b's is about deployment against a real forge, scanner, and CI. Merged, the gate would need a credential to say anything. That contradicts M0's constraint that the acceptance lane is never gated on a secret.

**The defect the challenge found.** The shared-PR model regenerates the pull request body on every run, as CVEs accumulate. `fiddle_core::effect::effect_id` derives from `(project, invocation_ref, kind, target)` and never from the payload. The shared PR's natural target is repo, head, and base, which does not change between runs. A nightly `scanner:<component>` reference is stable. The second run therefore computes the same effect identity. Step 3 finds the postcondition already satisfied, and the executor performs no mutation. The accumulated CVE table never appears, and nothing reports a failure, because the pull request opened on run one is real. Not a refusal. A silent no-op. The fix is to carry a digest of the intended body in the target. That is what M2's identity derivation is for, and what M3 already did when it made a moved head a different question.

An effect whose target is stable but whose payload is meant to change is invisible to postcondition inspection. Any future operation that updates rather than creates has this hazard. The identity is where it is fixed, not the postcondition.

**A third finding, which removed work rather than adding it.** The design was going to widen the workspace command's pinned four-name environment allowlist to admit `DOCKER_HOST`, with an ADR. That would have been M4's only incursion into the boundary M1 and M2 fixed. Measured instead: under `env_clear` plus `PATH`, `HOME`, and `LANG`, `docker version` reaches the daemon. The CLI defaults to the Unix socket, and setting `DOCKER_HOST` wrongly is what breaks it. Go needs nothing either. `GOMODCACHE` and `GOCACHE` default under the scratch `HOME` the workspace supplies outside the worktree. No ADR is owed, and `workspace::a_workspace_command_inherits_no_credential` keeps pinning four names.
Origin: fiddle:challenge --phase define during M4 seed planning (epic fiddle-eph7, seed fiddle-q7ct)
Tags: #debt #risk #infrastructure
Status: 2026-08-13 — split recorded in the RFC and the tracker. The effect-identity and allowlist findings are recorded in the M4a design spec, and must survive into bean bodies, since docs/specs/ is gitignored.

### 2026-08-14 — A plan's test snippets named real APIs with wrong shapes, and the lane that hit it was the third to find a DEFINE defect
The M4a plan's task bodies carry Rust test snippets written during planning and never compiled. Several name a real API with the wrong shape. `assess(&view)` against the real `assess(work, expected_marker)`. `Observation::NotApplicable` against the real `NotApplicable { reason: String }`. `ChangeSetState::none()`, which has zero hits repo-wide. And `WorkStateView { .. }` as a struct literal, where the constructor is `without_publication`. All four are confirmed against the tree.

The plan format already forbids "references to types, functions, or methods not defined in any task". This is the adjacent sin it does not name: referencing a type that does exist, with a signature that does not. A snippet against existing code is only evidence if it was compiled. A plan that cannot compile its snippets should say they are intent.

Three DEFINE defects in this epic were found by something other than the lead's own review, which is the pattern worth recording. A bean required helpers whose types no earlier bean builds. An assertion passed for two different causes, because `is_err()` is satisfied both by a refused field and by invalid JSON. And this one. Two were found by implementer lanes and one by the convergence machinery.
Origin: implementation (epic fiddle-eph7, Task 2 lane fiddle-uwk0)
Tags: #debt #infrastructure
Status: 2026-08-14 — recorded on epic fiddle-eph7 as an instruction to every remaining lane. Verify signatures against the tree, adapt, preserve intent, report the adaptation, and never add a shim to make a snippet compile as written.

### 2026-08-14 — assess's fallback narrowed its extent while its arm count stayed three, and one guard is single-witness
Two findings from the Task 2 lane about `crates/fiddle-core/src/assessment.rs`, both reported rather than left for a reader to discover.

`docs/technical/SYSTEM.md` states that exit 20's `assess → Blocked` route has exactly three arms. That count is unchanged at `43cb3d7`, verified site by site at both shas. The fallback's extent narrows. It previously caught `(NotApplicable work item, Available changes)` and no longer does. The clause "the fallback for a view M0's orchestration cannot act on" therefore stays true while no longer covering the trackerless case. The reason string is byte-unchanged, and no test asserts it.

The work-item half of the fail-closed guard is single-witness. Under the arm-merge inversion only `an_unavailable_work_item_blocks_too` failed. `unavailable_source_is_blocked` makes the changes half unavailable, which the narrowed guard still catches. That state is pre-existing, and the change makes it more load-bearing. An adjacent arm can now swallow that case, where before there was only the fallback.
Origin: implementation (epic fiddle-eph7, Task 2 lane fiddle-uwk0, reported as concerns with DONE_WITH_CONCERNS)
Tags: #debt #risk

### 2026-08-14 — Three descriptions still say an invocation reference is `<scheme>:<value>`, and one of them is a diagnostic that now misleads
ADR 019 admits a bare reference. `<scheme>:<value>` is therefore no longer the whole grammar. Three descriptions outside the M4a Task 1 lane's Files block were left stale deliberately:

- `crates/fiddle-cli/src/cli.rs`'s `inspect` and `run` positional help both read "as `<scheme>:<value>`". ADR 019 quotes the `run` one specifically, so the ADR and the help text disagree.
- `crates/fiddle-runtime/src/orchestration.rs` says "The canonical `<scheme>:<value>` text of the invocation."
- `InvocationRefError::Malformed`'s own diagnostic still says the form must be `<scheme>:<value>`. A caller who mistypes `cev` is therefore given guidance that omits the legal bare form. This one actively misleads.

A related latent bug in the same area was fixed by that lane, and it is the reason to take the rest seriously. `UnknownScheme`'s message hardcoded "beans, jira, scheduled, scanner" in its `#[error]` attribute. Adding a fifth scheme therefore left it naming four of five. A caller who correctly typed `cve` before the variant existed would have been told there is no such scheme. It is now derived from `ALL`, with a test over every scheme.
Origin: implementation (epic fiddle-eph7, Task 1 lane fiddle-typ7, reported as a concern with DONE_WITH_CONCERNS)
Tags: #debt
Status: 2026-08-19 — **all three resolved** (bean `fiddle-wr6v`). The entry was partly stale by the time it was acted on. The first bullet had already been fixed. `cli.rs`'s two positionals now read "as `<scheme>:<value>` — for example `beans:fiddle-m0-demo`. A scheme that finds its own work stands alone and takes no value: `cve` scans the configured image and inspects what it finds". The valued shape is therefore an example rather than a requirement. Nothing recorded that here, which is worth noting on its own: a backlog entry listing three sites is read as three live defects, and this one had one. `orchestration.rs`'s doc comment now spells both shapes. The third was called out as actively misleading. It was live for five days, and through the remediation round that swept this exact class. See the 2026-08-19 entry "A promise and a denial are one class, and a lane that hunts phrases catches one of them".

### 2026-08-14 — ADR 011's traversal table enumerated two schemes, and the one whose values come from outside was not among them
The M4a Task 1 lane's ninth mutation exempted standalone-scheme values from ADR 011's character class. That is the plausible over-generalisation of ADR 019, that a self-discovering scheme supplies its own input. That lane's new test caught it, and nothing else in the workspace did. `refuses_a_value_that_could_be_read_as_a_path` is the test that reads as the canonical list, and it enumerated only `beans` and `scanner`.

`cve` is the scheme whose valued form carries a scanner-supplied advisory id. That is an input fiddle does not control. It needed a row most, and had none. Rows for `cve:../../../pwned` and `cve:a/b` were added.

A test that reads as an exhaustive list over a closed set becomes a null the moment the set grows. ADR 019 admits a fifth scheme, and nothing made the traversal table notice.
Origin: implementation (epic fiddle-eph7, Task 1 lane fiddle-typ7, found by its own inversion)
Tags: #debt #risk #security

### 2026-08-18 — The base-image arm is reporting-only in M4a, and that leaves the OS half of dedup with no producer
**The decision.** M4a does not build a registry client, so it cannot select a base-image tag. Design §2.4 rule 4 is built as far as it goes without a network peer. An OS finding is attributed to `Target::DockerfileBaseImage` in `crates/fiddle-runtime/src/cve/attribute.rs`. Every one of them keys onto that single group. `select_target_version` already answers the floating-tag `needs-work` case when handed a tag list, held by `a_floating_tag_with_no_newer_pinned_tag_is_needs_work`. Missing are the tag list and the `Dockerfile` edit. That means an authenticated read of the image's published tags, a comparability rule for them, and a port, adapter, credential, and policy decision to carry it. `CveMitigate::target_version` in `crates/fiddle-runtime/src/capability/mitigate.rs` therefore refuses every base-image group. It refuses with `GroupError::Unselectable { why: "selecting a base-image tag needs a registry this build does not read" }`. An OS finding is reported, never attempted. This is not M4b's either. M4b is the release artifact, the host workflow, the CI-feedback fresh attempt, and the first real Wiz measurement. The work is unowned.

**The consequence.** That refusal removes the only producer the OS half of deduplication could have had. A refused group is recorded blocked and skipped before either commit producer runs. It runs before the fold's `--allow-empty` commit, whose message names the group's ids. It runs before `land`, which commits only `GroupStatus::Clean`. No M4a run can therefore write an OS advisory into a commit body. `already_fixed`'s `PackageType::Os` arm in `crates/fiddle-runtime/src/cve/dedup.rs` reads commit bodies and nothing else. It answers `true` only for history somebody else wrote, and every OS case in the suite seeds one. Design §2.7's stated reason for listing every CVE id in a commit body is that same OS path. It is likewise dormant.

**What this does not say.** `commit_log_dedup` and its shallow-history guard are not dead with it. Their set also feeds `Run::in_progress`'s `covers`, which filters every finding through the same scan and reaches the `AlreadyInProgress` disposition. Library groups do commit `Fixes:` bodies, whose log is read back on the next run. It is the OS half of the answer that has no producer. `covers` earns the commit body's completeness in the meantime.

The refusal is held from outside the process by `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch`, in `crates/fiddle-acceptance/tests/cve_mitigation.rs`. It asserts the OS verdict's rationale carries `registry this build does not read`. Wiring a registry turns that lane red. That is the intended way back to the three notes at the sites: `target_version`'s doc comment, `cve/dedup.rs`'s module header and OS arm, and `commit_body`'s doc comment in `crates/fiddle-runtime/src/capability/cve.rs`.
Origin: implementation (epic fiddle-eph7, remediation bean fiddle-rh0p)
Tags: #debt #feature

### 2026-08-18 — Verifying that the scanned image was built from the remediated tree needs a config field no milestone owns
`docs/technical/decisions/020-the-host-builds-the-image-fiddle-scans.md` records and accepts the decision. The host workflow builds the image fiddle scans, and fiddle does not build it. That is right for the offline gate. A real `docker build` pulls base layers, and a stubbed one yields a digest meaning nothing. This entry is for the half the decision leaves owed.

**What was built.** `TreeObservation` in `crates/fiddle-core/src/observation.rs` carries a fourth key, `scanned_image_digest`. `CveMitigate::sweep` assembles it, in `crates/fiddle-runtime/src/capability/mitigate.rs`. That is the one place where the scan's resolved digest and the checkout's revision are both in hand. The scan happens in `execute` before a worktree exists, and `Checkout` never sees a scanner. Until this, the `wizcli` adapter parsed `ScanReport::image_digest` and nothing read it. Design §2.2's "the digest is what makes a later re-scan comparable" was therefore true of a struct field that died with the process. A bundle now says these verdicts are about digest X, and I remediated revision Y.

**What is owed.** Making that a checked precondition rather than an auditable pair. The builder declares the revision it built the image at. Fiddle refuses a run where the declaration disagrees with `checkout.revision()`. Two halves have to land together. The host half is M4b's: a workflow step building at the checked-out revision and passing it in. M4b's scope is the release artefact, the workflow in `snowplow-incubator/snowplow-identities`, and the first real Wiz measurement. The fiddle half is in no milestone's scope, because M4b is the host side. That half is an `[orchestration.cve]` field carrying the declared revision, plus the comparison and its refusal. Landing the fiddle half alone adds a field nothing populates. It is then either off by default and asserts nothing, or refuses every existing run. Landing the host half alone gives fiddle a value it does not read.

**What this does not say.** It does not say the pair is worthless without the check. The pair makes the gap auditable at all. It is the value the stronger check would compare against, so the two are a sequence rather than alternatives. Do not assume provenance from it. Fiddle did not build the image. The doc comments on `TreeObservation::scanned_image_digest`, on `observed_tree`, and on `ScanReport::image_digest` say so at the site. Two lanes in `crates/fiddle-acceptance/tests/cve_mitigation.rs` hold the pair and its all-or-nothing publication from outside the process: `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch` and `an_unusable_scanner_exits_eleven_and_reaches_no_forge`.
Origin: implementation (epic fiddle-eph7, remediation bean fiddle-k38l)
Tags: #debt #feature

### 2026-08-18 — A false citation was caught, recorded, and then repeated into a dispatch by the lead who recorded it
The M4a design spec and a Task 18 dispatch both made the same claim. They said "`docs/technical/SYSTEM.md` records that nothing in `fiddle-runtime` edits a comment and that the absence is load-bearing." It does not. `grep -niE 'comment|RequestEdited' docs/technical/SYSTEM.md` returns exactly one hit, and it is about a purity grep that matches source comments.

The constraint is real. It lives on the variant itself, in `crates/fiddle-runtime/src/human/validate.rs`. Its doc comment reads "fiddle's own question has been edited, which fiddle has no path that does". M3's milestone handoff on epic `fiddle-eoqx` states it correctly. That is where the lead read it before mis-attributing it.

The repeat is the part worth recording. An earlier round of the same bean caught this. It recorded the correction on the epic body and in `cve_shared_pr.rs`. The lead then wrote a fresh dispatch for that bean without reading the correction, and reproduced the false citation verbatim. This is the propagation M3's handoff names as "the lead's own transcription errors propagated three times". It occurred again on the bean that had already caught it.

Write a dispatch for a bean with prior iterations from the bean body. Not from the plan, and not from memory. The corrections live on the bean. A dispatch composed without reading them reintroduces exactly what the previous iteration paid to find.
Origin: implementation (epic fiddle-eph7, Task 18 lane, second occurrence)
Tags: #debt #risk
Status: 2026-08-18 — corrected in the design spec with the real location, and a note that it had already been caught once.

### 2026-08-18 — A worktree teardown that does not check for a live lane destroys measurements, and reads as a test failure
The lead removed four lane worktrees and their branches while agents were still working in them. It had concluded the beans were already converged. The conclusion was right and the sequencing was not. No lane was asked to stand down first.

Three lanes lost in-flight measurements: a `cargo test --workspace` at 894 passed, another at 18 binaries reported, and an inversion probe mid-restore. Nothing was lost permanently, and only for one reason. All three lanes had correctly declined to implement anything, and so had nothing uncommitted. A lane with uncommitted work would have lost it silently. That is the failure `docs/technical/evidence-discipline.md` §3 records. The M4a seed evidence records it for M3, at 116 uncommitted deletions and 65,802 lines.

The diagnostic signature misleads. From inside a suite, a vanished tree presents as:

    error: test failed, to rerun pass `-p fiddle-runtime --test scanner`
    Caused by: could not execute process `.../deps/scanner-…` (never executed)
    Caused by: No such file or directory (os error 2)

`(never executed)` plus `No such file or directory` on the binary's own path is a tree disappearing under the runner. It is not a failing test. A lane that reported the non-zero exit as a failure would be reporting a defect that does not exist. One lane also observed `error: couldn't read crates/fiddle-runtime/src/lib.rs`, with `FAILED_BINARIES=0` across the binaries that had already reported. That is the same event from the compiler's side.

A teardown checks for a clean tree and for live build processes, and asks the lane to stand down first. Deleting the branch as well as the checkout is what made it read as deliberate rather than as corruption.
Origin: implementation (epic fiddle-eph7, lanes for Tasks 4, 11, 18, 19)
Tags: #debt #risk #infrastructure

### 2026-08-18 — Correcting the entry above on the vanished-tree signature: the detection was a disagreement, not a signature
The entry *A worktree teardown that does not check for a live lane destroys measurements, and reads as a test failure* describes the `(never executed)` and `No such file or directory` output as what a vanished tree looks like. That is true, and it names the wrong thing as the detection mechanism.

What caught it was that the tally and the exit code disagreed. Cargo reported `error: 35 targets failed` with `BASELINE_EXIT=101`. `FAILED_BINARIES=0` across the 18 binaries that had already reported. A lane reporting only the status would have published a 35-target failure at a sha where nothing was broken. A lane reporting only the count would have published a clean run of a suite that never finished. Neither number is trustworthy alone. Their disagreement is the signal.

That is `docs/technical/evidence-discipline.md`'s own argument for printing the count beside the status. It is the same rule that caught the `shellcheck` format mismatch. The recognisable failure is a discrepancy the discipline already tells you to look for, not a novel string to grep.

### 2026-08-18 — Pointing at evidence-discipline.md is not sufficient on its own, and two consecutive dispatches are the evidence
M3's handoff established that a dispatch should point at `docs/technical/evidence-discipline.md` rather than restate it. M3 had copy-pasted about 700 lines of method into every dispatch, and the lead's own transcription errors propagated three times. That rule was right about the failure it fixed. This entry records its limit.

Two consecutive M4a dispatches tripped on rules that document is the record of, in lanes that had been pointed at it. One launched its baseline as `cargo test --workspace --all-features 2>&1 | tail -60`, with no `EXIT=` marker and no `--no-fail-fast`. That is a pipe-truncated log of a 42-binary run, which §1 names in its first three paragraphs. The lane noticed only on reading the file afterwards. It discarded the run and re-ran instrumented. Another recorded `tail`'s exit status in place of clippy's, the same defect one level along, and also caught it by re-reading. The lead did the same thing twice in this milestone: counting tests from a `tail -60` of a log, and reporting a `FAIL` verdict computed from a scorecard file that was never written.

Pointing works for the rules a reader will look up. It fails for the rules that govern the command they are about to type. The measurement rules are needed before the log exists: print the exit code you mean, print the denominator, never pipe a status you intend to report. A pointer consulted afterwards only diagnoses. Two candidate fixes rather than a decision. Name those three inline in a dispatch's verify section, as concrete commands. Or give them a script with an exit-code contract, which is what `docs/technical/decisions/009-mechanical-gates-as-validators.md` argues for generally.
Origin: implementation (epic fiddle-eph7, Task 18 and Task 4 lanes, plus two lead-side instances)
Tags: #debt #infrastructure

### 2026-08-18 — The product manual's `fiddle.toml` example still cannot load, and `[orchestration.cve]` was only the table somebody looked at
`fiddle-c64d` wired `[orchestration.cve] severities`, which the manual documented and the schema refused. It added the `image` key that table always required. The lane it added reads that one table out of `docs/fiddle-agentic-factory-prd.md` and drives the binary over it. That table and the schema can no longer diverge unnoticed.

The rest of the example is the same defect, unmeasured until now. Extracting the whole `fiddle.toml` block and running the compiled binary over it exits 2. It exits at the first line of the first table after `[project]`:

```
 10 │ repository = "snowplow/icecube"
    ·      ╰── unknown field `repository`, expected one of `repo`, `base`, `token`, `cli`, `git`, …
```

`[github]` alone disagrees on `repository` against `repo`, and on `default_branch` against `base`. Behind it the example names tables and keys the shipped schema has no reader for: `[jira]`, `[execution]`, `[policy]`, `[artifacts]`, `[telemetry]`, `[orchestration] enabled`, `[orchestration.stabilize]`, `[orchestration.set_based]`, `[orchestration.toil]`, `[capabilities.*]`, and `[agent] default_runtime`. A deployment cannot copy the manual's example. That is true of far more than the one key a holistic pass happened to find.

Most of those tables belong to milestones that have not shipped. This is therefore not a claim that the schema is wrong. It is a claim about the document. An example presented as what a repository writes is refused at line 10, and nothing says which parts are aspirational. Two candidate fixes rather than a decision. Mark the unshipped tables as forward-looking. Or extend the new extraction lane from one table to the whole block, and let it fail until the two agree. The second is stronger, and it is a document decision rather than a test decision.
Origin: implementation (bean fiddle-c64d, epic fiddle-eph7 — measured with the compiled binary over the extracted block)
Tags: #debt #documentation

### 2026-08-18 — `check-thresholds.sh` returns PASS for a scorecard whose dimensions carry no threshold
The script compares `select(.value.score < .value.threshold)`. A scorecard's dimension objects may carry `score` and no `threshold`. jq then evaluates `5 < null` as false. Every dimension lands in `passing_dimensions`, and the script exits 0 with `"verdict": "FAIL"` unreachable. A holistic scorecard scoring 5, 6, 6, 6, 9 against thresholds of 7, 7, 8, 6, 9 was reported PASS on this path. `check-convergence.sh` then returned `PASS_PENDING`. That is one dispatch away from declaring a milestone's holistic review converged, over a scorecard that failed three of five dimensions.

The immediate cause was upstream and human. The envelope handed to the reviewer omitted the `threshold` field, so the reviewer produced a well-formed scorecard the gate could not grade. That is worth fixing separately. A gate that cannot tell "nothing failed" from "nothing was compared" is the defect that turned a spelling mistake into a false pass. The same shape would swallow a renamed field, or a merge that dropped the key.

Refuse rather than default. A dimension with no `threshold` should exit non-zero naming the missing field. So should a `--criteria` file whose entries carry no `pass`. Defaulting to the holistic thresholds would be worse. It would grade one scorecard by a different rule than the one its author was given.

`--criteria` has the same hazard from the other direction. It expects the scorecard's graded criteria array. An ungraded array of criteria descriptions yields zero failing criteria rather than an error. That ungraded array is the file used to brief the reviewer. It is the natural thing to reach for, and it carries the same `id` keys.
Origin: orchestration (epic fiddle-eph7, holistic iteration 4 — the verdict was re-derived by hand and the three failing dimensions recovered)
Tags: #bug #tooling #evaluation
Status: Resolved 2026-08-19 by `fiddle-fgam`. `check-thresholds.sh` refuses ungradeable input with exit 2 before comparing anything. It names each missing field with the dimension or criterion it belongs to: ``domain general dimension correctness: missing `threshold` ``, ``criterion c1: missing `pass` ``. The refusal also covers the same blind spot arrived at by type order rather than by a null. `"1" >= 7` is true and `"false" == false` is false, so a stringly-typed score or `pass` read as passing too. `scripts/test-check-thresholds.sh` holds those cases. It also replays the two verdicts of iteration 2 byte for byte, `fiddle-ek1e` on a criterion and `fiddle-o1ly` on a dimension. The `--criteria` half had a second cause this entry did not name. `skills/develop-holistic/SKILL.md` instructed the caller to pass `criteria-holistic.json`, which *is* the ungraded briefing file. It now extracts the graded array from the merged scorecard.

### 2026-08-18 — A probe taken from a stale binary, in the pack built to prevent exactly that
The holistic evidence pack for iteration 4 captioned probe 5 "help now names the bare form (lead fix)". It headed the pack "run at HEAD 8fce238". The transcript came from `target/release/fiddle` as it stood before the gate rebuilt it. It therefore showed the help text without the fix it was offered as evidence of. The reviewer caught it by running the binary and noticing the extra sentence.

The fix was real and its test passes. Only the evidence was wrong. A probe that agrees with what the author expects is not checked, and this pack exists to stop unchecked expectation reaching a verdict. Commit `5dd2c9c` recorded the same failure as "a predicted probe is not a probe". It recurred in the artefact written to prevent it, one iteration later.

What would have caught it. Take probes after the build the pack claims they came from. Stamp each probe with the binary's own mtime or `--version`, rather than with the HEAD the author believes is built.
Origin: orchestration (epic fiddle-eph7, holistic iteration 4 — reported by the reviewer as an antipattern against the pack rather than the tree)
Tags: #process #evidence-discipline

### 2026-08-19 — `dispatch-provider.sh` hands a provider whatever it is given, and a too-large prompt costs a whole review
A holistic dispatch to `codex` failed at `turn/start`. It reported `Input exceeds the maximum length of 1048576 characters. actual_chars: 2178394`. The cause was the caller. `--diff-file` was the whole 39k-line epic diff, and the assembled prompt was 2.1MB against a 1MB limit. The hook does `DIFF="$(cat "$2")"` and passes it straight through. The first thing that knows the prompt is too big is therefore the provider, after the dispatch is committed.

The shape of the failure is the cost. The wrapper's completion notification lagged. From the orchestrator's side this looked like a provider hanging for forty-five minutes. The first written account said codex "returned nothing after 45 minutes", which was true in effect and wrong in cause. A holistic iteration then reached a verdict on one reviewer instead of two. The second opinion was lost to an input error rather than to an unavailable provider. One is worth retrying and the other is not.

Two fixes, and the first is cheap. Have the hook measure the assembled prompt and refuse before dispatching, naming the byte count and which input dominates. A caller then learns at once, instead of from a provider error whose text does not mention `--diff-file`. And for whole-epic reviews, stop passing whole-epic diffs. Send the diffstat and let the provider read the tree, or scope the diff to the paths under review.
Origin: orchestration (epic fiddle-eph7, holistic iteration 4 — the second reviewer was lost and the iteration proceeded single-provider)
Tags: #bug #tooling #orchestration

### 2026-08-19 — A bean asked a lane to edit a file that does not exist in a lane worktree
`docs/specs/agentic-factory-m4-design.md` is gitignored. A `git worktree add` copies tracked content only. The design document, the thing every bean is derived from, is therefore absent from every lane worktree.

An evaluator dispatch died outright. `hooks/dispatch-provider.sh ... --design-doc-file docs/specs/agentic-factory-m4-design.md`, run from a lane, produced `cat: ... No such file or directory`. The provider was handed nothing. Passing the epic worktree's absolute path fixed it.

The quiet one is worse. Bean `fiddle-jq1g` carried two criteria requiring design-document edits. They asked it to state the reference-to-capability binding, and to resolve a split-table contradiction. The lane could not have satisfied them under any effort. Nothing in its environment said so, and it left them undone. The lead made those edits in the epic worktree at evaluation time. A criterion that cannot be met from the worktree it is dispatched into is a trap. The lane that hits it looks negligent.

Two independent fixes. `define-beans` should not write a criterion naming a gitignored path, which is one `git check-ignore` per referenced path. And decide whether the design document should be gitignored at all. It is the reference for two milestones, and every reviewer reads it. If it stays out, lane briefs must say so explicitly, as this milestone's later briefs began doing.
Origin: orchestration (epic fiddle-eph7, bean fiddle-jq1g — the lead completed the criteria the lane could not see)
Tags: #bug #orchestration #beans

### 2026-08-19 — The evaluator envelope has now failed four times, in three different shapes
Across this epic, external evaluator dispatches have returned three shapes. An object truncated one brace short, twice. An object with `criteria` nested under `.domains`. And an object with the domain key `general` at top level instead of under `domains`. Each was well-formed prose reasoning, wrapped in a shape the grading scripts could not read.

The two truncations were re-dispatched. Content was missing, and repairing them would have meant guessing at scores. The two mis-nestings were repaired mechanically, and the repair disclosed in the evaluation log. A single unambiguous move of a known key is lossless, and there is exactly one valid placement for a domain name. Keep that distinction. Missing content must be re-dispatched. Mis-shaped complete content may be normalized and said so.

Four failures in one epic is a tooling signal rather than four provider mistakes. `merge-scorecards.sh` should normalize a top-level domain key and a mis-nested `criteria` array, and say on stderr that it did. No caller then hand-fixes anything, and the normalization is recorded in one place instead of four evaluation logs. Spelling the envelope more loudly in each dispatch has been tried repeatedly and has not converged.
Origin: orchestration (epic fiddle-eph7 — beans c64d, uwk0, jq1g and holistic iteration 4)
Tags: #bug #tooling #evaluation
Status: 2026-08-19 — action redirected rather than resolved. See *Envelope normalisation does not belong in `merge-scorecards.sh`, and one shape never reaches it* below. The merge is the wrong host. One shape dies at `validate-scorecard.sh` before it, and the merge's stderr is already consumed as `disagreements-holistic.json`. The distinction this entry draws stands.

### 2026-08-19 — Envelope normalisation does not belong in `merge-scorecards.sh`, and one shape never reaches it
Acts on *The evaluator envelope has now failed four times, in three different shapes*. That entry proposed that `merge-scorecards.sh` normalise a top-level domain key and a mis-nested `criteria` array. Measured against the tree while fixing `check-thresholds.sh` (bean `fiddle-fgam`), that placement cannot cover both shapes. Its stderr is not free either.

The documented order is dispatch, then `validate-scorecard.sh` on the raw per-provider scorecard, then `merge-scorecards.sh`. `skills/develop-loop/dispatch-and-evidence.md` states it as "Gate each scorecard before the merge". The merge is on every path, because step 1g normalises even a single provider through it. Running the two shapes through it:

- **`criteria` nested under `.domains`.** `validate-scorecard.sh` exits 5 with `jq: error (at <unknown>): Cannot index array with string ("dimensions")`. It documents an exit-2 JSON error array instead. The cause is that `.domains | to_entries` hands the criteria array to `.value.dimensions`. The scorecard is rejected before the merge, so a normaliser inside the merge would never see this shape.
- **A top-level domain key.** `validate-scorecard.sh` exits 0 and accepts it. With no `.domains` it has zero dimensions to check. `merge-scorecards.sh` then exits 5 with nothing on stdout and nothing on stderr. The `2>/dev/null` on its merge `jq` swallows `null (null) has no keys`. A caller sees an empty file and no reason.

The merge's stderr is already a typed channel. `develop-holistic` runs `... | scripts/merge-scorecards.sh > scorecard-holistic.json 2> disagreements-holistic.json`. A "normalised X" line therefore lands inside a file parsed as a JSON array of disagreements.

The normalisation belongs between dispatch and validation, on the raw scorecard. That is a `normalize-scorecard.sh` whose stdout is the repaired card, and whose stderr is free to name what it moved. It carries two prerequisites. `validate-scorecard.sh` must report a mis-nested `criteria` instead of crashing on it. `merge-scorecards.sh` must stop hiding its jq errors. Three scripts and a suite of their own is why `fiddle-fgam` did not fold it in.

What `fiddle-fgam` changed is the consequence of not normalising. Neither shape can now be graded. Both stop at `check-thresholds.sh` with exit 2 naming the missing field, and a top-level domain key reports ``scorecard: missing `domains` ``. The cost of an un-normalised envelope is orchestrator toil, not a false pass. That makes this a bean to schedule rather than a rider on a critical gate fix.
Origin: implementation (bean fiddle-fgam, epic fiddle-eph7 — measured by running both recorded mis-shapes through validate-scorecard.sh and merge-scorecards.sh)
Tags: #bug #tooling #evaluation

### 2026-08-19 — The eval log annotates a failing dimension by the same comparison that could not see one
Related to *`check-thresholds.sh` returns PASS for a scorecard whose dimensions carry no threshold* above, in a script that gates nothing. `scripts/append-eval-log.sh` writes `if .value.score < .value.threshold then " (FAIL, threshold …)"`. That is the same comparison with the same blind spot. Run against the threshold-less scorecard from that finding, the entry it builds reads

    **general:**
    - correctness: 1/10
    - domain_spec_fidelity: 1/10
    …

with no FAIL annotation. `fiddle-fgam` left this alone deliberately. The log decides nothing, because convergence is decided by `check-thresholds.sh`, which now refuses such a scorecard. A refusal here would also break the one route required to log before routing. The SPEC_DEFECT path in `skills/develop-loop/scorecard-merge.md` logs a scorecard it has already declared defective. It should annotate the missing threshold rather than omit the verdict. The durable record cannot then read as a clean sheet.
Origin: implementation (bean fiddle-fgam, epic fiddle-eph7 — found by grepping for the same comparison elsewhere, measured by running the log's jq filter on the threshold-less scorecard)
Tags: #debt #tooling #evaluation
Status: 2026-08-19 — **resolved in the same bean, by annotation rather than refusal.** The evaluator priced the deferral first, at `code_quality` 8 → 7, for "leaving a misleading durable record". The reasoning for not refusing stood, and the fix keeps it. Nothing in `append-eval-log.sh` exits non-zero over a score, so the SPEC_DEFECT route still logs before it routes. That was verified end to end through `merge-scorecards.sh`. A dimension the comparison cannot make now reads `- correctness: 1/10 (UNGRADED, no threshold recorded)`. The same rule names a non-numeric score or threshold, a dimension that is not an object, and a missing `domains` or `dimensions` key. The last three used to abort the logger with a raw jq error and exit 5. No entry was written at all, which is a worse record than a bare score. So did an empty scorecard file, which is what a failed merge hands over. `parse-eval-log.sh` reports `last_verdict: UNGRADED` for such an entry, checked ahead of `FAIL`. Without that branch the entry carries no marker anywhere and falls through to `PASS`. Well-formed entries are byte-identical, and all 109 real eval logs in the store parse unchanged.

### 2026-08-19 — A doc comment that contradicts the binary is a review matter, and neither a doctest nor a grep changes that
The valued `cve` reference was advertised on four operator-facing surfaces and implemented on none. That is bean `fiddle-ye7n`, and ADR 019's M4a amendment. The lane written to stop that recurring reads `--help` and each diagnostic off the compiled binary. That is the right subject. Help written from an ADR describes what was decided, and only something driving the binary can say what was built. It is also structurally blind to source prose. A fifth surface was found behind it. The doc comment on `a_bare_slug_cannot_collide_with_a_valued_slug`, in `crates/fiddle-core/src/identity.rs`, stated as present fact that `cve:CVE-2026-1234` remediates one finding.

Two candidate guards were tried rather than assumed. Both fail for reasons that are properties of the tools.

**A doctest cannot reach a test-module comment.** rustdoc builds the crate without `cfg(test)`, so `#[cfg(test)] mod tests` is stripped before documentation is collected. A deliberately failing doctest in that module yielded `running 0 tests`, and `cargo test --doc -p fiddle-core` exited 0. The identical probe on the public `InvocationRef::slug` exited 101 and named the file and line. The control arm matters. Doctests do run in this crate under the gate's `cargo test --workspace`. This is rustdoc's blindness, and no harness setting moves it. A doctest checks the code in a comment and never its prose. A claim about behaviour the build lacks cannot be written as a passing assertion at all, which is why deleting it was the whole fix.

**A grep cannot separate a false claim from a true history note.** When the fifth surface was found, `remediates one finding` stood at five sites and four were correct. `orchestration.rs` and `inspect_ref.rs` quote the old sentence to record that it was wrong. Two ADR 019 lines state that nothing in this build does it. Only `identity.rs` asserted it. Writing this entry and the note on the lane added several more, every one saying the claim is false. What distinguishes them is framing, and a pattern does not read framing. A pattern narrow enough to exclude the four is pinned to today's exact wording. It therefore passes the next paraphrase and reds on the next legitimate history note. That is a lane providing false comfort, which is worse than no lane.

Contradiction between a source doc comment and the binary is caught by review here, and by nothing else. A reader meets that fact in the lane itself. `no_operator_facing_surface_promises_the_valued_form`, in `crates/fiddle-acceptance/tests/inspect_ref.rs`, carries the boundary in its doc comment with both experiments. The lane's name reads like whole-tree coverage and is not. If this recurs a third time, weigh a review step that reads every doc comment touching a milestone's changed behaviour, rather than a stricter grep. Price it as a process cost.

**Narrowed on review, 2026-08-19.** The two candidates are ruled out. "Review and nothing else" is broader than they establish. A `fiddle-ye7n` evaluator named a third mechanism the lane did not try: a file-scoped assertion on one known-false phrase. The grep objection was that the phrase stood at five sites, one false and four true history notes. That ambiguity is a property of searching the whole tree. It is not a property of asserting that one named file does not contain one named sentence. Such a test is narrow, it names its subject, and it would have caught this. It is not built, and the reason is scope rather than principle. The criterion was met, the dimension sat at threshold, and the milestone had one holistic dispatch left. Left as follow-up with the mechanism named, so the next reader inherits a bounded gap rather than a closed question.
Origin: bean `fiddle-ye7n` (epic fiddle-eph7, M4a — evaluation iteration 1 failed `no_operator_facing_surface_promises_the_valued_form` on the fifth surface)
Tags: #decision #testing #documentation

### 2026-08-19 — Two gates in one worktree produce a failure that belongs to neither
`scripts/gate.sh` was launched twice against `.worktrees/agentic-factory-m4`, while the first run was still in flight. Both share `target/`. Both invoke `cargo` and `nix develop`. The first reported `TOTALS: 175 passed, 1 failed, 0 ignored, 14 binaries` with `GATE: FAIL`. Every clean run of this epic reports 53 binaries. A count that low is a truncated run, not a failing tree, and the single failure belongs to the contention.

A FAIL from a raced gate is indistinguishable in the log from a real one. The orchestrator nearly read it as a regression in freshly landed work. The only thing that prevented it was the binary count being obviously wrong. Had the race truncated at 52 binaries instead of 14, nothing in the output would have caught it.

Two fixes, and the first is nearly free. `gate.sh` should refuse to start when another instance is running against the same worktree. Use a lock file keyed on the worktree path, removed on exit, reporting the holder's pid. And the TOTALS line should carry the expected binary count alongside the actual. A truncated run is then self-evidently truncated, rather than needing a reader who remembers that 53 is normal.

A related discipline for the caller, which is the actual mistake. A gate launched while another is running measures nothing. A gate launched after a rebase that aborted measures the wrong tree. Both happened here in one command. The same background invocation rebased two lanes, aborted the second on a conflict, and then gated. Sequence the landing and the gate as separate steps. Read the git result before trusting the gate that follows it.
Origin: orchestration (epic fiddle-eph7, final remediation round — the raced FAIL was discarded and a clean gate run in its place)
Tags: #bug #tooling #orchestration

### 2026-08-19 — A promise and a denial are one class, and a lane that hunts phrases catches one of them
`InvocationRefError::Malformed` read "invocation reference must be `<scheme>:<value>`, got `cvfoo`". Its help offered a colon and one valued example. That is the diagnostic a mistyped `cve` lands in. `cvfoo` has no separator, so it is malformed rather than an empty value. The operator one letter away from `fiddle run cve` was told a colon was mandatory, and shown nothing else to try. `fiddle run cve` is the invocation M4a exists to provide.

**Why it survived a round aimed at it.** The class is operator-facing text asserting a grammar the binary does not have. It has been met twice. Iteration 5 built `no_operator_facing_surface_promises_the_valued_form`. That lane reads `--help` and each diagnostic off the compiled binary, which is the right subject. It hunts for a promise of the valued form: occurrences of `cve:` followed by a value character. This string is the same class pointing the other way. It is a denial of the bare form, and saying that `cve` requires a value never spells `cve:`. Measured, not reasoned: restoring the old help in place leaves that lane green, and the whole file green but one. The 2026-08-14 entry above had already named this string, five days and one sweep earlier. Two searches at one class caught one of them. Both patterns were phrases, and the class is not a phrase.

**What replaces it.** The lane is now named `every_scheme_that_needs_no_value_is_named_on_each_surface_and_in_each_colonless_refusal`. It sits beside the older lane in `crates/fiddle-acceptance/tests/inspect_ref.rs`, and it hunts no phrase. It reads the scheme set off the `unknown_scheme` diagnostic, the one surface whose job is to name them all. That diagnostic is derived from `InvocationScheme::ALL`, so a sixth scheme joins the lane the day a caller may write it. It then asks the binary which of them stand alone, by driving each bare form and reading whether the grammar refuses it. Only then does it hold the rendered text to the answer. Every scheme the binary accepts alone must be named on every grammar surface, and must never carry a value. Two surfaces offer the set in halves, and each scheme must sit in the half its own behaviour puts it in. The oracle is behaviour rather than `stands_alone`. A lane reading the enum would ratify whatever the enum said. This one reds if the enum, the binary, and the prose disagree. Both directions were inverted in place. The old help alone reds it with "the `bogus` diagnostic from inspect says how a reference is written and never mentions `cve`". Swapping the two halves reds it with "must offer `cve` where no value is needed, because that is the invocation the binary accepts".

**The bounded gap, which is smaller than the last one and still real.** The surface list is enumerated by hand. A process cannot be asked to render every string it might print. Each diagnostic is reachable only through an input that provokes it, and a sixth defect added later is not discoverable from outside. The list cannot be replaced by a filter either. Requiring every surface to name the standing-alone schemes fails honestly-silent text: `fiddle --help` lists subcommands and says nothing about references. Triggering on a pattern such as `<scheme>:<value>` is the phrase hunt that let this defect through twice. Placement is therefore derived and cannot go stale. Membership of the list is a review matter, named on this lane's doc comment. The lane's name reads like whole-tree coverage and is not.

**The generalisable lesson, since two rounds have paid for it.** Build a guard against a false operator-facing claim over the property the claim is about, with the binary as oracle. Do not build it over the sentence that happened to be false. Both earlier attempts were searches for a known string, and a search knows only the direction it was pointed. The tell is a lane whose failure message quotes a phrase.
Origin: bean `fiddle-wr6v` (epic fiddle-eph7, M4a — proposed by holistic iteration 6, dispatched in the round after)
Tags: #decision #testing #bug
Status: 2026-08-19 — **the replacement lane did not, as first committed, guard the string this entry is about.** The paragraph above overstated it. The inversion recorded there reverted the *help*. The fix had changed two sites. Reverting the other one left the lane green, at `1 passed; 0 failed`, exit 0. That other site is the `Malformed` `#[error]` message; its signature was unchanged and it compiled clean. The revert was caught only by `inspect_rejects_a_malformed_invocation_ref` and `a_malformed_reference_is_reported_without_reference_to_configuration`. Both assert the message text, which is the coupling the new lane was built to replace. The cause was that the lane flattened each diagnostic into one string. `cve` appeared in the corrected advice, so "every surface names it" held, while the line above it went on calling the operator's reference illegal. The lane now holds each surface **part by part**: a diagnostic's verdict line and its advice are read separately. It forbids a part from giving a shape template unless it names the schemes that template is false of. A template is a colon-joined pair whose scheme side is not one of the schemes read off the binary. Both `--help` surfaces satisfy that as written, so it is not a ban on placeholders. Still uncovered: a universal denial written in **prose** rather than as a shape. "A value is required for this reference" would pass it. The template is a **gate**, and a rewording without the placeholder opens it. Two rules strong enough to catch that were weighed and both recorded as rejected. One of those rejections was wrong, and the third entry below reopened it.

The lesson is narrower and worth as much: **an inversion proves detection only at the site it was applied to.** This fix touched two sites, and one inversion was generalised to both. A fix with N changed sites needs N mutations. Otherwise the report has to say which site was inverted.

Origin: bean `fiddle-wr6v` continued (the guard was measured by the orchestrator, found green under the message revert, and dispatched back)
Tags: #decision #testing

Status: 2026-08-19 — **a guard that passes the mutation it was built against is weak evidence, and a different violation was constructed that it passed.** The evaluator's counterexample was the prose form of the same denial. `Malformed`'s `#[error]` was rewritten in place as "a value is required for this reference". No colon, no placeholder, nothing for a shape detector to see. The part-by-part lane exited **0**, measured. The class had been narrowed three times and named as closed each time.

**What closed it, and why it is a property rather than a longer phrase list.** The second clause added to the lane is gated on the **input**, not the wording. A refusal of an input with no colon in it must name the standing-alone schemes in every part an operator reads. No rewording opens that gate. Whatever the sentence says, it is said to a caller who typed no separator. That is the one caller for whom the bare form is a live repair. Both mutations now red by name: `` the `cva` diagnostic from inspect: the line that judges the reference answers an input with no colon in it and never names `cve` ``. A third mutation checked the other clause still has teeth where it is the only one. A `<scheme>:<value>` template was added to the *empty-value* verdict, whose input does carry a colon. It reds with `` gives ["<scheme>:<value>"] as the shape a reference takes and never names `cve` ``.

**The rejection that was wrong.** The paragraph above rejected "requiring every part to name the standing-alone schemes", because it reds on the corrected verdict. That is true of the **unscoped** rule. The scope is what was missing. Scoped to a colonless refusal, it holds `beans:`'s verdict to nothing, because its caller's repair is not a sweep. It still holds the one arm a mistyped `cve` lands in. The cost is one clause of product text. `Malformed`'s message now ends "nor one of the schemes that discover their own work (cve)", derived from `InvocationScheme::listed_standing_alone`. That is the parts split applied to the message itself, not a concession to the test. A verdict travels without its help. A verdict saying such schemes exist without saying which leaves the colonless caller with nothing to type.

**The surface inventory is no longer enumerated by hand.** The commands are read off `fiddle --help` and then **probed**. One is a grammar surface if it answers a malformed reference with `fiddle::invocation_ref::malformed`. A third subcommand taking a reference therefore joins the lane the day it is added. `config`, which takes none, is not held to a promise it never makes. The vacuity guard is that the probe must both select and reject. What remains written here is the *case analysis over inputs*. A one-letter typo of each standing-alone scheme, generated from the scheme set rather than spelled — `cva` today. A token that is no scheme. And an empty value after an unknown scheme. A sixth defect reachable only through a sixth input is still not discoverable from outside.

**The name was narrowed to what is held**, per the precedent from bean `fiddle-ye7n`. The old name read as whole coverage of the class, and three iterations proved it was not. Two gaps remain, and the doc comment states both. A part that **names** the standing-alone schemes and denies them anyway reds nowhere: "`cve` requires a value" is such a part. Catching it means deciding whether a sentence contradicts the binary. And a prose denial in the verdict of a refusal whose input **did** carry a colon is outside both clauses. It is covered only by `an_empty_value_is_told_every_repair_its_own_scheme_admits`, which holds advice rather than verdicts.

**The lesson, since a fourth round paid for it.** An inversion proves detection at the site it was applied to. A mutation proves it against the wording it was written in. A guard keyed on the *text* of a false claim can always be reworded around. A guard keyed on the *input that provokes it* cannot. Where a class has been narrowed three times, the question is not "what other phrasing". It is "what does the check read that the author of the next wording controls".

Origin: bean `fiddle-wr6v` continued (the evaluator constructed a violation the guard passed, and the lane was broadened and renamed)
Tags: #decision #testing

Status: 2026-08-19 — **the class is closed as far as an acceptance lane reaches, and the remaining dimension is accuracy, which review holds.** A fourth guard was considered and deliberately not built. The property the lane holds is **detectability**. Every scheme that stands alone is *named* on each surface and in each colonless part. Naming `cve` makes a wrong description findable. It does not make the description right. A verdict that names `cve` and misdescribes it passes, measured rather than argued. `Malformed`'s `#[error]` was replaced in place with "the normal form is a scheme and the item inside it, as in `beans:fiddle-m0-demo`, and that includes the schemes that discover their own work (cve)". That leaves the lane green and the whole file green, at `10 passed; 0 failed`, exit 0. That sentence names `cve`. It writes no template, because `beans:fiddle-m0-demo` is one real scheme's own example rather than a claim about the set. Its last clause is false of the one scheme it is about.

**The wording first reached for was not the one that gets through, and the difference is the point.** `cve:<id>` presented as the normal form does red. `valued_mentions` sees `cve:` in a valued position, and the lane reports "shows `cve` carrying a value". Paraphrasing the same false claim without putting a value after `cve:` opens it. A gap is only as narrow as the wordings that reach it. The example recorded is therefore the one that passes, checked, not the one that reads worst.

**Why not a fourth guard: the regress is structural.** A lane can assert that a string is present, absent, or shaped a certain way. It cannot assert that a sentence *means* what it should. Each of the three guards built here narrowed the class and left a semantically-wrong-but-detectable case behind. Those were a phrase hunt, then a shape template, then a naming rule. A fourth would do the same, because the thing being approximated is comprehension. This is the conclusion the entry above reached for a **source doc comment** contradicting the binary, in bean `fiddle-ye7n`. It is arrived at from the other side. The lane's doc comment points at that entry rather than restating it.

**What is stated where a reader meets it.** The lane's doc comment opens its limits section with the bound rather than a list of gaps. The property is detectability and not accuracy. The passing example above is recorded with its measurement. Review is named as what catches it, with a pointer to the `fiddle-ye7n` entry. The *input* inventory is named as hand-enumerated. It also says plainly what the lane *does* hold, because a limits section that only subtracts misleads in the other direction. The colonless case is gated on the input, and no rewording reaches it. Both earlier guards at this class were gated on text, and both were reworded around.

**The lesson, and it is a scoping one.** Three iterations on one dimension, each failing on a newly constructed violation, is a signature. It signals a dimension that is not reachable by the mechanism being aimed at it. It does not signal a guard that needs one more clause. The tell is that each new counterexample is *constructed* rather than found in the tree. The author of the next wording controls the text, so any check that reads the text can be satisfied without the claim becoming true. When that pattern appears, produce the bound, stated where a reader meets it, plus the review step that holds the residual. A stated bounded gap is a different artefact from a wrongly closed question. Only one of them decays quietly.

Origin: bean `fiddle-wr6v` continued (the evaluator constructed a third violation; the lead ruled the dimension unreachable as scoped and dispatched the bound rather than a fourth guard)
Tags: #decision #testing #documentation

### 2026-08-19 — Three design sections specified a rule the build does not execute, and none of them said so
Delivery's drift analysis found §2.6 naming five concrete checks in order: `go build`, `go fmt` with exit 0 and no output, `go vet`, `docker build`, and the `wizcli` rescan. `[[workspace.checks]]` is a `Vec<CheckRef>`. It constrains neither count, order, nor which programs appear. Nothing in the tree said the concrete contract had become a deployment's choice. It is annotated now.

That is the third instance of one shape in a single milestone:

- **§7's release-workflow bullet** was assigned to M4a by the split table, and to M4b by the same table's other row. Found by holistic iteration 4.
- **§2.4 rule 4** specified a Dockerfile base-image tag rule needing a registry seam. §7 never designs that seam, §9 never lists it, and §10 never excluded it. Found by holistic iteration 6 and raised as the epic's `spec_defect`.
- **§2.6** specified five concrete checks the schema does not enforce. Found by delivery drift analysis.

Each was a section stating a rule in the imperative, while the mechanism beneath it was general. In each case the gap was visible only by cross-referencing a different section: the split table, §7's seam list, or the schema. None was a coding defect. The implementations were right every time, and the documents were what changed.

A section that specifies a decision rule should say, in the same breath, which seam executes it. When no seam does, that is the sentence that must be present. The three annotations now in the tree are the pattern to copy. Better is not needing them, which means the design phase asking of every rule "what runs this?" before the plan is written. Worth weighing for M4b, whose design is the same document.

A cheaper mechanical partial. The acceptance lane that reads the capability census out of the binary's own diagnostic shows the shape of a guard that holds prose to a mechanism. Nothing equivalent exists for "this section names a rule and no seam executes it". It is not obvious one can. Deciding whether a paragraph implies an unbuilt seam is reading meaning, which this epic has twice concluded is a review matter.
Origin: delivery (epic fiddle-eph7 — drift analysis, verified by the lead against §2.6, the schema, and a tree-wide search for a statement of the divergence)
Tags: #process #documentation #design

### 2026-08-19 — SYSTEM.md is eight times its stated size constraint, and delivery made it worse
`skills/deliver-docs/SKILL.md` states SYSTEM.md's constraint as 1 to 2 pages maximum. It is 7,981 words, about 16 pages. M4a's delivery added roughly 700 of them: a Components entry for the CVE capability, ADR 022's scheme-selection invariant, and two Known issues. Those additions were the right content, and the gaps they filled were real. The constraint was checked after writing, and found already violated by a factor of eight.

**Why it grew.** Every milestone M0 through M4a has added to it, and almost every entry is load-bearing. The invariants record decisions whose absence caused defects. The Known issues record gaps a reader needs. The entries explain why rather than what. Pruning by summarising would delete exactly the reasoning that stops the same defect twice. This milestone's `wr6v` and `ye7n` both turned on a written-down reason surviving.

**Two additional constraints on any fix, both discovered rather than assumed.** Two acceptance lanes read this file. `capability_selection.rs` requires its capability census to name every id the binary advertises, and to state their number. `config_check.rs` requires its `fiddle.toml` paragraph to name every table the schema admits. Both were verified passing after delivery's edits. Parts of SYSTEM.md are therefore asserted. A split or a prune must keep those assertions pointing at the right file.

**The choice is between two honest positions and one dishonest one.** Either the document is too long and should be split. Components, Invariants, and Known issues are each plausibly their own file, with the guards re-pointed. Or the constraint is wrong for a system five milestones in, and should be raised to what the document needs. What should not continue is a stated limit of one to two pages that every milestone silently exceeds. A constraint nobody enforces makes the next author think the size was considered when it was not.

Recommend deciding at M4b's delivery rather than mid-milestone. A split can then be planned against the guards instead of racing them.
Origin: delivery (epic fiddle-eph7 — measured at 7,981 words after the update, against the skill's stated 1-2 page constraint)
Tags: #debt #documentation #process

### 2026-08-19 — A bean was landed and pushed red because the lead chose which suites to run
`fiddle-pv1o` (M4c Task 2) was merged onto `plan/agentic-factory-m4` and pushed to PR #14. `cve_mitigation` stood at 22 passed of 36. Fourteen acceptance lanes failed, all with `the report does not account for what it was shown`. That is the new `unaccounted()` protocol check doing its job, against acceptance fixtures written before the `findings` field existed.

**The mechanism was a choice, not an accident.** Before landing, the lead ran `cve_protocol`, `cve_dispositions`, and `binary_repair`. Those are the three suites it reasoned the diff touched. It did not run `cve_mitigation`, the black-box suite that drives the capability end to end. It did not run `scripts/gate.sh` at all. Reasoning about blast radius is the job a gate exists to replace. The reasoning was good, because the change was to a report struct and one call site. That is what makes it worth recording. A plausible blast-radius argument is more dangerous than a careless one, because it feels like diligence.

The next lane caught it by measuring its own base before starting work. That measurement also caught something else. The denominator the lead had handed it was taken before the previous bean landed. Neither the bean's own evaluation nor the lead found the red suite. Two evaluator iterations scored 9/9/8 against an evidence pack that did not contain it. An evaluator cannot ask for a suite it was not shown. An incomplete pack therefore yields a confident score about the wrong thing.

Three things follow, and the first is the only free one:

1. **Land nothing without a full gate.** Not the suites that look relevant. The gate, 53 binaries, exit code read before any pipe. This rule already existed in every lane brief in this milestone. The lead did not apply it to itself.
2. **A lane's first act should be measuring its own base.** Its report should carry inherited-red separately from self-caused-red. That is what made this findable within minutes. It belongs in the brief template, rather than depending on a lane thinking of it.
3. **An evidence pack should name the suites it does not contain.** A pack listing three green suites reads as complete. One that says "cve_mitigation not run" would have drawn the question from the evaluator.
Origin: orchestration (epic fiddle-v4ka, M4c — found by `fiddle-5swi` measuring its base; verified by the lead at 22/36 on the pushed head)
Tags: #process #evaluation #orchestration

### 2026-08-20 — The scorecard envelope is written down once, and two things can still drift from it
Closes the `--criteria` half of *`check-thresholds.sh` returns PASS for a scorecard whose dimensions carry no threshold* above. It also closes the finding behind bean `fiddle-njrc`. Every evaluator brief written in M4c asked for `criterion` and `met`. The checker required `id` and `pass`. `check-thresholds.sh` therefore exited 2, and the lead hand-translated fields before grading. Hand-translating evidence until the grader accepts it is the position a grader exists to prevent.

`skills/develop/scorecard-envelope.md` states the envelope once, and both checkers name it on exit 2. `validate-scorecard.sh` checks what `check-thresholds.sh` will grade on: numeric `score` and `threshold`, string `id`, boolean `pass`. The pre-flight and the grader therefore want the same card. `criterion`, `met`, `min`, and their siblings are named as wrong spellings rather than normalised. Normalising them silently is hand-translation one layer down. Its former jq crash on a `criteria` array mis-nested under `.domains` is now a reported problem. That closes one of the two prerequisites named in *Envelope normalisation does not belong in `merge-scorecards.sh`* above.

Two gaps stay open, both deliberate and neither a false pass:

1. **`merge-scorecards.sh` sits between the two enforcement points and checks neither.** A card that skips `validate-scorecard.sh` still reaches `check-thresholds.sh`, which refuses it. The cost is therefore a late refusal rather than a bad verdict. A third checker was out of the bean's scope, and would touch the merge's byte-for-byte recorded behaviour.
2. **The assembled evaluator brief restates the schema instead of citing it.** `assemble-evaluator-context.sh` inlines `skills/evaluate/SKILL.md`, whose field list is a second copy of the envelope. It is kept because a dispatched external provider cannot be relied on to read a repo file. Two copies is one fewer than the three that produced this finding, and it is still two. The honest fix is for the assembler to append the envelope document, and for `evaluate/SKILL.md` to drop its copy.
Origin: implementation (bean `fiddle-njrc`, lane `lane/tooling` — measured by running the recorded `criterion`/`met` card through both scripts)
Tags: #evaluation #tooling

### 2026-08-20 — The gate's denominator is derived per run, and one truncation shape is still invisible
Closes bean `fiddle-dn0j`. `scripts/gate.sh` ran `cargo test` with no `--no-fail-fast`. Its `TOTALS` line therefore printed where cargo gave up, in the shape of a complete run. That was a measured 6 binaries on a red head, against 53 for a complete run. The file's reconciliation block passed throughout, because a truncated log is internally consistent.

The log analysis is now `scripts/gate-report.sh`. It is testable against fixture logs, rather than only against whatever the tree happens to be. `scripts/test-gate-report.sh` holds the truncated, orphaned, un-enumerated, and crashed-lane shapes. `TOTALS` reads `N of M binaries`. `M` is derived per run from `cargo test --no-run --message-format=json`, plus the `doctest` targets in `cargo metadata`. Proved on a deliberately red tree. The old command line reported 2 of 53 result lines, and the new one 53 of 53 with the failure named. Three failures were placed early, middle, and late in the run order, and each was attributed to its own lane. `--no-fail-fast` therefore does not disturb the awk's positional attribution.

What remains invisible is a lane whose test count shrinks. Coverage is checked at the granularity of binaries. A suite that silently stops registering half its tests therefore still reports as one reached lane. `1005 passed` is the only signal, and nothing compares it to a previous run. That is the same shape as this finding one level down. It wants a recorded per-lane baseline rather than a derived denominator.
Origin: implementation (bean `fiddle-dn0j`, lane `lane/tooling` — measured on a red tree at both command lines)
Tags: #tooling #evidence

### 2026-08-20 — Four defects in one milestone that returned a plausible number instead of an error
M4c surfaced one failure shape four times in different tools. Each instance was found by accident rather than by looking:

1. **`gate.sh` without `--no-fail-fast`** stopped at the first failing binary. It printed the partial count in the shape of a complete run: 6 binaries where complete is 53. Four milestones' acceptance lanes had not run, and the line read as a denominator. (`fiddle-dn0j`, fixed to `N of M`.)
2. **`test-eval-log.sh` resolving `.beans` by walking up from cwd** aborts under `set -euo pipefail` with no output. That happens when it runs from a worktree outside the repository. `GATE: FAIL` on a green tree, nothing printed. (`fiddle-92v5`.)
3. **`jq`'s `index(.id)` inside `select`** resolves `.` against the array being searched rather than the item. It silently returns null and selects everything. Found while writing a comparison that would otherwise have shipped passing every case. (Fixed in `fiddle-cehd` with an explicit `. as $binding`.)
4. **`check-convergence.sh` comparing evaluator scores across an unchanged tree** returned `PASS_REGRESSED` three times on byte-identical code. The cheapest response is re-dispatching until two evaluators agree. That is score-shopping, indistinguishable in the log from convergence. (`fiddle-cehd`, fixed so same-tree pairs compare findings and a contradiction is terminal `CONTESTED`.)

Each returned something that looked like an answer: a count, a FAIL, a filtered list, a verdict. None returned an error, so none prompted a second look. In three of the four a human acted on the wrong number before noticing. The lane that found the second put the general case best. A gate that degrades quietly under contention is worse than one that refuses, because the quiet version gets believed.

What follows for tooling here, beyond the four fixes. A tool that cannot answer must say so rather than answer partially. Prefer exit 2 with a reason over a number computed from incomplete input. `check-thresholds.sh` does that, and it is why one class of false PASS was caught. Print the denominator, always: `957 passed` is not evidence, and `957 passed, 52 of 52 binaries` is. Assert preconditions up front, because the second defect would have been a one-line check on `.beans` reachability. And a comparison over a collection deserves a test that would fail if it matched everything. The third passed every case it was given while being wrong.
Origin: M4c (epic fiddle-v4ka) — one per tool, none found by looking for it
Tags: #tooling #evidence #process

### 2026-08-20 — A capability shipped Go-only and four holistic reviews passed it
M4a delivered `cve_mitigate` with 77 Go references in `fiddle-runtime/src`, 3 in `fiddle-core/src`, and 1 in `fiddle-cli/src`. They spread across ten files, including a whole `cve/go.rs`. There was no ecosystem seam anywhere. `PackageType` was Wiz's `library` and `osPackage` taxonomy, a scanner's vocabulary and not a language's. The capability could not mitigate a Python repository at all. Not less well: at all. The user found it, after delivery. No check, no lane, and no reviewer did.

Holistic review ran four times over that milestone and passed it every time. The reason is structural rather than a lapse. Every criterion asked whether the capability worked, and none asked what it assumed. Each iteration measured the thing against a Go fixture. Against a Go fixture a Go-only core is indistinguishable from an agnostic one. That is also why the M4c fix is a Python fixture pair on the same lane, rather than a grep for the word `go`.

The question that would have caught it is one sentence long. It belongs in holistic review's criteria, not in a lane: "what would this refuse to run against?" It is cheap, and it is answerable from the code. Its answer for M4a was every repository that is not a Go module. No artefact of that milestone stated that, because nothing asked. A capability whose answer is a named class of input is scoped. One whose answer is nothing is either genuinely general, or has not been looked at.
Origin: M4c (bean `fiddle-imoj`, epic `fiddle-v4ka`) — counted at M4a's close, found by the user
Tags: #process #evaluation #capability

### 2026-08-20 — A release note is owed for a config key that was deleted, and three documents outlived their code

**The release note.** Removing `[orchestration.cve] go` is a **breaking configuration change** and not a silent one. `OrchestrationCve` carries `deny_unknown_fields`, so a `fiddle.toml` still holding the key is **refused at load with exit 2** rather than ignored: a document that loaded yesterday fails today. Exposure is small — the key was never in the product manual (`:441` and `:526` both omitted it), so it only ever arrived via `default_go()` — but small exposure is not no exposure, and `docs/product/releases/` does not exist yet, so nothing carries this. `RUNBOOKS.md`'s common issues now names the exit-2 and the fix; the release note still owes the reader who has not hit it yet.

**Three documents outlived their code, and one of them misinstructed.** `SYSTEM.md:57` documented `go = { program, args }` as the seam the module graph is resolved through — which, given the strict schema above, pointed an operator straight at an exit-2 refusal. That is worse than stale: a stale sentence is ignored, an instruction into a refusal is followed. `SYSTEM.md:47` narrated `cve::attribute`'s four bump-target rules and `docs/fiddle-agentic-factory-prd.md:442` told operators that "major-version approval rules live in Rust", both describing code deleted in `58e7616`. All three are corrected.

Two more were found while correcting them, and neither was on the list:

- **`docs/fiddle-agentic-factory-prd.md:545`** claimed "immediate checks ... in Rust rather than dynamic configuration". M4 moved them into the document as `[[workspace.checks]]`, so the bullet had been false since M4a rather than since M4c. Corrected.
- **ADR 021's "four readers, one value"** counts readers of `Severities` and names `cve::fold` as one of them. `cve::fold` is deleted, so there are three. The reasoning is not rewritten, but the sentence now carries an in-document marker pointing at ADR 028, so the arithmetic is not read as current by a reader who never reaches this file.

The shape worth naming: **every one of these was found by a person reading the sentence next to a deleted symbol, not by any check.** A document that cites a symbol has a mechanically checkable claim in it — the symbol exists — and nothing in this repository checks it. Two lanes do pin `SYSTEM.md` prose (`config_check` pins the `fiddle.toml` paragraph's table list, `capability_selection` pins the capability census), and both are pins on *enumerations that the binary also prints*, which is why they work. A citation pin would be the same device applied to identifiers.

Origin: implementation (bean `fiddle-imoj`, lane `lane/m4c-imoj`)
Tags: #documentation #process #release

### 2026-08-20 — A citation by line number is invalidated by its own commit, and the remedy for that was itself unread

**The class, not the three instances.** Every one of ADR 028's three references into ADR 021 landed two lines above the sentence it meant — *"The second arm stays in Rust"*, *"An empty list is refused"*, and the `Severities` reader count. Two lines is exactly what commit `a9de6e0` added to 021's Status — **the amendment invalidated its own citations in the commit that wrote them**, and a reader following any of the three lands on the paragraph above the claim. Adding the four markers below shifted those lines again, which is the argument in one observation. `ADR 025:54`'s `capability/cve.rs:1001-1006` named six lines of an eleven-item list and was wrong when written; `a9de6e0` audited 025's citations, fixed two others, and left that one. All line-number citations are now gone from `decisions/` — six converted to a quoted sentence or a named symbol, three into ADR 021 and three into `.rs` files — and `grep -rn '\.rs:[0-9]' docs/technical/decisions/` returns nothing. The rule that replaces them is in the templates: cite a symbol, quote a sentence, and where a line number is unavoidable point it at a file the same change does not touch.

**A false sentence needs a marker where a reader meets it.** ADR 021 had four sentences that no longer hold — the exploit arm's rationale, the `severities = []` reason, `selected`'s argument count, and the `Severities` reader count — behind nothing but a Status-line pointer to another file. That is the arrangement ADR 025's own amendment argues against: a record must not go on reading as though the hole were still there. Each of the four now carries a one-line `>` marker naming 028. 028's count was wrong too: it said two sentences where three of its four are its own to correct.

**The staleness remedy is adopted rather than deleted, and it moved.** `writing-templates.md` called `Cites:` "the load-bearing line" while no ADR carried one, no skill read the file, and no gate checked it — a template that reads as a control and is not one. Deleting it was the alternative, and it was rejected because this entry's own predecessor asked for exactly this device ("a citation pin would be the same device applied to identifiers") and deleting it would have thrown away the one remedy already named. So: the file is `skills/using-fiddle/references/writing-templates.md`, beside the skills that apply it rather than in `docs/technical/`, which holds what the system is and how to run it; `skills/adr`, `skills/deliver-docs` and `skills/using-fiddle` read it; `Cites:` is retrofitted onto 021 through 028; `scripts/check-adr-cites.sh` runs in `scripts/gate.sh` and fails on an entry that resolves to nothing under `crates/`, or on an ADR numbered 021 or above with no such line. The retrofit floor is 021 because that is where the retrofit stopped: ten earlier ADRs reference symbols and are not backfilled, and asserting dependencies for decisions nobody re-read would be worse than the honest floor. `docs/technical/decisions/000-template.md` is deleted: it was a second ADR template disagreeing with the first about the Status line, with 13 of 29 ADRs written to one form and 16 to the other, nothing reconciling them and no skill pointing at either. The surviving template names the plain `Status:` form; the thirteen records written to the bold one are left alone.

**What the line does not do.** It makes a document's symbol dependencies greppable and it proves the symbols exist. It does not detect a symbol whose *meaning* changed under a document, which is what iteration 2 actually lost: `fiddle_core::selected` kept its name and shed an argument. The check would have passed. What catches that is `grep -rn selected docs/` returning ADR 021 — which it now does, and did not before.

Origin: M4c holistic remediation (lane `lane/i3fix`)
Tags: #documentation #process #evaluation

### 2026-08-21 — `[orchestration.cve] image` may need a second form, and only the host's workflow can say

ADR 040 gave `[agent] base_url` a second form, `{ env = "NAME" }`, so a repository can commit its `fiddle.toml` without writing its gateway URL. `image` was examined in the same lane and deliberately left alone: a tag names the publishing repository and is committable, and ADR 020 already owes the field a stronger change than a second form.

The case that reopens it: a host workflow that builds a per-run tag, such as one keyed by the commit. `snowplow-incubator/snowplow-identities` is the first host to commit a document, and its workflow is not written yet. If that workflow tags per run, the committed document cannot carry the tag, and the choice is between `WrittenOrNamed` on `image` and the declared-revision field ADR 020's losing alternative describes. The 2026-08-18 entry "Verifying that the scanned image was built from the remediated tree needs a config field no milestone owns" owns the second half of that choice.

Read the host's workflow before deciding. Adding the second form on the guess would put an environment indirection over a tag in the place a revision field is owed.

Origin: implementation (bean `fiddle-4fgu`, lane `lane/m4b-4fgu`)
Tags: #debt #feature

### 2026-08-21 — Nothing here has measured which directory `wizcli auth` writes, and the refusal reads that directory

ADR 042 stops fiddle from overriding `WIZ_CONFIG_DIR` and makes it read the login the caller left. `require_login` refuses before the scan when that directory holds no entry, so the directory it reads must be the one `wizcli auth` writes.

`DEFAULT_CONFIG_DIR` is `.wiz`, under `HOME`. That is an assumption. Design §2.4 measured the real scanner's arguments, its exit code and its JSON shape, and it measured no path. If wizcli keeps its login somewhere else, fiddle refuses a caller who did log in, and the refusal names a directory that was never the right one.

Two things settle it, and both need the real tool. Run `wizcli auth` on a host and list what appears. Or find the option that names the directory and set `WIZ_CONFIG_DIR` in the host workflow, which removes the assumption instead of confirming it. The second is the cheaper answer for one repository and leaves the default unmeasured for every other caller.

`holds_a_login` counts entries rather than reading a named file, for the same reason: the file name is unmeasured too. A directory holding unrelated wizcli state passes the check, and the scan's own exit then reports the failure, which is where this started.

Origin: implementation (bean `fiddle-j9gv`, lane `lane/m4b-j9gv`)
Tags: #debt #evidence
Status: 2026-08-21 — not applicable as written. The same lane narrowed to a
deletion: fiddle resolves no configuration directory and reads no login, so it
carries no `.wiz` assumption to measure. `DEFAULT_CONFIG_DIR` and
`require_login` never shipped. wizcli resolves its own directory, and a wizcli
that finds no login reports itself with exit 1 and no report. ADR 042 records
what replaced this.

### 2026-08-21 — The offered tool set is four names or five, and two entries above say four

`[[workspace.commands]]` gives an attempt a fifth tool, `run_command`, wherever a deployment declares a program for it to run. ADR 043 records the decision. Two entries in this file state a count that is now conditional, and this entry names them rather than rewriting them.

**2026-08-09 — The `output_mode` line is inert on the typed path, and the request shape is right for a reason nobody had checked** says "Turn 0 carries the four capability tools" and "The finalising turn carries the same four tools". Both remain true of the deployment that lane measured, which declares no program. Over a deployment that declares one they read five. Its finding about the synthetic `final_result` tool is unaffected: no synthetic tool is advertised on any turn under either count.

**2026-08-09 — The workspace command allowlist was stated four ways** is unaffected, and the distinction matters because both entries say "four". That one counts environment variable names, not tools. A declared command runs through `Workspace::run`, so it gets the same four names and no credential, and `workspace::a_workspace_command_inherits_no_credential` still pins them.

What is left open. Nothing bounds what a declared program does once it runs. **2026-08-09 — What M1's isolation does not claim: egress, injection, hostile processes** already records that the check command runs with a real `PATH` and that nothing stops it opening a socket. There are two such doors now rather than one, and the second is reachable by a model's own choice of arguments within a declared prefix. A deployment that declares a program which itself interprets a script has declared a shell, and no rule in Rust can see that.

Origin: implementation (bean `fiddle-56eq`, lane `lane/m4b-56eq`)
Tags: #debt #evidence
