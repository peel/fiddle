# 068 — A review asks for work, and a comment can waive a check

Status: accepted
Cites: read_reviews, read_line_comments, Reviewed, Annotated, ChangesRequested, REVIEW_FRAME, CHANGES_REQUESTED

## Context

A pull request carries three surfaces and fiddle read one.

| surface | endpoint | before |
| --- | --- | --- |
| the conversation | `/issues/{n}/comments` | read |
| the review | `/pulls/{n}/reviews` | not read |
| a comment on a diff line | `/pulls/{n}/comments` | not read |

A maintainer who opens the Files tab, writes on a line and clicks Request changes has done the ordinary thing. fiddle saw none of it. The review also blocks the merge, so the one signal that stops a pull request landing was the one signal fiddle could not read.

ADR 063 had already built the machinery for a comment. It was pointed at the surface a person is least likely to use.

## Decision

Read all three, and keep two meanings apart.

A review whose state is `CHANGES_REQUESTED` is **work to answer**. It is briefed under `REVIEW_FRAME`, which says so: "This is work to do, not permission to leave a check failing." That is where a failed check is already briefed, because it is the same kind of thing.

A comment is **direction**, which ADR 063 lets override a failing check.

Merging the two would invert them. A reviewer demanding work would be read as waiving the check, which is the opposite of what they asked for.

Both surfaces obey ADR 067: only `OWNER`, `MEMBER` or `COLLABORATOR` carries either meaning.

## Consequences

- The agent that answers a review is the capability that opened the pull request. There is no second agent and none is needed: the next run reads the pull request's state and briefs it, as it already does for a failed check.
- A review with an empty body is ignored. Clicking Request changes with nothing written says a person is unhappy and not what to do, and inventing the reason would be worse than waiting for it.
- An `APPROVED` review is not read as direction. Approval says the change is fine, not that a failing check may be ignored, and reading it as the latter would let approval waive a gate nobody discussed.
- A line comment reaches the brief with the file it was written on, and not the line number. A line number moves as the file changes; the path does not.
- Three surfaces are read where one was, so a run makes two more paginated reads per pull request, and it reads both the shared pull request and the unproved draft. That is up to six reads where there was one.
