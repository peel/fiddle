# 065 — The brief names what passed, so breaking it is a signal

Status: accepted
Cites: Baseline, already_passing_sentence, already_failing_sentence, the_brief_says_what_a_broken_green_check_means

## Context

ADR 061 made the brief name the checks the tree already failed, so the agent would stop spending its budget on defects it did not introduce. It said nothing about the checks that already passed.

Run 32706308580 shows what that costs. The transcript's own tool results:

```
t3 run_check        exit 0
t4 go build ./...   exit 0
t5 go vet ./...     exit 0
t6 edit_file go.mod lowered the `go 1.25.11` directive
t7 go build ./...   exit 1: google.golang.org/api@v0.285.0 requires go >= 1.25.8 (running go 1.24.13)
```

The tree was green. The agent changed a line with no bearing on the advisory, broke the build, and spent the remaining 33 turns adding further changes on top. It never touched `github.com/golang-jwt/jwt/v4`. Forty turns, no report.

It knew. It read exit 1 at turn 7. Nothing had told it that a green check turning red means the last change was wrong.

## Decision

`baseline` returns both halves as `Baseline { failed, passed }`. The brief names the passing checks and says what a break means: undo that change rather than making another one on top of it.

The two sentences are mirrors. One says a red check is not yours; the other says a green check is yours to keep.

## Consequences

- The remedy is named, not implied. A model that has just broken something reaches for another change unless told otherwise, which is exactly what this run did.
- Only `failed` decides whether an attempt is refused. `passed` is advice to the agent and changes no verdict, so ADR 061's rule stands unaltered.
- The list is as long as the deployment's check list, so the brief grows with it. Seven checks is one line.

## Correction to ADR 064

ADR 064's consequences say the transcript records a tool call and not its result, and cite `fiddle-pteu`. **That is false.** `TranscriptHook::on_tool_result` writes `.text("result", &event.presentation.render())`, and every call has recorded its result all along. The bean is scrapped.

The claim came from a reading script of mine that printed only the arguments. Two runs were diagnosed by inference from the model's next move when the answer was already in the file. The inferences happened to be right, which is the worst way to be wrong.

ADR 064 is released as v0.25.0 and is not edited. This record supersedes that line.
