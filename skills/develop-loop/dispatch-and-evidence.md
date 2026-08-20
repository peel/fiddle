# Dispatch and evidence

## Internal model selection

Before dispatching the implementer, resolve phase `develop`, role `implementer` with `scripts/resolve-subagent-model.sh`; pass `model` only when the resolver emits it. Before dispatching an internal evaluator, resolve phase `develop`, role `evaluator` the same way. External provider selection remains independent.

## 1a. Restart Check

If the bean is `in-progress`, follow: `skills/develop-loop/restart-recovery.md`

## 1b. Initialize Evaluation Log

For a fresh bean (not a restart), record the starting point:

```bash
BASE_SHA=$(git rev-parse HEAD)
scripts/append-eval-log.sh --bean-id {id} --init --base-sha "$BASE_SHA"
beans update {id} --status in-progress
mkdir -p .fiddle && printf '%s\nsession=%s\n' "{id}" "${CLAUDE_CODE_SESSION_ID:-unknown}" > .fiddle/active-bean
```

The `.fiddle/active-bean` marker arms the Stop-hook verdict gate for THIS session only (line 2 records the owner; the gate fails open for every other session, so bystander sessions and subagents are never dragooned into a loop they do not own): while it is
non-empty, turn-end is blocked until the bean reaches a terminal verdict. Every
terminal exit below clears it with `rm -f .fiddle/active-bean`.

Set `dispatch_count=0` and `iteration=0`.

## 1c. Resolve Domains

Read `domains` from the bean's eval block (e.g. `domains: [frontend, backend]`); default to `general` if absent. Resolve with the script rather than by hand, so config templates, runtimes, and fallbacks apply consistently:

```bash
scripts/resolve-domains.sh --domains "frontend,backend" --config orchestrate.json > resolved-domains.json
```

It returns one object per domain with `domain`, `template`, optional `runtime` and `ready_check`, and `resolved_via` (`config` or `fallback`). Store the list for steps 1e-2 and 1f.

Also read `runtime_order` from the eval block if present (e.g. `runtime_order: [backend, frontend]`); default to the `domains` order.

## 1d. Dispatch Implementer

Dispatch a subagent with the template at `skills/develop/implementer-prompt.md`, filling:

- `{ITERATION}` — iteration number (1 on first dispatch)
- `{TASK_TEXT}` — full bean body (title, description, acceptance criteria)
- `{CONTEXT}` — relevant file paths, architecture notes, codebase context
- `{EVAL_BLOCK}` — the task's Evaluation block criteria, excluding any criterion marked `holdout: true`
- `{ANTIPATTERNS}` — antipatterns to avoid (see below; empty if none configured)
- `{PRIOR_SCORECARD}` — previous evaluator scorecard with hold-out results removed (empty on first dispatch)
- `{PRIOR_GUIDANCE}` — evaluator fix instructions with hold-out-derived guidance removed (empty on first dispatch)
- `{WORK_DIR}` — worktree directory path

Increment `dispatch_count` after each dispatch.

### Hold-Out Criteria

A criterion marked `holdout: true` is evaluator-only: scored like any other, but never shown to the implementer in the prompt or in feedback. Exclude it from `{EVAL_BLOCK}`, and on re-dispatch (1m FAIL / PASS_REGRESSED) strip its `criteria[]` entry from `{PRIOR_SCORECARD}` and any guidance derived from it. Convergence has to come from the implementer generalizing to the spec; a criterion it can read is one it can rubric-match instead. Default: nothing is held out unless the eval block marks it.

### Antipattern Loading

For each resolved domain, read `evaluators.domains.<domain>.antipatterns` from `orchestrate.json` if the key is set, stopping at any `## Retired` heading (retired entries from deliver 5g are audit-only). Concatenate across domains into `{ANTIPATTERNS}`; leave it empty when no domain configures the key.

## 1e. Handle Implementer Status

- **DONE** / **DONE_WITH_CONCERNS** → evidence gathering (1e-2), then evaluation (1f). A DONE report is the implementer's claim about its own work, not an evaluation of it, so it never substitutes for the evaluator pass.
- **BLOCKED** → mark `needs-attention` with reason, `rm -f .fiddle/active-bean`, escalate to human, next bean.
- **NEEDS_CONTEXT** → supply the requested context and re-dispatch (1d).
- **SPEC_DEFECT** → mark `needs-attention`, `rm -f .fiddle/active-bean`, escalate to human, next bean. Do not re-dispatch: a faithful implementation of a defective spec cannot converge however many iterations run.
  - Record on the bean body what about the spec is defective (the implementer's evidence), a `fiddle:define` re-entry pointer, and the SPEC_DEFECT exit with the iteration number and the one implementer dispatch that occurred.
  - Decrement `dispatch_count` by 1, undoing the increment in 1d: a spec defect is DEFINE's failure, not the implementer's budget.
  - Write no eval-log entry: no evaluator ran, so no scorecard exists and `append-eval-log.sh` hard-requires `--scorecard`. The bean-body note is the record.

## 1e-2. Gather Evidence Pack (per domain)

For each resolved domain, before any evaluator dispatch:

1. Start runtimes if configured (below).
2. Run the project's test command, full output to `evidence-{domain}-tests.txt`.
3. Run invariant/validation scripts named in the bean's eval block, output to `evidence-{domain}-checks.txt`.
4. For runtime-configured domains, probe the running app per `skills/runtime-evidence/SKILL.md` into `evidence-{domain}-runtime.txt`.
5. Concatenate into `evidence-{domain}.txt` with one `### <source>` header per section.

The evaluator interprets this pack; it does not gather evidence itself.

### Runtime Start

If a resolved domain has a `runtime` array, start it before capturing runtime evidence or dispatching the evaluator — a runtime dimension scored without a live app is scored on nothing:

```bash
scripts/start-runtimes.sh --domains <resolved-domains-file>
```

Start in `runtime_order`: for `[backend, frontend]`, start backend, wait for ready, then frontend.

- Exit 0: runtime ready; proceed to evidence capture and evaluator dispatch.
- Exit 3 (harness failure): retry once; if the retry fails, escalate to human without counting against the dispatch budget.
- Exit 1 or 2: an app/config issue; include the error in the evidence pack and evaluator context.

Runtimes stay up through evidence capture and stop after all evaluators finish. Evaluators interpret the captured evidence pack; they never interact with the running app.

### Re-run the load-bearing probe, and only that one

A DONE report's probe evidence is the implementer's account of its own work, so
some of it has to be reproduced rather than read. Reproducing *all* of it costs
as much as the implementation did; reproducing none of it means the loop's
central discipline rests on self-report.

Re-run **the probe the bean's criterion rests on** — the one whose failure is
the reason to believe the change does anything. Where a bean has two directions
that must hold independently (a rule fires / the same rule declines), that is
two load-bearing probes, and running both is also what shows a single mutation
does not fail them both.

Check the failure text matches what the report claimed it said. A probe that
fails by starving a fixture, timing out, or failing to build reads exactly like
a probe that fails by assertion in a summary, and only the assertion is
evidence.

Everything else — the fixture-integrity lanes, the probes for behaviour the
criterion does not name — is the evaluator's to judge from the evidence pack.

**Snapshot before you mutate, because the tree is not yours.** The implementer's
work is uncommitted when you probe it, so the file you are about to edit already
holds changes that exist nowhere else. `git checkout -- <file>` to undo a probe
reverts to HEAD and takes those with it — the probe and the work land in the same
bin, and the loss is silent. Take a real snapshot first and restore from that:

```bash
SNAP=$(git stash create)          # a commit object; touches no stash stack
# ...mutate, run, then:
git checkout "$SNAP" -- <file>    # the tree as it was, probe removed
```

Do this even for a one-line mutation you are sure you can undo by hand. The
mutation is easy to reverse; the forty lines of somebody else's prose sitting in
the same file are not, and you will not notice they are gone — the suite still
passes without them.

**A probe that fails to reproduce is a finding, not a formality.** If the
reported red set does not appear, do not assume you mis-applied the mutation:
re-read the fixture and work out what the world actually does. Twice this has
turned out to be a real gap — a claimed invariant with nothing holding it —
and in both cases the implementer had reasoned about the mutation rather than
running it.

## 1f. Dispatch Per-Domain Evaluator

One evaluator per resolved domain, processed in `runtime_order` if specified, otherwise the `domains` order. Select its provider:

    scripts/select-evaluator-provider.sh \
      --preference "<providers array joined with commas>" \
      --implementer claude > selected-provider-{domain}.json

The domain's `providers` array is an ordered preference list. Implementers
are always claude subagents, so the first available external provider wins;
with none available the evaluator runs on claude in a fresh context.

Assemble the evaluator's static context with the script rather than by listing files to concatenate, so the protocol, the domain template, the project calibration anchors, and the antipatterns arrive together in the order `skills/develop-loop/context-loading-order.md` describes:

```bash
scripts/assemble-evaluator-context.sh --domain {domain} > context-{domain}.txt
```

Both provider paths use that same file. Assembling by hand is how calibration goes missing: the anchors are optional files whose absence is silent, so an evaluator scored without them looks indistinguishable from one scored with them.

**Provider `claude`:** dispatch an evaluator subagent on the assembled context, passing `evidence-{domain}.txt` alongside it.

**External provider:** dispatch via the provider hook:

```bash
hooks/dispatch-provider.sh <provider> \
  --role evaluator \
  --topic "Evaluate domain: {domain}" \
  --instructions "$(cat context-{domain}.txt)" \
  --diff-file <diff-file> \
  --design-doc-file <design-doc-file> \
  --evidence-file evidence-{domain}.txt
```

Then add the run-state sections the script cannot know about (positions 4 through 7 of the loading order): runtime state for runtime-configured domains, the bean's task criteria, and on iteration 2+ the diff since BASE_SHA with the prior scorecard and its guidance. An external provider's whole reply is the scorecard JSON — see `skills/develop/provider-context.md` for the prompt and `skills/develop/scorecard-envelope.md` for the field names the graders accept.

Every evaluator returns one scorecard JSON with per-dimension scores under `.domains`, pass/fail criteria under `.criteria`, and a `"provider"` field naming its producer. Save it per domain and count the dispatch:

```bash
hooks/dispatch-provider.sh <provider> ... > scorecard-{domain}-{provider}.json
dispatch_count=$((dispatch_count + 1))
```

Redirect the hook's stdout straight to the file; never hand-copy a scorecard out of provider output. The hook emits the reply already extracted from whatever transport the CLI uses — a provider whose CLI streams structured events declares an `extract` mode in `orchestrate.json` (codex sets `codex-jsonl`, for the JSONL stream `codex exec --json` produces). When it finds no reply it exits non-zero with the raw stream on stderr, so an empty or truncated scorecard fails loudly instead of reaching the merge. Reading a scorecard out of an event stream by eye is how one arrives a closing brace short, or with `criteria` nested under `.domains`.

Dispatch accounting: one implementer + one evaluator per domain per iteration (2 domains = 3 dispatches). PASS_PENDING re-evaluation reuses the provider recorded in `selected-provider-{domain}.json`.

### Validate the Scorecard

Gate each scorecard before the merge:

```bash
scripts/validate-scorecard.sh --scorecard scorecard-{domain}-{provider}.json \
  --criteria-ids "<comma-separated criterion ids from the bean's eval block>"
```

This is the pre-flight for `check-thresholds.sh`: it checks the same fields the grader will need — numeric `score` and `threshold` on every scored dimension, string `id` and boolean `pass` on every criterion — so a mis-shaped envelope is caught where it was produced rather than three steps later at grading. The accepted names are listed once in `skills/develop/scorecard-envelope.md`.

On exit 2, stderr is a JSON array naming the gaps: re-dispatch that evaluator once with those errors in its context, and if its second scorecard also fails, mark the bean `needs-attention`, `rm -f .fiddle/active-bean`, and escalate. Merging a scorecard whose criteria ids or evidence do not hold up launders an unusable evaluation into a convergence decision. The re-dispatch counts against the budget like any other.

Dimension justifications may arrive under `evidence` or under `comment`; the validator accepts either, so field naming alone never triggers that re-dispatch.

### Runtime Stop

After all domain evaluators finish, run `scripts/stop-runtimes.sh --state <runtime-state-file>` so no processes outlive the evaluation.
