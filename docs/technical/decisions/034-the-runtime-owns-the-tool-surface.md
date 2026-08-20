# 034 — The runtime owns the tool surface; no host fact leaves it

Status: accepted

Cites: fiddle_runtime::agent::tools, ToolContext, ToolReceipt,
binary_repair::the_serialized_request_offers_four_tools_and_carries_no_host_fact

## Context

`agent::tools` is the whole model-visible surface: `read_file`, `write_file`,
`list_files`, `run_check`. A tool schema is a menu, and anything named on it is
something the model may fill in. Rig documents its hooks as controls rather than as
authorization.

## Decision

Take every host fact from `ToolContext` and never from a tool's arguments, its
schema, its error text or its **success output**. Record each tool receipt in the
tool body, independently of any Rig hook.

## Consequences

**The claim is asserted against the serialized outbound request.**
`the_serialized_request_offers_four_tools_and_carries_no_host_fact` reads the
chat-completions bodies the compiled binary put on a loopback socket, pins the
offered set at exactly four names, and searches every body for both spellings of
the host root and for the credential's value. Asserting against the builders alone
would check the intention.

**A receipt survives a control that stops firing.** Receipts are read back on the
success arm and the failure arm, so a hook that no longer runs cannot empty the
record of what happened.

**A path in an error message is a leak.** `.git` in a linked worktree is a file
holding an absolute host path (ADR 031), and a fixture path in a diagnostic is the
same disclosure by a shorter route.

**What was given up: a more helpful error.** A refusal cannot tell the model which
root it fell outside of, so a confused attempt burns a turn rediscovering it.
