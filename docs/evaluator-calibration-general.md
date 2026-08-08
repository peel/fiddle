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

**The scope rule for this milestone, stated once.** Model output quality is nondeterministic and is
never the deterministic gate. Every criterion below is scored against what the *deterministic shell*
does with the model's output — the bounds it enforces, the checks it runs, the evidence it derives
independently — and never against how good the model's repair was. An evaluator that rewards a
better-worded agent summary, or penalises a scripted fixture for being easy, has scored the wrong
thing. Equally, a criterion is never met by asserting the model said it succeeded.

### `m1-tool-protocol-correctness`

The prompt, the advertised tool schemas, and what reaches a tool.

- **Poor (1–3).** A trusted value — workspace root, cancellation token, effect executor, or anything
  credential-bearing — appears in a tool's `Args` and is therefore model-visible. Tool schemas accept
  absolute paths or unbounded arguments. The prompt or a tool result carries a resolved secret. Tools
  are registered but nothing asserts which ones the model was actually offered.
- **Acceptable (4–7).** Trusted values reach tools only through Rig's host-only `ToolContext` via
  `context.require::<T>()`; `Args` carry relative paths and bounded values only. A test serializes the
  model-visible request and asserts that the advertised schema contains no absolute path, no host
  handle, and no credential, and that the offered tool set is exactly the capability's four tools. A
  call to an unregistered tool name is rejected rather than dispatched.
- **Excellent (8–10).** All of the above, plus the assertion is made against the *serialized outbound
  request* rather than against the builder that produced it, so a future Rig change that starts
  leaking context into arguments fails the test rather than passing it. The absence of host-only
  values is asserted positively (the serialized prompt, messages, and tool arguments are searched for
  the workspace root and for the credential variable's value) rather than inferred from the type.

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

Turn, attempt, time, and mutation limits, and cancellation.

- **Poor (1–3).** Only one bound exists, or the inner turn limit is enforced by a hand-rolled counter
  rather than by the runtime. Cancellation is assumed to follow from dropping a future. A bound is
  configured but nothing asserts it fires.
- **Acceptable (4–7).** Two independent bounds — an outer per-capability attempt limit owned by
  `fiddle-runtime` and an inner per-attempt turn limit enforced by Rig — plus a wall-clock deadline
  and a files-changed cap. Each has a test that drives it past the limit and asserts the specific
  error: a scripted model with more tool calls than `max_turns` yields Rig's `MaxTurnsError`. A
  cancellation token is passed into every tool and the check runner and is checked before mutation.
- **Excellent (8–10).** All of the above, plus a cancellation test that cancels *between inspection
  and mutation* and asserts the workspace is unmutated afterwards — proving cancellation prevents an
  effect rather than merely ending a future — and the cancelled attempt's outcome is shown to come
  from M0's existing post-execution re-derivation, adding no row to the exit-code table.

### `m1-workspace-isolation`

The ephemeral workspace, path validation, and environment sanitization.

- **Poor (1–3).** Path containment is a `starts_with(workspace_root)` check. `std::env::remove_var`
  is used to strip credentials, mutating the host process. The workspace outlives the attempt, or its
  teardown is skipped on the failure path. Build artefacts pollute the changed-file evidence.
- **Acceptable (4–7).** A per-attempt `git worktree add --detach`, removed after evidence capture on
  every path including failure. Paths are normalized, the deepest existing ancestor resolved, and
  `..`, absolute paths, NUL bytes, and platform prefixes rejected. Workspace commands run under
  `Command::env_clear()` with an explicit `HOME`/`PATH`/`LANG` allowlist. The fixture repository
  gitignores `target/`, so `git status --porcelain` reports source changes only.
- **Excellent (8–10).** All of the above, plus a symlink-escape case is a committed test — a symlink
  inside the workspace pointing outside it is refused for both read and write, not merely for a path
  containing `..` — and a test asserts that no credential-shaped variable survives into a workspace
  command's environment, by running a command that dumps its own environment and searching the
  output. ADR 011 exists because M0 shipped a path derived from an unvalidated value; the same class
  of defect is checked here rather than assumed absent.

### `m1-fixture-repair-acceptance`

The credential-free black-box proof.

- **Poor (1–3).** The lane calls library functions instead of launching the compiled binary. It
  requires a credential, or a network call, to pass. The repair is asserted from the agent's response
  rather than from the fixture on disk. M0's `m0_skeleton` lane was modified to accommodate M1.
- **Acceptable (4–7).** `cargo test -p fiddle-acceptance --test m1_bounded_capability` exits 0,
  driving the compiled binary as a subprocess over a deliberately broken zero-dependency Rust crate.
  The repair is asserted by running the configured check (`cargo test --offline`, which fails before
  and passes after) and by reading `git status --porcelain` in the worktree — both independent of the
  model's response. The lane passes with `ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, `GH_TOKEN`, and
  `JIRA_API_TOKEN` removed. `m0_skeleton` still exits 0, unmodified.
- **Excellent (8–10).** All of the above, plus the adversarial cases ride in the same lane rather
  than only in unit tests: `../outside.txt`, an absolute path, a symlink pointing out, an
  unregistered tool name, malformed structured output, mutation past the files-changed cap, and
  cancellation mid-attempt. The fixture repository is created by the harness with an explicit
  `-c user.email` / `-c user.name`, so the lane does not depend on the runner having a git identity.

### `m1-canary-evidence`

The scheduled real-model run, and what it reports when it cannot run.

The model provider is a **LiteLLM OpenAI-compatible gateway**, not Anthropic directly
(design §2.4). Two facts follow, and both are scored here. The gateway translates OpenAI
function-calling shape to the upstream provider, so the canary is the *only* proof that tool
calls survive that translation — the deterministic lane replaces the provider with
`MockCompletionModel` and never serializes a request to anyone. And the key carries a **$100
hard cap**, so requests begin failing on spend rather than on correctness.

- **Poor (1–3).** The canary gates merges, or fails the build when the credential is absent, or —
  worst — reports success without having called a model. Its output is unstructured log text. It
  records only pass/fail and a duration. A spent budget is reported as a capability failure.
- **Acceptable (4–7).** A scheduled workflow separate from the merge gate, carrying
  `workflow_dispatch` so it is runnable before it reaches the default branch. With
  `LITELLM_API_KEY` absent it emits a machine-readable
  `{"status":"degraded","reason":"missing_LITELLM_API_KEY","canary_exercised":false}` and does not
  fail the deterministic gate. When exercised it records runtime, rig version, toolchain, gateway
  base URL, model identifier, prompt and capability revision, fixture revision, configured limits,
  outcome classification, deterministic check results, stop reason, input/output token counts, and
  latency.
- **Excellent (8–10).** All of the above, plus `canary_exercised: false` is asserted by a committed
  test rather than left to the workflow's runtime behaviour; and `budget_exhausted`, `auth`,
  `quota`, `timeout`, `provider` and `capability` are distinguishable classes in the emitted
  payload rather than one `failed` — an exhausted budget that reads as a broken capability is the
  specific defect this anchor exists to prevent. No raw prompt, tool result, or repository content
  is persisted; content is carried by digest.

### Known-blocked criteria

None for M1's provider surface. The gateway credential exists locally, and the tool-call and
structured-output round trips were both proven live against `claude-sonnet-4-6` during the seed
(design §2.4). Two narrower notes:

- The **scheduled** canary cannot fire until M1 reaches the default branch, because GitHub runs
  `schedule` triggers only from the default branch's workflow file. Score the canary through
  `workflow_dispatch` and do not treat an unfired schedule as a defect.
- A **repository secret** for `LITELLM_API_KEY` may not exist when a bean is evaluated
  (`gh secret list --repo peel/fiddle --json name` returned `[]` at baseline). That gates the
  CI-exercised path only, never the local one; score it against the degraded path and record the
  CI path as blocked rather than redefining the criterion.

Model names come from the gateway, not from the RFC: the RFC's `claude-sonnet-4-5` is not
available there. A bean that hardcodes an RFC model name has a runtime defect, not a style
problem.
