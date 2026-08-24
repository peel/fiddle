# 069 — A draft cannot be approved, so its words have to carry

Status: accepted
Cites: read_reviews, ChangesRequested, HumanSaid, Followed::quoted, CHANGES_REQUESTED

## Context

ADR 068 read a `CHANGES_REQUESTED` review as work to answer and read no other review at all. Its stated reason: "An `APPROVED` review is not read as direction. Approval says the change is fine, not that a failing check may be ignored."

Two facts undo it.

**A policy gate asks for exactly what a review is.** `peel/fiddle-test` fails on "a dependency change needs sign-off from a maintainer". An approving review is that sign-off, and it is the most deliberate and most attributable signal the forge offers. fiddle ignored it while accepting a conversation comment.

**GitHub does not let anyone approve a draft.** The unproved draft is the pull request a person is asked to judge, and on a draft the only review state available is `COMMENTED`. So ADR 068 left the draft's only review channel dead. The user submitted `COMMENTED` / `OWNER` / "understood. publish it." on draft #2 and fiddle would have read nothing.

## Decision

The words carry the direction, not the state.

- A review body that is not `CHANGES_REQUESTED` joins the direction candidates beside conversation comments, under ADR 067's entitlement rule.
- A `CHANGES_REQUESTED` review stays work to answer, per ADR 068.
- An empty body contributes nothing either way.

A bare approval still waives nothing, because there is no sentence to quote. That is what ADR 068 was protecting and it survives.

## Consequences

- Steering a draft works through the channel a person actually has.
- Approving with "LGTM" waives no gate. Approving with a sentence that says why does. The difference is whether the maintainer wrote down what they meant, which is the same rule ADR 063 already applies to a comment.
- `APPROVED` and `COMMENTED` are treated alike. A person who comments without approving carries the same weight as one who approves, and on a draft that is the only option they have.
- This supersedes ADR 068's fourth consequence. ADR 068 is not edited.
