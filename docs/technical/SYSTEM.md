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
| agent | `fiddle-runtime/src/agent` | six tools, a seventh where a deployment declares a program, one bounded Rig attempt | the tool surface carries no host fact (ADR 034); an edit changes part of a file and is unique or refused (ADR 049); a tool result is bounded and a partial result names what it withheld (ADR 051); a program an attempt runs is one the deployment declared (ADR 044); a search answers with a path and a line so a long file costs one turn (ADR 071) |
| returns | `fiddle-runtime/src/agent/returns.rs` | returns a report to the model as a turn when it accounts for less than it was shown, or when its declaration and the diff are different sets | two returns over the attempt across both rules, and then the attempt ends on the rule the report still fails (ADR 053, ADR 055); a turn budget a return spent names the rule and the budget (ADR 056) |
| retry | `fiddle-runtime/src/agent/retry.rs` | sends the same request again when the provider answers with no message and no tool call | two retries over the attempt, each one a transcript record (ADR 054) |
| transcript | `fiddle-runtime/src/agent/transcript.rs` | records what the model was sent, what it returned, and when, off unless `FIDDLE_TRANSCRIPT=1` | every text passes through `Redaction`, and a run that wrote one says so (ADR 052) |
| gateway | `fiddle-runtime/src/gateway.rs` | the one credential-carrying model, and the redaction of its credential | an OpenAI-compatible gateway, not Anthropic (ADR 012); a provider's body is quoted with the resolved credential replaced (ADR 050) |
| effect executor | `fiddle-runtime/src/effect` | the seven-step authorization order | no mutation without an `AuthorizedEffect` (ADR 033) |
| forge adapter | `fiddle-runtime/src/{github,git}` | the one `gh`, the one `git fetch` and the one `git push` (ADR 046) | `gh api -i`, not a REST client (ADR 015) |
| scanner | `fiddle-runtime/src/scanner` | runs `wizcli`, records version and digest | the digest, never the tag it was handed (ADR 020); fiddle passes the scanner no credential (ADR 042) |
| sweep | `fiddle-runtime/src/{cve,capability/mitigate.rs}` | plan, check out, measure the tree, scan it, one attempt, rescan | all or nothing: one finding left standing sends the whole attempt to a draft nobody merges; the scan describes the tree the work happens in (ADR 066); a successful scan that omits an array observed nothing in it (ADR 058); the brief states that nothing has been changed yet (ADR 057) |
| baseline | `fiddle-runtime/src/evaluate` | runs the exit-code checks before the agent and reports both halves | a check the tree already failed is named to the agent and not held against it, and a check it already passed is named as one to keep (ADR 061, ADR 065) |
| direction | `fiddle-runtime/src/{github/comments.rs,capability}` | reads the conversation, the reviews and the diff-line comments of both pull requests | a review asking for changes is work to answer and a comment can waive a check (ADR 068); only `OWNER`, `MEMBER` or `COLLABORATOR` carries either (ADR 067); the words carry it and a bare approval waives nothing (ADR 069); the sentence must exist in the conversation or it is ignored, and the author and the reviewer cannot be one person (ADR 070) |
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
| filing report | `<report.dir>/filings.json` | whether this deployment files advisories at all, and for each proposal the ticket it filed or the refusal it met |
| attempt journal | `<report.dir>/.attempts/` | one intent record per attempt, then one `"effect_step"` line per executor step |
| bean state | `.beans/`, read by the external `beans` CLI | epics, tasks and tags; the unit of work for develop |

**`fiddle.toml`** (TOML, `deny_unknown_fields`) — the deployment document. `[project]`, `[stub]` and `[report]` are required. `[agent]`, `[workspace]`, `[github]`, `[jira]` and `[scanner]` are optional, and so is `[orchestration.cve]`. An absent table describes a deployment that does not do that work; it is never a blank filled in silently. `[agent]` and `[workspace]` bound one attempt, name the checks that decide it, and name the programs it may run. `[github]` and `[scanner]` are the second and third credential-carrying tables. `[jira]` is the fourth: it names the site, the project, the two halves of one basic credential, and, under `[jira.workflow]`, the status names this site uses. `[jira.filing]` is optional and separate: it names the `project` advisories are filed into, the `issue_type` a create is given and the `ledger_issue` that carries the exactly-once claims, and its absence means this deployment files nothing (ADR 080). `base_url` points a deterministic lane at a loopback server, and a deployment sets none. `[orchestration.cve]` names the `image`, the `severities` and the `max_findings` a sweep acts on, and those three keys are the whole table. `crates/fiddle-acceptance/tests/config_check.rs` requires this line to name every table the schema admits.

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
- An evidence-only scorecard emits an explicit `"dimensions": {}` and declares `"mode": "evidence-only"`. The key is never omitted, and the declaration is never inferred from the empty object.
- A scorecard tool refuses empty input rather than answering from it: zero dimensions with no declaration, zero dimensions and zero criteria, and one criterion id twice all exit 2.
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
- Every external mutation a capability makes passes the effect executor. Deployment policy may strengthen a capability's minimum and never weaken it (ADR 033).
- The model reaches no mutation. It is offered `read_file`, `edit_file`, `write_file`, `list_files`, `search_files` and `run_check`, and `run_command` where the deployment declares a program. Each one is confined to the workspace. None reaches a forge, and none proposes an effect.
- No type stops capability code from holding a live client and mutating outside the executor. `ProposedEffect` has public fields and a capability builds its own. That path is closed by review, not by a guarantee (ADR 075).
- An effect name is rejected at two moments: config load refuses a policy key naming no registered effect, and `Executor::walk` refuses an unregistered proposal before its first traced step (ADR 075).
- An effect's identity records the work, not the name it was told. `Executor::walk` compares the proposed kind and the proposed target against the operation that will perform them, before it derives an identity, and refuses a mismatch as `EffectError::IdentityDiverged`.
- An unknown answer is resolved by reading the world, never by repeating the write (ADR 032).
- A locator may be inherited, an authority may not. Three spawn sites keep three environments and share one bound, `process.rs::run_bounded` (ADR 029).
- Containment is checked syntactically, then against the resolved path. `.git` is refused at any depth (ADR 031).
- What counts as a change comes from the committed ignore rules, snapshotted before the attempt (ADR 030).
- The workspace supplies a scratch `HOME` beside the worktree, never inside it.
- The model-visible surface carries no host fact, and the runtime records each tool receipt itself (ADR 034).
- An attempt runs a program only where the deployment declared it, as a program and an argument list with no interpreter, bounded by `[workspace] command_timeout` (ADR 044). The declaration's arguments are a prefix; whether the model may append to them is the declaration's own answer, and the default is no. This is a legibility rule, not isolation: see Known issues.
- The brief names each declaration the model could have written itself, and withholds one whose program or argument is a host path (ADR 047). A withheld declaration still runs, and the refusal still names it.
- A mitigation attempt's diff touches exactly the files it declared, and Rust reads no meaning into a path (ADR 026). Nothing pre-filters an already-fixed finding.
- Fiddle scans an image it did not build, and the bundle pairs the digest with the revision (ADR 020).
- A `workflow_dispatch` lane on a feature branch is inert until its file reaches the default branch. `.github/workflows/github-effects.yml` carries the diagnosis.
- The check runs in its own process group, so `fiddle run` installs a `SIGINT` handler. The first interrupt cancels the token; the second exits 130.
- The deterministic suite gates. A real-model lane is opt-in, needs a credential, and never asserts that the model succeeded.
- The M0 acceptance command stays credential-free and green. Every later milestone runs it unchanged.
- An acceptance lane resolves its binary through `support::fiddle_binary()` (ADR 035).
- Jira is reached by request and not by subprocess (ADR 077). One construction holds the credential, derives neither `Debug` nor `Serialize`, and redacts every error text. This is a discipline and not a boundary.
- An observation port is async, and a `jira:` reference selects the Jira port arm by arm with no wildcard.
- A Jira effect that acts on an existing issue names the issue key and the revision it was read at. The derive refuses a target that names no field, so the rule is a compile error and not a review rule (ADR 078).
- One canonicaliser builds every Jira revision. A `fields.updated` that this build cannot read refuses the operation and derives no identity (ADR 078).
- `jira.issue_filed` decides on a claim held as a property of a configured ledger issue, read directly rather than through the index. A claim naming an issue reads, a claim naming none refuses as unresolved, and no claim files. The marker label still rides the create and still answers a search, which is how an unresolved claim is settled and how two matches refuse the write (ADR 079).
- A Jira refusal that sent no request classifies `NotCommitted` in both phases. It never reports a write that was made and lost (ADR 076).
- A live Jira lane records evidence and does not gate. No step of `scripts/gate.sh` calls one (ADR 079).

## Known issues

- Three permission-injection tests in `crates/fiddle-runtime/tests/attempt.rs` return early under an identity that ignores permission bits. On a root runner they no-op instead of skipping visibly.
- Parity between the in-repo and external acceptance lanes is kept by hand. Nothing checks it and the two have drifted once. See `docs/technical/acceptance-repository.md`.
- M2's exactly-once proof rests on one test. Only `an_ambiguous_write_then_a_fresh_process_leaves_exactly_one_of_each` fails under an inversion that lets the mutation retry.
- The dispatch locator's round trip is checked by no gating test. `github::checks::run_name` spells one half and `fiddle-check.yml` in the disposable repository spells the other.
- `.github/workflows/github-effects.yml` is inert until it merges to the default branch. One dispatch after the merge closes it.
- A forbidden test edit is a deployment's check now, not a guarantee this binary makes. A deployment that declares no test check gets no warning (ADR 026).
- `[[workspace.checks]]` constrains neither the count nor the order nor which programs appear. No lane exercises a real `docker build`.
- **A declared program is not sandboxed, and `[[workspace.commands]]` is not a security boundary (ADR 044).** It runs as an ordinary process with the invoking user's identity: it can read any file that user can read, write any file that user can write, and open any socket. Declaring one build tool grants arbitrary code execution, because `go test` runs code out of the repository under repair and `go generate` runs whatever the source names. What does hold is four things and no more — a per-attempt worktree as the working directory, no credential in the child, `WorkspacePath` bounding fiddle's own file tools, and a time bound that is cancellable. On a GitHub runner the disposable job supplies the isolation; local attended execution (the PRD's M6) is where the exposure is real. `Isolation` has one variant and the seam for a sandbox is empty; ADR 044 says what a second variant would need.
- The attempt now has an egress path it directs itself, where before this it had none: a declared dependency tool fetches from a registry (ADR 044). A sandbox cannot answer "no network" without breaking the repair the tool exists for.
- No check verifies that a document citing a symbol names one that exists. Two lanes pin prose in this file, and both pin enumerations the binary also prints.
- **The two review states that matter cannot be used where fiddle authors with a person's token (ADR 070).** GitHub refuses approve and request-changes on your own pull request, so `CHANGES_REQUESTED` — the only state that blocks a merge — is unavailable until a deployment gives fiddle its own identity. Proved working under a GitHub App on `peel/fiddle-test`; `snowplow-identities` still authors as a person.
- A check that fails for a reason unrelated to the change, and not every time, passes the baseline and fails after it, and the attempt is blamed. `findings.json` now names both halves of the baseline so a reader can see it, and nothing detects it.
- The offline harness cannot test that the scan follows the tree. Its scripted scanner answers the same document whichever tree it is pointed at, so no lane fails if ADR 066 is reverted.
- `peel/fiddle-test` cannot find a defect that needs a large file. Its `go.sum` is a few lines, and `snowplow-identities` run 32765904429 spent forty turns on 985 of them (ADR 071).
- The commit subject is fixed in Rust while the pull request title is a configured template. A repository with a commit convention cannot express it.
- `agent::offered` and the agent builder disagree. `offered` lists five tool names and omits `search_files`, and the builder registers six. `offered` feeds only the transcript record, so the model is unaffected and the recorded brief under-reports the tool set by one. Nothing detects the drift.
- `WorkflowCapability` is built, tested and unreachable. `toml` is a dev-dependency of `fiddle-runtime` only, no `fiddle-cli` path reads a workflow file, and `WORKFLOW` is absent from `CAPABILITIES` by design, because every name there must be selectable on the command line and a workflow needs a document. M5 wires it.
- **A session runs the skills the plugin root serves, not the ones in its worktree.** `.claude-plugin/plugin.json` declares `"skills": "./skills/"` relative to the plugin root, which is the main checkout. A worktree agent therefore executes the skill version of whichever branch the **main checkout** has out. Measured 2026-08-26: 24 files diverged, and the running copy of `skills/develop/SKILL.md` carried no `Live Acceptance` step while this branch's copy carried it as Step 3. The epic's standing live-acceptance criterion says the gate is "encoded in `skills/develop/SKILL.md` step 3", so the milestone depending on that gate was reading the copy without it. `scripts/gate.sh` now prints a `RUNNING SKILLS:` line naming the divergence and any missing step. It does not gate, because during a stacked milestone the divergence is expected. See `fiddle-wj6o`.
- A gate assertion that pins a heading **number** reds when a document is renumbered and stays green when the step is deleted. `test-multi-domain-holistic.sh` pinned `## Step 3: Holistic Review` and broke when M4d inserted Live Acceptance ahead of it. It now matches `^## Step [0-9][0-9]*: Holistic Review$` and asserts ordering instead, proved in both directions.
- `JiraHttp` shares this process's address space, environment and TLS configuration. Nothing structurally stops a future code path from reading the credential, unlike the `gh` child that `env_clear` bounds. ADR 077 records the discipline that replaces the boundary and says it is not one.
- **The Jira adapter's live evidence is three lanes against one site, and the write lane found that the write path cannot work.** `scripts/live-jira-observe.sh` read ISP-267 from `snplow.atlassian.net` on 2026-08-26, and `docs/technical/RUNBOOKS.md` records what came back. It refuses rather than skips when a credential, the site, the issue key or the binary path is absent, and it does not gate. Every other Jira claim in the suite is measured against a loopback stub — `crates/fiddle-runtime/tests/support/stub_jira.rs` for the unit lanes and `StubJira` in `crates/fiddle-acceptance/tests/support/mod.rs` for the acceptance lanes. That one read turned two of ADR 077's three arguments into measurements: the shape `/rest/api/3/issue` returns, and the `fields.updated` format, whose offset carries no colon and is therefore not RFC 3339. `scripts/live-jira-search-shape.sh` measured the third on 2026-08-28. Every issue type in `ISP` offers the same six status names, and `Blocked` shares the category `indeterminate` with `In Progress`, so a deployment that wants `Blocked` to read as blocked must name it in `[jira.workflow]` (ADR 077). One issue on one site measures no range: a second project, a second workflow and a second issue type are unmeasured, and so is every failure arm but one. Six probes on 2026-08-27 measured what a bad credential returns: an issue read answers 404 for a bad credential, for no credential and for an issue that does not exist, and `/rest/api/3/myself` answers 401 or 200 and tells them apart. `JiraWorkItemPort::read` asks it on a 404 for that reason (ADR 077). `scripts/live-jira-write.sh` ran twice against project `ISP` on 2026-08-28 and is the only check in M5b that compared this build against something this build did not write. It is also the only one that failed. ADR 079 records what it measured.
- **The typed work state has no reader that decides anything, and M5b did not become one.** `crates/fiddle-runtime/src/jira/work_item.rs` projects a Jira status into `WorkItemState.projected_status`, and `crates/fiddle-core/src/assessment.rs` reads none of it: `assess` branches on whether each observation is available and on the change-set marker, and `derive_next` maps its three verdicts. The projection reaches two surfaces and no decision: the `observations` object of `fiddle inspect --json`, and the human work item line, which carries the typed state beside the verbatim status. `jira.issue_transitioned` was expected to consume it. What shipped does not: `TransitionIssue::inspect` compares `status.jira_status_name` with the requested name, and `TransitionIssue::read` builds `ConfiguredNames::new(None, None, None, None, None)`, so `[jira.workflow]` reaches the transition effect nowhere (ADR 078). The receipt carries a `ProjectedStatus` that nothing branches on. So a wrong projection still moves no outcome a test can see, and the one live status measured is the case where the configured-name path and the category fallback agree anyway (ADR 077). `no_projected_work_state_moves_the_assessment_or_the_next_action` holds the absence still. It pins that the decision path ignores the projection over all six `WorkState` variants and all three marker cases, and it is not a claim that the projection is right.
- **`jira.issue_filed` was repaired against the measurements and has now reached a real site.** MEASURED on `snplow.atlassian.net` on 2026-08-28: `GET /rest/api/3/search/jql` answers each issue with `id` and no `key` unless the caller asks for `fields=key`; `createmeta` for `ISP` lists `issuetype` among the required fields of a `Task`; and an issue property is readable the moment it is written and invisible to JQL. The stub was corrected to answer all three that way first, and seven existing tests red on that commit alone — that red was the defect becoming visible. `FileVerdict` now asks `fields=key` on every page, names `fields.issuetype` in the create, and decides from a claim on a configured ledger issue rather than from a search. ARGUED and not measured: that the old build could never recognise its own ticket, and that its create would have been refused. MEASURED on 2026-08-29: `crates/fiddle-runtime/tests/live_jira_filing.rs` drove `ticket_proposals`, `FileVerdict` and the executor against `snplow.atlassian.net`. Run one filed `ISP-276`; run two over the same invocation reference answered `ISP-276` from `inspect` and created nothing; with the claim removed, `FileVerdict::inspect` reached the search, asked `fields=key` and read the same key back. The indexing lag on that run was more than 634 ms and at most 1940 ms, with both bounds taken from searches whose counts the run compared with its own create count. ADR 079 carries the record. The run path landed in M5b: a mitigate run calls `ticket_proposals` and executes each `FileVerdict` through the executor when `[jira.filing]` names a project, an issue type and a ledger issue (ADR 080). `crates/fiddle-runtime/tests/cve_filing.rs` measures that against the loopback stub and against nothing else.
- **The Jira indexing lag is unmeasured, and the exactly-once claim is bounded by it.** `scripts/live-jira-write.sh` ran twice on 2026-08-28 and filed two tickets, `ISP-272` and `ISP-273`, because run two searched for the marker run one had written and matched nothing. The lane reported `0 seconds` for the lag and, in the same run, reported one issue where two existed. The two numbers cannot both be true, so the lane's own number is unsound and must not be quoted. `the_search_then_create_protocol_files_a_second_issue_where_the_claim_ledger_files_one` drives both protocols against one stub inside that window and counts two creates against one. The lag itself is still described by no number, and the rewritten lane refuses to print one unless the search it read agrees with the number of creates it made (`fiddle-jh1z`).
- **The live Jira write lane cleans up by deleting, and the project it was run against forbids deletion.** `ISP` answered `HTTP 403` to both deletes by project policy, not by a missing permission. The operator ruled on 2026-08-28 that the lane closes its ticket to `Won't Do`, or at worst to `Done`, and never deletes. `ISP-272` and `ISP-273` were closed by hand through transition `id=51`. The lane still deletes, and its failure text still advises a reader to delete by hand, which this operator cannot do. Residue is not inert: two issues carrying one marker are the `Ambiguous` case, so each unrun cleanup makes the next run refuse (`fiddle-jh1z`, ADR 079).
- **Eighteen of 22 enum-exhaustiveness tests compare one hand-written list with another.** Such a test claims that every variant is pinned and proves only that two lists agree. Two are false today: `workflow_capability.rs` asserts a list of eight for the nine variants of `EffectError` under the message "an effect failure was added without a case here", and `parses_every_known_scheme` in `crates/fiddle-core/src/identity.rs` omits `InvocationScheme::Cve`. `JiraError` is closed by the `VariantCount` derive and is the only type that is (`fiddle-0lcc`).
- **A `fiddle` binary files a verdict, and no run of the binary has reached a real site.** `CveMitigate` calls `ticket_proposals` and executes each `FileVerdict` through the executor, and it does so only when `[jira.filing]` is configured; a deployment that names no such table writes `{"filing": "not_configured"}` to `filings.json` and sends no request (ADR 080). A refusal from the site is recorded as `TicketFiled::Refused` beside the repair and does not fail the run, so a misconfigured project files nothing until somebody reads that file. `human::publish`, the other route a Jira write could take, still has no caller outside tests. Whether `docs/technical/host-workflow-m4b.patch` can now retire its verdict mapping step and its Jira step is `fiddle-2690`, and this bullet does not answer it. Every claim here is measured against the loopback stub in `crates/fiddle-runtime/tests/cve_filing.rs`. The live proof of 2026-08-29 does not cover this bullet: `crates/fiddle-runtime/tests/live_jira_filing.rs` builds a `TicketFiling` itself and calls `ticket_proposals` and the executor as `CveMitigate::file` does, so it measures `FileVerdict` against Atlassian and says nothing about the mapping from `fiddle.toml` to `TicketFiling` in `crates/fiddle-cli/src/main.rs`. The only run path to `FileVerdict` through the binary is a full CVE sweep needing a scanner, an agent and a GitHub repository.
- **The GitHub adapter bounds no reason string it builds from remote text.** The Jira adapter clamps at `CLAMP` through `JiraHttp::quotable`. `crates/fiddle-runtime/src/github/` defines and calls no bound, and two sites build an `Observation::Unavailable` from a raw `String`. `fiddle inspect` is the unbounded surface. The Jira success path is uncapped too: `fields.status.name` is copied verbatim into `WorkItemState.status` (`fiddle-y4zt`).
- Three registered effects have no live evidence: `publish_decision_request`, `ensure_check_requested` and `ensure_pull_request_ready`. `scripts/live-github.sh` reaches exactly those three and requires a fine-grained token scoped to the disposable repository. They are covered hermetically only, so "unchanged behaviour" for them is an argument rather than a measurement.
- **The fine-grained credential the epic specifies has never been shown to work.** `scripts/live-cve-steering.sh` produced M5a's standing forge result with the operator's `gh` keyring token, whose scopes are `repo` and `workflow` and which reaches every repository that account can push to. `FIDDLE_GITHUB_TOKEN`, scoped to one repository, still answers the dispatch endpoint with 403 and `x-accepted-github-permissions: actions=write`. Both credentials issue the same API calls, so the forge behaviour is measured and the credential scope is not. An evaluator scored that gap 5 against a threshold of 7; the operator waived the dimension on 2026-08-27 rather than regenerate the token, so the lane's result stands and the specified credential stays unproven. `docs/technical/RUNBOOKS.md` records it.

---
Last reviewed: 2026-08-29
