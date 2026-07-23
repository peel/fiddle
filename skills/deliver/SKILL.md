---
name: fiddle:deliver
description: Run the DELIVER phase — drift analysis comparing design to implementation, documentation update via deliver-docs, product artifact generation (if configured), evaluator evolve (calibration/antipattern updates), and epic closure. Requires a completed epic.
argument-hint: --epic <id>
---

# Deliver

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

Fire all providers in parallel (`run_in_background: true`). Collect results in **attended** mode.

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
```
Skill(skill: "fiddle:deliver-docs", args: "--epic <epic-id>")
```

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

Present a summary:
```
"Evaluator evolve complete:
- Blind spot-check: [beans sampled] sampled, [count] dimensions diverged
- Calibration anchors added: [count]
- Antipatterns recorded: [count]
- Threshold adjustments: [list or 'none']
- High-iteration tasks: [list or 'none']

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
```
Skill(skill: "fiddle:archive", args: "--epic <epic-id>")
```
