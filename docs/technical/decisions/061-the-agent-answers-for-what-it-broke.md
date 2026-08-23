# 061 — The agent answers for what it broke, not for what it found broken

Status: accepted
Cites: evaluate::baseline, Contract::excused, CheckResult::excused, NeedsWork::CheckFailed, already_failing_sentence

## Context

fiddle ran the declared checks only after the agent had worked, and never asked which of them the tree already failed.

Run 32654571617 shows the cost. `golangci-lint run` was declared for the first time. `probe_cve.go` carries an unused variable, in the base at commit `2ad2281`, which the agent never touched. The agent repaired the advisory, ran the check, was handed a failure about a file it did not write, and exhausted all 40 turns on it. The two repairs measured in ADR 059 used 16 and 12.

## Decision

Run the declared checks over the workspace before the agent starts, and record which already fail.

The brief names them: "These checks already failed on this project before you changed anything. They are not yours to fix, and this run does not hold them against you."

A check that was already failing does **not** excuse the attempt. `first_failure` still finds it, so the tree is still not proved and the reader still gets a draft. What changes is the reason, which now says the attempt did not break it.

The baseline skips `ArtefactWritten`, because scanning the tree before the repair proves nothing and costs minutes.

## Consequences

- The agent stops spending its budget on a defect it did not introduce. That was the whole cost of the defect.
- fiddle still refuses to stand behind a tree whose checks fail, whoever broke them. Three acceptance tests say a failing check decides whether a reader sees a repair or a draft, and they were right: publishing a red tree as a proved pull request would have made it unmergeable while claiming otherwise. An earlier version of this change excused the failure from refusal, and those tests caught it.
- A repository whose base fails a declared check therefore never gets a proved pull request from fiddle. That is the truth about that repository, and the fix is to repair the base or stop declaring the check.
- Every exit-code check now runs twice per attempt. For a deployment declaring a build, a test suite, a lint and a container build, that is minutes. A baseline cached per base revision would pay for itself; a stale one would be worse than none.
