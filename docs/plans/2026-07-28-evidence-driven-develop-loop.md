# Evidence-Driven Develop Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use fiddle:develop to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-domain, per-provider adversarial evaluation with one evidence-focused evaluator per domain, selected cross-provider by preference, fed by a pre-gathered evidence pack, with single-pass convergence for evidence-only domains and a Stop-hook verdict gate.

**Architecture:** New provider-selection and Stop-hook scripts plus targeted edits to check-convergence.sh, dispatch-provider.sh, develop-loop skill docs, and evaluator templates. The criteria/dimensions split in check-thresholds.sh already models evidence vs judgment; no schema changes. Spec: `docs/specs/2026-07-28-evidence-driven-develop-loop-design.md`.

**Tech Stack:** bash + jq scripts with `test-*.sh` harnesses (assert_exit/assert_json pattern), markdown skill files, Claude Code hooks (hooks/hooks.json).

---

### Task 1: Evaluator provider selection script

**Files:**
- Create: `scripts/select-evaluator-provider.sh`
- Test: `scripts/test-select-evaluator-provider.sh`

- [ ] **Step 1: Write the failing test**

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"; fi
}
assert_json() {
  local desc="$1" field="$2" expected="$3" json="$4"
  local actual; actual=$(echo "$json" | jq -r "$field")
  if [ "$expected" = "$actual" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"; fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT
mkdir -p "$TMPDIR/bin"
printf '#!/bin/sh\nexit 0\n' > "$TMPDIR/bin/codex"; chmod +x "$TMPDIR/bin/codex"

echo "Test 1: external provider available and differs from implementer"
OUT=$(PATH="$TMPDIR/bin:/usr/bin:/bin" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,claude" --implementer claude); EXIT_CODE=$?
assert_exit "selection exits 0" 0 "$EXIT_CODE"
assert_json "picks codex" ".provider" "codex" "$OUT"

echo "Test 2: unavailable external falls back to implementer provider"
OUT=$(PATH="/usr/bin:/bin" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,claude" --implementer claude); EXIT_CODE=$?
assert_exit "fallback exits 0" 0 "$EXIT_CODE"
assert_json "falls back to claude" ".provider" "claude" "$OUT"
assert_json "reason mentions fallback" '.reason | test("fallback")' "true" "$OUT"

echo "Test 3: preference order respected among differing providers"
printf '#!/bin/sh\nexit 0\n' > "$TMPDIR/bin/gemini"; chmod +x "$TMPDIR/bin/gemini"
OUT=$(PATH="$TMPDIR/bin:/usr/bin:/bin" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "gemini,codex" --implementer claude)
assert_json "first preferred wins" ".provider" "gemini" "$OUT"

echo "Test 4: missing --preference is invalid input"
EXIT_CODE=0
"$SCRIPT_DIR/select-evaluator-provider.sh" --implementer claude 2>/dev/null || EXIT_CODE=$?
assert_exit "missing preference exits 2" 2 "$EXIT_CODE"

echo "Test 5: empty preference list still returns claude"
OUT=$(PATH="/usr/bin:/bin" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference " " --implementer claude)
assert_json "defaults to claude" ".provider" "claude" "$OUT"

echo; echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash scripts/test-select-evaluator-provider.sh`
Expected: FAIL (script not found / non-zero exit)

- [ ] **Step 3: Write the implementation**

```bash
#!/usr/bin/env bash
# select-evaluator-provider.sh — Pick the evaluator provider for a domain.
# The providers list is an ordered preference; the first available provider
# that differs from the implementer's provider wins. Fallbacks: the
# implementer's provider (fresh context), then claude.
# Exit 0 = selected, 2 = invalid input.
set -euo pipefail

PREFERENCE="" IMPLEMENTER="claude"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --preference) PREFERENCE="$2"; shift 2;;
    --implementer) IMPLEMENTER="$2"; shift 2;;
    *) echo '{"error":"unknown argument: '"$1"'"}' >&2; exit 2;;
  esac
done
[[ -n "$PREFERENCE" ]] || { echo '{"error":"missing --preference"}' >&2; exit 2; }

available() {
  local p="$1"
  [[ "$p" == "claude" ]] && return 0
  command -v "$p" &>/dev/null
}

emit() { jq -n --arg provider "$1" --arg reason "$2" '{"provider":$provider,"reason":$reason}'; }

IFS=',' read -ra PROVIDERS <<< "$PREFERENCE"
FALLBACK=""
for p in "${PROVIDERS[@]}"; do
  p="$(echo "$p" | xargs)"
  [[ -z "$p" ]] && continue
  available "$p" || continue
  if [[ "$p" != "$IMPLEMENTER" ]]; then
    emit "$p" "first available provider differing from implementer"
    exit 0
  fi
  [[ -z "$FALLBACK" ]] && FALLBACK="$p"
done

if [[ -n "$FALLBACK" ]]; then
  emit "$FALLBACK" "fallback: implementer provider in a fresh context"
  exit 0
fi
emit "claude" "fallback: no configured provider available"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bash scripts/test-select-evaluator-provider.sh`
Expected: `Results: 8 passed, 0 failed`, exit 0

- [ ] **Step 5: Commit**

```bash
git add scripts/select-evaluator-provider.sh scripts/test-select-evaluator-provider.sh
git commit -m "Add evaluator provider selection by ordered preference"
```

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: preference-order-respected
      check: "select-evaluator-provider.sh returns the first available provider differing from --implementer, in list order"
    - id: fallback-chain
      check: "Unavailable externals fall back to the implementer provider, then to claude; reasons distinguish the cases"
    - id: invalid-input-exit-2
      check: "Missing --preference exits 2 with a JSON error on stderr"
thresholds: {}
```

### Task 2: Evidence file support in dispatch-provider.sh

**Files:**
- Modify: `hooks/dispatch-provider.sh`
- Test: `scripts/test-dispatch-evidence.sh`

- [ ] **Step 1: Write the failing test**

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../hooks/dispatch-provider.sh"
PASS=0; FAIL=0

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qF "$needle"; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (missing '$needle')"; fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT
echo "TestOutput: 12 passed, 0 failed" > "$TMPDIR/evidence.txt"

# Fake provider that echoes its stdin back, so we can inspect the payload.
mkdir -p "$TMPDIR/bin"
printf '#!/bin/sh\ncat\n' > "$TMPDIR/bin/echoprov"; chmod +x "$TMPDIR/bin/echoprov"

echo "Test 1: --evidence-file content lands in the provider payload"
OUT=$(PATH="$TMPDIR/bin:$PATH" FIDDLE_PROVIDER_CMD_echoprov="echoprov" "$HOOK" echoprov \
  --role evaluator --topic "t" --instructions "i" \
  --evidence-file "$TMPDIR/evidence.txt" 2>/dev/null || true)
assert_contains "payload has evidence section" "## Evidence" "$OUT"
assert_contains "payload has evidence content" "TestOutput: 12 passed" "$OUT"

echo; echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
```

Note: if dispatch-provider.sh resolves provider commands from orchestrate.json rather than an env override, adapt the test to write a minimal `orchestrate.json` into `$TMPDIR` declaring `echoprov` with `command: "echoprov"`, and run the hook with `CLAUDE_PROJECT_DIR="$TMPDIR"` (or cwd there) — match how the hook already resolves commands; do not add an env mechanism just for the test.

- [ ] **Step 2: Run test to verify it fails**

Run: `bash scripts/test-dispatch-evidence.sh`
Expected: FAIL (no `## Evidence` section in payload)

- [ ] **Step 3: Add --evidence-file to the argument parser and payload**

In `hooks/dispatch-provider.sh`, alongside the existing `--diff-file` handling (`--diff-file) DIFF="$(cat "$2")"; shift 2 ;;`), add:

```bash
    --evidence-file) EVIDENCE="$(cat "$2")"; shift 2 ;;
```

Initialize `EVIDENCE=""` next to the other variable declarations. Where the payload/prompt is assembled (the same place the `## Diff`/diff content is appended), append:

```bash
if [[ -n "$EVIDENCE" ]]; then
  PAYLOAD+=$'\n\n## Evidence\n'"$EVIDENCE"
fi
```

Match the exact variable name used for the assembled prompt in the existing code (follow the diff-section pattern verbatim).

- [ ] **Step 4: Run test to verify it passes**

Run: `bash scripts/test-dispatch-evidence.sh`
Expected: `Results: 2 passed, 0 failed`, exit 0

- [ ] **Step 5: Commit**

```bash
git add hooks/dispatch-provider.sh scripts/test-dispatch-evidence.sh
git commit -m "Pass evidence pack to external evaluators via --evidence-file"
```

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: evidence-in-payload
      check: "dispatch-provider.sh --evidence-file appends an '## Evidence' section with the file content to the provider payload"
    - id: no-regression-existing-args
      check: "--diff-file and --design-doc-file behavior is unchanged (existing dispatch tests still pass)"
thresholds: {}
```

### Task 3: Single-pass convergence for evidence-only domains

**Files:**
- Modify: `scripts/check-convergence.sh`
- Test: `scripts/test-check-convergence.sh`

- [ ] **Step 1: Write the failing tests (append to existing test file; do not modify existing assertions)**

```bash
echo "Test N: PASS with empty dimensions map → CONVERGED on first pass"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],"dimensions":{}}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "evidence-only first pass → exit 0" 0 "$EXIT_CODE"
assert_json "status CONVERGED" ".status" "CONVERGED" "$OUTPUT"
assert_json "mode evidence-only" ".mode" "evidence-only" "$OUTPUT"

echo "Test N+1: PASS with populated dimensions still requires double-pass"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],"dimensions":{"general.correctness":8}}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "judgment first pass → exit 1" 1 "$EXIT_CODE"
assert_json "status PASS_PENDING" ".status" "PASS_PENDING" "$OUTPUT"

echo "Test N+2: evidence-only FAIL still fails"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"FAIL","failing_dimensions":[],"failing_criteria":["tests-pass"],"dimensions":{}}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
assert_exit "evidence-only fail → exit 1" 1 "$EXIT_CODE"
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `bash scripts/test-check-convergence.sh`
Expected: existing tests PASS; new "evidence-only first pass" assertions FAIL (status PASS_PENDING, exit 1)

- [ ] **Step 3: Implement evidence-only short-circuit**

In `scripts/check-convergence.sh`, directly after `VERDICT` is read and the `!= "PASS"` FAIL branch, insert:

```bash
# Evidence-only verdicts (no scored dimensions) converge on the first pass:
# re-running the same checks on unchanged code re-measures the same facts.
DIM_COUNT=$(jq '.dimensions // {} | length' "$CURRENT")
if [[ "$DIM_COUNT" -eq 0 ]]; then
  echo '{"status":"CONVERGED","mode":"evidence-only"}'
  exit 0
fi
```

- [ ] **Step 4: Run tests to verify all pass**

Run: `bash scripts/test-check-convergence.sh`
Expected: all assertions pass, exit 0

- [ ] **Step 5: Commit**

```bash
git add scripts/check-convergence.sh scripts/test-check-convergence.sh
git commit -m "Converge evidence-only verdicts on a single pass"
```

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: evidence-only-single-pass
      check: "PASS verdict with empty dimensions map returns CONVERGED (exit 0) with empty history"
    - id: judgment-double-pass-retained
      check: "PASS verdict with populated dimensions still returns PASS_PENDING on first pass"
    - id: existing-behavior-untouched
      check: "All pre-existing test-check-convergence.sh assertions pass unmodified"
thresholds: {}
```

### Task 4: Rework scorecard-merge.md to single-evaluator flow

**Files:**
- Modify: `skills/develop-loop/scorecard-merge.md`
- Test: `scripts/test-merge-scorecards.sh` (existing; add single-input pin)

- [ ] **Step 1: Add a pinning test for single-input normalization (append; do not modify existing assertions)**

```bash
echo "Test N: single-element array normalizes without min-merge artifacts"
cat > "$TMPDIR/single.json" << 'EOF'
[{"provider":"codex","domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[{"id":"tests-pass","pass":true}]}]
EOF
OUT=$("$SCRIPT_DIR/merge-scorecards.sh" < "$TMPDIR/single.json" 2>/dev/null)
assert_json "score preserved" ".domains.general.dimensions.correctness.score" "8" "$OUT"
assert_json "criteria preserved" ".criteria[0].pass" "true" "$OUT"
```

Run: `bash scripts/test-merge-scorecards.sh` — if this already passes, keep the test as a regression pin and move on; no script change needed.

- [ ] **Step 2: Rewrite the Per-Domain Provider Merge section**

Replace the "Per-Domain Provider Merge (Step 1g)" section of `skills/develop-loop/scorecard-merge.md` with:

```markdown
## Per-Domain Normalization (Step 1g)

Each domain has exactly one evaluator scorecard. Run it through
merge-scorecards.sh as a single-element array so downstream consumers see a
uniform shape:

    jq -s '.' scorecard-{domain}-{provider}.json | \
      scripts/merge-scorecards.sh > scorecard-{domain}.json

Provider min-merging and disagreement tracking apply only to holistic review
(see skills/develop-holistic/SKILL.md). The per-task path has no
disagreements file; pass nothing to --disagreements in the eval-log step.
```

Keep the Spec-Defect Check section, updated to scan the single per-domain scorecard (`scorecard-{domain}-{provider}.json`) instead of a `scorecard-{domain}-*.json` glob over multiple providers. Keep the Cross-Domain Merge section unchanged.

- [ ] **Step 3: Verify consistency**

Run: `grep -n "min\|disagreement" skills/develop-loop/scorecard-merge.md`
Expected: only the holistic pointer and spec-defect text remain; no per-task min-scoring instructions.

- [ ] **Step 4: Commit**

```bash
git add skills/develop-loop/scorecard-merge.md scripts/test-merge-scorecards.sh
git commit -m "Reduce per-task scorecard merge to single-input normalization"
```

```eval
domains: [general]
criteria:
  general:
    - id: single-input-pinned
      check: "test-merge-scorecards.sh pins single-element normalization preserving scores and criteria"
    - id: no-per-task-min-merge
      check: "scorecard-merge.md no longer instructs per-task provider min-merging or disagreement tracking; cross-domain merge and spec-defect check remain"
thresholds: {}
```

### Task 5: Evidence-first evaluator templates

**Files:**
- Modify: `skills/evaluate/evaluator-general.md`
- Modify: `skills/evaluate/evaluator-infrastructure.md`
- Modify: `skills/evaluate/SKILL.md`

- [ ] **Step 1: Restructure evaluator-general.md**

Add at the top, after the title:

```markdown
## Evidence Pack

You receive an evidence pack alongside the diff: test output, invariant
results, and runtime probe transcripts gathered before your dispatch. Your
job is to interpret this evidence against the task's criteria. You do not
gather evidence, and you do not judge qualities the evidence cannot show.

Every criterion verdict MUST cite the evidence artifact that supports it
(file name and the relevant line/excerpt). A criterion with no supporting
evidence is scored fail with reason "no evidence".

## Dimensions (optional)

Scored dimensions are OPTIONAL for this domain. Include the `dimensions`
object in your scorecard only when the task's eval block sets thresholds for
this domain. When no thresholds are set, emit `"dimensions": {}` — the task
converges on evidence criteria alone.
```

Keep the existing dimension definitions (Correctness, Domain Spec Fidelity, Code Quality) under the optional section as the definitions to use when thresholds are configured.

- [ ] **Step 2: Apply the same restructure to evaluator-infrastructure.md**

Same two sections inserted after the title; existing dimension definitions preserved below as the when-configured definitions.

- [ ] **Step 3: Update the scorecard contract in skills/evaluate/SKILL.md**

Where the scorecard JSON schema is described, state: `dimensions` may be an empty object for evidence-only evaluation; `criteria[]` entries gain a required `evidence` field citing the artifact; the `provider` field remains required.

- [ ] **Step 4: Verify internal consistency**

Run: `grep -rn "dimensions" skills/evaluate/*.md | grep -iv "optional\|empty\|when.*threshold" | head`
Expected: no remaining text asserting dimensions are always required.

- [ ] **Step 5: Commit**

```bash
git add skills/evaluate/evaluator-general.md skills/evaluate/evaluator-infrastructure.md skills/evaluate/SKILL.md
git commit -m "Make evaluator templates evidence-first with optional dimensions"
```

```eval
domains: [general]
criteria:
  general:
    - id: evidence-citation-required
      check: "Templates require each criterion verdict to cite its evidence artifact; missing evidence scores fail"
    - id: dimensions-optional
      check: "Templates and evaluate SKILL.md state dimensions are emitted only when the eval block sets thresholds; empty dimensions object otherwise"
    - id: role-boundary
      check: "Templates say the evaluator interprets pre-gathered evidence and does not gather it or judge beyond it"
thresholds: {}
```

### Task 6: Rework develop-loop SKILL.md to the simplified flow

**Files:**
- Modify: `skills/develop-loop/SKILL.md`

- [ ] **Step 1: Add evidence-pack and provider-selection steps**

Insert a new step between 1e and 1f (number it 1e-2):

```markdown
## 1e-2. Gather Evidence Pack (per domain)

For each resolved domain, before any evaluator dispatch:
1. Start runtimes if configured (existing Runtime Start gate moves here).
2. Run the project's test command and capture full output to
   `evidence-{domain}-tests.txt`.
3. Run invariant/validation scripts named in the bean's eval block, capturing
   output to `evidence-{domain}-checks.txt`.
4. Probe the running app for runtime-configured domains (existing runtime
   evidence protocol) into `evidence-{domain}-runtime.txt`.
5. Concatenate into `evidence-{domain}.txt` with one `### <source>` header per
   section.

The evaluator interprets this pack; it does not gather evidence itself.
```

- [ ] **Step 2: Replace step 1f dispatch with single-evaluator selection**

Replace the "Per-Domain, Per-Provider Evaluator Dispatch" content with:

```markdown
### Per-Domain Evaluator Dispatch (single evaluator)

For each resolved domain, select the evaluator provider:

    scripts/select-evaluator-provider.sh \
      --preference "<providers array joined with commas>" \
      --implementer claude > selected-provider.json

The domain's `providers` array is an ordered preference list. Implementers
are always claude subagents, so the first available external provider wins;
with none available the evaluator runs on claude in a fresh context.

Dispatch ONE evaluator for the domain with the selected provider:
- claude: evaluator subagent per skills/evaluate/SKILL.md with the domain
  template, passing the evidence pack file in context.
- external: hooks/dispatch-provider.sh <provider> --role evaluator
  --instructions "$(cat skills/evaluate/{template}.md)"
  --diff-file <diff-file> --design-doc-file <design-doc-file>
  --evidence-file evidence-{domain}.txt

Dispatch accounting: one implementer + one evaluator per domain per
iteration (e.g. 2 domains = 3 dispatches per iteration).
PASS_PENDING re-evaluation reuses the provider recorded in
selected-provider.json for the pass being confirmed.
```

- [ ] **Step 3: Update 1g reference and accounting text**

Point 1g at the reworked scorecard-merge.md ("Per-Domain Normalization"). Remove the "2 providers x 2 domains = 4 dispatches" examples in 1f and 1l; replace with the single-evaluator accounting. Update the 1l eval-log instruction: `--disagreements` is omitted on the per-task path, and the log entry records the selected provider and reason from selected-provider.json (this captures fallback substitutions).

- [ ] **Step 4: Verify no stale multi-provider references**

Run: `grep -n "per-provider\|each provider\|providers array\" \|min scoring" skills/develop-loop/SKILL.md`
Expected: only the preference-list semantics remain; no instruction to dispatch every provider. Then run: `bash scripts/check-portability.sh` — expected exit 0.

- [ ] **Step 5: Commit**

```bash
git add skills/develop-loop/SKILL.md
git commit -m "Dispatch one evidence-fed evaluator per domain in develop-loop"
```

```eval
domains: [general]
criteria:
  general:
    - id: evidence-pack-step
      check: "SKILL.md has a per-domain evidence-pack step (tests, checks, runtime probes) before evaluator dispatch"
    - id: single-dispatch
      check: "Exactly one evaluator per domain per iteration, provider chosen via select-evaluator-provider.sh; accounting examples updated"
    - id: no-stale-fanout
      check: "No remaining instruction dispatches evaluators for every provider on the per-task path"
thresholds: {}
```

### Task 7: Stop-hook verdict gate

**Files:**
- Create: `hooks/develop-verdict-gate.sh`
- Modify: `hooks/hooks.json`
- Modify: `skills/develop-loop/SKILL.md` (marker writes)
- Test: `scripts/test-develop-verdict-gate.sh`

- [ ] **Step 1: Write the failing test**

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../hooks/develop-verdict-gate.sh"
PASS=0; FAIL=0

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"; fi
}
assert_json() {
  local desc="$1" field="$2" expected="$3" json="$4"
  local actual; actual=$(echo "$json" | jq -r "$field")
  if [ "$expected" = "$actual" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"; fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo "Test 1: no marker → allow (fail-open, empty output)"
OUT=$(CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK"); EXIT_CODE=$?
assert_exit "no marker exits 0" 0 "$EXIT_CODE"
[ -z "$OUT" ] && { PASS=$((PASS+1)); echo "  PASS: empty output"; } || { FAIL=$((FAIL+1)); echo "  FAIL: expected empty output"; }

echo "Test 2: active marker → block with reason naming the bean"
mkdir -p "$TMPDIR/.fiddle"
echo "fiddle-sip9" > "$TMPDIR/.fiddle/active-bean"
OUT=$(CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK"); EXIT_CODE=$?
assert_exit "marker exits 0" 0 "$EXIT_CODE"
assert_json "decision block" ".decision" "block" "$OUT"
assert_json "reason names bean" '.reason | test("fiddle-sip9")' "true" "$OUT"

echo "Test 3: empty marker file → allow (fail-open)"
: > "$TMPDIR/.fiddle/active-bean"
OUT=$(CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK"); EXIT_CODE=$?
assert_exit "empty marker exits 0" 0 "$EXIT_CODE"
[ -z "$OUT" ] && { PASS=$((PASS+1)); echo "  PASS: empty output"; } || { FAIL=$((FAIL+1)); echo "  FAIL: expected empty output"; }

echo; echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash scripts/test-develop-verdict-gate.sh`
Expected: FAIL (hook script missing)

- [ ] **Step 3: Implement the hook**

```bash
#!/usr/bin/env bash
# Stop hook: while a develop-loop bean is active without a terminal verdict,
# block turn-end so the loop continues. Fail-open on any missing dependency.
set -uo pipefail

MARKER="${CLAUDE_PROJECT_DIR:-.}/.fiddle/active-bean"
[[ -f "$MARKER" ]] || exit 0
BEAN_ID="$(cat "$MARKER" 2>/dev/null | head -1 | tr -d '[:space:]')"
[[ -n "$BEAN_ID" ]] || exit 0
command -v jq &>/dev/null || exit 0

jq -n --arg reason \
  "develop-loop bean $BEAN_ID has no terminal verdict. Continue the loop: run the evaluation chain to CONVERGED, or record needs-attention escalation, then clear .fiddle/active-bean." \
  '{"decision":"block","reason":$reason}'
exit 0
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bash scripts/test-develop-verdict-gate.sh`
Expected: `Results: 7 passed, 0 failed`, exit 0

- [ ] **Step 5: Register the hook**

In `hooks/hooks.json`, add a top-level `Stop` entry alongside the existing hook groups:

```json
"Stop": [
  {
    "hooks": [
      {
        "type": "command",
        "command": "bash \"${CLAUDE_PLUGIN_ROOT}/hooks/develop-verdict-gate.sh\"",
        "timeout": 5
      }
    ]
  }
]
```

Validate: `jq . hooks/hooks.json` — expected: parses, exit 0.

- [ ] **Step 6: Wire marker lifecycle into develop-loop SKILL.md**

In step 1b (Initialize Evaluation Log), add:

```bash
mkdir -p .fiddle && echo "{id}" > .fiddle/active-bean
```

Clear the marker (`rm -f .fiddle/active-bean`) at every terminal exit: 1m CONVERGED and DISPATCHES_EXCEEDED rows, the 1e BLOCKED and SPEC_DEFECT exits, and the 1g-1h spec-defect gate. Add `.fiddle/` to `.gitignore` if not already ignored.

- [ ] **Step 7: Commit**

```bash
git add hooks/develop-verdict-gate.sh hooks/hooks.json scripts/test-develop-verdict-gate.sh skills/develop-loop/SKILL.md .gitignore
git commit -m "Gate turn-end on develop-loop terminal verdicts via Stop hook"
```

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: block-when-active
      check: "Hook emits {decision: block} naming the bean when .fiddle/active-bean is non-empty"
    - id: fail-open
      check: "Hook exits 0 with empty output when marker is missing, empty, or jq is unavailable"
    - id: marker-lifecycle
      check: "develop-loop SKILL.md writes the marker at 1b and clears it on every terminal exit path"
thresholds: {}
```

### Task 8: Plan critique round in write-plan

**Files:**
- Modify: `skills/write-plan/SKILL.md`

- [ ] **Step 1: Insert the critique section between Self-Review and Create Beans from Plan**

```markdown
## Plan Critique (external providers)

After self-review and before bean creation, give external providers one
critique pass. Read `providers.phases.define` from `orchestrate.json`; skip
this section when the list is empty or no listed provider is installed.

For each available external provider:

    hooks/dispatch-provider.sh <provider> \
      --role plan-critic \
      --topic "Critique implementation plan: <feature name>" \
      --instructions "Review this implementation plan against the design
        doc. Report only: spec requirements with no covering task, steps
        that cannot be verified as written, files referenced but never
        created or modified, and tasks too large for 1-2 TDD cycles.
        Be terse; one finding per line; no rewrites." \
      --design-doc-file <spec-path> \
      --diff-file <plan-path>

Fold accepted findings into the plan inline. Reject findings that conflict
with the spec or the user's recorded decisions, and note why. One round
only; do not re-dispatch after folding.
```

- [ ] **Step 2: Verify placement and portability**

Run: `grep -n "## Plan Critique\|## Self-Review\|## Create Beans" skills/write-plan/SKILL.md`
Expected: Self-Review, then Plan Critique, then Create Beans from Plan, in that order. Then run: `bash scripts/check-portability.sh` — expected exit 0.

- [ ] **Step 3: Commit**

```bash
git add skills/write-plan/SKILL.md
git commit -m "Add single external critique round before bean creation"
```

```eval
domains: [general]
criteria:
  general:
    - id: critique-placement
      check: "Critique section sits between Self-Review and Create Beans from Plan; single round; skip rule for empty/unavailable providers"
    - id: critique-scope
      check: "Critique instructions ask only for coverage gaps, unverifiable steps, missing files, and oversized tasks; findings folded inline with rejections noted"
thresholds: {}
```

### Task 9: Config contract docs and multi-provider test rework

**Files:**
- Modify: `skills/orchestrate/SKILL.md`
- Modify: `skills/develop/SKILL.md`
- Modify: `README.md`
- Modify: `scripts/test-multi-provider.sh`

- [ ] **Step 1: Update the evaluators config contract text**

In `skills/orchestrate/SKILL.md` and `skills/develop/SKILL.md`, wherever `evaluators.domains.<d>.providers` is described, state: ordered preference list for selecting the single evaluator (first available provider differing from the implementer wins; implementers are always claude); it is not a dispatch fan-out. Keep `evaluators.holistic.providers` documented as dispatch-all (unchanged).

- [ ] **Step 2: Update README.md**

Replace the "Multi-provider scoring" paragraph under Evaluator Loop with:

```markdown
**Provider selection.** Each domain's `providers` list is an ordered
preference; the evaluator runs on the first available provider that differs
from the implementer's, falling back to the implementer's provider in a
fresh context. Evidence (tests, invariant checks, runtime probes) is
gathered before dispatch and handed to the evaluator as an artifact.
Holistic review still scores across all configured providers.
```

- [ ] **Step 3: Rework test-multi-provider.sh**

Repurpose the test to the new contract, keeping the existing harness style: (a) selection honors preference order (reuse fake-bin PATH technique from `scripts/test-select-evaluator-provider.sh`); (b) merge-scorecards.sh still min-merges a two-provider array (holistic path); (c) single-element normalization preserves scores. Delete assertions that require per-task multi-provider dispatch. Do not modify assertions still valid for holistic.

Run: `bash scripts/test-multi-provider.sh`
Expected: all assertions pass, exit 0

- [ ] **Step 4: Full check**

Run: `for t in scripts/test-*.sh; do bash "$t" >/dev/null || echo "FAIL: $t"; done`
Expected: no FAIL lines. Then: `bash scripts/check-portability.sh` — exit 0.

- [ ] **Step 5: Commit**

```bash
git add skills/orchestrate/SKILL.md skills/develop/SKILL.md README.md scripts/test-multi-provider.sh
git commit -m "Document preference-list provider semantics and rework tests"
```

```eval
domains: [general]
criteria:
  general:
    - id: contract-docs-updated
      check: "orchestrate/develop SKILL.md and README describe providers as an ordered preference list for per-task evaluation, dispatch-all only for holistic"
    - id: test-suite-green
      check: "All scripts/test-*.sh pass and check-portability.sh exits 0"
thresholds: {}
```
