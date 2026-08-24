# 052 — The transcript is asked for by the environment, and written beside the report

Status: accepted
Cites: crates/fiddle-runtime/src/agent/transcript.rs, Transcripts, TranscriptHook, Record, Wrote, transcript::SWITCH, transcript::requested, transcript::FIELD_LIMIT, transcript::FILE_LIMIT_BYTES, Redaction::redacted, Redacted, agent::attempt_briefed, agent::offered, render::transcript_note, crates/fiddle-acceptance/tests/model_transcript.rs, the_switch_off_writes_no_transcript_and_says_nothing, the_switch_on_writes_the_transcript_and_says_it_did, the_transcript_carries_the_model_response_and_not_the_credential, no_host_fact_reaches_the_transcript, a_switch_value_the_run_cannot_read_is_refused_before_any_work, transcript::ELAPSED, transcript::TOOK, transcript::FINISH, transcript::finish_reason, TranscriptModel, TranscriptHook::took, State::elapsed, every_record_carries_an_elapsed_value_that_never_decreases, the_finish_reason_of_every_response_reaches_the_transcript, a_tool_record_says_how_long_the_tool_took, a_finish_reason_is_read_from_the_response_the_provider_returned

## Context

Run 32621918902 ran for 45 minutes, made zero tool calls, and died on the 2700-second deadline. Its record was `evidence entries: 0` and `tool receipts: 0`.

The model produced output for 45 minutes. None of it was a tool call. Nothing recorded what it produced, so the cause is not in fiddle's output.

A probe cannot reach this shape. The request that fails is fiddle's own: six tools, its real brief, `tool_choice: Required`, inside its own loop. Twelve releases shipped in one day, and several existed only to learn a fact a transcript shows in one run.

## Decision

`FIDDLE_TRANSCRIPT=1` records what the model was sent and what it returned. Any other non-empty value refuses the run. An unset or empty variable records nothing.

The transcript is written to `<report.dir>/transcript/<slug>-<token>.jsonl`. One JSON object per line. Every line is flushed as it is written.

`attempt_briefed` writes the first record and installs `TranscriptHook`. The hook records five kinds of record:

- `sent` — the turn, the count of history messages, and the prompt.
- `spent` — the turn, the count of content blocks, and the tokens the provider reported.
- `received` — one record per content block. A text block carries its text. A tool call carries its name and its arguments.
- `tool` — the tool, the arguments it ran with, and the result the model saw.
- `invalid` — a tool the model named that the run never offered, and the set it did offer.

The first record is `brief`. It carries the preamble, the task, the offered tool names, the tool choice, and the three bounds. No probe can reach that shape, because it is the shape fiddle builds.

`sent` records the prompt and a count of history messages, and never the history itself. Turn 40 would otherwise repeat the 39 turns before it. Every message in the history is already one of the earlier records, so the file read whole is the conversation.

## Why the environment, and not the document

A debug switch belongs to one run. A deployment should not commit a change to its configuration document to diagnose one failure, and should not carry the change afterwards.

`config check` reads the document and reports it. A switch in the document would appear there, and a switch in the environment does not. So the run reports the switch itself, in the ordinary output.

## Why `[report] dir`

`[report] dir` is uploaded as a workflow artifact. A transcript written there reaches an operator with no further plumbing.

The alternative was stdout. A run's log truncates, and `--json` puts one document on stdout that a second stream would corrupt.

## What the file is named, and what it is not named

The name is the invocation slug and a minted token. It is not the attempt id.

The file must exist while the model is running. The 45-minute case never returned, so a transcript assembled at the end would not have existed. `attempt_briefed` opens the file before the first request, and the attempt id is minted above it in `orchestration::attempt` and reaches `execute` through the grant. Threading it further, into `GroupMigration::migrate` through `sweep`, buys a matching name and costs two more signatures.

Both the token and the attempt id are minted by the same generator, so both sort by the millisecond they were minted. One invocation writes at most one transcript, and the run's note names the exact path beside the bundle's path.

## The credential

`Redaction` from ADR 050 is the only mechanism. `Redaction::redacted` is added beside `Redaction::excerpt`, and both call one replacement and one cut. `excerpt` is `redacted` at 240 characters, quoted with Rust's debug escape.

Every text in a record passes through `Redaction`. `Record::text` holds the raw string, and `Record::rendered` redacts it at write time. No text reaches the file by another route.

A redaction that holds no credential writes `fiddle holds no credential to redact, so it withholds this text` in place of the text. ADR 050 argued this: a path that cannot redact cannot promise a safe field. Every such path today drives a mock model.

The order is replace, then cut. A cut before a replacement can leave a prefix of the credential.

The redaction runs on raw text and not on serialized JSON. An assistant text block is recorded from `Text::text`, so the credential is in the encoding the redaction holds. Serializing first would escape a credential that carries a quote or a backslash, and an escaped credential does not match the raw one. A tool call's arguments are a `serde_json::Value` and are recorded from `to_string`, so that one field keeps the property ADR 050 already has. The model never sees the credential, so the model's own arguments are the least likely field to carry it.

## The host fact

ADR 034 requires every host fact to come from `ToolContext`, and never from a tool's arguments, schema, error text or output. The transcript records tool arguments and tool results, so it records the same bytes that already cross the wire to the provider. It opens no new surface.

`no_host_fact_reaches_the_transcript` asserts it rather than arguing it. It searches the written file for the scenario's own root, the way `the_serialized_request_offers_six_tools_and_carries_no_host_fact` searches the request bodies.

The transcript's own path is a host fact, and fiddle prints it in its own output. `report.dir` already appears there, in `config check` and in an evidence failure.

## The bound

One field holds at most `FIELD_LIMIT`, which is 16,384 characters. ADR 051 caps one tool result at 16,384 bytes, so any tool result fits in one field whole. A cut field ends with `[fiddle cut this text at 16384 characters]`.

One file holds at most `FILE_LIMIT_BYTES`, which is 8,388,608 bytes. Forty turns of a capped tool result and a capped model turn stay inside it. A record that would pass the bound is dropped and counted, and the run's note reports the count.

## Being on is visible

`render::transcript_note` prints one of three things on stderr after the run, and prints nothing when no record was written:

- `fiddle wrote the model transcript to <path>, as <n> records`, followed by `the transcript carries the project's content and the model's replies`.
- `the transcript reached its bound of 8388608 bytes and dropped <n> records`.
- `error: could not write the model transcript`, with the path and the cause.

The note is printed after the run, from what the file recorded. A run that opened no file says nothing. A run that could not write says so, and its outcome is unchanged: a diagnostic must not decide whether a repair succeeded, and must never claim a file it does not have.

## Consequences

- `attempt` and `attempt_briefed` take one more argument. Four capability configs carry `Option<Transcripts>`. ADR 050 chose an explicit argument over a process-wide registry for the same reason: a call site with no answer is a compiler error.
- `Transcripts` is `Clone` and shares one file and one set of counters. `CveMitigate` clones it into `MigrationConfig`.
- The transcript is off by default and stays off. It carries the repository's content and the model's reasoning about it. A deployment did not agree to publish that by installing fiddle.
- `FIDDLE_TRANSCRIPT=true` exits 2 and names the value. A switch that read `true` as off would report nothing and look like a run that was never asked.
- A rejected and recovered turn suppresses `on_completion_response`, so that turn has no `received` record. Its `invalid` record is written by `on_invalid_tool_call`.
- The pair of acceptance tests differs by one input. `the_switch_off_writes_no_transcript_and_says_nothing` removes the variable from the child's environment, and the other sets it to `1`. Both assert the same exit code.
- `the_transcript_carries_the_model_response_and_not_the_credential` reads the file on disk. Its stub gateway echoes the exported credential in the model's own reply, and the test asserts the reply survives, the marker is present once, and the credential is absent.

## Amendment (M4b) — the transcript records when, and the reason a response ended

The file as first written records what happened and not when. So it cannot tell the two causes of run 32621918902 apart: one response that took 45 minutes, or hundreds of fast responses of which none was a tool call. The first needs a per-request timeout, the second needs whatever is looping stopped, and the file that exists to end the guessing could not end this one.

**Every record carries `elapsed_ms`.** The first record is the origin, so `brief` reads 0 and every later record is measured from it. The value is stamped inside the lock that writes the line, so the order of the values is the order of the file. A stamp taken before the lock can be written after a later one, and a time that falls down the file is worse than none, because a reader will trust it.

Elapsed, and not a wall clock. It needs no clock source in a test, it sorts, and it states what a reader wants without arithmetic. There is one time field, because two invite a reader to reconcile them.

**A `finish` record names the reason a response ended.** `finish: "length"` with no tool call is an exhausted ceiling, and `finish: "stop"` with empty content is a model answering nothing. The two need different fixes and both were invisible.

The hook events carry canonical content, usage and a message id, and no finish reason; rig-agent says so and points at the direct completion API for the provider's typed response. So `TranscriptModel` wraps the model the attempt runs, reads `choices[0].finish_reason` from the raw response it already holds, and appends one record before that turn's `spent`. The reason is read and never invented: a provider that reports none writes no record, which is what the mock model in the unit tests does.

`TranscriptModel` counts its own calls to the provider, and that count is the record's turn. It is a second counter beside rig's, so `the_finish_reason_of_every_response_reaches_the_transcript` asserts the two agree.

**A `tool` record carries `duration_ms`.** A `run_check` of four minutes and a `read_file` of four milliseconds were the same record. `on_tool_call` and `on_tool_result` both carry rig's `internal_call_id`, so the hook holds the start against that id and measures at the result. A result with no recorded start carries no duration.

**Rig exposes no retry, and none is invented.** `AgentRun` increments `current_turn` before every model call, so a `ModelTurnAction::Retry` is sent under the next turn number and two `sent` records cannot share one. `HookContext` carries a run id, a turn, a streaming flag, an agent name and a scratchpad; `CompletionCall` carries the prompt, the history length and the turn. Nothing in either says that a request is a second attempt at an earlier turn. fiddle installs no hook that retries a turn, so no run it makes retries one today. An invented field is worse than a missing one, so the field is not added.

**The bound and the redaction are unchanged.** Time and a reason add tens of bytes to a line and repeat nothing. `elapsed_ms` and `duration_ms` are numbers, so no new text reaches the file and `Record::rendered` is still the one redaction path. `TranscriptModel` reads one field of the raw response and records no other part of it.
