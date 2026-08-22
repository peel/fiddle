# 051 — A tool result is bounded, and a partial result names what it withheld

Status: accepted
Cites: fiddle_runtime::agent::tools::RESULT_CAP_BYTES, STREAM_CAP_BYTES, NOTE_ALLOWANCE_BYTES, ReadFileArgs, ListFilesArgs, Listing, page_of_lines, page_of_paths, head_and_tail, ToolError::ReadRefused, ToolError::ListingRefused, a_read_inside_the_limit_gives_the_whole_file_and_withholds_nothing, the_same_read_beyond_the_limit_gives_part_and_counts_the_lines_it_withheld, an_offset_reaches_the_last_line_of_a_file_no_one_read_can_hold, a_line_longer_than_the_limit_is_cut_and_the_note_counts_the_bytes, a_read_refuses_an_offset_past_the_end_and_names_the_line_count, a_read_result_stays_inside_one_bound_however_large_the_file_grows, a_listing_beyond_the_limit_gives_part_and_counts_the_paths_it_withheld, a_check_that_prints_past_the_limit_keeps_its_start_and_its_end, forty_turns_of_the_largest_tool_result_stay_inside_the_measured_context

## Context

`read_file` returned a whole file. Nothing bounded what any tool put into the conversation.

Against the real gateway, a history carrying one copy of the branch's `go.sum` sent 96,489 bytes and got 200. Four copies sent 382,623 bytes and finished on `length`. Sixteen copies sent 1,527,171 bytes and got 400: `This model's maximum context length is 262144 tokens`.

The repository under repair holds a `go.sum` of 94,050 bytes and a `cmd/serve_test.go` of 66,015 bytes. Twenty-one of its files exceed 20 KB. At four bytes per token `go.sum` is about 23,500 tokens, so eleven full reads exhaust a 262,144-token context. `max_turns` is 40.

`tool_choice: Required` makes every turn a tool call, so the model cannot stop by replying. ADR 048 records why `Required` stays.

## Decision

**One tool result holds at most `RESULT_CAP_BYTES`, which is 16384 bytes.** A stream a program printed holds at most `STREAM_CAP_BYTES`, which is half of that. So a result carrying two streams stays inside the same bound. A note that reports a cut holds at most `NOTE_ALLOWANCE_BYTES`, which is 512 bytes.

**`read_file` takes an `offset` and a `limit`, and addresses in lines.** `offset` names the first line and starts at 1. `limit` counts lines. The cap stops a page that the limit did not.

**`list_files` takes the same `offset` and `limit`, and addresses in paths.** Its output is a `Listing` with a `paths` list and a `withheld` sentence.

**No result is cut in silence.** A partial read opens with a note in square brackets. The note counts the lines it withheld. It says it is not part of the file. It names the offset that reads on. A partial listing carries the same statement in `withheld`. A cut stream carries the statement where the lines were dropped.

**A read that cannot answer refuses.** An offset past the end of a file returns `ToolError::ReadRefused` and names the count of lines the file holds. An offset of 0, and a limit of 0, are refused the same way. `list_files` refuses through `ToolError::ListingRefused`.

## Why lines, and not bytes

Bytes are what the context costs, so the cap is in bytes. Lines are what the model addresses in, for two reasons.

`edit_file` takes text copied from the file. A byte offset cuts a line in half. Half a line is not text a model can quote back into `edit_file`. The two tools must compose.

A person and a diff both count lines. The tool fiddle replaced addressed in lines. ADR 049 records that `edit_file` cites no line number, for a different reason: a line number goes stale after an edit above it. A read is one call, and nothing edits under it. So an offset is safe where a citation is not.

## What one long line does

A line longer than the cap does not fit in one read. `read_file` returns the first `RESULT_CAP_BYTES` bytes of it, cut on a character boundary. The note counts the bytes it withheld **of that one line**. It says a line longer than the cap does not fit, and that `read_file` reaches no further into it.

That is the one case where the note counts bytes instead of lines, because a line count cannot describe part of one line. It is also the one case where a byte of the file is out of reach. The alternative was to refuse the read, which would make a minified file unreadable and report nothing.

## Which tools are bounded, and which are not

**`read_file` is bounded and addressed.** It is the tool the measurement indicts.

**`list_files` is bounded and addressed.** Its result grows with the count of files a project tracks. `tool_choice: Required` lets the model call it on all 40 turns. A cap without an offset would hide the rest of a project. So it takes the same two arguments. A path is never cut. Half a path reads as a whole path, and the model would then call `read_file` on it. So a listing holds the cap plus at most one path.

**`run_check` and `run_command` are bounded and not addressed.** A program's output is the one thing fiddle does not control. `cargo test` on a broken project prints megabytes. There is nothing to address into, because the model chooses the program and not the bytes. So each stream keeps its start and its end. It drops the middle. A check prints its first error near the start, and its count of failures at the end. Both survive.

**`write_file` and `edit_file` are not bounded.** Their results are a path and a byte count.

**`edit_file` still reads a whole file.** It counts the occurrences of the text to find, and a count over part of a file would be wrong. So the bound is in the tool and not in `Workspace::read`.

## Consequences

- **The brief says nothing about this.** ADR 048 holds: the schemas carry the arguments, and the brief names no tool and no count.
- **The offered set is still five tools, and six where a deployment declares a program.** The two acceptance tests that count them are unchanged.
- **The bound is asserted against the measured numbers.** `forty_turns_of_the_largest_tool_result_stay_inside_the_measured_context` multiplies `default_max_turns()` by the cap and the note allowance. It asserts the product is under the 1,048,576 bytes the context holds, and under the 1,527,171 bytes the gateway refused. It couples the cap to the real default and not to a number written twice.
- **The pair of tests differs by one input.** `a_read_inside_the_limit_gives_the_whole_file_and_withholds_nothing` reads 100 lines and gets them all. `the_same_read_beyond_the_limit_gives_part_and_counts_the_lines_it_withheld` reads 2,000 and gets 409, with a note naming offset 410.
- **What the bound test does not prove.** It bounds one result. It does not prove a run of 40 turns stays inside the context. A turn also carries the brief, the model's own text and the report. It proves that no one call can put a 94 KB file into the conversation.
- **What was given up: turns.** A model that needs one line of a 94,050-byte `go.sum` now pages through it. Six reads cover the file where one did before. `fiddle-1z63` holds the turn budget, and 40 turns is now survivable because a turn cannot add 60 KB.
- **A note can be mistaken for file content.** A model may copy the note into `edit_file`'s `find`. `edit_file` then refuses, because the note is not in the file. The note says it is not part of the file, and the square brackets mark it.
