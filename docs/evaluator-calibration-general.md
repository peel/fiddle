# Evaluator Calibration — general

## all dimensions — Correction (2026-07-29)
**Evaluator scored:** 9/10 — dimension scores emitted on a doc-only diff (fiddle-qxyk iteration 1)
**Human corrected to:** n/a — "there's no code here; nothing to score"
**Anchor:** For this project, doc-only and markdown-only beans (thresholds {}) get NO judgment dimension scores: emit "dimensions": {} and let evidence-backed criteria verdicts gate convergence. Dimension scoring on such beans is ceremony that adds double-pass cost without signal.

**Scope clarification (2026-08-07):** the trigger is *doc-only diff*, not `thresholds: {}`. A bean that changes Rust, Nix, or workflow files still receives dimension scores even though its eval block declares `thresholds: {}`. Every M0 bean below is a code bean.

---

## all dimensions — Correction (2026-08-08)
**Evaluator scored:** varied by one point across repeated passes over an identical tree — `domain_spec_fidelity` 10 then 9 (fiddle-5fwq), `correctness` 9 then 8 then 9 (fiddle-oek3), with substantively identical rationales, empty guidance and no antipatterns on every pass.
**Human corrected to:** n/a — not a score correction. Recorded from the mechanical score history: no implementer ran between the passes, so the deltas cannot be code regressions.
**Anchor:** For this project, a one-point dimension delta on an unchanged tree is within evaluator noise, not a regression. `check-convergence.sh` reports PASS_REGRESSED on any downward move, which cost these two beans 9 dispatches each against a 16 budget — the only two in the epic to exceed 5. When re-scoring a tree no implementer has touched, hold a dimension at its prior score unless the second look found something the first missed, and say what that was. A confirmation-pass prompt must not name a dimension to reconsider: doing so manufactured the fiddle-5fwq regression outright.

## M0 — Executable skeleton

Anchors for milestone epic `fiddle-7lmw`. Criterion IDs are milestone-namespaced and appear verbatim in the
`eval` blocks of the generated implementation beans. Each level names the exact fixture and the exact
failure signal; do not substitute an adjective for a command and its output.

Standing rule for this milestone: **a criterion is met only by an assertion made from outside the process** —
an exit code, a `--json` field, a file on disk, or a byte-level fixture comparison. "The type exists",
"the trait is implemented", or "the code looks correct" is never evidence. An evaluator that scores a
criterion met without quoting the observed command output has scored it wrong.

### `m0-cli-contract-correctness`

Command surface, exit-code mapping, `--json` field stability, and the read-only guarantee on `inspect`.

- **Poor (1–3).** Exit codes are produced at more than one site, or `inspect` is asserted only by exit
  status. Any of: `fiddle inspect bogus`, `fiddle inspect mystery:x`, `fiddle inspect beans:` returns a
  code other than 2, or returns 2 with a diagnostic that does not name the specific defect. `fiddle config
  check` accepts the unknown key `nickname` instead of exiting 2 with `nickname` on stderr. A run of
  `fiddle inspect` leaves `<stub.root>` byte-different or creates `<report.dir>`.
- **Acceptable (4–7).** `exit_code_for` in `crates/fiddle-cli/src/main.rs` is the single mapping site and
  the acceptance tests assert the numeric codes 0/2/10/11/20 from outside the process. `fiddle config
  check --config tests/fixtures/fiddle.toml --json` exits 0 emitting `{"status":"valid"}` with
  `project.name = icecube`. `fiddle inspect beans:fiddle-m0-demo --json` exits 0 and echoes
  `invocation_ref` `beans:fiddle-m0-demo` with scheme `beans`. A byte-level snapshot of `<stub.root>` is
  identical before and after `inspect`.
- **Excellent (8–10).** All of the above, plus each rejected `InvocationRef` carries a distinct stderr
  diagnostic naming its own defect (unknown scheme vs. missing scheme vs. empty value), and the `--json`
  payload shape is asserted field-by-field rather than by substring match, so a renamed field fails the
  test rather than passing silently.

### `m0-typed-boundary-fidelity`

Crate ownership, `Observation`/`RunOutcome` fidelity, and fail-closed `Unavailable`.

- **Poor (1–3).** An unreadable source is rendered as an empty value, an absent value, or `not_applicable`
  instead of `Observation::Unavailable`, or a blocked derivation still executes the capability.
  `fiddle-core` acquires `tokio`, `rig_core`, or `reqwest` in its resolved dependency graph, or performs
  filesystem, clock, or environment access inside `assess`/`derive_next`. The crate boundary is asserted by
  reading code rather than by `cargo metadata`.
- **Acceptable (4–7).** With `<stub.root>` removed, `fiddle inspect beans:fiddle-m0-demo --json` emits
  `observations.work_item.unavailable` with a reason and no `available` key, and `fiddle run` exits 20 with
  outcome `failed` and an empty `capability_executions`.
  `crates/fiddle-acceptance/tests/crate_boundary.rs` parses `cargo metadata` and fails on a forbidden
  `fiddle-core` dependency. Stub ports sit behind `WorkItemPort`/`ChangePort` in `fiddle-runtime`.
- **Excellent (8–10).** All of the above, plus all three `assessment` cases are proven from the outside
  against distinct fixtures — `not_started` + `next_action execute(stub_mark)` on an unmarked fixture,
  `satisfied` + `complete` on a marked one, `blocked` + `blocked` on an unreadable `<stub.root>` — and the
  purity of `assess`/`derive_next` is enforced by signature (`&WorkStateView` only) rather than by comment.

### `m0-evidence-quality`

Report-bundle completeness, atomic publication, and build identity.

- **Poor (1–3).** `report.json` omits `schema`, `fiddle.package_version`, or `fiddle.source_revision`, or
  carries an empty or fabricated revision. A failed publication leaves a partial `report.json` or an
  orphaned `.tmp` directory. The bundle is asserted from the process's own stdout rather than read back
  from disk.
- **Acceptable (4–7).** The published `report.json` contains `schema` `fiddle.report.v0`,
  `fiddle.package_version` as a semver string, and `fiddle.source_revision` as a 40-hex sha or the literal
  `unknown`, plus `outcome`, `next_action`, `capability_executions`, and the full `observations` view. With
  `<report.dir>` at mode `0o500`, `fiddle run` exits 11 naming the path on stderr and leaves neither
  `report.json` nor a `.tmp` directory. **11, not 20**: `RunOutcome::Failed` means "will not succeed by
  being repeated as invoked", and repeating this run once the operator has fixed the permissions does
  succeed, so reporting it as `Failed` would tell the caller the opposite of the truth. Exit 20 stays
  reachable through the unobservable-`<stub.root>` case, which asking again genuinely does not fix.
- **Excellent (8–10).** All of the above, plus the bundle is read back from disk and asserted independently
  of process output, and the unwritable-directory failure injection is a committed test rather than a
  manual check — the newly introduced publication boundary has its own negative case. Executing and
  recording are one transaction owned by `fiddle-runtime`, with a test in that crate covering the two
  together: a capability that succeeded and a bundle that could not be published leaves durable evidence
  that the capability ran, and an attempt interrupted between the effect and publication is detectable
  afterwards rather than indistinguishable from one that never ran.

### `m0-black-box-acceptance`

Fresh-process invocation, external assertion, and the stability proof.

- **Poor (1–3).** Acceptance tests call library functions instead of launching the compiled binary as a
  subprocess. The stability claim rests on one invocation, or on a re-entrant in-process call rather than a
  second fresh process. The scenario requires a credential to pass. Test support code has leaked into a
  production adapter or runtime mode.
- **Acceptable (4–7).** `cargo test -p fiddle-acceptance --test m0_skeleton` exits 0, driving config check,
  inspect, run, bundle assertion, and a second fresh invocation in one cumulative scenario against the
  compiled binary via `assert_cmd`. After two fresh `fiddle run` processes exactly one marker file exists
  with unchanged bytes, the second bundle's `capability_executions` is empty, and the two bundles carry
  different `attempt_id` values with the same `work_ref`. The scenario passes with `GITHUB_TOKEN`,
  `GH_TOKEN`, `ANTHROPIC_API_KEY`, and `JIRA_API_TOKEN` removed.
- **Excellent (8–10).** All of the above, plus the identical scenario runs against `peel/fiddle-acceptance`
  leaving no branch, PR, or other residue; `.github/workflows/rust.yml` runs the proof as a named step so
  it is visible in the job log; and `docs/technical/SYSTEM.md` records the exact command so the M1 seed can
  execute it as a baseline without rediscovering it.

### Known-blocked criteria

`m0-black-box-acceptance` at the excellent level depends on `peel/fiddle-acceptance`, which did not exist
at baseline (design §2.8, blocker B2). Until the provisioning bean lands, score that criterion against the
credential-free in-repo lane and record the external lane as blocked — do not mark the criterion met by
redefining it, and do not fail a bean for an external repository it does not own.

## M1 — Bounded Rig capability

Anchors for the milestone that inserts one bounded Rig agent attempt inside M0's deterministic
spine. Every M1 bean declares `thresholds: {}` and every M1 bean changes code, so the 2026-07-29
doc-only correction does not apply to any of them.

**Reconciled against what was built (2026-08-09).** This section was written before implementation
and anchored on **four** things that do not exist: an `m1_bounded_capability` acceptance lane, a
scheduled degraded-JSON canary workflow, a live round-trip proof against a Claude-family model,
and an outer per-capability attempt limit owned by `fiddle-runtime`. Each has been replaced below
by the artifact that actually ships. An anchor that names a file is a promise that the file is
there; where one is not, the anchor is the defect, not the implementation. Criterion ids are
unchanged, because they appear verbatim in the `eval` blocks of epic `fiddle-y1w6`'s beans — read
`m1-canary-evidence` as *the real-model lanes*.

The fourth was found a pass later than the other three, in this document rather than by it, and it
is the reason this paragraph now counts. `agent.max_capability_attempts` parses and defaults to 3
and is read by nothing; M1 ships one bound, and the decision, with what taking up the second would
cost, is `decisions/013-one-attempt-bound-not-two.md`. `m1-bounded-behavior` below asks for the
bounds that fire.

**That sweep reached three criteria of five, and this completes it (2026-08-09).**
`m1-tool-protocol-correctness` named a test that had never been written, so its Excellent level
was unreachable and its Acceptable level described nothing. `m1-workspace-isolation` carried two
statements the tree contradicts, and the second of them had *already* been corrected for a sibling
criterion thirty lines further down and left standing here — which is the worse of the two
failures, because a document that answers the same question two ways scores a bean on which
paragraph the evaluator happened to read. Both are settled at their criteria below, and each says
how: an anchor is a promise about the tree, so *reconciling the anchor* and *building what it
names* are different answers and a reader is owed which one was given.

**The scope rule for this milestone, stated once.** Model output quality is nondeterministic and is
never the deterministic gate. Every criterion below is scored against what the *deterministic shell*
does with the model's output — the bounds it enforces, the checks it runs, the evidence it derives
independently — and never against how good the model's repair was. An evaluator that rewards a
better-worded agent summary, or penalises a scripted fixture for being easy, has scored the wrong
thing. Equally, a criterion is never met by asserting the model said it succeeded.

### `m1-tool-protocol-correctness`

The prompt, the advertised tool schemas, and what reaches a tool.

**The serialized-request test now exists; it was written for this anchor rather than found by it.**
When this section was first reconciled, no test in the workspace had ever read an outbound request:
`MockCompletionModel::requests()` and `request_count()` had zero call sites, and the whole of the
protocol evidence was `ReadFile.parameters()` and its siblings inspected on the builder — which
made the Excellent level below unreachable and the Acceptable level a description of nothing. The
test is `binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact`, and it
reads the chat-completions bodies the **compiled binary** put on a loopback socket, which is the
outbound request in the strongest available sense.

Two corrections it forced, both measured rather than reasoned:

- **The offered set is four names, not five.** `agent::attempt` asks for `OutputMode::Tool`, whose
  documented behaviour would advertise a synthetic finalising tool as a fifth. Rig 0.41's
  `prompt_typed` overrides the mode to `Native`, so no synthetic tool is ever sent and the anchor's
  "exactly the capability's four tools" is right for a reason nobody had checked. Deleting the
  `output_mode` line changes nothing on the wire. See BACKLOG, *The `output_mode` line is inert on
  the typed path*.
- **The native `response_format` constraint is sent, on the finalising turn only.** A criterion
  scored against "no native constraint is sent" would be scored against a claim the wire refutes.

- **Poor (1–3).** A trusted value — workspace root, cancellation token, effect executor, or anything
  credential-bearing — appears in a tool's `Args` and is therefore model-visible. Tool schemas accept
  absolute paths or unbounded arguments. The prompt or a tool result carries a resolved secret. Tools
  are registered but nothing asserts which ones the model was actually offered.
- **Acceptable (4–7).** Trusted values reach tools only through Rig's host-only `ToolContext` via
  `context.require::<T>()`; `Args` carry relative paths and bounded values only. A test serializes the
  model-visible request and asserts that the advertised schema contains no absolute path, no host
  handle, and no credential, and that the offered tool set is exactly the capability's four tools —
  `read_file`, `write_file`, `list_files`, `run_check`. A call to an unregistered tool name is
  rejected rather than dispatched.
- **Excellent (8–10).** All of the above, plus the assertion is made against the *serialized outbound
  request* rather than against the builder that produced it, so a future Rig change that starts
  leaking context into arguments fails the test rather than passing it. The absence of host-only
  values is asserted positively (the serialized prompt, messages, and tool arguments are searched for
  the workspace root — in both of its spellings, since macOS's `/var` is a symlink to `/private/var`
  and searching for one alone is vacuous — and for the credential variable's *value*, which is
  legitimately in the `authorization` header of the same request and must therefore be searched for
  in the body alone) rather than inferred from the type.

### `m1-typed-output-fidelity`

Structured output, and the standing of what the model claims.

- **Poor (1–3).** The run outcome is derived from the agent's own `claimed_complete`, its prose, or
  the mere fact that the attempt returned `Ok`. Malformed or out-of-range structured output is
  accepted, coerced, or silently defaulted.
- **Acceptable (4–7).** `output_schema::<T>()` plus `prompt_typed::<T>()` return a validated Rust
  value; malformed output is an error naming the field, not a default. `claimed_complete` is recorded
  in evidence and never consulted for the outcome. A test drives a scripted model that returns
  malformed JSON and asserts the attempt fails rather than proceeding.
- **Excellent (8–10).** All of the above, plus **the model-lies case is a committed test**: a scripted
  model returns `claimed_complete: true` over a fixture whose `cargo test --offline` still fails, and
  the run concludes `Retryable` with the check failure named in its evidence. This is the criterion's
  centre of gravity — a milestone that inserts a component able to assert its own success has not
  proven anything until it has proven it can be disbelieved.

### `m1-bounded-behavior`

Turn, time, tool, and mutation limits, and cancellation.

**There is no outer per-capability attempt bound, and its absence is a decision, not a gap.**
`agent.max_capability_attempts` parses, defaults to 3, and is consumed by nothing:
`fiddle_runtime::attempt` runs one attempt and reports `RunOutcome::Retryable` for a caller to
repeat. `decisions/013-one-attempt-bound-not-two.md` records the decision and prices the change —
`Retryable` has four producers of which only one is "the capability tried and lost", so the loop
needs a taxonomy the outcome type does not carry, and both placements for it move something M0
asserts. Score against the bounds that fire. An evaluator that marks this criterion down for the
missing outer bound is scoring against a superseded plan; one that marks a bean down for *not
having noticed* the key is unconsumed is scoring correctly only if the bean touched that path.

- **Poor (1–3).** No bound is enforced at all, or the inner turn limit is enforced by a hand-rolled
  counter rather than by the runtime. Cancellation is assumed to follow from dropping a future. A
  bound is configured but nothing asserts it fires. A configuration key that fires nothing is
  presented as though it did.
- **Acceptable (4–7).** Four bounds that fire, each with a test that drives it past its limit and
  asserts the specific error: the per-attempt turn limit enforced by Rig (a scripted model with
  more tool calls than `max_turns` yields Rig's `MaxTurnsError` —
  `agent::the_turn_budget_is_enforced_by_the_runtime`), the wall-clock deadline
  (`the_deadline_bounds_an_attempt_that_would_otherwise_run_on`), the files-changed cap
  (`exceeding_the_changed_file_cap_fails_the_attempt`), and the per-tool timeout
  (`the_budgets_tool_timeout_bounds_a_single_tool_without_ending_the_run`), with
  `workspace::a_command_that_overruns_its_timeout_is_killed` under it. A cancellation token is
  passed into every tool and the check runner and is checked before mutation.
- **Excellent (8–10).** All of the above, plus a cancellation test that cancels *between inspection
  and mutation* and asserts the workspace is unmutated afterwards — proving cancellation prevents an
  effect rather than merely ending a future — and the cancelled attempt's outcome is shown to come
  from M0's existing post-execution re-derivation, adding no row to the exit-code table.

### `m1-workspace-isolation`

The ephemeral workspace, path validation, and environment sanitization.

**Two statements in this criterion were reconciled a pass late (2026-08-09), and the second one had
already been corrected for a sibling criterion.** The allowlist was written here as three names and
is four. And `git status --porcelain` was named here as the whole of the changed-file derivation,
which the two-half derivation invalidated — `m1-fixture-repair-acceptance` below says so and this
criterion did not, so a bean touching `workspace::changes` could be scored against a superseded
question and a corrected one thirty lines apart in the same document.

- **Poor (1–3).** Path containment is a `starts_with(workspace_root)` check. `std::env::remove_var`
  is used to strip credentials, mutating the host process. The workspace outlives the attempt, or its
  teardown is skipped on the failure path. Build artefacts pollute the changed-file evidence.
- **Acceptable (4–7).** A per-attempt `git worktree add --detach`, removed after evidence capture on
  every path including failure. Paths are normalized, the deepest existing ancestor resolved, and
  `..`, absolute paths, NUL bytes, and platform prefixes rejected. Workspace commands run under
  `Command::env_clear()` and an allowlist of exactly four names — `HOME` at the workspace's scratch
  home, `LANG` fixed to `C`, `PATH` inherited (or `/usr/bin:/bin` when the parent has none), and
  `RUSTUP_HOME` inherited only when the parent has one — which is the statement in
  `docs/technical/SYSTEM.md`'s Invariants, the code in `crates/fiddle-runtime/src/workspace/command.rs`, and the two exact
  sets `workspace::a_workspace_command_inherits_no_credential` asserts. The changed-file set is
  derived in two halves and not from `git status` alone: tracked changes from
  `git status --porcelain=v1 -z -uno`, created files from `git ls-files --others` under the ignore
  rules committed at the branched HEAD, so a `target/` the check produced is excluded by the
  project's own rules rather than by the worktree's current ones.
- **Excellent (8–10).** All of the above, plus a symlink-escape case is a committed test — a symlink
  inside the workspace pointing outside it is refused for both read and write, not merely for a path
  containing `..` — and a test asserts that no credential-shaped variable survives into a workspace
  command's environment, by running a command that dumps its own environment and searching the
  output. ADR 011 exists because M0 shipped a path derived from an unvalidated value; the same class
  of defect is checked here rather than assumed absent.

### `m1-fixture-repair-acceptance`

The credential-free black-box proof.

**There is no `m1_bounded_capability` lane.** The proof landed as three files, and an evaluator
scoring this criterion runs those rather than looking for a lane that was never written:
`crates/fiddle-runtime/tests/repair_protocol.rs` (the deterministic protocol suite, in-process by
design, carrying the adversarial cases), and in `crates/fiddle-acceptance/tests/`,
`capability_selection.rs` (black-box, reaching the selection, rejection and failure arms) and
`binary_repair.rs` (black-box, driving a repair that succeeds against a loopback stub gateway).

- **Poor (1–3).** The black-box lanes call library functions instead of launching the compiled
  binary. Any of them requires a credential, or a network call beyond loopback, to pass. The repair
  is asserted from the agent's response rather than from the fixture on disk. M0's `m0_skeleton`
  lane was modified to accommodate M1.
- **Acceptable (4–7).** `cargo test -p fiddle-runtime --test repair_protocol` and
  `cargo test -p fiddle-acceptance` both exit 0. The success path is proven with zero model
  dependence: a scripted model writes known-correct content, the configured check runs over the tree
  the attempt actually left, and the correlation marker appears only after the check exits 0 — all
  read back from disk and from git, never from the model's response. (Which git question is not the
  criterion's business, and this line named one that is no longer asked: the derivation is
  `git status --porcelain=v1 -z -uno` for tracked changes plus `git ls-files --others` under the
  ignore rules committed at the branched HEAD for created files. See `workspace::changes`.)
  Nothing in `src/` knows a test is happening: no transcript provider, no test-only runtime mode.
  Every lane passes with `ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, `GH_TOKEN` and `JIRA_API_TOKEN`
  removed, and `LITELLM_API_KEY` set to a sentinel that authenticates nothing. `m0_skeleton` still
  exits 0, unmodified.
- **Excellent (8–10).** All of the above, plus the adversarial cases exist as committed tests, each
  asserting the fixture was left unmutated: `../outside.txt`, an absolute path, a symlink pointing
  out, an unregistered tool name, malformed structured output, mutation past the files-changed cap,
  turn-budget exhaustion, and cancellation mid-attempt. They currently ride in `repair_protocol`
  rather than in a black-box lane, which is a defensible split — their claim is about the *shell's*
  response to a model input, not about the assembled binary — so an evaluator does not mark a bean
  down for their location; it marks one down if a case is absent, or is asserted from the model's
  own report. The fixture repository is created by the harness with an explicit `-c user.email` /
  `-c user.name`, so no lane depends on the runner having a git identity. At this level the
  document-to-capability wiring is also gated offline: `binary_repair.rs` answers real
  chat-completions requests from a loopback socket, so a `build_capability` that mapped `deadline`
  onto `tool_timeout` fails a gate command rather than surviving until someone runs Tier 1.

### `m1-canary-evidence`

The real-model lanes, and what they report when they cannot run.

The model provider is a **LiteLLM OpenAI-compatible gateway**, not Anthropic directly. Two facts
follow, and both are scored here. The gateway translates OpenAI function-calling shape to the
upstream provider, so a real-model lane is the *only* proof that tool calls survive that
translation — the deterministic suite replaces the provider with `MockCompletionModel` and never
serializes a request to anyone, and `binary_repair.rs` serializes to its own loopback socket, which
proves the wire format and not the translation. And the key carries a **$100 hard cap**, so
requests begin failing on spend rather than on correctness.

**There is no scheduled canary, and its absence is a decision, not a gap.** No workflow carries a
`schedule:` trigger, no crate has a canary subcommand, and no degraded-status payload exists
anywhere. Two opt-in local lanes replace it — Tier 1 (`crates/fiddle-cli/tests/smoke.rs`, one
`#[ignore]`d test) and Tier 2 (`scripts/tier2.sh`) — for the reasons ADR 012 records, including
what the substitution gives up. Score against those two. An evaluator that marks this criterion met
by pointing at a scheduled workflow has found something that does not exist, and one that marks a
bean down for not having built it is scoring against a superseded plan.

- **Poor (1–3).** A real-model lane gates merges, or is invoked from `.github/workflows`, or fails
  a gate command when the credential is absent. It asserts that the model succeeded. It reports
  success without having reached a model turn. Its output is unstructured log text, or records only
  pass/fail and a duration. A spent budget is reported as a capability failure with nothing recorded
  that would let a reader tell the two apart.
- **Acceptable (4–7).** Tier 1 is `#[ignore]`d so the gate stays offline and free, needs
  `LITELLM_API_KEY`, and fails loudly naming the variable rather than skipping silently — a test
  that passes for want of a key proves nothing and says it proved something. It asserts protocol
  only: the run reached the capability it was asked for, concluded on a row of the exit-code table,
  the exit code is that row's, a bundle was published and parses, the fixture repository is
  untouched, the marker is present exactly when the check earned it, and the credential appears in
  no stdout, no stderr and no file. A run that never reached a model turn is classified
  *inconclusive* and fails, rather than being reported as a weak model. Tier 2 writes one JSON
  record per fixture plus a `summary.json` carrying model, gateway base URL, exit code, outcome
  kind, reason, elapsed seconds, the marker, `capability_executions` and `repair_landed`, and exits
  0 whatever the model made of the fixtures.
- **Excellent (8–10).** All of the above, plus the credential-free half of the same wiring is pulled
  into the gate rather than left to a lane nobody runs on a schedule — `binary_repair.rs` answers
  the real chat-completions requests from loopback, so Tier 1 is left proving only what genuinely
  needs a real model. And the failure classes an operator must tell apart are separated in what is
  recorded rather than collapsed into one `failed`: `budget_exhausted` above all, since an exhausted
  budget that reads as a broken capability is the specific defect this anchor exists to prevent.
  **That separation is not implemented** — a spend-cap refusal reaches `AgentError::Provider` with
  the gateway's message text, distinguishable by a human reading Tier 2's `reason` field and by
  nothing else — and ADR 012 records it as an open consequence with a backlog entry. Score a bean on
  what it did about that gap, and do not treat the gap itself as the bean's defect. Persistence is
  the other half of this level: no raw prompt, tool result or repository content should be kept
  beyond what a human needs to read the run, and today Tier 2 keeps the model's report verbatim and
  a 4000-character stderr excerpt, which is a cost a bean touching that path should name rather than
  widen.

### Known-blocked criteria

None for M1's provider surface. The gateway credential exists locally and both real-model tiers have
been run against it. Three notes, all of them corrections to what this section claimed before the
milestone was built:

- **There is no live proof against a Claude-family model, and one is not expected.** An earlier
  draft recorded the tool-call and structured-output round trips as proven live against
  `claude-sonnet-4-6`. Measurement superseded that: through this gateway `claude-haiku-4-5` and
  `claude-sonnet-5` both finalise after a single `list_files`, and sonnet then fails its own report
  schema. The round trips are proven live against `bedrock/moonshotai.kimi-k2.5`, `deepseek.v3.2`
  and `zai.glm-5` — see ADR 012's table, which is the committed record. A bean whose real-model lane
  defaults to a Claude-family model is choosing the worst-measured path on this gateway.
- **No repository secret exists, and no lane needs one.** `gh secret list --repo peel/fiddle --json
  name` returns `[]`. Nothing in `.github/workflows` invokes a real-model lane, so there is no
  CI-exercised path to be blocked. Score the tiers from their local invocation; the absence of a CI
  path is ADR 012's decision, not a blocked criterion.
- **Model names come from the gateway, not from the RFC.** The RFC's `claude-sonnet-4-5` is not
  available there. A bean that hardcodes an RFC model name has a runtime defect, not a style
  problem.

## M2 — Safe GitHub effects

Anchors for the milestone that gives fiddle its first authenticated, mutating reach outside the
process. Every M2 bean declares `thresholds: {}` and every M2 bean changes code, so the 2026-07-29
doc-only correction does not apply to any of them.

**The scope rule for this milestone, stated once.** M2 is scored on how the deterministic shell
*interprets* GitHub, never on whether GitHub cooperated. A network that was slow, a rate limit that
fired, a workflow that queued — none of these is a defect in a bean. What is a defect is a shell
that reads any of them as a settled answer. The single question behind every criterion below is:
when the result was not known, did the code go and look, or did it guess? A bean that retries a
mutation to resolve an unknown has failed this milestone's central property however green its tests
are.

**A second rule, because this is the first milestone that can damage something.** An anchor here is
scored against the *external* state a run left behind, not against its exit code or its report. M1
could be judged by reading a bundle; M2 cannot, because a bundle claiming one pull request and a
repository holding two is exactly the failure the milestone exists to prevent.

### `m2-effect-boundary-coverage`

The executor, the authorization envelope, and stable effect identity.

- **Poor (1–3).** A capability, a tool, or an adapter performs a mutation without passing the
  executor. `AuthorizedEffect` is constructible outside it, so the envelope proves nothing. Effect
  identity is a UUID, a timestamp, a counter, or anything else a fresh process cannot recompute —
  or it is not derived at all and the code relies on a local file to remember what it did. Policy is
  consulted after the mutation, or not at all.
- **Acceptable (4–7).** Every mutation passes the executor, which walks the PRD's order: validate,
  derive `EffectId` and payload hash, inspect the postcondition, combine capability minimum with
  deployment policy, obtain the handle, construct the envelope, delegate, observe the postcondition,
  return a receipt. `EffectId` derives through `blake3` over canonical inputs in the same shape as
  `correlation_key`, so a fresh process recomputes it from `(project, invocation_ref, kind, target)`
  with no local state. `AuthorizedEffect`'s constructor is private to the executor. The receipt
  carries effect id, payload hash, target identity, observed postcondition, and external reference.
  `combine` is a total function over its enums and is tested as one.
- **Excellent (8–10).** All of the above, plus the envelope's privacy is proven rather than
  asserted — a compile-fail test, or a boundary test in the shape of `crate_boundary.rs`, shows that
  no path outside the executor constructs one. The payload hash is load-bearing rather than
  decorative: a changed payload against an unchanged identity is *detected* and reported, which is
  what stops an approved effect from being widened later. And the policy combination's
  never-weaken rule is proven exhaustively over the product of both enums rather than sampled at
  three interesting points.

### `m2-ambiguity-classification`

The three-valued outcome, and what the code does when it does not know.

- **Poor (1–3).** A transport timeout, a killed subprocess, or a 5xx is classified as failure. A 422
  is mapped to success on its face, or to failure on its face. `gh`'s exit code is read as though it
  carried an HTTP status. An unknown result is resolved by retrying the mutation. Any of these
  alone is Poor: each is a way of turning "I do not know" into a confident wrong answer, and each
  produces the duplicate the milestone forbids.
- **Acceptable (4–7).** `EffectOutcome` separates `Committed`, `NotCommitted` and `Unknown`, and the
  three are produced by distinct evidence rather than by one branch with a comment. A killed `gh`, a
  transport timeout and a 5xx all reach `Unknown`. `Unknown` is resolved by a postcondition read and
  by nothing else. A 422 goes back to the same lookup before being called anything. The status comes
  from the `gh api -i` status line — exit 4 is authentication, exit 2 is cancellation — because
  `gh help exit-codes` documents exit 1 for every HTTP failure regardless of status, so a bean that
  branches on the exit code has read the wrong surface.
- **Excellent (8–10).** All of the above, plus `Unknown` is not a leaf: the postcondition read has
  its own three outcomes and a read that itself fails leaves the effect unresolved and *says so*,
  rather than degrading to one of the two confident answers. The retry budget is per-effect and
  bounded, `Retry-After` and the `X-RateLimit-*` headers `-i` exposes are honoured rather than
  parsed and dropped, and more than one matching object is reported as a duplicate-state error
  rather than resolved by choosing the first. A bean that silently picks `[0]` from a two-element
  result has written the bug this milestone is about.

### `m2-credential-isolation`

Where the two credentials live, and what can reach them.

- **Poor (1–3).** A token appears in `argv` — `git push https://<token>@…` is the specific form,
  and `/proc/<pid>/cmdline` is world-readable on Linux. A token reaches a workspace command, a
  model-visible tool, a tool schema, an error message, or a published bundle. The `gh` subprocess
  inherits the ambient environment, so which credential it used depends on the operator's machine.
  A configuration document accepts a literal token.
- **Acceptable (4–7).** `gh` runs under `env_clear()` plus exactly `PATH`, `GH_TOKEN`,
  `GH_CONFIG_DIR`, `GH_PROMPT_DISABLED` and `NO_COLOR`, with `HOME` deliberately absent. The push
  credential reaches git through `GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>`
  as an `http.…extraHeader`, never through `argv`, with `GIT_TERMINAL_PROMPT=0`. The token resolves
  from `{ env = "NAME" }` only. Neither spawn site is a workspace command, and
  `workspace::a_workspace_command_inherits_no_credential` still pins the four-name set exactly —
  a bean that widens *that* allowlist to make GitHub work has broken M1's invariant to build M2.
- **Excellent (8–10).** All of the above, plus the isolation is proven the way
  `acceptance-repository.md` proves its credential-free clone — by running the thing and showing it
  refuses. With `HOME` absent and `GH_CONFIG_DIR` pointed at an empty directory, `gh` answers
  `please run: gh auth login` rather than reaching the operator's keyring; with `credential.helper`
  set empty, the push cannot fall back to the keychain. Both are assertions, not paragraphs. And the
  credential's *value* is searched for in every published bundle, every diagnostic and every stdout
  byte, in the shape `capability_selection.rs` already established with its `LITELLM_API_KEY`
  sentinel — a second sentinel for the GitHub token, not a second prose promise.

### `m2-duplicate-proof`

The milestone's mandatory automated proof.

- **Poor (1–3).** Exactly-once is asserted only against a live repository, so the gate does not
  carry it. The ambiguous write is simulated by calling the code twice rather than by interrupting
  it after dispatch, which proves the read-before-write path and nothing about a lost response. The
  proof asserts an exit code or a report field instead of counting objects at the remote. Cleanup is
  absent, so the second run's starting state is the first run's residue.
- **Acceptable (4–7).** The fault is injected after the mutation was dispatched and before its
  result was observed — the scripted `gh` stub records what it was asked for and then exits as
  though killed, which is a genuine lost answer rather than a mocked one. A *fresh process* then
  recomputes the identity, observes the recorded state, and publishes nothing further. Exactly one
  branch, one pull request and one requested check are asserted afterwards. Because the stub owns
  both halves, this runs in the gating offline suite, for the same reason M1's adversarial cases
  ride in `repair_protocol` rather than in a live lane.
- **Excellent (8–10).** All of the above, plus the injection point is exercised at *each* of the
  three objects rather than once at the easiest of them — and the check request is the one that
  counts, because `git push` to a named ref is already idempotent and GitHub already refuses a
  second pull request for the same head and base, while `workflow_dispatch` will genuinely start a
  second run. A proof that demonstrates exactly-once only where GitHub was going to provide it
  anyway has demonstrated GitHub's property, not fiddle's. The live lane repeats the same walk
  against the disposable repository and asserts zero residue afterwards, so a failure leaves the
  next run a clean world.

### `m2-live-github-grounding`

Whether the design and its beans are anchored in the GitHub that exists.

- **Poor (1–3).** An endpoint, a permission, a `gh` flag or an output shape is asserted from the RFC
  or from training rather than from a probe. The disposable repository is assumed to exist. The CI
  lane is assumed to work with the default `GITHUB_TOKEN`.
- **Acceptable (4–7).** Read-only evidence is recorded for token scopes, repository permissions, the
  acceptance repository's state, `gh`'s error and exit-code surface, and git's environment config
  channel. Assumptions that could not be proven read-only are recorded as unproven and assigned to a
  bean rather than quietly assumed. An isolated implementation/acceptance bean owns creating the
  disposable repository, the deterministic naming, and the cleanup assertions.
- **Excellent (8–10).** All of the above, plus each probe is recorded with the command and its actual
  output, so a later reader can rerun it rather than trust it — and where a probe *changed* the
  design, the anchor says so. The two facts that most need this treatment are that `gh`'s exit code
  carries no HTTP status (so `-i` is not a preference) and that the credential has no `delete_repo`
  scope (so the standing-repository choice is forced, not stylistic).

### Known-blocked criteria

Two, both recorded read-only during planning and neither resolvable without a write:

- **Cross-repository write permission is unproven.** `peel/fiddle-effects-acceptance` does not
  exist; `gh repo list peel` returns exactly `fiddle` and `fiddle-acceptance`. The credential holds
  `repo` and ADMIN on both existing repositories, so write is *plausible*, and planning's read-only
  mandate is why it is not *proven*. Score the bean that creates the repository on whether it proves
  the write and cleans up; do not score sibling beans as though the repository were already there.

  **Resolved 2026-08-09, before implementation began.** The repository exists, is public, and holds
  zero secrets and zero deploy keys. A fine-grained token created and deleted `refs/heads/probe` in
  it, leaving branches at exactly `main`. So this is no longer a blocked criterion and a bean must
  not be scored as though it were.

  **This entry said "scoped to it alone" and that was false when it was written.** The token's
  repository selection was two repositories wide — `peel/fiddle-effects-acceptance` *and*
  `peel/fiddle-acceptance`, M0's external acceptance repository — and
  `repos/peel/fiddle-acceptance/collaborators` answered **200** for a full day of M2's
  implementation. The operator narrowed the selection on 2026-08-10; the probe table in
  `docs/technical/effects-repository.md` now records 200 for the effects repository and 403 for both
  others, and a ref-create against `peel/fiddle-acceptance` answers
  `403 Resource not accessible by personal access token`. **Score against that table, not against
  this paragraph** — a document restating a scope is a claim, and the table is the measurement.

  Four things the probes settled that the anchors above now depend on:

  - **The duplicate create returns exactly `422 "Reference already exists"`**, observed rather than
    predicted. `m2-branch-422-resolved-by-reading` is therefore scored against a known response
    shape, and a bean that maps that status to failure on its face has contradicted a measured fact.
  - **The token's scope is proven by 403, not by 404.** `peel/fiddle` is public, so reading it
    succeeds with any credential and proves nothing; what proves the grant is absent is
    `Resource not accessible by personal access token` on a permission-gated endpoint. A bean whose
    isolation evidence is a successful public read has asserted nothing, and `.permissions` on the
    repository payload is worse than nothing — it reports the *user's* rights, reading
    `admin=true` on a repository the token cannot write.
  - **A probe that cannot discriminate is not evidence.** This is how the two-repository selection
    survived a milestone: `/actions/secrets` answers 403 for *every* repository, so a 403 there says
    only that the `Secrets` permission is absent — nothing about which repositories are selected.
    `/collaborators` answers 200 for the selected one and 403 for the others, so it can tell the
    cases apart. Score a scope claim on whether its probe could have come out the other way; a probe
    whose result is the same for the repository in question and for one plainly out of scope is
    decoration. This applies to the evaluator's own reasoning as much as to the bean's.
  - **`Actions: write` is what a `workflow_dispatch` requires**, not `Workflows: write`, which
    governs pushes touching `.github/workflows/**`. Both exist and both are real, so a bean naming
    the wrong one is wrong specifically about the dispatch: the credential it provisions 403s where
    it matters *and* gains the authority to rewrite the target's CI. `.env.example`,
    `.github/workflows/github-effects.yml` and `docs/technical/effects-repository.md` name the same
    list; a bean that disagrees with all three has not checked one of them.
- **No repository secret exists.** `gh secret list --repo peel/fiddle` is empty, and the default
  `GITHUB_TOKEN` is scoped to `peel/fiddle` and cannot write to the disposable repository whatever
  its `permissions:` block says. The `workflow_dispatch` lane therefore needs a cross-repository PAT
  the operator must add. Until it exists, that lane is blocked rather than failing, and a bean is
  scored on having named the requirement precisely — not on having made a lane pass without a
  credential it cannot have.

## all dimensions — Measurement gap, not a correction (2026-08-10)

**M2 recorded no dimension scores at all.** Every evaluator across all 21 converged beans returned `"dimensions": {}` — the evidence-only shape — so `trend-eval-history.sh` reports M2's row blank where every other epic carries `code_quality`, `correctness` and `domain_spec_fidelity` averages. Exactly one scorecard in the milestone (Task 5, iteration 2) carried real dimension scores.

Two consequences, and the second is the dangerous one.

The invariant is that an explicitly-empty `dimensions` object signals single-pass convergence, which is deliberate and documented. But applied to a whole milestone it means **21 beans converged without dimensional scrutiny**, and the decay-detection machinery this file exists to feed received nothing from the largest epic in the history.

And the trend numbers now mislead. M2 shows the **lowest dispatches-per-task (2.86) and lowest iterations (1.19) of any recorded epic** — which reads as the most efficient milestone ever run, and is in fact an artifact of single-pass convergence. A future reader comparing epics on those two columns would conclude M2 went unusually smoothly. It did not: nine planning defects, two blind spots in the mandatory proof, five remediation beans after a failed holistic review. The efficiency is in the convergence *rule*, not the work.

This is not an anchor and corrects no score — there are no scores to correct. It is here because this file is what future evaluators read, and the thing worth carrying forward is: **an evidence-only scorecard buys a fast convergence at the price of contributing nothing to calibration.** A milestone that uses it exclusively should say so at delivery, which M2 is doing here.

Also recorded: the **blind spot-check was deliberately skipped** for M2 at the operator's instruction. Four beans were sampled at the default rate of 5 — `fiddle-amol` (the mandatory proof), `fiddle-swgf`, `fiddle-6o8w` and `fiddle-ufv3` — and none was blind-reviewed, so the divergence between evaluator scoring and unanchored human judgment is unmeasured for this milestone. Combined with the absent dimension data, M2 contributes no calibration signal in either direction.
