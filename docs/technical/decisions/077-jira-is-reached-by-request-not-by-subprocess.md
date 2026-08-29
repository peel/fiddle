# 077 — Jira is reached by request, not by subprocess

Status: accepted

Cites: JiraHttp, JiraHttp::api, JiraError::Unauthorized, JiraError::Forbidden, JiraError::Absent, JiraError::AbsentOrRefused, JiraWorkItemPort::read, JiraWorkItemPort, WorkItemPort, EnvRef, CREDENTIAL_MUST_BE_NAMED, CLAMP, REDACTED, port_kind_for, the_jira_client_can_be_neither_printed_nor_serialized, no_surface_a_reader_sees_carries_the_jira_credential, the_same_search_finds_the_credential_when_a_surface_does_carry_it, every_surface_searched_is_output_of_a_jira_read, a_written_jira_token_is_refused, a_payload_reaches_the_text_verbatim_so_its_construction_site_redacts, JiraHttp::quoted, a_credential_planted_in_the_sites_error_body_is_redacted_before_a_reader_sees_it, no_workspace_crate_pulls_openssl_into_its_closure, crates/fiddle-runtime/src/jira/http.rs, crates/fiddle-runtime/tests/compile_fail/jira_http_is_not_printable.rs, crates/fiddle-runtime/tests/compile_fail/jira_http_is_not_serializable.rs, crates/fiddle-runtime/tests/support/stub_jira.rs, crates/fiddle-acceptance/tests/jira_credential.rs, scripts/live-jira-observe.sh, scripts/live-jira-search-shape.sh, scripts/live-jira-write.sh, scripts/test-live-jira-lanes.sh, crates/fiddle-runtime/tests/jira_effect_credential.rs, a_body_that_parses_and_echoes_the_token_is_handed_to_the_caller_with_it_replaced, a_token_a_json_body_must_escape_is_replaced_in_the_value_the_caller_reads, a_workflow_name_that_carries_the_credential_reaches_no_diagnostic, a_revision_field_that_carries_the_credential_reaches_no_diagnostic, an_issue_key_that_carries_the_credential_reaches_no_receipt, a_credential_the_site_echoes_reaches_no_published_report_bundle, issue_from, read_instant, canonical_revision, ConfiguredNames, state_for, WorkState, ProjectedStatus, projected_status, assess, derive_next, no_projected_work_state_moves_the_assessment_or_the_next_action, crates/fiddle-core/src/assessment.rs, crates/fiddle-runtime/src/jira/work_item.rs, a_refused_credential_and_a_missing_issue_do_not_read_alike, the_stub_answers_a_refused_credential_the_way_the_measured_site_answers_it, a_read_the_site_answers_asks_the_site_nothing_further, a_status_other_than_404_names_its_own_cause_and_costs_no_credential_check, crates/fiddle-runtime/tests/jira_work_item.rs

## Context

The RFC prefers an existing tool to new code. ADR 015 applied that rule to
GitHub and made `gh api -i` the adapter. It gave one reason for the shape, and
the reason was containment rather than convenience: the credential is the asset,
and `crates/fiddle-runtime/src/github/cli.rs` builds a child from `env_clear`
plus five names and nothing else.

M5a adds a Jira observation port. The RFC names `acli` as the Jira adapter in
its adapter section, its milestone list and its adapter contract tests, so the
plan of record points at the same shape a second time. That plan does not
survive contact with how `acli` takes a credential.

The rule the plan rests on already anticipates this. It is an ordering, not a
mandate, and it is a per-operation choice: an existing CLI **when it provides
stable structured output and suitable authentication**, then a library, then a
narrow direct API call. `acli` fails the second half of the first condition, so
this record moves one operation down the same list rather than overriding it.
The RFC's own open question asks which `gh` or `acli` operations need a narrow
fallback. It asks about output stability. The answer here is about
authentication.

## Decision

Reach Jira by request. `JiraHttp` holds one `reqwest::Client`, one base URL, one
credential and one timeout, and `JiraHttp::api` sends every call.
`JiraWorkItemPort` reads an issue through it and answers with an `Observation`.

ADR 015 chose a subprocess because the credential is the asset, and a child
built from `env_clear` plus five names is an adapter whose whole view of the
world fits on one screen. `gh` accepts `GH_TOKEN` in that child. `acli` does
not: Atlassian's CI guide pipes a token into `acli jira auth login`, which
writes credential state to a configuration directory. An `acli` adapter must
hold a credential directory on disk, which is weaker containment than a header
on one request, and ADR 029 refuses an inherited authority. A login that
happened before this process started is exactly that authority.

The one-screen property does not survive a library. `JiraHttp` answers with
discipline and not with a boundary: one construction, no `Debug`, no
`Serialize`, redaction and a length bound on every error text. A discipline is
not a guarantee, and this ADR says so rather than implying otherwise.

`reqwest` becomes a direct dependency of `fiddle-runtime`, declared
`default-features = false, features = ["rustls", "json"]`. ADR 015 already
recorded that a subprocess saves no dependency here, because `reqwest` was in
the resolved graph through `rig-core` before this milestone.

## The discipline, stated so a reader can check it

- **One construction.** `JiraHttp::new` is the only place a Jira credential
  becomes an `Authorization` header. The header is built once, marked sensitive,
  and cloned onto each request. The other header-building site in the crate is
  `git/publish.rs`, and it carries the GitHub push credential, not this one.
- **Nothing prints the client.** `JiraHttp` derives neither `Debug` nor
  `Serialize`. `the_jira_client_can_be_neither_printed_nor_serialized` pins both
  with trybuild, so the reason is part of the assertion and a derive added later
  fails the case. `JiraResponse` derives `Debug` and holds no credential.
- **Redaction, then a bound.** Every error text `JiraHttp` builds passes the
  encoded credential and the raw token through a replacement with `REDACTED`,
  then a clamp at `CLAMP` bytes on a character boundary. Since M5b the body
  `JiraHttp::api` hands back is scrubbed too: `JiraResponse::body` is walked
  member by member and the credential is replaced in every string and every
  member name. Redacting only error text held while Jira was read-only, because
  a read that succeeded put nothing on a reader's surface. A write puts the
  site's own words into a receipt and into a published bundle, and those are
  built from a 2xx body that no error path ever touches. Three effect-level
  cases reded before the scrub landed:
  `a_workflow_name_that_carries_the_credential_reaches_no_diagnostic`,
  `a_revision_field_that_carries_the_credential_reaches_no_diagnostic` and
  `an_issue_key_that_carries_the_credential_reaches_no_receipt`, all in
  `crates/fiddle-runtime/tests/jira_effect_credential.rs`. The invariant is now
  statable in one sentence: no string derived from the site reaches a reader
  without passing the client's redaction. `JiraError` itself
  redacts nothing:
  `a_payload_reaches_the_text_verbatim_so_its_construction_site_redacts` asserts
  that it prints a caller's payload word for word, so the obligation stays where
  the credential is, at the construction site.
- **Two variants carry no caller text at all.** `JiraError::Unauthorized` and
  `Forbidden` carry a status and nothing else, so the two failures most likely
  to quote a credential exchange have no text to quote.
- **The credential is named, never written.** `[jira] user` and `token` are
  `EnvRef` values, so `fiddle.toml` holds the variable name. A written string
  fails deserialization with `CREDENTIAL_MUST_BE_NAMED`, so a document that
  carries a secret does not load. `a_written_jira_token_is_refused` and
  `a_written_jira_user_is_refused_because_it_is_half_the_credential` cover both
  halves, because basic authentication signs with both.

The port is async. `WorkItemPort::observe` is an `async fn` under
`async_trait`, because a request is awaited rather than waited on, and
`port_kind_for` selects the Jira port from `InvocationScheme::Jira` arm by arm
with no wildcard. A scheme added later is a compile error rather than a silent
fall through to the stub port.

## Consequences

- The test seam moves, and it moves toward the real protocol. ADR 015's seam
  substitutes a scripted `gh`, and that ADR records the cost: the suite proves
  the parser reads what the stub prints and can never prove the stub prints what
  `gh` prints. `[jira] base_url` points at a loopback HTTP server instead. The
  stub still decides every byte, so the fidelity question does not disappear. It
  narrows. ADR 015's parser reads a status line, splits headers from a body and
  guesses at line endings, and all of that is code this repository has to get
  right. Here hyper does the framing, and a stub that speaks something other than
  HTTP fails at the client rather than being read as an answer. What stays ours
  is the body: `serde_json` decodes it and `issue_from` names the four fields it
  needs. So the suite still proves only that this adapter reads what the stub
  serves.
- The TLS closure is asserted, not assumed.
  `no_workspace_crate_pulls_openssl_into_its_closure` refuses `openssl`,
  `openssl-sys` and `native-tls` in the resolved closure. Measured 2026-08-26,
  and it corrects the obvious reading of the feature line above:
  `default-features = false` is not what holds the property. reqwest 0.13
  declares `default-tls = ["rustls"]`, so removing that line still resolves to
  rustls and the guard still passes. The feature that reds the guard is
  `native-tls`. Both inversions were run when the dependency landed: removing
  `default-features = false` gives `1 passed`, exit 0; enabling `native-tls`
  gives `FAILED`, exit 101, with `openssl`, `openssl-sys`, `native-tls`,
  `tokio-native-tls` and `hyper-tls` all in the closure.
- One observation is one request with one timeout, where a GitHub publication
  spawns ten children.
- The failure surface is typed. ADR 015 records that `gh` returns bytes on two
  streams and that the project gave up typed failures for GitHub. `JiraError`
  names five read failures, and a unit test matches all five with no wildcard
  and pins each one's words, so a sixth variant is a compile error in that test
  rather than a new sentence nobody reviewed.

## What this gives up

**1. The one-screen argument cannot be made again.** `JiraHttp` runs in this
process. It shares the address space, the environment and the TLS configuration
with the agent loop, the capability code and every dependency in the graph.
There is no `env_clear` to point at and no five names to enumerate. The
statement ADR 015 could make about `gh` cannot be made about this adapter, and
no test can restore it.

**2. What replaces the boundary is a discipline, and a discipline is not a
guarantee.** Four of the five items above are pinned by a test. A test that
passes is still not a type that refuses.

The item with no test is the first one. Nothing asserts that `JiraHttp::new` is
the only place a Jira credential becomes a header. ADR 015 has the same gap for
`-i`, and it says so: no test can catch a second call path that does not exist
yet. This adapter inherits that sentence.

The redaction claim is measured, and three tests keep the measurement from
being vacuous. `no_surface_a_reader_sees_carries_the_jira_credential` searches
every surface a Jira invocation writes or says, and it pins that set as a
42-name census, so a surface this build starts writing cannot join the tree
unsearched and a surface it stops writing cannot leave the search passing on an
absent file. Ten of those names are surfaces of a run that filed a ticket into a
loopback stub, so the census covers the path that writes and not only the paths
that read. `the_same_search_finds_the_credential_when_a_surface_does_carry_it`
plants the credential on a surface, so the search cannot pass by finding nothing
anywhere. `every_surface_searched_is_output_of_a_jira_read` requires at least six
of the searched surfaces to name both the site and the issue key, so a surface
that is not Jira output cannot pad the census.

What the census cannot reach is a surface written outside a Jira invocation. The
lane bounds this adapter's own output; it says nothing about a future code path
that reads the credential out of this heap and writes it somewhere else.

**3. The credential now lives where ordinary code can reach it.** With `gh`, a
capability that wanted the GitHub token would have to spawn its own child and
supply it. With `JiraHttp`, the token is a `String` in this heap for the life of
the port. Nothing structural stops a future code path from reading it. This is
recorded as a known issue in `docs/technical/SYSTEM.md` and not resolved here.

**4. A dependency is now on the credential path.** A `gh` that changes its
output shape fails loudly on the first call, and ADR 015 records
`[github] cli.program` as the operator's answer. A `reqwest` or `rustls` that
changes behaviour is a lockfile move inside this process, and the operator has
no equivalent pin.

## What is verified, and against what

The adapter is verified against two loopback stubs, and each one binds
`127.0.0.1:0` and speaks HTTP over a real socket.

`crates/fiddle-runtime/tests/support/stub_jira.rs` serves the unit lanes. It
answers the issue route, an absent issue, a refusal with a JSON body, a refusal
with an HTML body, an unrouted path, a method it does not serve, a request line
it cannot parse, and a body that arrives with no content length. `StubJira` in
`crates/fiddle-acceptance/tests/support/mod.rs` serves the acceptance lanes, and
`crates/fiddle-acceptance/tests/jira_credential.rs` runs the public CLI against
it and searches every surface a reader sees.

One issue is verified against Atlassian.
`scripts/live-jira-observe.sh` is the Jira counterpart to
`scripts/live-github.sh`. It reads one issue two ways — a direct
`/rest/api/3/issue/KEY?fields=status,updated` call beside
`fiddle inspect jira:KEY --json` — and it refuses rather than skips when the
site, the issue key, either half of the credential or the binary path is absent.
It records evidence and does not gate. It read ISP-267 from
`snplow.atlassian.net` on 2026-08-26, and `docs/technical/RUNBOOKS.md` records
what came back.

When this record was first written, three things were arguments rather than
measurements: the shapes Atlassian's `/rest/api/3/issue` returns, the
`fields.updated` formats a real site emits, and whether a real workflow's status
names match a deployment's `[jira.workflow]` table. That read settles the first
two and leaves the third.

- **Now a measurement: the shape `/rest/api/3/issue` returns.** The site
  answered with `fields.status.id`, `fields.status.name`,
  `fields.status.statusCategory.name` and `fields.updated`, which are the four
  values `issue_from` reads. The response root carried no `version` key, so
  `fields.updated` is the only revision the site offers and the target identity
  has nothing better to rest on.
- **Now a measurement: the `fields.updated` format.** Jira Cloud sends a
  **colonless** offset. That is not RFC 3339, so `read_instant` needs the two
  further format descriptions it carries after `Rfc3339`. Without them
  `canonical_revision` answers `None` and the port reports `Unavailable`.
- **Now a measurement: real `[jira.workflow]` status names.** M5b's
  `scripts/live-jira-search-shape.sh` read `/rest/api/3/project/ISP/statuses` on
  2026-08-28. Every issue type in `ISP` — Task, Story, Bug, Epic, Spike and
  Sub-task — offers the same six statuses: `To Do` in category `new`,
  `In Progress`, `In Review` and `Blocked` all in category `indeterminate`, and
  `Done` and `Won't Do` both in category `done`. So the category cannot separate
  blocked work from work in flight, and a deployment that wants `Blocked` to read
  as blocked has to name it in `[jira.workflow]`; `ConfiguredNames` and
  `state_for` are what carry that, and `project` falling back to the category
  would answer the same state for three different situations. This is one project
  on one site: a second workflow is still unmeasured.

The measurement is one issue on one site. A second project, a second workflow
and a second issue type are unmeasured, and so is every failure arm but the 404
the paragraph below measures.

**Now a measurement: what a bad credential returns.** Six probes ran against
`snplow.atlassian.net` on 2026-08-27, with an operator's valid credential and
with that same credential corrupted by four appended characters. A read of
`ISP-239`, a real issue on the real tenant, answered **200** with the valid
credential, **404** with the corrupted one and **404** with no credential at all.
A read of `ISP-99999999` with the valid credential answered **404** too. So a bad
credential, no credential and an issue that does not exist are one answer on an
issue read, and no issue read reaches `JiraError::Unauthorized` or
`JiraError::Forbidden` by its own status. `/rest/api/3/myself` separates them: it
answered **401** with the corrupted credential and **200** with the valid one. It
reads the credential alone and does not depend on issue permissions. This
supersedes the one observation the claim first rested on, a 404 on
`/rest/api/3/project/ISP` from a request that authenticated against the wrong
tenant, which was neither a bad credential nor an issue read.

`JiraWorkItemPort::read` therefore asks `/rest/api/3/myself` once, and only when
an issue read answered 404. A 401 there names `JiraError::Unauthorized`, a 2xx
names `JiraError::Absent`, and any other answer names
`JiraError::AbsentOrRefused`, which carries both causes and the endpoint that
settles them rather than reporting an absence no probe established. The check
costs one request on the 404 path and none on any other, so a 404 can wait one
further `[jira] timeout` before it reports, which
`a_read_the_site_answers_asks_the_site_nothing_further` and
`a_status_other_than_404_names_its_own_cause_and_costs_no_credential_check` hold.
The suite no longer pins the opposite shape:
`a_refused_credential_and_a_missing_issue_do_not_read_alike` in
`crates/fiddle-runtime/tests/jira_work_item.rs` drives both worlds with a 404
issue read, as the site answers, and
`the_stub_answers_a_refused_credential_the_way_the_measured_site_answers_it`
pins `crates/fiddle-runtime/tests/support/stub_jira.rs` to the probes.
`fiddle-2n67` held this read and is now done, so the tracker no longer carries it
as status `todo` tagged `blocked` and `needs-attention`.
`docs/technical/RUNBOOKS.md` carries the same measurement as operator advice.

**No decision reads the projection, so nothing measures what it decides.** No
consumer branches on `WorkState`. Serde writes it and `assess` ignores it:
`assess` branches on whether each observation is available and on the
change-set marker, `derive_next` maps its three verdicts, and
`projected_status` therefore reaches the `fiddle inspect --json` payload and
stops there. So a projection that named the wrong state would move no outcome,
and the third item above can compare a status name with a configured one and
can compare nothing that follows from one. M5b supplies the first consumer:
`docs/plans/2026-08-26-m5b-jira-effects.md` Task 6 gives
`jira.issue_transitioned` an `inspect` that reads an issue's current status to
choose a target state. Until that lands,
`no_projected_work_state_moves_the_assessment_or_the_next_action` in
`crates/fiddle-core/src/assessment.rs` pins the absence and not the projection.
It holds the assessment and the next action equal across all six `WorkState`
variants and all three marker cases, and it reds when a projected `Done` is
allowed to complete the work. `docs/technical/SYSTEM.md` carries it as a known
issue.

## The identifier question ADR 011 left open

ADR 011 admits ASCII letters, digits, `-`, `_` and `:` in an invocation
reference value, and it asks somebody to confirm the real Jira format before the
adapter lands. The answer is that a Jira key cannot fail that grammar.

Atlassian documents a project key as `[A-Z][A-Z0-9]+` and an issue key as
`<project-key>-<number>`. Both alphabets are subsets of ADR 011's class, so
every issue key parses and `jira:IDENT-1` needs no widening. This is Atlassian's
documented format, not a range this repository measured, and a site with a
renamed project key would still produce a key of that shape.

The same question about a `scheduled` or a `scanner` identifier stays open. This
ADR answers for Jira only.

## What would reverse it

**An `acli` that takes a credential per invocation.** If Atlassian documents an
environment variable that authenticates one `acli` call with no login step, the
containment argument returns and the delegate rule points back at `acli`.

**An authentication flow that cannot be a header on one request.** OAuth 2.0 3LO
holds a refresh token and a token lifetime, which is credential state over time
rather than one value. That state has to live somewhere, and where it lives is a
new decision rather than an extension of this one.

Reversing does not reach the port. `JiraWorkItemPort` is written against
`JiraHttp::api` and `JiraError`, so a different transport replaces one
construction and leaves the observation, the status projection and the
`WorkItemPort` implementation untouched.

This supersedes no earlier ADR. It bounds ADR 015's one-screen argument to the
adapter that argument was made about.
