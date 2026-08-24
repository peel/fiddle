# 071 — The agent can find text without reading a file to the end

Status: accepted
Cites: SearchFiles, SearchFilesArgs, Found, Match, MATCH_CAP, a_match_says_which_file_and_which_line

## Context

The tool set offered `read_file`, `edit_file`, `write_file`, `list_files`, `run_check` and `run_command`. Nothing searched.

`snowplow-identities` run 32765904429 spent forty turns and reported nothing. The repair was made at turn 2 and every check passed by turn 7. The rest went on seeing the `go.sum` entries:

```
t3  read_file go.sum {limit:1, offset:1}   "lines 1 to 1 of 985. It withheld 984 lines."
t10 run_command go grep golang-jwt/jwt/v4  go grep: unknown command
t11 read_file go.sum {limit:1, offset:400} one more line of 985
```

It tried to invent a search through `go`, then read a 985-line file a line at a time.

`peel/fiddle-test` has a `go.sum` of a few lines, so `read_file` shows all of it and the question never arises. The same binary passed there and failed here on the size of one file.

## Decision

Add `search_files`: a literal text, an optional path, and every matching line with its path and line number.

Literal, not a pattern. The agent wants to know where a module name appears. A regular expression is a way to be wrong slowly, and a bad one is a way to hang a run.

`MATCH_CAP` is 200 matches, bounded also by `RESULT_CAP_BYTES`. A search that found more says how many it withheld, and a search that found none says so rather than answering with an empty list that reads like a failure.

A deployment can also declare `grep` under `[[workspace.commands]]`, which needs no release. Both were done. The tool is the one that does not depend on each deployment remembering to declare a Unix utility.

## Consequences

- The tool searches what `git ls-files --cached --others` lists, so it sees a file the agent has just written and does not see an ignored one. `read_file` reads by path and is not bounded the same way, so the two disagree about an ignored file.
- A line longer than 400 bytes is cut with an ellipsis, so one long line cannot spend the result cap.
- Seven tools are offered where a program is declared and six where none is. Two acceptance lanes pin those numbers and were renamed, which the ADR-cites check then traced to six documents.
- This is the first defect this milestone that `peel/fiddle-test` could not have found. Its files are too small. A testbed that is easier to run is also easier to pass.
