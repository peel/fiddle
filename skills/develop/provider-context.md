# Provider Context

Respond with your analysis only — no preamble, no meta-commentary.

## Role
{PROVIDER_ROLE}

## Topic
{TOPIC}

## Approaches
{APPROACHES}

## Design Document
{DESIGN_DOC}

## Diff
{DIFF}

## Evidence
{EVIDENCE}

## Previous Feedback
{PREVIOUS_FEEDBACK}

## Instructions
{INSTRUCTIONS}

## Scorecard Output Requirements

When your role is `evaluator`, output a valid JSON scorecard as the **last content block** in your response: a single JSON object, not wrapped in markdown fences, conforming to the schema below.

Any text before the scorecard is treated as analysis/reasoning and is discarded. Only the final JSON block is parsed, so a scorecard that is missing, fenced, or off-schema is unusable and costs a re-dispatch.

### Scorecard JSON Schema

```json
{
  "provider": "<your-provider-name>",
  "task_id": "<bean-id>",
  "iteration": <number>,
  "timestamp": "<ISO-8601>",
  "domains": {
    "<domain-name>": {
      "dimensions": {
        "<dimension-name>": {
          "score": <1-10>,
          "threshold": <1-10>,
          "comment": "<brief justification>"
        }
      }
    }
  },
  "criteria": [
    {
      "id": "<criterion-id>",
      "pass": <true|false>,
      "evidence": "<brief evidence>"
    }
  ],
  "antipatterns_detected": [],
  "guidance": "<actionable fix instructions if any dimension is below threshold>",
  "dispatch_count": 1
}
```

### Field Requirements

- **provider** (required): Your provider identifier (e.g. `"codex"`, `"gemini"`). Match the provider name used to dispatch you.
- **domains** (required): Object keyed by domain name. Each domain contains a `dimensions` object with scored dimensions. An explicitly empty `dimensions: {}` is valid for an evidence-only scorecard.
- **score** (required): Integer 1-10 for each dimension.
- **threshold** (required): The minimum passing score for this dimension (copied from the evaluation template).
- **comment** (required per scored dimension): Brief justification, non-empty. `evidence` is accepted as an alias; `scripts/validate-scorecard.sh` checks either.
- **criteria** (required): Array of pass/fail criteria results. Each entry has `id`, `pass` (boolean), and non-empty `evidence`. The ids match the task's eval-block criteria exactly.
- **guidance** (required): Actionable instructions for the implementer. Empty string if all dimensions pass.
- **dispatch_count** (required): Always `1` (each scorecard represents one dispatch).
