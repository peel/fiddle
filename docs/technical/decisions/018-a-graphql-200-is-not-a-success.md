# 018 — A GraphQL 200 is not a success, and the ready transition is GraphQL only

Status: accepted

## Context

M3 has to move a pull request from draft to ready for review. This design's first
draft had that as a REST `PATCH`, and that was wrong twice over — once about
where the transition lives, and once about how its failures arrive.

`scripts/verify-graphql-ready.sh` measures both against real GitHub, and this ADR
is written from that transcript rather than from the documentation. Every response
quoted below is from a run of that script against
`peel/fiddle-effects-acceptance`; `target/graphql-probe.log` holds the whole
thing.

**The transition is not REST, and REST does not say so.** `draft` is absent from
`PATCH /repos/{owner}/{repo}/pulls/{number}`'s body parameters, which accepts
`title`, `body`, `state`, `base` and `maintainer_can_modify`. What that means in
practice is worse than a refusal:

```
REST PATCH draft=false: HTTP/2.0 200 OK
draft after that PATCH: true
```

The field is accepted, ignored, and reported **200 OK**. A REST implementation
would have believed a success that moved nothing. So the transition exists only
as the GraphQL mutation `markPullRequestReadyForReview`, whose input is
`{ pullRequestId: ID }` — a node id, not a number, which the pull request's own
REST read already returns as `node_id`.

**A refused GraphQL mutation answers 200.** Not 404, not 422:

```
HTTP/2.0 200 OK
{"data":{"markPullRequestReadyForReview":null},
 "errors":[{"type":"NOT_FOUND","path":["markPullRequestReadyForReview"],
            "locations":[{"line":1,"column":22}],
            "message":"Could not resolve to a node with the global id of 'PR_kwDOnosuchnode'"}]}
gh stderr: gh: Could not resolve to a node with the global id of 'PR_kwDOnosuchnode'
gh exit: 1
```

Three things in that response matter. The status line says 200. `data.<field>` is
`null`. And `errors[]` carries a machine-readable `type` beside its human
`message`, which is what makes a classification possible at all.

`gh` exits **1** — the same code a REST 404 produces, and the same code
`gh help exit-codes` gives every other failure. So the exit code discriminates
nothing here, which is [ADR 015](015-gh-cli-as-the-github-adapter.md)'s own
argument one level deeper.

**This qualifies [ADR 015](015-gh-cli-as-the-github-adapter.md).** That decision
states `api`'s contract as *the status line carries the verdict*, and
`crates/fiddle-runtime/src/github/cli.rs` realises it in one line:

```rust
if response.status >= 400 {
```

That line is correct and load-bearing for the five REST operations M2 built on
it. Against a refused GraphQL mutation it returns `Ok(GhResponse)` — a success.
A reader of 015 alone would conclude the status line is always the verdict, and
would be right about every call 015 was written for and wrong about this one.
Hence an ADR rather than a comment: someone will otherwise correct it back.

The blast radius is worth stating precisely rather than dramatising. Step 8 of
the effect protocol reads the postcondition on every path, so the *verdict* would
still come out right — the pull request would be read back and found still a
draft. What would be wrong is the diagnostic: the `Ok(None)` branch would report
"the adapter reported success and the postcondition was not observed" about a
call that was refused, sending an operator to look for a lost write that never
left.

## Decision

**GraphQL is reached through a sibling method, `GhCli::graphql`, not through a
widened `GhCli::api`.** Making `api`'s contract conditionally untrue would leave
the adapter with one method whose classification depends on which URL it was
given, and the five REST operations that rely on that contract would each become
a call site to re-read. The sibling puts the new failure mode in one place with
its own tests and leaves the existing contract exactly as written. It is the same
spawn site, the same five-name environment with no `HOME`, the same bound in
`run_bounded`, and the same credential — everything 015's "one screen" argument
protects is unchanged, because none of it is about the URL.

**A 200 carrying a non-empty `errors[]` is a refusal, classified by
`errors[0].type` and never by the status.**

| `errors[].type` | `EffectOutcome` | Why |
| --- | --- | --- |
| `NOT_FOUND` | `NotCommitted` | GitHub could not resolve the node, so nothing was reached to mutate. |
| `FORBIDDEN` | `NotCommitted` | GitHub declined before acting, in terms that leave no room for it having acted anyway. |
| `UNPROCESSABLE` | `Unknown` | REST 422's reason exactly — see below. |
| anything else, including absent | `Unknown` | An unrecognised refusal is not evidence about the world. |
| empty `errors[]`, `data` present | claimed success, still not believed | Step 8 decides, as it does for every other operation. |

`NOT_FOUND` and `FORBIDDEN` are both measured. The `FORBIDDEN` shape came from a
mutation this credential is not permitted to issue, and carries an `extensions`
key the others do not:

```
HTTP/2.0 200 OK
{"data":{"closeIssue":null},
 "errors":[{"type":"FORBIDDEN","path":["closeIssue"],
            "extensions":{"saml_failure":false},
            "locations":[{"line":1,"column":19}],
            "message":"Resource not accessible by personal access token"}]}
```

**`UNPROCESSABLE` is `Unknown` for REST 422's reason, and that is not an
analogy — it is the same refusal in two spellings.** The probe issues one cause,
creating a ref that already exists, down both surfaces in the same run against
the same repository:

```
graphql: HTTP/2.0 200 OK / exit 1
graphql body: {"data":{"createRef":null},"errors":[{"type":"UNPROCESSABLE",
  "path":["createRef"],"locations":[{"line":1,"column":42}],
  "message":"A ref named \"refs/heads/main\" already exists in the repository."}]}
rest:    HTTP/2.0 422 Unprocessable Entity / exit 1
rest body: {"message":"Reference already exists",
  "documentation_url":"https://docs.github.com/rest/git/refs#create-a-reference",
  "status":"422"}
```

`GhError::outcome` already classifies 422 as `Unknown`, and its stated reason is
that the number "covers malformed input, invalid ref syntax, spam protection and
'already exists' — a refusal and a success wearing the same number".
`UNPROCESSABLE` inherits that reasoning because it inherits the ambiguity: the
message above is an "already exists", which is a world where the object the
caller wanted is present. Classifying it `NotCommitted` would report "nothing
happened" about a world where the thing had happened, possibly earlier and
possibly by this same run. Being `Unknown` is what forces the postcondition read
that can actually tell those apart, and that read is the only thing that can.

**An unrecognised `type` is `Unknown` and not a failure**, for a different reason
from the one above. GitHub's error-type set is GitHub's to extend, and a `type`
this build has never seen carries no information about whether the mutation
landed. `NotCommitted` would be a claim — "nothing happened" — made on no
evidence, and it is the claim that permits a retry, so getting it wrong is how a
duplicate is written. `Unknown` is the direction in which an error in this
classification costs a second read rather than a second write. The same applies
to a 200 whose `errors[]` is present but shaped unexpectedly, and to a body that
cannot be parsed at all: this classification's failure mode is looking again.

## Consequences

**The transcript is the evidence, and it is a script rather than a test.**
`scripts/verify-graphql-ready.sh` needs a credential and real GitHub, so it
cannot gate — the gate is offline and credential-free, as 015 requires. What the
deterministic suite can prove is that `GhCli::graphql` classifies the responses
above the way this table says; it cannot prove GitHub still sends them. That is
the same division 015 already made and the same residual risk: the stub's
fidelity is asserted by this probe and by nothing else. Re-running the probe is
what re-establishes it.

**Two classification rows are measured and two are argued.** `NOT_FOUND`,
`FORBIDDEN` and `UNPROCESSABLE` came off the wire. The unrecognised-`type` row
cannot be measured by construction, and it is the row most likely to matter
later.

**`markPullRequestReadyForReview` turned out to be idempotent, which the design
did not rely on and now knows.** Issued a second time against a pull request that
is already ready, it answers 200 with `data` populated and `isDraft: false`, no
`errors[]`, and `gh` exits 0. So the "already in that state" case for *this*
mutation is a success rather than an `UNPROCESSABLE`, and the ambiguity the row
above exists for arrives from other mutations instead. This is worth knowing
because it means a repeated attempt at the ready transition is safe on its face —
and step 8 still decides, because idempotence observed once is not a promise.

**A 200 with `data` and no errors is still not believed.** Nothing in this
decision makes the mutation's own answer authoritative. It moves the adapter from
"believes a refusal" to "reports a refusal", and the postcondition read that
already existed is what turns either into a verdict.

This supersedes no earlier ADR. It qualifies
[015](015-gh-cli-as-the-github-adapter.md) for one method and leaves that
decision's reasoning, its seam and its five REST operations untouched.
