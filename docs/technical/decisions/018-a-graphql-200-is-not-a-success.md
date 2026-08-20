# 018 — A GraphQL 200 is not a success, and the ready transition is GraphQL only

Status: accepted
Amends 015, which stands for its five REST operations.
Cites: GhCli::graphql, GhCli::api, GhError::outcome, EffectOutcome, process::run_bounded, scripts/verify-graphql-ready.sh

## Context

M3 has to move a pull request from draft to ready for review. This design's first draft had that as a REST `PATCH`, and it was wrong twice over. `scripts/verify-graphql-ready.sh` measures both errors against real GitHub, and this ADR is written from that transcript.

## Decision

Reach GraphQL through a sibling method, `GhCli::graphql`, and leave `GhCli::api`'s contract exactly as written. Treat a 200 carrying a non-empty `errors[]` as a refusal, and classify it by `errors[0].type` rather than by the status. Believe no mutation's own answer; let the postcondition read decide.

## Consequences

- The transcript is the evidence, and it is a script rather than a test. `verify-graphql-ready.sh` needs a credential and real GitHub, so it cannot gate. The offline suite can only prove that `GhCli::graphql` classifies these responses this way.
- The project gave up widening `GhCli::api`. A single method whose classification depended on the URL would cost more. Each of 015's five REST call sites would become something to re-read.
- Two classification rows are measured and two are argued. `NOT_FOUND`, `FORBIDDEN` and `UNPROCESSABLE` came off the wire. The unrecognised-type row cannot be measured by construction, and it is the one most likely to matter later.
- `markPullRequestReadyForReview` turned out to be idempotent, which the design did not rely on and now knows. A second call answers 200 with `data` populated and `isDraft: false`, no `errors[]`, and `gh` exits 0.
- A 200 with `data` and no errors is still not believed. This decision moves the adapter from believing a refusal to reporting one.

## The transition is not REST, and REST does not say so

`draft` is absent from `PATCH /repos/{owner}/{repo}/pulls/{number}`'s body parameters, which take `title`, `body`, `state`, `base` and `maintainer_can_modify`. What that means in practice is worse than a refusal:

```
REST PATCH draft=false: HTTP/2.0 200 OK
draft after that PATCH: true
```

The field is accepted, ignored and reported 200 OK. A REST implementation would have believed a success that moved nothing. So the transition exists only as the GraphQL mutation `markPullRequestReadyForReview`, whose input is a node id rather than a number, which the pull request's own REST read already returns as `node_id`.

## A refused mutation answers 200

Not 404, and not 422:

```
HTTP/2.0 200 OK
{"data":{"markPullRequestReadyForReview":null},
 "errors":[{"type":"NOT_FOUND","path":["markPullRequestReadyForReview"],
            "message":"Could not resolve to a node with the global id of 'PR_kwDOnosuchnode'"}]}
gh exit: 1
```

Three things there matter. The status line says 200, `data.<field>` is null, and `errors[]` carries a machine-readable `type` beside its human message. That `type` is what makes a classification possible at all. `gh` exits 1, the same code a REST 404 produces, so the exit code discriminates nothing here.

**This qualifies ADR 015.** That decision states `api`'s contract as "the status line carries the verdict", and `crates/fiddle-runtime/src/github/cli.rs` realises it in one line, `if response.status >= 400`. That line is correct and load-bearing for the five REST operations built on it. Against a refused GraphQL mutation it returns a success. A reader of 015 alone would conclude the status line is always the verdict, and would be right about every call 015 was written for and wrong about this one. Hence an ADR rather than a comment, because somebody will otherwise correct it back.

The blast radius is worth stating precisely. Step 8 of the effect protocol reads the postcondition on every path, so the verdict would still come out right and the pull request would be read back and found a draft. What would be wrong is the diagnostic, which would report a lost write about a call that was refused.

## The classification

| `errors[].type` | `EffectOutcome` | Why |
| --- | --- | --- |
| `NOT_FOUND` | `NotCommitted` | GitHub could not resolve the node, so nothing was reached to mutate. |
| `FORBIDDEN` | `NotCommitted` | GitHub declined before acting, leaving no room for it having acted. |
| `UNPROCESSABLE` | `Unknown` | REST 422's reason exactly. |
| anything else, including absent | `Unknown` | An unrecognised refusal is not evidence about the world. |
| empty `errors[]`, `data` present | claimed success, still not believed | Step 8 decides, as for every other operation. |

`FORBIDDEN` came from a mutation this credential may not issue, and carries an `extensions` key the others do not:

```
{"data":{"closeIssue":null},
 "errors":[{"type":"FORBIDDEN","path":["closeIssue"],
            "extensions":{"saml_failure":false},
            "message":"Resource not accessible by personal access token"}]}
```

**`UNPROCESSABLE` is `Unknown` for REST 422's reason, and that is the same refusal in two spellings.** The probe issues one cause, creating a ref that already exists, down both surfaces in one run against one repository:

```
graphql: 200 / {"errors":[{"type":"UNPROCESSABLE","path":["createRef"],
  "message":"A ref named \"refs/heads/main\" already exists in the repository."}]}
rest:    422 / {"message":"Reference already exists","status":"422"}
```

`GhError::outcome` already classifies 422 as `Unknown`, because the number covers malformed input, invalid ref syntax, spam protection and "already exists" — a refusal and a success wearing one number. `UNPROCESSABLE` inherits that reasoning because it inherits the ambiguity. The message above is an "already exists", which describes a world where the thing the caller wanted is present. Calling it `NotCommitted` would report that nothing happened about a world where it had, possibly earlier and possibly by this same run.

**An unrecognised `type` is `Unknown` and not a failure**, for a different reason. GitHub's error-type set is GitHub's to extend, and a type this build has never seen carries no information about whether the mutation landed. `NotCommitted` would be a claim made on no evidence, and it is the claim that permits a retry, so getting it wrong is how a duplicate is written. `Unknown` is the direction in which an error costs a second read rather than a second write. The same holds for a 200 whose `errors[]` is shaped unexpectedly, and for a body that cannot be parsed at all.

`GhCli::graphql` is the same spawn site as `api`, the same five-name environment with no `HOME`, the same `run_bounded` deadline and the same credential. Everything 015's one-screen argument protects is unchanged, because none of it is about the URL.
