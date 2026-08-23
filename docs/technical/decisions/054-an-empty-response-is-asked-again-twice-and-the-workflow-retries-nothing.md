# 054 — An empty response is asked again twice, and the workflow retries nothing

Status: accepted
Cites: crates/fiddle-runtime/src/agent/retry.rs, RetryingModel, RetryingModel::bounded, RetryingModel::allowed, RetryingModel::retried, retry::RETRIES, retry::empty_response, agent::attempt_briefed, transcript::RETRY, transcript::UNANSWERED, TranscriptModel, crates/fiddle-acceptance/tests/model_retry.rs, a_gateway_that_answers_emptily_once_is_asked_again_and_the_repair_completes, a_gateway_that_only_answers_emptily_ends_the_attempt_after_the_stated_bound, the_text_the_provider_path_returns_for_an_empty_response_is_the_text_this_retries, an_answer_is_never_retried, a_response_that_arrives_after_one_empty_one_finishes_the_attempt, a_provider_that_only_answers_emptily_ends_the_attempt_after_the_stated_bound, the_bound_counts_the_retries_of_one_attempt_and_not_of_one_turn, a_retry_names_the_turn_the_request_was_sent_under, a_bound_of_none_retries_nothing_and_records_the_refusal

## Context

Run 32639415370 asked the gateway twice and died on the second answer.

```
[0]    brief   choice=required max_turns=40 max_tokens=8192
[0]    sent    turn=1 history=0
[2419] finish  turn=1 reason=tool_calls
[2419] spent   turn=1 in=1349 out=25
[2419] recv    turn=1 list_files
[2422] TOOL    list_files {}
[2422] sent    turn=2 history=2
```

The transcript ends there. Turn 1 was ordinary. The outcome was `retryable — the
provider did not hold up its end: CompletionError: ResponseError: Response
contained no message or tool call (empty)`.

The attempt had 38 turns and 2632 seconds left. It spent none of them. An earlier
run of the same configuration family reached `Fixed by updating golang-jwt/jwt/v4
to v4.5.2`, so the model can do this work.

An empty response is not an answer. A refusal, a 400 and a schema failure each say
something about the request. An empty response says nothing about anything. Rig
builds it in the provider's response conversion: a 200 whose one choice carries no
text and no tool call cannot become a `CompletionResponse`, so
`CompletionError::ResponseError` is returned in its place.

## Decision

**fiddle asks again, inside the attempt.** `RetryingModel<M>` wraps the model and
sends the same `CompletionRequest` again when the provider answers emptily. The
request is unchanged, because nothing about it was refused.

**The bound is two retries, counted over the whole attempt.** `RETRIES` is 2. An
attempt spends at most 2 extra provider calls, whatever its turn budget is. After
the second retry the empty response is returned, and the attempt ends exactly as
it does today.

**Two texts are retried, and no others.** `empty_response` matches
`CompletionError::ResponseError` carrying `Response contained no message or tool
call (empty)` or `Response contained no choices`. Both are a 200 that arrived
holding nothing. Every other error is passed straight back.

## Why two, and why over the attempt

One retry answers the observed failure. The gateway answered once, well, and then
answered nothing. One more call is the whole fix for that run.

Two cover a second, unrelated empty response in a long run. A third covers
nothing that is not already covered. Two retries have proved the provider answers
nothing, and a third is a paid call whose answer is known.

The count is over the attempt, and not over one turn. A per-turn allowance lets a
40-turn attempt spend 80 extra calls, and it spends them on a provider that has
already answered nothing twice. A whole-attempt allowance states the money the
attempt can spend as one number: two calls.

**`max_turns` does not bound this, and it cannot.** ADR 053's returns are turns,
so `max_turns` bounds them a second time. A retry is not a turn: the empty
response never reaches a hook, because rig raises it below the agent loop, so
`RetryingModel` sits below the loop too and rig's turn counter never moves. That
is why the retry needs a bound of its own.

## What is retried, and what is an answer

| the provider said | fiddle | why |
| --- | --- | --- |
| a 200 carrying no message and no tool call | asks again, twice | nothing was refused, and nothing was said |
| a 200 carrying no choice | asks again, twice | the same absence, in the same conversion |
| a 200 whose choice is not an assistant message | ends the attempt | a shape, not an absence, and it will not change |
| a refusal, a 401, a 400 | ends the attempt | the gateway answered, and the answer holds |
| `finish: length` with no tool call | ends the attempt | the ceiling is a bound the deployment set |
| a report that fails the schema | ends the attempt | the model answered, and ADR 053 handles the report |
| a report that accounts for nothing | returns it to the model, twice | ADR 053 |

A retry loop over a deterministic failure spends real money to learn nothing, so
this table prefers too few to too many. `an_answer_is_never_retried` asserts the
three error shapes above the line stay unretried.

**The two texts are pinned to rig and not guessed at.**
`the_text_the_provider_path_returns_for_an_empty_response_is_the_text_this_retries`
deserializes an empty gateway body into the provider's own response type, runs
rig's own conversion, and asserts `empty_response` matches what came back. A text
this code guessed at would be a retry that never fires and a bug that looks like
the fault it was written for. A rig version that renames the text reddens that
test.

## Which layer holds which fault

The host dispatches one run a night, and exit 11 tells an operator the run may
succeed if it is repeated. **No fault is retried by the workflow, and the empty
response is retried by the attempt.**

- **An empty response, mid-attempt.** The attempt. It holds a workspace with
  edits, a history of turns and a scan the deployment paid for. A fresh run
  rebuilds the image, runs the container scan again, and starts at turn 1 to reach
  the state the attempt already had. One provider call is cheaper by every
  measure.
- **An unusable scan, and checks that cannot be read.** Neither. Both exit 11
  before any model call, so the nightly schedule already asks again tomorrow at no
  extra cost. A retry inside the run would repeat the same refused read seconds
  later.
- **An exhausted turn budget, and an elapsed deadline.** Neither. Both are bounds
  the deployment set in its document. Repeating the run repeats the bound. Exit 11
  names the number an operator can raise, and raising it is a decision, not a
  retry.
- **A refusal, a 400, a schema failure, an unknown tool call.** Neither. A fresh
  run reproduces them.

**A 5xx and a transport failure are not retried, and this is a gap and not a
choice.** They have the shape this decision retries: the provider said nothing
about the request. No run in M4b showed one, so no bound for them is written from
evidence, and each retry is a paid call. They belong to the attempt when a run
shows one.

## The transcript

Two record kinds are added.

- `retry` — fiddle asked again. It carries the turn, which retry of the attempt it
  was, the bound, and the text the provider path returned.
- `unanswered` — the bound is spent, and the attempt ends on this response. It
  carries the same four fields.

```
{"record":"retry","elapsed_ms":2422,"turn":2,"retries":1,"bound":2,
 "reason":"Response contained no message or tool call (empty)"}
```

Every retry writes a record. An intermittent fault that leaves no record is an
invisible fault, and ADR 052 exists because invisible faults cost this project
twelve releases. `unanswered` is written for the same reason from the other side: a
file that stops with two `retry` records and nothing after them makes a reader
guess whether the bound was reached or the process died.

**The record's turn is the turn the request was sent under.** `RetryingModel`
counts the requests it is given, and `TranscriptModel` calls it once per turn, so
the two counters move together. ADR 052 already asserts `TranscriptModel`'s
counter against rig's, and
`a_retry_names_the_turn_the_request_was_sent_under` asserts the retry's turn
against a `sent` record.

**`RetryingModel` is wrapped by `TranscriptModel`, and not the other way round.**
A retry above `TranscriptModel` would call it three times for one turn, its
counter would run ahead of rig's, and every later `finish` record would name the
wrong turn.

**The brief names the bound.** `brief` carries `max_retries` beside `max_turns`,
`max_tokens` and `deadline_ms`, so the first record of the file states every bound
the attempt holds.

**ADR 052's rules are unchanged.** `Redaction` is the one path to the file, the
reason is text that passes through it, and the transcript is off unless
`FIDDLE_TRANSCRIPT=1`. Both records are short, and an attempt writes at most three
of them, so neither bound moves.

## What ADR 052 said, and what is true now

ADR 052's amendment says fiddle installs no hook that retries a turn, so no run it
makes retries one. ADR 053 changed the first half, and this decision changes
nothing in it: a retry here is not a turn, and rig's turn counter does not see it.
The amendment's reason for adding no retry field to a `sent` record still holds,
because the retry has a record of its own.

## Consequences

- `attempt_briefed` wraps the model twice. Both wrappers hold the redaction and
  the optional transcript, and neither is installed conditionally, so an attempt
  that writes no transcript retries the same way and records nothing.
- The two bounds are proved by different mutations. Removing the match in
  `empty_response` reddens six unit tests and both acceptance tests. Loosening the
  bound in `RetryingModel::allowed` by one reddens three unit tests and one
  acceptance test, and leaves the detection tests green.
- `a_bound_of_none_retries_nothing_and_records_the_refusal` holds the bound at 0
  and asserts one call and one `unanswered` record. A bound that is a share of the
  turn budget could not be tested this way.
- The acceptance pair differs by one input: the gateway's script. One answers
  emptily once and then repairs the fixture, the other answers emptily three
  times. The second asserts the gateway served exactly three requests, so a
  fourth would reach a closed listener and change the reason the run reports.
- `RETRIES` is not read by the acceptance lane, which re-derives 2 from this
  record. `fiddle_acceptance_depends_on_neither_library_crate_anywhere_in_its_closure`
  forbids that crate from depending on `fiddle-runtime`.
