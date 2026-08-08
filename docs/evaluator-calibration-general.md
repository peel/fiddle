# Evaluator Calibration — general

## all dimensions — Correction (2026-07-29)
**Evaluator scored:** 9/10 — dimension scores emitted on a doc-only diff (fiddle-qxyk iteration 1)
**Human corrected to:** n/a — "there's no code here; nothing to score"
**Anchor:** For this project, doc-only and markdown-only beans (thresholds {}) get NO judgment dimension scores: emit "dimensions": {} and let evidence-backed criteria verdicts gate convergence. Dimension scoring on such beans is ceremony that adds double-pass cost without signal.

**Scope clarification (2026-08-07):** the trigger is *doc-only diff*, not `thresholds: {}`. A bean that changes Rust, Nix, or workflow files still receives dimension scores even though its eval block declares `thresholds: {}`. Every M0 bean below is a code bean.

---

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
