# Fiddle v2 Infrastructure Design

Status: draft for written-spec review

Date: 2026-08-01

## Summary

Fiddle v2 adds a stateless deterministic automation engine in Rust beneath the existing skill system. Skills remain the doctrine and judgment layer: they preserve DISCOVER → DEFINE → DEVELOP → DELIVER, NATO 1968/1969 software-engineering lessons, compound engineering, systematic challenge, genuinely separate inference providers for important debates, evidence-based evaluation, calibration, and longitudinal quality controls.

The Rust engine owns mechanics that should not depend on model compliance: routing markers, lifecycle transitions, attempt identity, progress and decision updates, verification gates, provider dispatch records, idempotency, and restart reconstruction. It has no service and no database. Beans is authoritative locally; Jira is authoritative in the first remote path; GitHub is authoritative for branches, pull requests, checks, and code evidence.

The first remote vertical slice uses a managed cloud agent as a tracker supervisor and GitHub Actions as the execution environment. The supervisor monitors Jira, triages new work, writes a routing label, and wakes a GitHub workflow. GitHub Actions runs Fiddle and one-shot Claude or Codex workers. Fiddle writes durable progress, decisions, questions, and outcomes directly to Jira. Linear shapes the tracker contract but its production adapter is deferred until after v2.0.

## Goals

- Preserve Fiddle's current conceptual premises and skill semantics.
- Keep local Claude Code and Codex use service-free and Beans-compatible.
- Make remote work disposable, reconstructible, auditable, and idempotent.
- Keep tracker, runtime, harness, and inference-provider choices replaceable.
- Move current Crops reporting and enforcement responsibilities into deterministic Rust commands.
- Deliver one credible remote path: managed supervisor → Jira → GitHub Actions → Claude/Codex → pull request.
- Distinguish genuine provider diversity from a fresh context of the same provider family.

## Non-goals

- Building a general workflow engine or hosted Fiddle control plane.
- Adding PostgreSQL, an event database, or an always-running Fiddle service.
- Replaying or resuming an interrupted model process.
- Replacing the skills with a Rust workflow definition.
- Requiring Crops for reporting, decisions, notifications, or gates.
- Implementing every tracker and remote runtime in v2.0.
- Running multi-provider debate for every routine implementation decision.
- Encoding Jira workflow statuses or hierarchy as Fiddle's lifecycle model.

## Architectural Boundary

### Skills: doctrine and judgment

The Markdown skills remain the user-facing lifecycle and reasoning system. They decide what research is needed, how to challenge a design, when to request human judgment, how to implement a task, and how to interpret evidence. Existing skill names and the four lifecycle phases remain stable through v2.x.

Skills invoke the Rust engine for deterministic effects. They do not duplicate tracker mutation rules, provider identity rules, or hook-specific reporting logic.

### Rust: stateless automation engine

The `fiddle` executable contains a small command-oriented core, not a configurable workflow interpreter. Its responsibilities are:

- Parse a tracker-neutral work reference.
- Load and normalize tracker state.
- Validate routing and lifecycle transitions.
- Create and validate attempt identities.
- Record progress, decisions, questions, evidence links, and outcomes.
- Apply idempotent tracker and GitHub updates.
- Inspect durable state and calculate a safe resume action.
- Invoke configured one-shot provider commands in unattended execution.
- Record provider identity and independence coverage.
- Implement hook protocols that currently call Crops or rely on free-form shell logic.

The engine does not choose architecture, implement code, simulate debate, score subjective quality, or maintain private durable state.

### Adapters

The core depends on ports implemented by adapters:

- `Tracker`: Beans in v2.0 local mode; Jira in the v2.0 remote reference path; Linear after v2.0.
- `CodeHost`: GitHub in v2.0.
- `AgentRunner`: attended Claude Code/Codex and unattended `claude -p`/`codex exec`.
- `Harness`: thin Claude, Codex, and GitHub Actions entrypoints.
- `Supervisor`: a provider-neutral interaction contract demonstrated by a managed Claude agent.

Adapters translate the shared concepts into native tracker or runtime operations. Jira-specific fields, Atlassian Document Format, Linear GraphQL identifiers, and Beans CLI flags do not enter the core.

## Tracker Contract

### Work reference

A work item is addressed as a typed reference such as `beans:fiddle-123`, `jira:FID-42`, or `linear:ENG-19`. Repository and tracker configuration resolve credentials and project-specific mappings; secrets never appear in a work reference or task body.

### Shared operations

Every production tracker adapter must implement and pass the same contract suite:

- Fetch one work item and its current revision.
- Find work carrying a Fiddle routing marker.
- Add or remove one routing marker.
- Claim a routing request for an attempt.
- Append an idempotent progress update.
- Record a decision and rationale.
- Ask a structured human question.
- Read responses after a question marker.
- Link design, plan, commit, pull-request, check, and evaluation evidence.
- Record attempt outcome and release or complete work.
- Resolve parent, child, and dependency relationships without imposing one vendor's hierarchy.

### Tracker-native representation

The logical contract is uniform while storage remains native:

| Concept | Beans | Jira | Linear target contract |
|---|---|---|---|
| Work item | Bean | Issue | Issue |
| Route | Tag | Label | Issue label |
| Progress, decisions, questions | Structured body sections | Comments | Comments |
| Active attempt | Bean metadata/body | Issue property plus visible comment | Adapter-owned issue metadata/comment |
| Parent and dependency graph | Bean relationships | Configurable parent/link mapping | Parent/sub-issue and relation mapping |

Labels, tags, statuses, and issue types are configurable mappings. The core works with semantic values rather than hard-coded names or IDs.

## Routing and Triage

Fiddle exposes two portable routing intents:

- `orchestrate`: run the complete lifecycle, including DISCOVER and DEFINE before implementation.
- `develop`: begin from an already approved and current definition bundle.

Their default tracker markers are `fiddle-orchestrate` and `fiddle-develop`.

`fiddle triage <work-ref>` loads the task and related artifacts, invokes one configured triage agent, and requests structured output containing the route, rationale, confidence, missing information, and evidence references. The agent recommends; Rust validates and performs the mutation. Low-confidence output leaves the task unqueued and records a request for human confirmation.

`develop` is allowed only when the adapter can resolve an approved design, an executable plan or task graph, actionable acceptance criteria, verification expectations, and no unresolved blocking question. A detailed-looking issue without this definition bundle does not bypass DEFINE. `orchestrate` is the conservative fallback. Quickfix remains an internal optimization selected by the existing orchestrate criteria rather than a third remote routing protocol.

Humans may add a routing marker directly. Direct `develop` still runs the readiness validator and stops with an actionable tracker update when prerequisites are absent.

## Local Execution

Local attended execution keeps the current interaction model:

1. Claude Code or Codex loads the appropriate Fiddle skill.
2. The skill uses `fiddle` commands for state transitions and durable updates.
3. The Beans adapter uses the Beans CLI and optimistic revision matching.
4. Reasoning and implementation happen in the current harness.
5. Commits, evaluations, and decisions are reflected in the bean.

No daemon, remote tracker, or network service is required. Existing Beans projects and skill names remain supported throughout v2.x. Thin compatibility wrappers preserve current script and hook entrypoints while callers migrate to native `fiddle` commands.

## Remote Reference Path

### Managed cloud supervisor

The supervisor interacts with Jira but does not implement code or own Fiddle state. It:

- Observes new issues, routing requests, questions, and human responses through capabilities supplied by its cloud platform.
- Runs or applies Fiddle triage.
- Adds one routing label and records the rationale.
- Triggers a GitHub `repository_dispatch` for the configured repository.
- Wakes execution again when a waiting question receives a human response.
- Escalates low-confidence, stale, or repeatedly interrupted work.

The supervisor contract is intentionally small so Claude, Codex, Hermes, or another managed agent can implement it. Fiddle does not prescribe whether the cloud platform uses webhooks, native connectors, or scheduled wake-ups. Loss of the supervisor loses no durable work state.

### GitHub Actions execution

The initial GitHub workflow receives a tracker reference, requested route, trigger identity, operation ID, and observed task revision. It then:

1. Uses a concurrency group derived from the tracker reference.
2. Checks out the configured repository and installs the pinned Fiddle binary.
3. Re-fetches the Jira issue; trigger payload text is never treated as authoritative.
4. Verifies the routing label and issue revision.
5. Consumes the routing request and records a new attempt in Jira.
6. Runs `fiddle orchestrate`, `fiddle develop`, or `fiddle resume`.
7. Launches one-shot Claude or Codex processes as selected by policy.
8. Pushes implementation commits and creates or updates a pull request.
9. Writes progress, decisions, questions, evidence links, and attempt outcome directly to Jira.

GitHub Actions provides execution isolation and CI; it is not the source of task truth. Jira provides task truth; it is not a code or evidence store.

### Human interaction and resumption

Remote workers never wait in a terminal for a person. A structured question is written to Jira with a stable question ID, then the attempt exits at a durable boundary. The supervisor observes a later human response and triggers a fresh workflow. The new worker reconstructs context from Jira, the Git branch and pull request, design and plan artifacts, and evaluation evidence.

Fiddle resumes work, not model memory. An interrupted LLM process is always replaced by a fresh process with an assembled context pack.

## State and Attempt Semantics

The portable task lifecycle is:

`ready → claimed → running → verifying → review-ready → completed`

`waiting-for-human`, `blocked`, and `cancelled` are explicit branches. These are semantic states mapped onto tracker-native fields, labels, metadata, and comments; they do not require matching Jira workflow statuses.

Failure belongs to an attempt, not to the task lifecycle. Attempt outcomes include success, interrupted, provider-unavailable, invalid-output, verification-exhausted, stale-input, and side-effect-error. Rust maps the outcome to one of three actions:

- Retryable: release or requeue the task with a new attempt number.
- Needs judgment: mark blocked or waiting and ask a human.
- Explicit abandonment: a human marks the work cancelled.

A failing test during an active repair loop is progress evidence, not an attempt failure.

### Idempotency and duplicate triggers

Every side effect has a stable operation ID derived from work reference, attempt, command kind, and logical sequence. Tracker updates contain a machine-readable operation marker, allowing adapters to detect a previous successful write. GitHub branch, pull-request, and check operations use deterministic names or external IDs.

GitHub workflow concurrency serializes execution for one tracker reference in the initial remote path. The first run consumes the routing marker; a duplicate run re-fetches the task, observes that the request was consumed or the operation already exists, and exits without repeating work. The initial reference deployment supports one owning GitHub repository for each configured tracker queue.

## Crops Responsibility Migration

Fiddle currently delegates or refers to Crops in four places: automatic progress after commits, decision reporting, decision enforcement at agent stop, and local notification wiring. V2 replaces those with Rust-owned commands:

- `fiddle hook post-tool-use` recognizes relevant commits and records progress through the active tracker adapter.
- `fiddle report decision` records a decision and rationale using a stable operation ID.
- `fiddle hook agent-stop` validates that required decisions were recorded and emits the harness-native blocking response.
- `fiddle notify` emits the configured local or tracker-visible notification without a Crops dependency.

Harness configuration continues to use thin command wrappers where required, but the behavioral rules and formatting live in Rust. Crops-specific MCP configuration and shell commands are removed. Crops may independently visualize tracker data, but Fiddle has no runtime dependency or Crops-specific protocol.

## Provider and Harness Independence

Provider selection records:

- Provider family and model.
- Harness and execution mode.
- Attempt and context identity.
- Tool permission profile.
- Role: implementer, evaluator, debate participant, or supervisor.

Coverage is classified as `independent-provider`, `same-provider-fresh-context`, `same-context`, or `unavailable`. A second Codex process is not an independent provider from a Codex implementer. Claude Code and `claude -p` are separate contexts but the same provider family.

Critical DEFINE debates and configured holistic reviews request genuinely independent provider families. Routine per-task evaluation retains the evidence-driven single-evaluator design: use an independent provider when available, otherwise a fresh isolated context with explicitly degraded coverage. Degraded runs are visible in tracker updates and evidence packs; they are never reported as full adversarial coverage. Policy may require human override before a high-risk task proceeds without an independent provider.

## Security and Trust Boundaries

- Tracker text and comments are untrusted input to agents and commands.
- Trigger payloads contain references and routing intent, not authoritative task content.
- GitHub Actions secrets provide Jira, GitHub, and provider credentials only to the steps that need them.
- Provider commands receive bounded context packs and explicit tool permissions.
- Rust validates structured agent output before applying tracker or GitHub mutations.
- Logs and tracker updates redact configured secret patterns.
- Pull-request review remains the final code integration gate.

## Packaging and Compatibility

The repository becomes a Rust workspace producing one `fiddle` executable. Release automation publishes pinned binaries for supported Linux and macOS architectures. Project initialization and plugin installation detect the executable and provide a clear installation path.

The existing skill package remains portable across Claude, Codex, and Pi-style skill discovery. Existing Beans projects, skill names, configuration meanings, and current lifecycle artifacts remain supported throughout v2.x. Migration is incremental:

1. Introduce Rust commands behind current wrappers.
2. Replace Crops hooks and direct tracker mutations.
3. Make Beans local execution authoritative through the adapter.
4. Add the Jira and GitHub remote vertical slice.
5. Remove compatibility wrappers only in a later major version after measured parity.

## Testing Strategy

- Pure unit and property tests for routing, transition, attempt, idempotency, and provider-independence rules.
- A shared tracker contract suite run against in-memory fixtures and every adapter.
- Golden tests using representative existing Beans projects and bodies.
- Mock-server integration tests for Jira and GitHub APIs, including duplicate, stale-revision, rate-limit, and partial-side-effect cases.
- Failure-injection tests for killed provider processes, interrupted workers, expired attempts, repeated triggers, and tracker write failures.
- GitHub workflow tests with fake provider commands and no live model dependency.
- Optional live Jira/GitHub sandbox tests when credentials are explicitly supplied.
- Existing portability, evaluator, convergence, calibration, and trend tests remain required.
- Release installation tests for each published architecture.

## V2.0 Release Boundary

V2.0 includes:

- Rust core and CLI.
- Beans adapter and v1 compatibility path.
- Jira adapter.
- GitHub code-host adapter and GitHub Actions worker workflow.
- Managed cloud supervisor contract with a Claude reference configuration.
- Local Claude Code/Codex and remote `claude -p`/`codex exec` runners.
- Crops removal and Rust-owned reporting/gates.
- Harness-aware provider diversity policy.
- Documentation, migration guidance, and the full automated test layers above.

V2.0 does not include a production Linear adapter, Kubernetes runtime, managed Fiddle service, or database. Linear is the next tracker adapter and must be implementable without changing the core tracker contract. If implementation reveals that the contract requires Jira-specific behavior, the contract—not Linear—is considered defective and must be corrected before v2.0 is declared portable.

## Panel Coverage Note

The DEFINE panel attempted to obtain an independent Claude position in addition to the Codex lead. Claude produced no usable response within repeated bounded invocations, and Gemini was unavailable. The design therefore records degraded panel coverage rather than claiming a multi-provider consensus. Making provider availability and genuine independence machine-visible is part of this design's acceptance criteria.

## Decisions Log

- Ship a factory-ready v2.0 with one complete remote reference path.
- Keep skills as doctrine and judgment; move deterministic mechanics into Rust.
- Keep Fiddle stateless and tracker-backed; add no service or database.
- Use Beans locally and Jira remotely in v2.0; defer the Linear production adapter.
- Use `fiddle-orchestrate` and `fiddle-develop` as portable routing intents represented by tracker-native markers.
- Let a managed cloud agent supervise trackers and trigger Fiddle; run Fiddle in GitHub Actions initially.
- Let Fiddle workers update trackers directly; the supervisor is not a Crops replacement or progress relay.
- Treat attempt failure separately from task lifecycle state.
- Preserve provider diversity, independent evidence, calibration, and longitudinal controls.
- Preserve existing Beans projects and skill names throughout v2.x.
