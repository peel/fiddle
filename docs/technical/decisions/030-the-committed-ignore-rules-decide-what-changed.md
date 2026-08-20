# 030 — The project's committed ignore rules decide what changed

Status: accepted
Cites: fiddle_runtime::workspace::changes, workspace/changes.rs, changes::STATUS, Workspace::baseline_ignore, Workspace::changed_files

## Context

`workspace::changes` derives what an attempt changed from `git status --porcelain=v1 -z -uno` plus `git ls-files --others`. An attempt can write `.gitignore`, because `.gitignore` is a versioned file in the project it repairs. So the thing being judged could author the question.

## Decision

Snapshot the ignore file from the branched HEAD before the attempt starts. Keep the copy outside the worktree, and name it with `--exclude-from`. Honour no other ignore source.

## Consequences

- `--exclude-standard` is refused. A `*` written into `.gitignore` hides every created file, bypasses the changed-file cap, and publishes a count that is not true.
- `--ignored` is refused too. It would answer that attack and lose what the exclusion is for, since one `run_check` writes a whole `target/` tree.
- A nested ignore file is not honoured, and neither is the operator's global excludes file. The error runs towards reporting more.
- One attempt's evidence must not depend on whose machine ran it. That is the whole of the rule.
- What was given up: a project with a legitimate nested ignore file sees extra paths in its change set. No deployment has hit this, and the alternative let a machine's configuration decide what an attempt is accountable for.
