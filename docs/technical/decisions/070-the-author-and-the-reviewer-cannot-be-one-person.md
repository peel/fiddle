# 070 — The author and the reviewer cannot be one person

Status: accepted
Cites: read_reviews, ChangesRequested, CHANGES_REQUESTED, Reviewed, entitled

## Context

ADR 069 said "GitHub does not let anyone approve a draft" and used that to explain why a `COMMENTED` review was all the user could submit on draft #2.

That is the wrong reason. `peel/fiddle-test#2` and `#3` are both authored by `peel`, because `FIDDLE_GITHUB_TOKEN` is that person's token, and **GitHub does not let anyone approve or request changes on a pull request they authored**. The draft state is not what limited them.

The observation in ADR 069 stands and its decision stands. Its stated cause does not, and this record corrects it. ADR 069 is not edited.

## Why the correction matters

It moves the problem. If a draft were the obstacle, the native review states would work on a published pull request. Because authorship is the obstacle, they never work anywhere in this deployment, and two mechanisms built on them are unreachable:

- ADR 068 reads `CHANGES_REQUESTED` as work to answer.
- `fiddle-bcaa` makes such a review start a run.

`CHANGES_REQUESTED` is also the only review state that blocks a merge, so the strongest thing a maintainer can say is the thing they cannot say.

## Decision

fiddle needs an identity of its own. A pull request it opens must not be authored by the person expected to review it.

A GitHub App is preferred. Its pull requests are authored by the app, its tokens are short lived and scoped per installation, and its pushes start workflow runs, which `GITHUB_TOKEN` does not. A dedicated machine account with its own token is the cheaper answer and costs one more credential to look after.

This is a deployment change. Nothing in the product moves: the review states are read already, and `fiddle-oh9d` carries the work.

## Consequences

- Until an identity exists, a maintainer can steer only by comment. That path is proved: `peel/fiddle-test#3` was published over a failing check on the strength of a `COMMENTED` review.
- ADR 067's entitlement rule is unaffected. It reads `author_association`, which a machine account or an app installation reports the same way.
- A bot author changes what `conversation` filters. It already drops what a bot wrote, so fiddle's own comments will not be read back as direction, which is what that filter was for.
- Two deployments will differ until both move. `snowplow-identities` and `peel/fiddle-test` both author as a person today.
