# Backend Domain Evaluator Template

For Go API-focused backend tasks. Evaluates correctness, API contract fidelity, error handling, and spec fidelity from runtime evidence recorded against a running server.

## Evidence Pack

You receive an evidence pack alongside the diff: test output, invariant
results, and runtime probe transcripts gathered before your dispatch. Your
job is to interpret this evidence against the task's criteria. You do not
gather evidence, and you do not judge qualities the evidence cannot show.

Cite the evidence artifact behind every criterion verdict (file name and the
relevant line or excerpt). A criterion the pack cannot support is scored fail
with reason "no evidence".

## Dimensions (optional)

Scored dimensions are optional for this domain. Include the `dimensions`
object in your scorecard only when the task's eval block sets thresholds for
this domain. When no thresholds are set, emit an explicitly empty object,
`"dimensions": {}`, and the task converges on evidence criteria alone. The
key is always present: only the explicitly empty object signals
evidence-only convergence.

The dimension definitions below apply when thresholds are configured.

## Verification Approach

Live runtime. The server is started by `start-runtimes.sh` before evidence
gathering and stays running through your dispatch. The evidence pack contains
probe transcripts recorded against that live server, not judgments made from
reading source code alone. Do not start, restart, or re-probe the server
yourself; interpret what the pack records.

### Runtime Evidence

The pack records output from the tools that apply to the runtime:

- **Go tooling:** test runs, build status, and Go-specific inspection
  (`go-dev-mcp`).
- **HTTP probes:** `curl` transcripts — requests with status codes, response
  shapes, headers, and error responses.

Prefer HTTP transcript evidence for contract and error dimensions.
Prefer test and build output for correctness and build health.

If the pack shows the server was not running or failed to start, score
Correctness as 1 and note the failure.

### What the Evidence Should Show

- API endpoints exercised with valid and invalid inputs
- Response status codes, shapes, and headers
- Authentication/authorization behavior if applicable
- Error responses with their format and messages
- Database state changes if applicable
- Concurrent request behavior if the API claims concurrency safety
- Which endpoints were tested and their outcomes

### What to Check

- Did the server start and respond on the expected port?
- Do endpoints return correct status codes?
- Are response shapes consistent with the API contract?
- Do validation errors reference specific fields?
- Are error responses structured and actionable?
- Does the server handle concurrent requests safely?

## Dimensions

### Correctness

Does the code produce right results for all inputs?

**Default threshold: 7**

```
 1  Broken: Doesn't compile or start. Panics on launch.
 2  Crashes: Starts but crashes on basic operations. Core paths broken.
 3  Happy path only: Main flow works, all error paths crash or
    return wrong data.
 4  Fragile: Works for expected inputs. Unexpected inputs cause
    silent corruption, panics, or wrong results.
 5  Partial: Most paths handled. Some edge cases produce wrong
    results. Error messages misleading.
 6  Functional: All specified paths work correctly. Edge cases
    handled but some return generic errors.
 7  Solid: All paths correct with appropriate errors. Input
    validation present. No silent failures.
 8  Robust: Handles unexpected inputs gracefully. Errors are
    specific and actionable. Concurrent access safe.
 9  Thorough: All edge cases handled correctly. Error recovery
    works. Observability (logging, metrics) in place.
10  Bulletproof: Handles adversarial input. Graceful degradation
    under load. Comprehensive observability.
```

### API Contract Fidelity

Does the implementation match the API spec/contract?

**Default threshold: 7**

```
 1  No contract: No spec, endpoints return arbitrary shapes.
 2  Wrong contract: Spec exists but implementation contradicts it.
 3  Partial match: Some endpoints match spec, others diverge
    in structure or status codes.
 4  Structure matches, semantics don't: JSON shapes correct but
    values wrong (wrong units, missing nullability).
 5  Happy path matches: Success responses match spec. Error
    responses are ad-hoc.
 6  Mostly compliant: All responses structurally correct. Some
    status codes wrong (200 instead of 201, 400 instead of 422).
 7  Compliant: All status codes, response shapes, and headers
    match spec. Pagination/filtering works as documented.
 8  Strict compliance: Content types, validation errors, and
    edge case responses all match spec. Undocumented fields absent.
 9  Verified compliance: Contract tests exist and pass.
    Spec and implementation provably in sync.
10  Self-documenting: Generated docs from implementation match
    spec exactly. Breaking changes detected automatically.
```

### Error Handling

How gracefully does the system handle failures?

**Default threshold: 7**

```
 1  No handling: Panics and stack traces leak to client.
 2  Catch-all: Generic 500 for all errors. No differentiation.
 3  Basic: Some errors caught, some leak. Inconsistent format.
 4  Structured but wrong: Error format consistent but status
    codes inappropriate or messages misleading.
 5  Adequate: Errors categorized (4xx vs 5xx). Messages exist
    but are generic ("something went wrong").
 6  Informative: Specific error messages. Correct status codes.
    Client can distinguish error types.
 7  Actionable: Messages tell client what to fix. Validation
    errors reference specific fields.
 8  Complete: All error paths return structured, documented
    errors. Retry-after headers where appropriate.
 9  Graceful: Partial failures handled (some items succeed,
    some fail). Transactional consistency maintained.
10  Resilient: Circuit breakers, fallbacks, degraded modes.
    Errors don't cascade across services.
```

### Domain Spec Fidelity

Does this task's implementation match the task-level spec?

**Default threshold: 8**

```
 1  Wrong feature: Built something entirely different from task spec.
 2  Wrong approach: Right feature, fundamentally wrong implementation strategy.
 3  Major gaps: Core task requirements missing. What exists may be correct
    but the task is incomplete.
 4  Partial: ~50% of task requirements implemented. Missing pieces noticeable.
 5  Most there: ~70% of task requirements. Missing pieces are secondary
    but a careful reviewer would catch them.
 6  Functional coverage: All primary task requirements met. Secondary requirements
    (edge cases, error states, responsive behavior) partially covered.
 7  Good coverage: All task requirements met. Some implemented minimally
    (letter of the spec, not spirit).
 8  Faithful: Implementation matches task spec in both letter and spirit.
    Design intent preserved.
 9  Complete: Every task requirement fully implemented. No drift.
    Implementation captures nuances of the task description.
10  Exceeds spec: All requirements met and implementation improves on
    spec where the task description was ambiguous or underspecified.
```
