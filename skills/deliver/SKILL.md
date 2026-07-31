---
name: deliver
description: Run the DELIVER phase — drift analysis comparing design to implementation, documentation update via deliver-docs, product artifact generation (if configured), evaluator evolve (calibration/antipattern updates), and epic closure. Requires a completed epic.
---

# Deliver


## Usage

Invoke as `fiddle:deliver --epic <id>`.

Analyze design-vs-implementation drift, update documentation, and close the epic.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--epic <id>` | **required** | The epic to deliver |

### Config File

Read `orchestrate.json` (project root) if it exists. Extract:
- `providers.phases.deliver` — provider list (default: `["codex"]`)
- Provider declarations (`providers.<name>.command`, `.flags`)
- `providers.timeout` — attended/unattended timeouts
- `models.deliver` — model override for drift analysis
- `evaluators.spot_check.rate` — blind spot-check sampling rate (integer N; absent defaults to 5, 0 or less disables)
- `evaluators.aging.window_days` — age threshold for flagging calibration anchors (integer days; absent defaults to 90)
- `evaluators.aging.quiet_epics` — number of recent epics an anchor must go without a correction (or an antipattern without a detection) to be retired/collapsed (integer; absent defaults to 3)

## Steps

### Step 1: Validate Epic

```bash
beans show <epic-id> --json
```

Confirm it exists. Check child bean states — if beans are still `todo` or `in-progress`, warn: "Some beans are not complete. Proceed with delivery anyway?"

### Step 2: Drift Analysis

If providers are configured (default: codex), read `hooks/dispatch-provider.sh` for collection rules. For each provider:

```bash
# Write large content to temp files first
DESIGN_FILE=$(mktemp /tmp/design-XXXX.md)
DIFF_FILE=$(mktemp /tmp/diff-XXXX.txt)
# <write design doc to $DESIGN_FILE, git diff to $DIFF_FILE>

hooks/dispatch-provider.sh <provider> \
  --role "Drift analyst" \
  --topic "Design vs implementation drift for <epic-id>" \
  --design-doc-file "$DESIGN_FILE" \
  --diff-file "$DIFF_FILE" \
  --instructions "Analyze: did the implementation match the design? Flag any drift, missing features, scope creep, or unintended changes."
```

Run provider dispatches in parallel when the harness supports it; otherwise run them sequentially. Collect results in **attended** mode.

If no provider CLI is available, perform the drift analysis yourself: read the design doc, review the full diff, and compare.

Present the drift analysis to the user:
```
"Drift analysis complete:
- Implemented as designed: [list]
- Drift detected: [list with explanations]
- Missing from design: [list]
- Added beyond design: [list]

Proceed with documentation update?"
```

Wait for user confirmation before proceeding.

### Step 3: Documentation Update

Invoke deliver-docs:
Use the `fiddle:deliver-docs` skill with `--epic <epic-id>`.

This updates SYSTEM.md, creates ADRs for architectural decisions, and appends to BACKLOG.md.

Present the deliver-docs results to the user for confirmation. Wait for approval.

### Step 4: Product Artifact Generation

Skip this step if `deliver.product_artifacts` is not configured in `orchestrate.json`, or if the `artifacts` array is empty or missing.

#### Configuration

Read from `orchestrate.json`:
```json
"deliver": {
  "product_artifacts": {
    "templates_path": "docs/product/templates",
    "output_path": "docs/releases",
    "artifacts": ["release-notes", "social"]
  }
}
```

- `templates_path` — directory containing one markdown file per artifact type (e.g., `release-notes.md`, `social.md`). Each file is **instructions** for generating that artifact — voice, format, audience, examples. The project supplies these.
- `output_path` — where generated artifacts are written
- `artifacts` — which artifact types to generate (must match template filenames without extension)

#### Process

Create `<output_path>` directory if it does not exist.

For each artifact type in `artifacts`:

1. Read the template from `<templates_path>/<artifact-type>.md`. If the template file does not exist, warn: "Template missing for `<artifact-type>` at `<expected-path>`. Skipping." and continue with remaining artifacts.
2. Gather context:
   - Design spec — read the epic bean body (`beans show <epic-id>`), find the line starting with `Design:` and use that path. If no `Design:` line, look for a `Plan:` line and check for a sibling `-design.md` file in the same directory.
   - Drift analysis results (from Step 2 — "implemented as designed" and "added beyond design" are the most useful)
   - Git diff summary
   - Product docs — if they exist, read `docs/product/VISION.md` and `docs/product/GTM.md` for voice/positioning context. These are optional.
3. Generate the artifact following the template's instructions, using the gathered context
4. Write to `<output_path>/YYYY-MM-DD-<epic-id>-<artifact-type>.md`. Overwrite if the file already exists.

Present all generated artifacts to the user:
```
"Product artifacts generated:
- Release notes: <path>
- Social copy: <path>

Review and confirm?"
```

Wait for user confirmation. Apply any edits the user requests before proceeding.

### Step 5: Evaluator Evolve

After documentation is confirmed, review the evaluation artifacts from this run.

#### 5.0 Blind Spot-Check

<HARD-GATE>
This step runs BEFORE 5a. Step 5a presents the evaluator scorecards, which would anchor the human's judgment on the evaluator's own evidence. The blind spot-check MUST complete first so the human scores a sample of converged beans cold.

Run the blind spot-check following: `skills/deliver/blind-spot-check.md`

Read the sampling rate from `evaluators.spot_check.rate` in `orchestrate.json` (integer N — review every Nth converged bean). If the key is absent, default to 5. If the value is 0 or less, skip this step and proceed to 5a.

Do NOT reveal any evaluator scorecard (in this step or by starting 5a) before the human has committed blind scores for the sampled beans.
</HARD-GATE>

Per-dimension divergence between the human's blind scores and the evaluator scores is recorded in each sampled bean's Evaluation Log (via `append-eval-log.sh --corrections`) and encoded as calibration anchors in the same format as attended-gate corrections. Carry the divergence summary into the 5e summary.

#### 5a. Review Scorecards

Collect all evaluator scorecards produced during the epic (stored in `.beans/` eval-log beans).
Present them to the user:

```
"Here are the evaluator scorecards from this run:

[scorecard summary per task — dimension, score, evidence]

Where did the evaluator get it wrong?"
```

Wait for user corrections before proceeding.

#### 5b. Update Calibration

For each correction the user provides, append an anchor to the matching calibration file `docs/evaluator-calibration-<domain>.md`:

```markdown
## [dimension] — Correction (YYYY-MM-DD)
**Evaluator scored:** X/10 — "[evaluator evidence]"
**Human corrected to:** Y/10 — "[human reason]"
**Anchor:** For this project, score Y means: [human's description]
```

If the calibration file does not exist yet, create it with a top-level heading `# Evaluator Calibration — <domain>`.

After writing calibration anchors, ensure `orchestrate.json` has `evaluators.domains.<domain>.calibration` set to the file path (e.g., `docs/evaluator-calibration-<domain>.md`). This wires the calibration file into the develop loop so evaluators receive it on future runs.

#### 5c. Add Antipatterns

For each real failure found post-delivery (bugs, regressions, missed requirements), append to `docs/antipatterns-<domain>.md`:

```markdown
## [antipattern-id] (YYYY-MM-DD)
**Pattern:** What the failure looks like
**Example:** Concrete code/behavior from this run
**Fix:** How to avoid it
```

If the antipattern file does not exist yet, create it with a top-level heading `# Antipatterns — <domain>`.

After writing antipatterns, ensure `orchestrate.json` has `evaluators.domains.<domain>.antipatterns` set to the file path (e.g., `docs/antipatterns-<domain>.md`). This wires the antipattern file into the develop loop so both implementer and evaluator receive it on future runs.

#### 5d. Adjust Thresholds

If the evaluator was consistently too strict or too lenient across multiple tasks:
- Update the relevant threshold in `orchestrate.json` at `evaluators.domains.<domain>.thresholds`
- Present the change to the user for confirmation before writing

#### 5e. Review Iteration Counts

High iteration counts (>5 develop-evaluate cycles on a single task) suggest calibration gaps. Identify dimensions that caused the most iterations and focus calibration updates (4b) on those dimensions.

#### 5f. Review Longitudinal Decay Trends

Drift analysis (Step 2) and per-task evaluators only see one epic at a time. Architectural decay shows up across epics: the codebase getting harder to work in, evaluators quietly disagreeing more. Rising dispatches-to-convergence for similar-sized epics is the earliest slop signal available.

<HARD-GATE>
Do NOT aggregate eval-log history by hand or eyeball the beans. You MUST run the trend script, which reads the "## Evaluation Log" sections from every task bean's body across all epics and computes the aggregates and cross-epic direction verdicts:

```bash
scripts/trend-eval-history.sh --beans-path <beans-path>
```

(Omit `--beans-path` only when running from the repo root where `.beans` is discoverable; always pass it from a worktree.)

Read the JSON it emits. Do not reconstruct any number the script already reports.
</HARD-GATE>

The output carries per-epic aggregates ordered oldest to newest (mean/max dispatches-to-convergence, mean iterations, per-dimension mean scores, disagreement count), a `trends` array comparing consecutive epics, and a top-level `alarm` flag with `alarm_reasons`.

Present the trend to the user:

```
"Longitudinal decay trends (oldest → newest epic):
- Dispatches-to-convergence: [from → to] ([direction])
- Iterations: [from → to] ([direction])
- Per-dimension scores: [dimension: from → to (direction), ...]
- Provider disagreements: [from → to] ([direction])

Decay alarm: [RAISED — <alarm_reasons> | none]"
```

If `alarm` is `true`, a metric declined across the two most recent consecutive epics (dispatches up, a dimension score down, or disagreements up). Treat it as a signal that calibration, thresholds, or scope discipline need attention before the next epic; fold the affected dimensions into the calibration updates from 5b. If `trends` is `null` (fewer than two epics have eval data yet), report that there is not enough history to trend and continue.

#### 5g. Age and Prune Anchors and Antipatterns

Calibration and antipattern files are append-only, so they grow without bound and old entries mis-calibrate current evaluators: an anchor encodes human judgment against the evaluator and model version at a specific time, and models change underneath it. Run this pass after the 5b/5c writes so newly added anchors and antipatterns are considered too.

Read `evaluators.aging.window_days` (default 90) and `evaluators.aging.quiet_epics` (default 3) from `orchestrate.json`.

**Flag stale anchors.** For each domain, locate its calibration file (`evaluators.domains.<domain>.calibration`, else the default `docs/evaluator-calibration-<domain>.md`). Scan only the anchor blocks above the `## Retired` section (never re-process already-retired content). For each `## [dimension] — Correction (YYYY-MM-DD)` block, parse the heading date; flag the anchor when it is older than `window_days` from today.

**Decide retire vs. re-anchor.** For each flagged anchor, check whether the evaluator has been correcting on that anchor's dimension recently. Read the Human Corrections recorded in eval logs (task-bean `## Evaluation Log` sections) and attended-gate corrections from the last `quiet_epics` epics:
- No corrections against that dimension across the last `quiet_epics` epics → the evaluator agrees consistently; **retire** the anchor.
- One or more corrections against that dimension → keep it, but note it as a **stale reference still correcting** so it can be re-anchored against current evaluator behavior (fold into 5b).

**Collapse quiet antipatterns.** For each domain, locate its antipattern file (`evaluators.domains.<domain>.antipatterns`, else `docs/antipatterns-<domain>.md`). Scan the `## [antipattern-id] (YYYY-MM-DD)` blocks above the `## Retired` section. Read the **Antipatterns detected:** sections recorded in eval logs (task-bean `## Evaluation Log` sections, written by `append-eval-log.sh --antipatterns` at develop-loop step 1l) from the last `quiet_epics` epics. An antipattern whose ID appears in no such section across those epics is **collapsed** (moved to the archive).

**Auditability.** Retired anchors and collapsed antipatterns are never deleted: MOVE each block, verbatim, to a `## Retired` section at the bottom of the same file (create the heading if absent), appending a line `**Retired YYYY-MM-DD:** [reason]`. Keeping retired content in the same file (rather than a separate `docs/…-archive.md`) preserves the single path already wired into `orchestrate.json`, so no new configuration is needed; the `## Retired` heading is the boundary that keeps retired content out of evaluator context.

Retired content MUST NOT be loaded into evaluator context: the calibration load (context-loading-order position 3) and the `{ANTIPATTERNS}` injection stop reading at the `## Retired` heading.

**Present and confirm.** Like the other step-5 sub-steps, present the proposed changes and wait for confirmation before writing:
```
"Aging pass (window: <window_days>d, quiet: <quiet_epics> epics):
- Anchors flagged stale: [dimension (date), ...]
- Anchors to retire (evaluator agrees): [dimension (date), ...]
- Anchors to keep for re-anchoring (still correcting): [dimension (date), ...]
- Antipatterns to collapse (undetected): [antipattern-id (date), ...]

Apply these retirements?"
```

Wait for user confirmation before moving or writing anything.

Present a summary:
```
"Evaluator evolve complete:
- Blind spot-check: [beans sampled] sampled, [count] dimensions diverged
- Calibration anchors added: [count]
- Antipatterns recorded: [count]
- Threshold adjustments: [list or 'none']
- High-iteration tasks: [list or 'none']
- Longitudinal decay: [alarm RAISED — <reasons> | no alarm | not enough history]
- Aging: [count] anchors flagged, [count] retired, [count] antipatterns collapsed

Proceed to close epic?"
```

Wait for user confirmation.

### Step 6: Close Epic

After user confirms evaluator evolve:
```bash
beans update <epic-id> --status completed
```

### Step 7: Archive

Invoke archive to clean up stale artifacts:
Use the `fiddle:archive` skill with `--epic <epic-id>`.
