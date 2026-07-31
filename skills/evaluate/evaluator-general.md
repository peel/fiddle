# General Domain Evaluator Template

For tasks that don't fit a specific domain (scripts, configuration, tooling).

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

### Code Quality

Is the code clean, maintainable, and idiomatic?

**Default threshold: 6**

The per-task threshold stays at 6 by design; drift over time is caught by the
longitudinal decay alarm (DELIVER 5f, `scripts/trend-eval-history.sh`) rather
than by raising the bar. See `docs/technical/decisions/006-code-quality-threshold-equilibrium.md`.

```
 1  Broken: Syntax errors, doesn't parse.
 2  Garbage: Runs but incomprehensible. No structure, no naming,
    no separation of concerns.
 3  Spaghetti: Works but tangled. Functions do too many things.
    Copy-paste duplication. Global state.
 4  Rough: Some structure but inconsistent. Mix of patterns.
    Long functions with unclear responsibilities.
 5  Adequate: Reasonable structure. Functions mostly do one thing.
    Some duplication. Naming is okay but not great.
 6  Clean: Clear structure, good naming, minimal duplication.
    Consistent with documented conventions and justifiable on its
    own merits; a reviewer new to the codebase could follow it
    without archaeology.
 7  Good: Well-organized with clear interfaces. Easy to read.
    Follows language idioms. Tests are clear.
 8  Strong: Clean abstractions, good separation of concerns.
    Code reads like documentation. Easy to modify.
 9  Excellent: Elegant and simple. Minimal surface area.
    Another developer could maintain this easily.
10  Exemplary: Could be used as a teaching example. Every
    abstraction earns its complexity.
```
