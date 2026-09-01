# 074 — A workflow file is read, not evaluated

Status: accepted
Cites: Step, WorkflowFile, Workflow, WorkflowError, WorkflowRefusal, WORKFLOW_VERSION, Capability, NextAction, PROPOSE_CHANGE, CVE_MITIGATE

## Context

A capability can be a list of steps in a file. Such a list has no conditions. The next request is therefore a condition. This record states what keeps the file format small, and why a condition is unnecessary rather than only unwanted.

The file is TOML. `WorkflowFile` carries a `version`, a `name`, a `stage` and a list of `Step`. `Step` has five shapes and no more: `Agent`, `Evaluate`, `Check`, `Commit` and `Effect`. None of the five holds an expression.

## Decision

**A reader reads the file. Nothing evaluates it.**

A list of step names is read. `when = "priority == 'high'"` is evaluated. A file that needs an evaluator needs a language. A language needs a runtime. `Workflow::try_from` checks the version and refuses an empty list, and that is the whole of its interpretation.

**The world holds the state, not the file.**

Steps pass work through the workspace and the forge. Re-running the steps reproduces the workspace. Nothing remembers a position between invocations.

**Five additions break those two rules, and each one forces the next.**

| Addition | What it forces |
|---|---|
| A condition on a step | An expression language |
| Variables between steps | A scope, then interpolation, then expressions |
| A branch, a parallel or a subflow | An else arm and a fork follow |
| A loop | An iteration variable, which needs variables |
| A remembered position | A store, and re-derivation ends |

**A request for a condition in a file means the capability belongs in Rust.** `propose_change` and `cve_mitigate` are both Rust for that reason. The escape hatch is the `Capability` trait. It is never a wider file format.

**An effect asks for a state, and does not command an action.** Five of the six built-in effects are named `ensure_*`. An effect compares a wanted state to an actual one through `IntegrationOperation::inspect`. It performs no write when the two already agree. A new effect follows the same rule: `ensure_status`, never `do_transition`.

The sixth built-in effect is `publish_decision_request`. It asks a person a question, so it has no prior state to compare. It is the exception that the rule needs an author to argue for.

## Consequences

The format answers conditionality three ways, and none of them is a condition in the file.

Each effect reads its postcondition before it writes. A step whose work is already done returns a receipt with the outcome `Committed` and performs no mutation. A failed check ends the run, so later steps do not execute. The run derives `NextAction::Execute`, `NextAction::Complete` or `NextAction::Blocked` before any step runs. A first run and a resumed run execute the same list. They differ only in which steps find the postcondition already met.

The format stays small enough to validate rather than to interpret. A workflow needs a parser and a validator. It needs no evaluator, no variable scope and no store. `WorkflowError` holds two refusals: an empty step list, and a version this build does not read. `WorkflowRefusal` holds five more, and `WorkflowCapability::new` raises all five before the first step runs.

Some capabilities cannot be files. Two exclusive paths, a loop over many items, a prompt built at run time, and a check that is itself a domain operation each need Rust. This is a real limit. The `cve_mitigate` capability sits outside the format because of it.

The `ensure_*` rule constrains an effect author. An author who models a verb rather than a state writes an effect that always mutates. That defeats replay, and it invites the condition this record refuses. The rule is therefore a review point for a new effect, not a naming preference.

A version 1 workflow runs to an end or it fails. `WorkflowRefusal::Gated` refuses a workflow that names an effect with a `Human` minimum, and it refuses it when the capability is built. `without_waiting` maps an awaiting outcome to `CapabilityError::WouldWait`, which is permanent. A workflow therefore cannot pause for a person.

A future requirement may genuinely need an evaluator, a variable scope and a remembered position. The correct response is to adopt an engine, not to grow one. That trade gives up re-derivation, which is what makes a crashed run safe to repeat. Record the trade here when it is taken.
