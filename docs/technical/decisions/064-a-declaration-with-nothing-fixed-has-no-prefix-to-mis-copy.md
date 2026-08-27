# 064 — A declaration with nothing fixed has no prefix to mis-copy

Status: accepted
Cites: resolve, spelled_as_one_line, Undeclared::Program, NAMED_DECLARATIONS, HOW_TO_WRITE_A_DECLARATION, MAX_EXTRA_ARGUMENTS

## Context

Run 32698559042 exhausted 40 turns and produced no report. The agent made the repair at turn 4 and spent about fifteen turns on how to call `run_command`.

Three texts disagreed. The brief renders a declaration through `spell()` as `` `go get` (you may append arguments) `` and says "Write the whole of a line". The tool's parameter description says "the program to run, named as the project declares it". `resolve` matched a declaration on `program` alone, which is `go`, with `get` among its arguments.

The refusal was worse than useless:

```
`go get` is not a program this project declares, and these are:
`go get` (you may append arguments), `go mod tidy`
```

It denied and listed the same string in one sentence. The agent read that as a whitespace problem and sent `"go get "` on the next turn.

## Decision

Two changes, and the second is the one that matters.

**`resolve` accepts the spelling the brief teaches.** A `program` holding whitespace whose first word names a declaration is split, and the leading words move to the arguments. Both spellings reach one command.

**A deployment declares the program, not a prefix.** `[[workspace.commands]] program = "go", args = []` with `extend = "arguments"` permits every subcommand. The brief then spells it `` `go` (you may append arguments) ``, and there is no prefix left to copy into the wrong field.

The refusal names the shape: "Name the program by itself and put the rest in the arguments."

## Consequences

- A prefix declaration is still available and still works. A deployment that wants only `go get` can have it, and now pays no turns for the ambiguity.
- Declaring `go` wholesale is a wider grant than two prefixes. `go run` executes arbitrary code, and this is the same grant the workspace already gives through `[[workspace.checks]]`. It is consistent with the sandbox posture accepted for V1, not an increase beyond it.
- `MAX_EXTRA_ARGUMENTS` of 8 and `MAX_ARGUMENT_BYTES` of 256 still bound every call, and each argument must be one line of printable text. Eight covers `go test ./... -count=1 -shuffle=on -race`.
- Declaring `go` declares `go`. `curl` is still refused.
- The transcript records a tool call and not its result, so the refusal a call received had to be inferred from what the model did next. Tracked as `fiddle-pteu`. **Amended.** ADR 065 shows this consequence is false: the transcript recorded every result all along, and the tracker carries `fiddle-pteu` as `scrapped`.
