# 073 — A built-in token creates a workflow run it cannot execute

Status: accepted
Cites: observe_genuine_failure, Settlement, CHANGES_REQUESTED, entitled

## Context

ADR 070 said fiddle authors its pull requests as the person expected to review them, and named `snowplow-identities` as a deployment that does. **Both halves are wrong for that repository.**

Its workflow passes `secrets.GITHUB_TOKEN`, so its pull requests are authored by `app/github-actions`. A person there can approve and request changes, because they did not author them. The authorship problem ADR 070 describes is real, and it is `peel/fiddle-test` before its app, not this one.

What is wrong there is different and worse. On #255's head:

```
ci [pull_request]  completed/action_required
Wiz Data Scanner (wiz-...)  success   × 6
```

`action_required` means the run was created and never executed. GitHub does not run a workflow for a push made with `GITHUB_TOKEN` without a human approving it. So the repository's own `build`, `test-race` and lint never run on anything fiddle pushes, and `observe_genuine_failure` reads six external app checks and nothing else.

The earlier `build: FAILURE` on #254 came from a push made with a person's token, not fiddle's.

## Decision

A deployment gives fiddle a token that executes workflows: an app installation token, or a personal one. `GITHUB_TOKEN` is not sufficient, and the reason is not authorship.

Both workflows now mint an app token when `FIDDLE_APP_ID` is set and fall back to what they had, so neither changes until an app is installed.

## Consequences

- Until an app is installed on `snowplow-identities`, its CI feedback is Wiz's app checks alone. A repair that breaks the build is not detected by fiddle, and the draft it publishes says the checks passed because the only checks that ran did.
- ADR 070's decision stands — fiddle should have its own identity — and its stated cause covered one deployment rather than both. This record separates the two reasons: authorship blocks the review states, and `GITHUB_TOKEN` blocks workflow execution. A deployment can have either problem without the other.
- `action_required` is invisible to `Settlement`. A created-but-unexecuted run reports no check, so the head reads as settled with nothing failing. That is the ADR 062 ambiguity in a form ADR 062 does not cover, and it is not fixed here.
