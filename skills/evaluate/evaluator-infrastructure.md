# Infrastructure Domain Evaluator Template

For terraform, kubernetes, helm, nix, and docker infrastructure tasks. Evaluates correctness, idempotency, security posture, drift resistance, and spec fidelity via dry-run validation.

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
`"dimensions": {}`, together with the top-level declaration
`"mode": "evidence-only"`, and the task converges on evidence criteria
alone. Both are required: the empty object on its own cannot be told from a
dimension you dropped, and the graders refuse a card that scores nothing
without the declaration.

The dimension definitions below apply when thresholds are configured.

## Verification Approach

No live runtime. The evidence pack contains dry-run validation output,
gathered before your dispatch based on the files changed in the diff.

### Validation Commands

The evidence pack records output from the tools that apply to the diff:

- **Terraform:** `terraform validate`, `terraform plan`
- **Kubernetes/Helm:** `helm template` + `kubectl apply --dry-run=client`, check for deprecated APIs
- **Nix:** `nix flake check`, `nix build --dry-run`
- **Docker:** `docker build`, `docker compose config`

Multiple tools may apply per task. The pack records which tools were run
and their outcomes; do not run them yourself.

### What the Evidence Should Show

- Validation/plan output for each tool detected
- No unexpected resource destroys or recreates
- No hardcoded secrets in config
- No deprecated APIs or features
- Idempotency where verifiable (plan shows no changes on reapply)

### What to Check

- Do all configs validate without errors?
- Does the plan show only intended changes?
- Are secrets properly externalized?
- Are resource lifecycle rules appropriate?
- Are deprecated features or APIs avoided?
- Is state management configured correctly?

## Dimensions

### Correctness

**Default threshold: 7**

```
 1  Broken: Doesn't parse. Syntax errors, missing required fields.
 2  References broken: Parses but references undefined resources,
    variables, or modules.
 3  Type errors: References resolve but types mismatch. Wrong
    argument types, invalid attribute access.
 4  Plan fails: Validates locally but plan/dry-run fails against
    provider or cluster. Missing providers, auth issues.
 5  Partial plan: Some resources plan cleanly, others error.
    Mixed state.
 6  Plans clean: All resources plan without error. Some warnings
    (deprecated features, provider suggestions).
 7  Clean plan, expected changes: Plan shows only intended changes.
    No unexpected destroys or recreates. Warnings addressed.
 8  Robust: Handles variable inputs gracefully. Conditional
    resources work for all configurations. Cross-module
    references correct.
 9  Thorough: All variable combinations validated. Outputs
    verified. Module interfaces well-typed.
10  Bulletproof: Adversarial inputs handled. Plan is clean
    across all target environments.
```

### Idempotency

**Default threshold: 7**

```
 1  Destructive: Every apply recreates resources. Data loss
    on reapply.
 2  Unstable: Most resources recreated on reapply. Force-new
    triggers everywhere.
 3  Partial stability: Some resources stable, others recreate
    due to missing lifecycle rules or volatile inputs.
 4  Name-dependent: Resources stable only with specific naming.
    Random suffixes or timestamps cause recreate.
 5  Mostly stable: Second apply shows minor changes (tag updates,
    metadata) but no recreates.
 6  Stable: Second apply shows no changes for core resources.
    Some data sources re-read.
 7  Idempotent: Second apply is a clean no-op. Lifecycle rules
    prevent unnecessary recreates. Ignore-changes where
    appropriate.
 8  Resilient: Handles external drift gracefully. Import-friendly
    resource definitions.
 9  Convergent: Repeated applies converge even from partially-
    applied state. Recovery from interrupted applies.
10  Self-healing: Detects and corrects drift automatically. State
    is always convergent regardless of starting point.
```

### Security Posture

**Default threshold: 7**

```
 1  Secrets in code: Passwords, tokens, or keys hardcoded in
    config files.
 2  Wide open: 0.0.0.0/0 ingress, * IAM permissions, privileged
    containers, root users.
 3  Default insecure: Using provider defaults that are insecure
    (public buckets, unencrypted storage, default passwords).
 4  Partially locked: Some resources secured, others use overly
    broad permissions. Mix of secure and insecure patterns.
 5  Functional security: No hardcoded secrets. Basic network
    restrictions. IAM roles exist but are broader than needed.
 6  Reasonable: Secrets from vault/SSM/sealed-secrets. Network
    policies restrict traffic. IAM scoped to service level.
 7  Solid: Least privilege IAM. Network segmentation. Encryption
    at rest and in transit. No privileged containers. Non-root.
 8  Hardened: Pod security standards enforced. Resource quotas
    set. Audit logging enabled. Secret rotation configured.
 9  Defense in depth: Multiple security layers. Network policies
    + service mesh. RBAC + OPA/admission controllers.
10  Zero trust: Every component authenticated and authorized. All
    traffic encrypted and verified. Immutable infrastructure.
```

### Drift Resistance

**Default threshold: 6**

```
 1  No state: Local state files, no backend. State will be lost.
 2  Fragile state: Remote backend but no locking. Concurrent
    applies will corrupt state.
 3  Manual dependencies: Resources depend on manually-created
    prerequisites not in code.
 4  Partial coverage: Some resources in code, others created
    manually alongside. Mixed management.
 5  Managed but brittle: All resources in code. State locked.
    But no drift detection or lifecycle rules.
 6  Drift-aware: Lifecycle rules prevent accidental recreates.
    Ignore-changes for externally-managed fields. State backend
    with locking.
 7  Drift-resistant: All resources fully managed. External
    dependencies declared as data sources. State is the source
    of truth.
 8  Drift-detected: Plan runs in CI catch drift. Externally-
    modified resources flagged.
 9  Drift-corrected: Automated remediation for common drift
    patterns. State refresh integrated into workflow.
10  Immutable: Infrastructure replaced, not mutated. Drift is
    impossible by design.
```

### Domain Spec Fidelity

**Default threshold: 8**

```
 1  Wrong feature: Built something entirely different from task spec.
 2  Wrong approach: Right feature, fundamentally wrong implementation
    strategy.
 3  Major gaps: Core task requirements missing. What exists may be
    correct but the task is incomplete.
 4  Partial: ~50% of task requirements implemented. Missing pieces
    noticeable.
 5  Most there: ~70% of task requirements. Missing pieces are secondary
    but a careful reviewer would catch them.
 6  Functional coverage: All primary task requirements met. Secondary
    requirements partially covered.
 7  Good coverage: All task requirements met. Some implemented minimally
    (letter of the spec, not spirit).
 8  Faithful: Implementation matches task spec in both letter and spirit.
    Design intent preserved.
 9  Complete: Every task requirement fully implemented. No drift.
    Implementation captures nuances of the task description.
10  Exceeds spec: All requirements met and implementation improves on
    spec where the task description was ambiguous or underspecified.
```
