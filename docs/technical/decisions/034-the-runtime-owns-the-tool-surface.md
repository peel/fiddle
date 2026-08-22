# 034 — The runtime owns the tool surface, and no host fact leaves it

Status: accepted
Cites: fiddle_runtime::agent::tools, ToolContext, ToolReceipt, ReadFile::NAME, WriteFile::NAME, ListFiles::NAME, RunCheck::NAME, binary_repair::the_serialized_request_offers_five_tools_and_carries_no_host_fact

## Context

`agent::tools` is the whole model-visible surface: `read_file`, `edit_file`, `write_file`, `list_files` and `run_check`. ADR 048 added `edit_file` and left this decision standing. A tool schema is a menu, and anything named on it is something the model may fill in. Rig documents its hooks as controls rather than as authorization.

## Decision

Take every host fact from `ToolContext`, and never from a tool's arguments, schema, error text or success output. Record each tool receipt in the tool body, independently of any Rig hook.

## Consequences

- The claim is asserted against the serialized outbound request. The lane reads the chat-completions bodies the compiled binary put on a loopback socket.
- It pins the offered set at five names, and searches every body for the host root and the credential. Asserting against the builders alone would check the intention.
- A receipt survives a control that stops firing. Receipts are read back on both arms, so a hook that stopped running cannot empty the record.
- A path in an error message is a leak. A fixture path in a diagnostic is the same disclosure as `.git`'s absolute host path, by a shorter route.
- What was given up: a more helpful error. A refusal cannot name the root the model fell outside. A confused attempt burns a turn rediscovering it.
