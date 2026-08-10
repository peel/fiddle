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

**`fiddle` CLI** (`crates/`) — The agentic factory binary. Four crates with a hard ownership boundary: `fiddle-core` is the pure domain (identity, observation, assessment, outcome, report types) and reaches for no process, filesystem, network, environment or clock; `fiddle-runtime` owns every effect (ports, stub adapters, capabilities, orchestration, the attempt journal, evidence publication, and every Rig import); `fiddle-cli` owns argument handling, configuration, rendering and the single exit-code mapping; `fiddle-acceptance` drives the compiled binary as a subprocess. Commands: `--version`, `config check`, `inspect [--capability ID]`, `run [--capability ID]`. Both take the same selection flag with the same default, so the read-only command can never name a capability the executing one would not run; `inspect` takes the id only as far as the derivation and builds nothing from it, which is what keeps it read-only and credential-free for every value of the flag. Two capabilities are registered: M0's `stub_mark` and M1's `fixture_repair`, each naming its own progress stage (`mark`, `repair`) so a published bundle is labelled in the vocabulary of whatever ran. The boundary is enforced mechanically, not by review — see Invariants.

**Bounded agentic capability** (`crates/fiddle-runtime/src/{workspace,agent,capability,gateway}`) — What M1 builds, in one chain. `workspace` gives an attempt a detached git worktree of the repository under repair, created per attempt and removed by a `Drop` guard; `workspace::path` turns a model-supplied string into a `WorkspacePath` that by construction names something inside it and is not the repository's own metadata, and `Workspace::resolve` walks it one component at a time to the deepest *existing* ancestor, canonicalizing every component that is there and refusing any that resolves outside — so a write into a directory the project does not have yet succeeds, with the intervening directories made by the workspace and re-proven contained before anything is written through them; `workspace::command` runs programs under `env_clear` plus the four-name allowlist stated in Invariants, in their own process group, with a timeout and a cancellation token; `workspace::changes` derives what actually changed in two halves — tracked changes from `git status --porcelain=v1 -z -uno`, created files from `git ls-files --others` under the project's ignore rules *as committed at the branched HEAD*, snapshotted outside the worktree before the attempt begins. `agent::tools` is the whole model-visible surface — `read_file`, `write_file`, `list_files`, `run_check` — each taking its host facts from Rig's `ToolContext` rather than from its arguments, and each recording its own receipt. `agent::attempt` assembles one bounded Rig run (turns, tokens, deadline, changed-file cap, per-tool timeout) and returns a typed `RepairReport`. `capability::repair` (`fixture_repair`) runs the configured check over whatever tree the attempt left behind and decides the outcome from its exit code. `gateway` is the single construction of a credential-carrying model: an OpenAI-compatible client against a LiteLLM gateway rather than the RFC's Anthropic integration, for the reasons and with the consequences recorded in `decisions/012-openai-compatible-gateway.md`. Verification runs in three tiers — a deterministic offline suite that gates, and two opt-in real-model lanes that do not.

**Skill quality tooling** (`scripts/audit-skills.sh`, `scripts/check-portability.sh`) — Validates portable skill metadata, reachable companion documentation, primary-skill size, and optionally trigger-first descriptions. `skill-quality.yml` runs these checks and their fixtures in CI.

## Data

**`orchestrate.json`** (JSON) — Declares external provider participation, evaluator settings, plans, and internal subagent models. `models.roles.<role>` overrides `models.phases.<phase>`; `default` inherits the current session model. External provider CLI selection is independent. Merge order is defaults, config file, then CLI flags.

**Report bundle** (`<report.dir>/<invocation-slug>/<attempt-id>/report.json`) — What a `fiddle run` publishes as evidence: `schema` `fiddle.report.v0`, the build identity (package version and a 40-hex source revision or the literal `unknown`), the invocation and work refs, the attempt id, mode, outcome, next action, capability executions, progress, and the full observations view. Staged in a temporary directory and moved by rename, so a reader never observes a partial bundle; the staging directory is removed by a `Drop` guard on every failure path.

**`fiddle.toml`** (TOML, `deny_unknown_fields`) — The binary's deployment document. `[agent]` names `model`, `base_url`, `api_key = { env = "NAME" }` and the bounds that fire (`max_turns`, `max_tokens`, `max_changed_files`, `deadline`, `tool_timeout`), plus `max_capability_attempts`, which parses, defaults to 3 and **is consumed by nothing** — M1 ships one bound, for the reasons and at the price recorded in `decisions/013-one-attempt-bound-not-two.md`; `[workspace]` names `root`, `isolation`, `command_timeout`, `cleanup`, the `fixture` under repair and the `check = { program, args }` that decides its outcome. `api_key` deserializes only from `{ env = … }`, so a document holding a literal secret does not load.

**Attempt journal** (`<report.dir>/.attempts/`) — Records an attempt's intent *before* its capability mutates anything, so an attempt interrupted between effect and publication is detectable afterwards rather than indistinguishable from one that never ran. If the journal cannot be written the capability does not run at all.

**Bean state** — Managed by external `beans` CLI. Epics, tasks, tags (worktree slots, CI retries, stall respawns, needs-attention). Beans are the unit of work for develop and swarm.

## Infrastructure

Runs entirely locally as portable skills. Claude loads `.claude-plugin/plugin.json`, Codex loads `.codex-plugin/plugin.json`, and Pi reads `package.json` with `pi.skills`. Requires bash and jq for helper scripts. External providers (codex, gemini) are optional local CLIs; this project's own `orchestrate.json` currently dispatches to codex alone, gemini having been removed after failing auth in two consecutive plan critiques.

**Rust workspace gate** (`.github/workflows/rust.yml`) — The `fiddle` binary's Cargo workspace (`crates/fiddle-core`, `fiddle-runtime`, `fiddle-cli`, `fiddle-acceptance`) is gated by `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, and `cargo build --release`, run against the channel pinned by `rust-toolchain.toml`. `nix flake check` stays a local-only developer gate because `inputs.ai-devtools` is a machine-local `path:` input no runner can resolve. Locally, prefix each command with `nix develop -c`.

**M0 acceptance command** — The M0 milestone's proof is one cumulative black-box scenario driven through the compiled `fiddle` binary as a subprocess — config check, inspect, run, bundle assertion, and a second fresh invocation, in that order, sharing one temporary fixture project:

```
cargo test -p fiddle-acceptance --test m0_skeleton -- --nocapture
```

`.github/workflows/rust.yml` runs exactly that command as the named step *M0 acceptance scenario (credential-free black-box)*. It needs no credentials and no external repository: the scenario removes `GITHUB_TOKEN`, `GH_TOKEN`, `ANTHROPIC_API_KEY`, and `JIRA_API_TOKEN` from every subprocess it launches, then re-supplies them once to show the behaviour is identical either way. That four-name list is pinned by an assertion inside the scenario and mirrored in the external lane, so it is deliberately not extended per milestone. `LITELLM_API_KEY` is covered separately, by `capability_selection.rs`: it sets the variable to a sentinel and asserts the sentinel appears in no stdout, no diagnostic and no published bundle.

Later milestones run the M0 command unchanged as their regression baseline; locally, prefix it with `nix develop -c`.

**M1 acceptance commands** — M1's proof is **three files, not one lane.** The plan named a single `m1_bounded_capability` acceptance lane; it was never written, and the substitution is recorded here rather than only in the evaluator calibration, because what a milestone ships is a property of the system and not of how a bean is scored. All three are credential-free, offline, and part of the gate:

```
# the deterministic protocol suite: MockCompletionModel, the real tools, a real
# worktree, a real check — and every adversarial case (path escape, symlink,
# unregistered tool, malformed output, the file cap, turn exhaustion,
# cancellation mid-attempt), each asserting the fixture was left unmutated
cargo test -p fiddle-runtime --test repair_protocol

# the two black-box lanes, driving the compiled binary as a subprocess:
#   capability_selection.rs — selection, rejection and failure arms, plus the
#     LITELLM_API_KEY sentinel assertion and the SIGINT cancellation path
#   binary_repair.rs — a repair that succeeds, answered by a loopback stub
#     gateway speaking the OpenAI chat-completions wire format, so
#     `build_capability`'s document-to-capability wiring and the serialized
#     outbound request are both gated rather than left to Tier 1
cargo test -p fiddle-acceptance
```

`repair_protocol` is in-process by design: its claim is about the deterministic shell's response to a model input, not about the assembled binary, so its adversarial cases ride there rather than in a black-box lane. `m0_skeleton` runs unmodified alongside them as M1's regression baseline.

**M1 real-model tiers** — Two more lanes, neither of which gates.

```
# tier 1 — is the agent loop still wired up? one #[ignore]d test, real model, cheap
( set -a; . .env; set +a; cargo test -p fiddle-cli -- --ignored --nocapture )

# tier 2 — realistic fixtures against a real model, on demand only
( set -a; . .env; set +a; ./scripts/tier2.sh )
```

Both real-model lanes need `LITELLM_API_KEY` and cost money, so nothing in `.github/workflows` invokes either, and neither ever asserts that the model succeeded. Tier 1 fails loudly when the credential is absent rather than skipping. `FIDDLE_TIER{1,2}_MODEL` and `FIDDLE_TIER{1,2}_BASE_URL` point a run at another model or gateway; both default to `bedrock/moonshotai.kimi-k2.5`, for the measured reasons in ADR 012. Tier 2 writes one JSON artifact per fixture plus a `summary.json` under `target/tier2/`.


## Invariants

- Skills must degrade gracefully when external providers are unavailable. Never fail solely because a provider is missing — fall back to the current harness.
- Hooks must exit 0 on success or non-applicable scenarios. Exit 2 to reject with feedback (task-completed-verify pattern).
- Provider calls use `hooks/dispatch-provider.sh` — never inline provider CLI prompts in skill files.
- External provider calls run in parallel when the harness supports it; otherwise run sequentially and report reduced coverage.
- Append-only docs (FEEDBACK, BACKLOG, research logs) grow at the end. **An entry's finding text is never rewritten and no entry is ever deleted**, because the record of what was found is worth more than the tidiness of a list. One thing may be added to an entry in place and one only: a `Status:` line recording its resolution. Everything else — correcting a claim, superseding an action, closing a finding — is a new entry that names the one it acts on. `docs/BACKLOG.md`'s header states the same rule for its readers; this is the invariant it points at.
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
- A capability's outcome is decided by the check it runs itself, over the tree the attempt actually left behind. `RepairReport::claimed_complete` is recorded as evidence beside the exit code that overruled it and is branched on nowhere.
- The correlation marker is written only after the check exits 0. A repair that did not pass its check earns nothing, however confident the model was.
- **The workspace command environment, stated once.** A workspace command runs under `Command::env_clear()` and then an allowlist of exactly four names, and every other statement of it in this repository points here rather than restating it: `HOME`, set to the workspace's scratch home; `LANG`, set to the fixed value `C`; `PATH`, inherited from this process or `/usr/bin:/bin` when it has none; and `RUSTUP_HOME`, inherited **only when the parent has one** and absent otherwise. The rule that admits the last two is **a locator may be inherited, an authority may not** — `PATH` and `RUSTUP_HOME` say where a toolchain is, while `CARGO_HOME` and every credential say what may be done with it and are not passed. `crates/fiddle-runtime/src/workspace/command.rs` is the code and `workspace::a_workspace_command_inherits_no_credential` pins both shapes of the set exactly, so a fifth name cannot be added without changing that assertion.
- The workspace supplies a scratch `HOME` beside the worktree rather than inside it. A check run with `HOME` in the worktree writes caches into the very diff that is its evidence.
- Path containment is validated syntactically first — so it cannot be defeated by a race — and then re-checked against the canonicalized path, because only the filesystem knows where a symlink points. A **dangling** symlink is refused explicitly rather than falling through canonicalization's error path.
- Resolution walks the requested path one component at a time to the deepest *existing* ancestor, and the rule does not change with depth: every component that is there is canonicalized and refused if it resolves outside, whether it is the leaf or a directory halfway along. Below the first component that is absent there is nothing to follow, so the remaining names are joined onto a path already proven inside. This is what lets a model create a file in a directory the project does not have yet — the alternative was a bare `WorkspaceError::Io` reaching it as "writing the file did not succeed".
- Directories are made by the workspace on a write, never by the model working around a refusal, and they are proven contained by the same check the leaf gets: the parent is canonicalized *again* after `create_dir_all` and the leaf rebuilt on that proven path, so a directory the check would refuse is refused before anything is written through it.
- The workspace is removed by a `Drop` guard as well as by an explicit call. The explicit call exists so a teardown failure can be reported; the guard exists because an early return, a `?`, or a panic would otherwise leak a directory the next attempt collides with.
- The check runs in its own process group, so a timed-out `cargo test` reaps its test binaries rather than orphaning them — and therefore `fiddle run` installs a `SIGINT` handler, because `^C` no longer reaches the child on its own. First interrupt cancels the token; second exits 130.
- **The rules that decide what counts as the project are the project's own, as committed, never the worktree's current ones.** `.gitignore` is a versioned file an attempt can write, so `--exclude-standard` would let the thing being judged author the question: `*` written into it hides every created file, bypassing the changed-file cap and publishing a count that is not true. `--ignored` would answer that and lose what the exclusion is for, since one `run_check` writes a whole `target/` tree. So the ignore file is snapshotted from the branched HEAD before the attempt begins, kept outside the worktree, and named with `--exclude-from`. Nested ignore files and the operator's global excludes are deliberately not honoured — the error is towards reporting more, and one attempt's evidence does not depend on whose machine it ran on.
- A checkout holds three kinds of thing and only one of them is the project. The repository's own metadata (`.git`, at any path depth, case-insensitively) is refused by `WorkspacePath::parse` for reading and writing alike — in a linked worktree it is a *file* whose contents are an absolute host path — and what the project's committed rules exclude is refused by `Workspace::read`, because a build tree's dependency files carry absolute host paths and have no name worth denylisting.
- Tool receipts are recorded by the runtime inside each tool body, independently of any Rig hook, and are read back on both the success and the failure arm. Rig documents hooks as controls rather than authorization, and a control that stops firing must not be able to empty the record of what happened.
- The deterministic suite gates. Tier 1 and Tier 2 are opt-in, need a credential, cost money, and never gate — no workflow invokes them, and a real-model lane never asserts that the model succeeded.
- No model-visible surface carries the host layout — not a tool's JSON schema, not an error message, and not a tool's **success output**. Host facts reach a tool through `ToolContext`; a schema is a menu, and anything named on it is something the model may fill in. Asserted against the *serialized outbound request* and not only against the builders that produce it: `binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact` reads the chat-completions bodies the compiled binary put on a loopback socket, pins the offered set at exactly `read_file`, `write_file`, `list_files`, `run_check`, and searches every body for both spellings of the host root and for the credential's value.

## Known issues

- Three permission-injection tests in `crates/fiddle-runtime/tests/attempt.rs` return early under an identity that ignores permission bits (root), so on a root CI runner they no-op silently instead of skipping visibly.
- Parity between the in-repo and external acceptance lanes is maintained by hand. `docs/technical/acceptance-repository.md` states they assert the same properties; nothing mechanically checks it, and the two have already drifted once.

---
Last reviewed: 2026-08-09
