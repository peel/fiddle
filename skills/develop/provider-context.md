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

When your role is `evaluator` or `holistic-reviewer`, your reply is the scorecard: one JSON object in the shape described below.

The shape is enforced rather than requested. `hooks/dispatch-provider.sh` builds a JSON Schema from this envelope with `scripts/build-scorecard-schema.sh` and passes it to the provider CLI, which constrains the reply to it. So write the evidence the finding deserves and do not trim it to fit: the guarantee lives in the schema, not in how short the answer is. A reply cannot arrive a closing brace short, and no sentinel or trailing marker is needed to detect one.

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
          "comment": "<justification>"
        }
      }
    }
  },
  "criteria": [
    {
      "id": "<criterion-id>",
      "pass": <true|false>,
      "evidence": "<evidence>"
    }
  ],
  "antipatterns_detected": [],
  "guidance": "<actionable fix instructions if any dimension is below threshold>",
  "dispatch_count": 1
}
```

### Field Requirements

- **provider** (required): Your provider identifier (e.g. `"codex"`, `"gemini"`). Match the provider name used to dispatch you.
- **domains** (required): Object keyed by domain name. Each domain contains a `dimensions` object with scored dimensions. An explicitly empty `dimensions: {}` is valid only on a scorecard that also carries the top-level declaration `"mode": "evidence-only"`; without it the graders refuse the card.
- **score** (required): Integer 1-10 for each dimension.
- **threshold** (required): The minimum passing score for this dimension (copied from the evaluation template).
- **comment** (required per scored dimension): The justification, non-empty. `evidence` is accepted as an alias; `scripts/validate-scorecard.sh` checks either.
- **criteria** (required): Array of pass/fail criteria results. Each entry has `id`, `pass` (boolean), and non-empty `evidence` naming the artifact and line behind the verdict. The ids match the task's eval-block criteria exactly. `criterion` and `met` are not accepted spellings; the graders refuse a card that uses them rather than translating it. Full field list: `skills/develop/scorecard-envelope.md`.
- **guidance** (required): Actionable instructions for the implementer. Empty string if all dimensions pass.
- **dispatch_count** (required): Always `1` (each scorecard represents one dispatch).
