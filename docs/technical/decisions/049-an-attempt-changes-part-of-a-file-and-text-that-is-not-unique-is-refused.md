# 049 — An attempt changes part of a file, and text that is not unique is refused

Status: accepted
Cites: fiddle_runtime::agent::tools::EditFile, EditFileArgs, edit_file, ToolError::EditRefused, WriteFile, PREAMBLE, REGISTERED_TOOLS, fiddle_runtime::workspace::WorkspacePath, edit_file_changes_the_one_place_the_text_occurs_and_keeps_the_rest, edit_file_refuses_the_same_edit_where_the_text_occurs_twice, edit_file_is_bounded_by_the_same_path_rules_as_read_file_and_write_file, binary_repair::a_one_line_repair_of_a_long_file_leaves_every_other_line_where_it_was, binary_repair::the_serialized_request_offers_five_tools_and_carries_no_host_fact, binary_repair::the_serialized_request_offers_a_sixth_tool_only_where_the_deployment_declares_a_program

## Context

ADR 034 named the model-visible surface: `read_file`, `write_file`, `list_files` and `run_check`. None of them changes part of a file.

To change one line of a 985-line `go.sum`, the model had to write all 985 lines again. It wrote the entries it judged relevant and dropped the rest. The file went from 985 lines to 2. Pull request #251 carries that file, and `go build ./...` fails on every dependency in the project.

ADR 047 gave the model the declared program that regenerates a derived file. A file no program derives still needs a partial change.

## Decision

Add `edit_file` to the surface ADR 034 owns. It takes a path, the text to find, and the text to put in its place. It replaces the text one time. Every other byte of the file stays.

`edit_file` refuses in three cases, and changes nothing when it refuses.

- The text to find is empty.
- The text to find does not occur in the file.
- The text to find occurs more than one time. The refusal says the text is not unique and counts the occurrences.

`edit_file` addresses no line number. A line number becomes wrong when an earlier edit adds a line above it. `docs/technical/style.md` records the same rule for a citation.

## Why `write_file` stays

`write_file` keeps its whole-file write. Its description now says which of the two tools to use.

Two things need it. A new file holds no text to find, so `edit_file` cannot create one. A short file is cheaper to write whole than to quote twice.

The alternative was to narrow `write_file` to creation. It costs more than it saves. fiddle has no tool that deletes a file, so a model that must replace a short file whole would have no way left to do it. The fault in #251 was the choice of tool, not the whole-file write itself. The brief and the two descriptions now state the choice, and they name `edit_file` first.

## Consequences

- **The offered set is five tools, and six where a deployment declares a program.** `the_serialized_request_offers_five_tools_and_carries_no_host_fact` and `the_serialized_request_offers_a_sixth_tool_only_where_the_deployment_declares_a_program` read the serialized outbound request. Each name carries its count, so the next tool renames them both again.
- **ADR 034 holds unchanged.** `edit_file` takes its host facts from `ToolContext`. It records its own receipt. Its refusals name the path the model wrote and no host path.
- **`WorkspacePath` bounds `edit_file` as it bounds `read_file`.** `edit_file` reads through `Workspace::read`, so a path that leaves the project, and `.git` at any depth, are refused before any byte moves.
- **The line count is the assertion, and the new content is not.** A file the model rewrote from memory carries the new entry too. A test that greps for the repair passes for a truncated file, which is how #251 got through. `a_one_line_repair_of_a_long_file_leaves_every_other_line_where_it_was` counts the lines of a 400-line lock after the attempt.
- **fiddle reads no syntax to resolve an ambiguous edit.** ADR 025 keeps ecosystem semantics on the model's side, so two matches are refused rather than ranked.
- **What was given up: a turn.** Text that occurs twice needs a second call with more lines around it. The refusal counts the occurrences, so the model learns what to add.
- **The evidence names the new tool.** `REGISTERED_TOOLS` holds six names in `capability::repair` and five in `capability::propose`. An `edit_file` call is published as `tool:edit_file:<outcome>:<count>` and never as `unregistered`.
