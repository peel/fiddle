# System

## Overview

Fiddle is two things that ship together. A portable Agent Skills library runs a
four-phase lifecycle: DISCOVER, DEFINE, DEVELOP, DELIVER. A Rust binary, `fiddle`,
runs one bounded agentic attempt against a repository and publishes evidence.

One canonical `skills/` tree serves Claude, Codex and Pi through thin manifests.
External providers are optional. A skill falls back to the current harness.

## Components

Paths are relative to the repository root. A skill path omits `/SKILL.md`.

| name | path | what it does | invariant |
| --- | --- | --- | --- |
| orchestrate | `skills/orchestrate` | routes the lifecycle | a provider call goes through `hooks/dispatch-provider.sh` |
| panel | `skills/panel` | multi-model adversarial analysis | degrades to the current harness alone |
| develop | `skills/develop` | validates a bean body, then delegates | a body without an eval block, files and steps does not run |
| develop-loop | `skills/develop-loop` | one bean: implement, pack evidence, evaluate | one evaluator per domain, never the implementer (ADR 007) |
| develop-holistic | `skills/develop-holistic` | cross-domain review and remediation beans | scorecards merge by minimum |
| challenge | `skills/challenge` | walks every branch of a plan | phase-aware, also standalone |
| using-fiddle | `skills/using-fiddle` | routes requests, maps tool vocabulary | one text serves every harness (ADR 008) |
| hooks | `hooks/` | check binaries, guard archives, gate a verdict | exit 0, or exit 2 to reject with feedback |
| validators | `scripts/audit-skills.sh`, `check-portability.sh`, `check-github-effects-lane.sh` | hold repository shape mechanically (ADR 009) | exit 2 with a JSON error array |
| ADR citations | `scripts/check-adr-cites.sh` | every `Cites:` symbol and path an ADR names still resolves | a retrofit floor at 021, and `Cites: none` is the deliberate answer |
| `fiddle-core` | `crates/fiddle-core` | the pure domain: identity, assessment, outcome, report | reaches no process, file, socket, environment or clock (ADR 035) |
| `fiddle-runtime` | `crates/fiddle-runtime` | every effect: ports, adapters, capabilities, journal | the only crate that imports Rig |
| `fiddle-cli` | `crates/fiddle-cli` | arguments, configuration, rendering, the exit code | `main.rs::exit_code_for` is the single mapping |
| `fiddle-acceptance` | `crates/fiddle-acceptance` | drives the compiled binary as a subprocess | observes an exit code, a `--json` payload, or a written file |
| workspace | `fiddle-runtime/src/workspace` | a detached worktree per attempt | a `Drop` guard removes it on every path |
| agent | `fiddle-runtime/src/agent` | four tools, a fifth where a deployment declares a program, one bounded Rig attempt | the tool surface carries no host fact (ADR 034); a program an attempt runs is one the deployment declared (ADR 043) |
| gateway | `fiddle-runtime/src/gateway.rs` | the one credential-carrying model | an OpenAI-compatible gateway, not Anthropic (ADR 012) |
| effect executor | `fiddle-runtime/src/effect` | the seven-step authorization order | no mutation without an `AuthorizedEffect` (ADR 033) |
| forge adapter | `fiddle-runtime/src/{github,git}` | the one `gh` and the one `git push` | `gh api -i`, not a REST client (ADR 015) |
| scanner | `fiddle-runtime/src/scanner` | runs `wizcli`, records version and digest | the digest, never the tag it was handed (ADR 020); fiddle passes the scanner no credential (ADR 042) |
| sweep | `fiddle-runtime/src/{cve,capability/mitigate.rs}` | select findings, one attempt, rescan | all or nothing: one finding left standing sends the whole attempt to a draft nobody merges |
| release | `.github/workflows/release.yml` | builds the `linux-amd64` binary on a `v*` tag | the SHA256 file ships beside the binary |

Five capabilities are registered: `stub_mark`, `fixture_repair`, `publish_change`, `propose_change` and `cve_mitigate`. Each names its own progress stage, so a bundle is labelled in the vocabulary of whatever ran. `crates/fiddle-acceptance/tests/capability_selection.rs` reads the list `--capability` validates against out of the binary's own diagnostic, and requires this line to name every id and to state their number.

`inspect` and `run` share one selection flag and one default, so the read-only
command cannot name a capability the executing one would refuse. `inspect` takes the
id as far as the derivation and builds nothing from it.

| outcome | exit | reached by |
| --- | --- | --- |
| `Completed` | 0 | re-derivation reaches `Complete`, or `Execute` over a reference with no completion state (ADR 023) |
| `Suspended` | 10 | nothing yet; M3's attended decision is the first |
| `Retryable` | 11 | an obstacle in front of the request |
| `Failed` | 20 | a conclusion about the request (ADR 016) |
| — | 2 | usage or invalid input, before a run begins |

`CapabilityError::recurrence` picks the row per failure, delegating to
`EffectError::recurrence`. Both match exhaustively with no wildcard. Ten failures
share rows 11 and 20, and only the reason text separates them; `docs/BACKLOG.md`
records that gap.

## Data

| name | path | what it holds |
| --- | --- | --- |
| `orchestrate.json` | repository root | provider participation, evaluator settings, plans, subagent models |
| report bundle | `<report.dir>/<slug>/<attempt-id>/report.json` | schema `fiddle.report.v0`, build identity, refs, outcome, executions, observations |
| verdict report | `<report.dir>/verdicts.json` | one row per finding this run could not patch, read by the host's remediation gate |
| complete findings | `<report.dir>/findings.json` | the whole projection the scan produced, with its count and the two array arms |
| attempt journal | `<report.dir>/.attempts/` | one intent record per attempt, then one `"effect_step"` line per executor step |
| bean state | `.beans/`, read by the external `beans` CLI | epics, tasks and tags; the unit of work for develop |

**`fiddle.toml`** (TOML, `deny_unknown_fields`) — the deployment document. `[project]`, `[stub]` and `[report]` are required. `[agent]`, `[workspace]`, `[github]` and `[scanner]` are optional, and so is `[orchestration.cve]`. An absent table describes a deployment that does not do that work; it is never a blank filled in silently. `[agent]` and `[workspace]` bound one attempt, name the checks that decide it, and name the programs it may run. `[github]` and `[scanner]` are the second and third credential-carrying tables. `[orchestration.cve]` names the `image`, the `severities` and the `max_findings` a sweep acts on, and those three keys are the whole table. `crates/fiddle-acceptance/tests/config_check.rs` requires this line to name every table the schema admits.

`[github.policy]` and `[github.read_retry]` are strict tables of their own, because
`deny_unknown_fields` on a parent does not reach a child. A mistyped `attempt = 8`
would otherwise be a bound an operator believes they set.

`agent.max_capability_attempts` bounds one pull request's rework, counted in its
body (ADR 037).
`github.required_checks` is observed and enforced by nothing (ADR 017).
`[orchestration.cve] go` was deleted in M4c and is now refused at load; see
`RUNBOOKS.md`.

## Infrastructure

Everything runs locally. Claude reads `.claude-plugin/plugin.json`, Codex reads
`.codex-plugin/plugin.json`, and Pi reads `package.json`'s `pi.skills`. Helper
scripts need bash and jq. This project's `orchestrate.json` dispatches to codex
alone; gemini was removed after two consecutive authentication failures.

| lane | command | gates |
| --- | --- | --- |
| the accumulated gate | `scripts/gate.sh --full` | yes |
| Rust workspace | `.github/workflows/rust.yml`: fmt, clippy `-D warnings`, test, release build | yes |
| repository shape | `.github/workflows/skill-quality.yml`: the validators above | yes |
| M0 acceptance | `cargo test -p fiddle-acceptance --test m0_skeleton -- --nocapture` | yes |
| real-model tiers | `cargo test -p fiddle-cli -- --ignored`, `scripts/tier2.sh` | no |
| live forge | `scripts/live-github.sh` | no |
| dispatched forge | `gh workflow run github-effects.yml` | no |

`nix flake check` stays a local gate: `inputs.ai-devtools` is a machine-local
`path:` input no runner resolves. Prefix a local command with `nix develop -c`.
`RUNBOOKS.md` holds the credentials and the operating detail.

## Invariants

- A skill degrades to the current harness. It never fails only because a provider is missing.
- Provider calls run in parallel where the harness allows it, and report reduced coverage where it does not.
- An append-only record grows at the end. A finding's text is never rewritten and no entry is deleted. One `Status:` line may be added in place; every other change is a new entry naming the one it acts on.
- A bean body is self-contained. An implementer works from it without reading a plan file.
- `docs/specs/` and `docs/plans/` stay gitignored. The bean body carries the durable contract.
- A worktree agent routes every bean call through `--beans-path`. Only the lead changes bean status (ADR 002).
- An evaluator interprets a pre-gathered evidence pack and gathers nothing itself (ADR 007).
- An evidence-only scorecard emits an explicit `"dimensions": {}`. The key is never omitted.
- Every scorecard carries a criteria array; `merge-scorecards.sh` rejects one without it.
- `check-convergence.sh` accepts a terminal result from the final allowed dispatch before it reports `DISPATCHES_EXCEEDED`.
- A subagent model resolves through `scripts/resolve-subagent-model.sh`. A role override wins over a phase default, and `default` inherits the session.
- The code carries no comments (ADR 024).
- An invocation reference value is ASCII letters, digits, `-`, `_` and `:`, validated at the parse boundary (ADR 011).
- An absent `--capability` resolves through the scheme, matched arm by arm with no wildcard (ADR 022).
- An attempt's intent is journaled before its capability mutates anything. A capability whose intent could not be recorded does not run (ADR 010).
- A run's outcome comes from its post-execution re-derivation, never from the fact that a capability ran.
- A capability's outcome is decided by the check it runs over the tree the attempt left. `RepairReport::claimed_complete` is evidence and is branched on nowhere.
- The correlation marker is written only after the check exits 0.
- A report bundle is staged, then moved by rename, so a reader never observes a partial bundle.
- Every external mutation passes the effect executor. Deployment policy may strengthen a capability's minimum and never weaken it (ADR 033).
- An unknown answer is resolved by reading the world, never by repeating the write (ADR 032).
- A locator may be inherited, an authority may not. Three spawn sites keep three environments and share one bound, `process.rs::run_bounded` (ADR 029).
- Containment is checked syntactically, then against the resolved path. `.git` is refused at any depth (ADR 031).
- What counts as a change comes from the committed ignore rules, snapshotted before the attempt (ADR 030).
- The workspace supplies a scratch `HOME` beside the worktree, never inside it.
- The model-visible surface carries no host fact, and the runtime records each tool receipt itself (ADR 034).
- An attempt runs a program only where the deployment declared it, as a program and an argument list with no interpreter, bounded by `[workspace] command_timeout` (ADR 043). The declaration's arguments are a prefix; whether the model may append to them is the declaration's own answer, and the default is no. This is a legibility rule, not isolation: see Known issues.
- A mitigation attempt's diff touches exactly the files it declared, and Rust reads no meaning into a path (ADR 026). Nothing pre-filters an already-fixed finding.
- Fiddle scans an image it did not build, and the bundle pairs the digest with the revision (ADR 020).
- A `workflow_dispatch` lane on a feature branch is inert until its file reaches the default branch. `.github/workflows/github-effects.yml` carries the diagnosis.
- The check runs in its own process group, so `fiddle run` installs a `SIGINT` handler. The first interrupt cancels the token; the second exits 130.
- The deterministic suite gates. A real-model lane is opt-in, needs a credential, and never asserts that the model succeeded.
- The M0 acceptance command stays credential-free and green. Every later milestone runs it unchanged.
- An acceptance lane resolves its binary through `support::fiddle_binary()` (ADR 035).

## Known issues

- Three permission-injection tests in `crates/fiddle-runtime/tests/attempt.rs` return early under an identity that ignores permission bits. On a root runner they no-op instead of skipping visibly.
- Parity between the in-repo and external acceptance lanes is kept by hand. Nothing checks it and the two have drifted once. See `docs/technical/acceptance-repository.md`.
- M2's exactly-once proof rests on one test. Only `an_ambiguous_write_then_a_fresh_process_leaves_exactly_one_of_each` fails under an inversion that lets the mutation retry.
- The dispatch locator's round trip is checked by no gating test. `github::checks::run_name` spells one half and `fiddle-check.yml` in the disposable repository spells the other.
- `.github/workflows/github-effects.yml` is inert until it merges to the default branch. One dispatch after the merge closes it.
- A forbidden test edit is a deployment's check now, not a guarantee this binary makes. A deployment that declares no test check gets no warning (ADR 026).
- `[[workspace.checks]]` constrains neither the count nor the order nor which programs appear. No lane exercises a real `docker build`.
- **A declared program is not sandboxed, and `[[workspace.commands]]` is not a security boundary (ADR 043).** It runs as an ordinary process with the invoking user's identity: it can read any file that user can read, write any file that user can write, and open any socket. Declaring one build tool grants arbitrary code execution, because `go test` runs code out of the repository under repair and `go generate` runs whatever the source names. What does hold is four things and no more — a per-attempt worktree as the working directory, no credential in the child, `WorkspacePath` bounding fiddle's own file tools, and a time bound that is cancellable. On a GitHub runner the disposable job supplies the isolation; local attended execution (the PRD's M6) is where the exposure is real. `Isolation` has one variant and the seam for a sandbox is empty; ADR 043 says what a second variant would need.
- The attempt now has an egress path it directs itself, where before this it had none: a declared dependency tool fetches from a registry (ADR 043). A sandbox cannot answer "no network" without breaking the repair the tool exists for.
- No check verifies that a document citing a symbol names one that exists. Two lanes pin prose in this file, and both pin enumerations the binary also prints.

---
Last reviewed: 2026-08-20
