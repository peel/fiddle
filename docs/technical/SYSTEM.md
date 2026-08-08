# System

## Overview

Fiddle is a portable Agent Skills library that orchestrates a four-phase development lifecycle (DISCOVER, DEFINE, DEVELOP, DELIVER) with optional multi-model support. It ships one canonical `skills/` tree plus thin Claude, Codex, and Pi manifests. External providers (Codex via CLI, Gemini via CLI) participate in debate and review phases but are optional — skills degrade to the current harness when providers are unavailable.

## Components

**Orchestrate** (`skills/orchestrate/SKILL.md`) — Top-level lifecycle coordinator. Its primary skill is a router; configuration and resumption details load from focused references. External provider calls go through `hooks/dispatch-provider.sh`.

**Panel** (`skills/panel/SKILL.md`) — Structured multi-model adversarial analysis. The current harness, Codex, and Gemini argue independent positions, cross-review, then the lead synthesizes a verdict. External providers are called via `hooks/dispatch-provider.sh`. Degrades to current-harness analysis when no external providers are available.

**Develop** (`skills/develop/SKILL.md`) — Thin orchestrator for the implementation phase. Validates bean bodies (eval block, files, steps checklist required), then delegates to sub-skills: `develop-loop` (`skills/develop-loop/SKILL.md`) handles per-task iteration for one bean at a time — implement, gather a per-domain evidence pack (tests, checks, runtime probes), dispatch ONE evaluator per domain (provider chosen by `scripts/select-evaluator-provider.sh` from the domain's ordered preference list, first available provider differing from the always-claude implementer), normalize the single scorecard, and converge via scripts; evidence-only scorecards (explicit empty dimensions) converge on a single pass. `develop-holistic` (`skills/develop-holistic/SKILL.md`) handles cross-domain integration review with remediation and keeps multi-provider dispatch with min-merge. All evaluation state tracked via beans and eval-log scripts.

**Using Fiddle** (`skills/using-fiddle/SKILL.md`) — Bootstrap skill for routing common requests, mapping Claude-style tool vocabulary across Claude, Codex, and Pi, and resolving internal subagent models.

**Hooks** (`hooks/`) — Claude-oriented hooks check provider binaries, add code-navigation guidance, guard archives, and report progress. `develop-verdict-gate.sh` is a Stop hook that blocks turn-end while `.fiddle/active-bean` names a develop-loop bean without a terminal verdict (fail-open when the marker is absent or jq is missing). Codex has a minimal `.codex/hooks.json`. Pi support in v1 is skill/package discovery, not hook parity.

**Challenge** (`skills/challenge/SKILL.md`) — Decision-tree interrogation skill. Walks every branch of a plan or design until shared understanding is reached. Phase-aware: in DISCOVER, opens by synthesizing findings and confirming scope; in DEFINE, challenges design edge cases and panel dissent. Also usable standalone.

**Supporting skills** — `fiddle:discover-docs` (project context scan), `fiddle:deliver-docs` (post-ship doc updates), `fiddle:define-beans` (task sizing), `fiddle:adr`/`fiddle:feedback`/`fiddle:backlog` (append-only records).

**`fiddle` CLI** (`crates/`) — The agentic factory binary M0 builds. Four crates with a hard ownership boundary: `fiddle-core` is the pure domain (identity, observation, assessment, outcome, report types) and reaches for no process, filesystem, network, environment or clock; `fiddle-runtime` owns every effect (ports, stub adapters, the `stub_mark` capability, orchestration, the attempt journal, evidence publication); `fiddle-cli` owns argument handling, rendering and the single exit-code mapping; `fiddle-acceptance` drives the compiled binary as a subprocess. Commands: `--version`, `config check`, `inspect`, `run`. The boundary is enforced mechanically, not by review — see Invariants.

**Skill quality tooling** (`scripts/audit-skills.sh`, `scripts/check-portability.sh`) — Validates portable skill metadata, reachable companion documentation, primary-skill size, and optionally trigger-first descriptions. `skill-quality.yml` runs these checks and their fixtures in CI.

## Data

**`orchestrate.json`** (JSON) — Declares external provider participation, evaluator settings, plans, and internal subagent models. `models.roles.<role>` overrides `models.phases.<phase>`; `default` inherits the current session model. External provider CLI selection is independent. Merge order is defaults, config file, then CLI flags.

**Report bundle** (`<report.dir>/<invocation-slug>/<attempt-id>/report.json`) — What a `fiddle run` publishes as evidence: `schema` `fiddle.report.v0`, the build identity (package version and a 40-hex source revision or the literal `unknown`), the invocation and work refs, the attempt id, mode, outcome, next action, capability executions, progress, and the full observations view. Staged in a temporary directory and moved by rename, so a reader never observes a partial bundle; the staging directory is removed by a `Drop` guard on every failure path.

**Attempt journal** (`<report.dir>/.attempts/`) — Records an attempt's intent *before* its capability mutates anything, so an attempt interrupted between effect and publication is detectable afterwards rather than indistinguishable from one that never ran. If the journal cannot be written the capability does not run at all.

**Bean state** — Managed by external `beans` CLI. Epics, tasks, tags (worktree slots, CI retries, stall respawns, needs-attention). Beans are the unit of work for develop and swarm.

## Infrastructure

Runs entirely locally as portable skills. Claude loads `.claude-plugin/plugin.json`, Codex loads `.codex-plugin/plugin.json`, and Pi reads `package.json` with `pi.skills`. Requires bash and jq for helper scripts. External providers (codex, gemini) are optional local CLIs.

**Rust workspace gate** (`.github/workflows/rust.yml`) — The `fiddle` binary's Cargo workspace (`crates/fiddle-core`, `fiddle-runtime`, `fiddle-cli`, `fiddle-acceptance`) is gated by `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, and `cargo build --release`, run against the channel pinned by `rust-toolchain.toml`. `nix flake check` stays a local-only developer gate because `inputs.ai-devtools` is a machine-local `path:` input no runner can resolve. Locally, prefix each command with `nix develop -c`.

**M0 acceptance command** — The M0 milestone's proof is one cumulative black-box scenario driven through the compiled `fiddle` binary as a subprocess — config check, inspect, run, bundle assertion, and a second fresh invocation, in that order, sharing one temporary fixture project:

```
cargo test -p fiddle-acceptance --test m0_skeleton -- --nocapture
```

`.github/workflows/rust.yml` runs exactly that command as the named step *M0 acceptance scenario (credential-free black-box)*. It needs no credentials and no external repository: the scenario removes `GITHUB_TOKEN`, `GH_TOKEN`, `ANTHROPIC_API_KEY`, and `JIRA_API_TOKEN` from every subprocess it launches, then re-supplies them once to show the behaviour is identical either way. Later milestones run this command unchanged as their regression baseline; locally, prefix it with `nix develop -c`.


## Invariants

- Skills must degrade gracefully when external providers are unavailable. Never fail solely because a provider is missing — fall back to the current harness.
- Hooks must exit 0 on success or non-applicable scenarios. Exit 2 to reject with feedback (task-completed-verify pattern).
- Provider calls use `hooks/dispatch-provider.sh` — never inline provider CLI prompts in skill files.
- External provider calls run in parallel when the harness supports it; otherwise run sequentially and report reduced coverage.
- Append-only docs (FEEDBACK, BACKLOG, research logs) are never edited or deleted.
- Bean bodies must be self-contained — implementer agents work from the bean body alone without reading plan files.
- Design specs in `docs/specs/` and implementation plans in `docs/plans/` are local lifecycle artifacts and remain gitignored; bean bodies carry the durable executable contract.
- Worktree agents must route all bean CLI operations through `--beans-path` to the main checkout's `.beans/`. Only the lead manages bean status transitions.
- Evaluators interpret pre-gathered evidence packs; they never gather evidence themselves. Read-only external providers receive the pack via `dispatch-provider.sh --evidence-file`.
- Evidence-only scorecards emit an explicit `"dimensions": {}` — the key is never omitted; only the explicitly empty object signals single-pass convergence.
- Every scorecard must carry a criteria array; `merge-scorecards.sh` rejects criteria-less input with exit 2.
- Holistic reviewers use the canonical scorecard envelope. `merge-scorecards.sh` conservatively merges `spec_coverage_matrix` and deduplicates `remediation_beans` by requirement while retaining source providers.
- Dispatch budgets govern whether another dispatch may start. `check-convergence.sh` first accepts terminal results from the final allowed dispatch, then reports DISPATCHES_EXCEEDED when a nonterminal result would require more work.
- Skills are written as judgment plus rationale. Mechanical invariants live in scripts with exit-code contracts, not in prose, and skill files carry no emphatic markup (gate blocks, capitalized emphasis, rationalization tables, red-flag lists, announcement lines). Frontmatter `description` fields, JSON schemas, and quoted external content are the exceptions, since they are interface text rather than instruction. See the authoring note in `skills/using-fiddle/SKILL.md`.
- Internal subagent models resolve through `scripts/resolve-subagent-model.sh`: a role override wins over a phase default, while `default` omits an explicit model and inherits the session. Provider CLI selection never flows through this resolver.
- `scripts/audit-skills.sh` returns exit 2 with JSON errors for malformed metadata, missing references, orphaned companions, or configured primary-skill size violations.
- Acceptance tests launch the compiled `fiddle` binary as a subprocess and observe only its exit code, its `--json` payload, or a file it wrote; they never call library functions directly.
- `fiddle-core` stays pure, enforced two ways rather than by review: a `cargo metadata` walk of its full resolved closure fails on `tokio`, `rig-core`, `reqwest`, `hyper` or `mio`, and a source grep fails on `std::process`, `std::fs`, `std::net`, `std::env`, `SystemTime::now` or `Instant::now` — including inside comments.
- An invocation reference value is constrained at the parse boundary to ASCII letters, digits, `-`, `_` and `:`. Every path `fiddle` derives comes from that value, so validating once at parse is what keeps the bundle, the journal and the stub reads inside their configured roots; an invalid value exits 2 before any filesystem access.
- An attempt's intent is journaled before its capability mutates anything, and a capability whose intent could not be recorded does not run.
- A run's outcome is derived from its post-execution re-derivation, never assumed from the fact that a capability executed. `Complete` maps to `Completed`, `Blocked` to `Failed`, `Execute` to `Retryable`.
- Acceptance tests resolve the binary under test through `support::fiddle_binary()`, which builds it and takes the path cargo reports. `harness_discipline.rs` fails if any acceptance source names `cargo_bin`, because a lane that resolves a path by convention silently tests whatever the last build left.
- The M0 acceptance command (`cargo test -p fiddle-acceptance --test m0_skeleton -- --nocapture`) must stay credential-free and green. The milestone lane is never gated on a secret or an external repository, and later milestone seeds run this exact command as their baseline.

## Known issues

- Three permission-injection tests in `crates/fiddle-runtime/tests/attempt.rs` return early under an identity that ignores permission bits (root), so on a root CI runner they no-op silently instead of skipping visibly.
- Parity between the in-repo and external acceptance lanes is maintained by hand. `docs/technical/acceptance-repository.md` states they assert the same properties; nothing mechanically checks it, and the two have already drifted once.

---
Last reviewed: 2026-08-08
