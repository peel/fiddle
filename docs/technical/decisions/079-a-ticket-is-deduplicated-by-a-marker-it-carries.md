# 079 — A ticket is deduplicated by a marker it carries

Status: accepted; amended in M5b by the note "What one live run measured", by
080, and superseded in part by "The claim ledger, which is what exactly-once now
rests on". The marker stands as identity. It is no longer what a run reads to
decide whether to file.

Cites: FileVerdict, a_start_at_offset_is_refused_because_this_endpoint_pages_by_token, FiledIssue, ticket_proposals, TicketProposal, Filing, TICKET_MARKER_PREFIX, TICKET_LABEL_PREFIX, effect_id, JIRA_ISSUE_FILED, JiraError::Ambiguous, JiraError::Malformed, JiraError::NotSent, EffectError::DuplicateState, EffectError::Unresolved, EffectOutcome::Unknown, PAGE_WALK_BOUND, RULE_KEYS, VariantCount, two_marker_matches_refuse_the_write_and_create_nothing, a_marker_matching_across_more_than_one_search_page_is_still_ambiguous, the_marker_is_written_in_the_create_and_never_in_a_second_edit, an_issue_that_already_carries_the_marker_is_answered_by_a_read_and_no_write, an_interrupted_create_and_a_fresh_process_after_the_lag_leave_exactly_one_issue, a_fresh_process_inside_the_lag_window_reads_the_claim_and_files_no_second_issue, a_claim_with_no_key_inside_the_lag_window_is_unresolved_and_never_a_second_create, the_search_then_create_protocol_files_a_second_issue_where_the_claim_ledger_files_one, a_search_answers_an_id_and_no_key_unless_the_caller_asks_for_the_key_field, an_issue_property_is_readable_the_moment_it_is_written_and_the_search_never_sees_it, a_jql_naming_an_issue_property_answers_no_issue_rather_than_every_issue, a_create_that_names_no_issue_type_is_refused_and_stores_nothing, the_search_asks_for_the_key_field_because_the_site_answers_an_id_alone, the_create_names_the_issue_type_the_project_requires, the_claim_is_written_before_the_create_and_carries_the_key_after_it, a_create_the_site_refuses_releases_the_claim_so_the_next_run_is_not_wedged, a_ledger_issue_the_site_does_not_hold_is_named_and_no_create_is_sent, JiraError::Claimed, Filing, a_count_taken_from_one_search_page_is_a_floor_and_never_a_total, crates/fiddle-runtime/src/jira/file_verdict.rs, crates/fiddle-runtime/src/cve/verdict.rs, crates/fiddle-runtime/tests/support/stub_jira.rs, crates/fiddle-runtime/tests/jira_effects.rs, a_ticket_file_verdict_filed_is_found_by_a_later_inspect_against_the_real_site, crates/fiddle-runtime/tests/live_jira_filing.rs, scripts/live-jira-write.sh, scripts/live-jira-file-verdict.sh, scripts/live-jira-search-shape.sh, scripts/gate.sh, docs/technical/RUNBOOKS.md

## Context

Jira offers no client-supplied idempotency key on issue creation. A search and a
create are two requests, so they are not one atomic step. JQL indexing lags a
create by an amount the site does not publish.

ADR 032 fixes the rule that answers the general case. An unknown answer is
resolved by reading the world, never by repeating the write. A create whose
answer was lost needs something in the world to read.

## Decision

**`FileVerdict` writes a marker label on the issue it creates, and reads the
marker back.**

`ticket_marker` derives the marker from `effect_id` over the project, the
invocation reference, `JIRA_ISSUE_FILED` and the project key with the advisory.
`TICKET_MARKER_PREFIX` spells it. The marker is therefore a function of the
identity and not of the run.

- `inspect` searches `project = KEY AND labels = MARKER`. Zero matches answer
  `None`. One match answers `Some`. **Two or more matches answer
  `JiraError::Ambiguous`, never a second create.** `Ambiguous` surfaces as
  `EffectError::DuplicateState`, so a person reads a duplicate as a duplicate.
- `apply` writes the marker in the create body, not in a second edit. A create
  that answers and a create whose answer is lost are then the same state in the
  world, and the next `inspect` tells them apart.
  `the_marker_is_written_in_the_create_and_never_in_a_second_edit` pins it.
- The search walks `nextPageToken` to the end. A count taken from one page is a
  floor and never a total, so a match on page two is still a match.
  `PAGE_WALK_BOUND` bounds the walk and refuses rather than answering from a
  part.

## What the marker does not promise

Jira offers no client-supplied idempotency key. The marker makes an interrupted
run safe, because the next `inspect` finds what the last `apply` wrote. It does
not make two concurrent invocations safe: both can search, both can see nothing,
and both can create. This milestone claims exactly-once across an interruption
and does not claim it across concurrent invocations.

The interruption claim once carried a second bound. It held across an interruption
**longer than the indexing lag** and failed inside it. The claim ledger closed that
window. A run arriving inside the lag window reads the claim on the ledger issue,
which is a direct read the index never touches, and sends no second create.
`a_fresh_process_inside_the_lag_window_reads_the_claim_and_files_no_second_issue`
states the bound this build ships and shows the outcome: one create and one filed
issue, where the search-then-create design filed two. That is measured against the
loopback stub. Against Atlassian on 2026-08-29 two executions over one invocation
reference filed one issue; that two executions in one process stand for two
processes is argued and not measured.

One window stays inside the interruption case, and it is a refusal rather than a
duplicate. A process that dies between the claim and the create leaves a claim
naming no issue. The next run files nothing and names the ledger issue and the
claim for a person.
`a_claim_with_no_key_inside_the_lag_window_is_unresolved_and_never_a_second_create`
holds it. "The claim ledger, which is what exactly-once now rests on" states the
mechanism and its scope.

## What one live run measured

`scripts/live-jira-write.sh` ran twice against `snplow.atlassian.net` project
`ISP` on 2026-08-28, with an operator's read and write token. It was the first
time any part of M5b reached a real Jira write path. Bean `fiddle-jh1z` carries
the captured output.

**1. The search answers no `key`. Measured.** `GET /rest/api/3/search/jql`
returns each issue carrying `id` and no `key`. Adding `fields=key` returns the
keys. The lane read `ISP-273` and `ISP-272` only after it asked for the field.

**2. `FileVerdict` cannot recognise a ticket it filed. Argued from that
measurement.** `FileVerdict` builds its search path with no `fields` parameter,
and `filed` reads `issue["key"]` on every result. The path is unconditional, so
the first result answers `JiraError::Malformed`. No live run has driven
`FileVerdict` itself, so this is an argument from a measured premise and one
line of code, and not a measurement.

**3. Two runs filed two tickets. Measured.** Run one created `ISP-272`. Run two
searched for the marker run one had just written, matched nothing, and created
`ISP-273`. This is the lag window, observed for the first time. The lane sends
the search-then-create shape that `FileVerdict` sends; it is not `FileVerdict`.

**4. The indexing lag is unmeasured.** The lane reported `0 seconds` and, in the
same run, reported one issue carrying the marker where two existed. A later
search returned two. The two numbers cannot both be true, so the lane's lag
computation is unsound and must not be quoted. The bound on the interruption
claim is therefore a duration no number in this repository describes.

> Amended 2026-08-29 by `fiddle-2690`. That last sentence held until the filing
> lane ran. It bracketed one run's lag at more than 634 ms and at most 1940 ms;
> see "FileVerdict has now reached a real site" below. One run of one project on
> one day brackets one lag and characterises none, so the interruption bound is
> still not a number that predicts the next run.

**5. The create omits a required field. Measured, with an argued
consequence.** `scripts/live-jira-search-shape.sh` read `createmeta` for `ISP`
and issue type `Task`. The required fields are `issuetype`, `project` and
`summary`. `FileVerdict::body` sends `project`, `summary`, `labels` and
`description`. So a create this build sends would be refused. `fiddle-zlc4`
carries the missing `[jira]` configuration the fix needs.

**The grade of the milestone's central claim.** `jira.issue_filed` files exactly
one ticket across an interruption: **measured against the loopback stub,
refuted for a real site.** Two of the three refutations are independent of each
other. The mechanism above is not withdrawn, because nothing measured says the
marker is the wrong mechanism. What is withdrawn is the claim that this build
performs it.

Every hermetic lane in this milestone passed on the tree that carries these
defects. That is the correct result and not a fault in the lanes. ADR 077
already states what the suite proves: that this adapter reads what the stub
serves. A green suite was never evidence about Atlassian, and this milestone is
where that sentence acquired a price.

## Why every hermetic test passed

`crates/fiddle-runtime/tests/support/stub_jira.rs` returns `id` and `key` on
every search result. This milestone wrote the stub, and the stub was the only
definition in this repository of Jira's search shape. The code reads `key`, the
stub serves `key`, and the two agree. A check that compares two things one
milestone wrote proves that they agree and nothing else.

The same shape produced three further defects inside M5b, and each was found by
the milestone rather than by a user.

- `RULE_KEYS` was a hand-written list of six against a registry of ten. A policy
  rule for any Jira effect was covered by no case. It now derives from the
  registry in order (`fiddle-njxx`).
- `JiraError::cases()` was a hand-written list. A variant added to all six
  exhaustive matches and omitted from the list passed silently at 67 tests. The
  `VariantCount` derive now measures the list against the enum
  (`fiddle-wglg`). The sweep that followed found 22 sibling
  enum-exhaustiveness tests, 18 with the same hole and two already false
  (`fiddle-0lcc`).
- `JiraError::Malformed` classified `Unknown` on `Apply`. A refusal that sent no
  request was reported as `EffectError::Unresolved`, which says the write was
  not observed and its answer was lost. `JiraError::NotSent` now classifies
  `NotCommitted` in both phases. Two lanes reached this conclusion from
  different call sites (`fiddle-wglg`).

The lesson is one lesson. A guard that compares this build against itself
measures agreement, not correctness. The live lane is the only check in this
milestone that compared the build against something it did not write, and it is
the only check that failed.

## How the live lane is run, and how it cleans up

Two operator rulings, taken on 2026-08-28 after the run left residue.

**Cleanup is a close, never a delete.** The lane transitions its ticket to
`Won't Do`, or at worst to `Done`. `Won't Do` says what is true, because the
ticket was never real work. Deletion is not a fallback and is not attempted:
`ISP` refuses a delete by project policy, not by a missing permission, and the
site answered `HTTP 403` to both. `ISP-272` and `ISP-273` were closed by hand,
each through the single transition `id=51`, and each verified by a second read.

The closing transition is resolved and never assumed. ADR 077 measured that
`Won't Do` and `Done` share the category `done`, and that `Blocked` and
`In Progress` share `indeterminate`. So a category match cannot find the closing
transition, and a name must be configured. The lane must also refuse before it
writes when it cannot close, because discovering it after the create is what
left two issues behind.

`scripts/live-jira-write.sh` still deletes. Its failure text still advises a
reader to delete by hand, which the operator cannot do in `ISP`. `fiddle-jh1z`
carries the correction.

**The lane is a human gate.** A person runs it and reads the result. It never
runs in `scripts/gate.sh`, and no gate step calls it. This matches
`scripts/live-jira-observe.sh`, which records evidence and does not gate. The
lane refusing on an absent variable is therefore correct behaviour and not an
obstacle.

## Consequences

**Residue poisons the next run.** Two issues carrying one marker are the
`Ambiguous` case. A run that leaves residue makes the next run refuse. The
marker search must exclude closed issues, or every unrun cleanup costs the next
run.

**The host workflow keeps its Jira step.** `docs/technical/host-workflow-m4b.patch`
records why the step is not retired.

> Amended 2026-08-29 by `fiddle-2690`. That sentence was written while
> `ticket_proposals` reached no run path. The patch now records the verdict
> mapping step as removed, and the Jira step as keeping only the work that does
> not read `non-patchable.json`, which is the pull request linking. The
> retirement rests on `FileVerdict` measured against Atlassian, on a run path
> measured against the loopback stub, and on an argued equivalence between the
> jq program and `ticket_proposals`. No run of a `fiddle` binary has written to
> a real site.

**Follow-up.** `fiddle-jh1z` is critical and carries the four defects above.
`fiddle-0lcc` is high. `fiddle-zlc4`, `fiddle-4bul`, `fiddle-y4zt` and
`fiddle-nry2` carry the rest.

> Amended by 080. `fiddle-zlc4` is closed: filing reaches the mitigate run path.

## The claim ledger, which is what exactly-once now rests on — 2026-08-29

Probed against `snplow.atlassian.net` with the operator's token on 2026-08-28.

    PUT    /rest/api/3/issue/ISP-272/properties/fiddle-dedup-probe   201
    GET    same, immediately, no delay                               200
           {"key":"fiddle-dedup-probe","value":{...}}
    JQL    issue.property[fiddle-dedup-probe].marker = "probe"       0 issues
    DELETE same                                                      204

**An issue property is immediately consistent on a direct read and invisible to
JQL.** The lag lives in the index; a property read does not touch it. Project
properties were tried first and need admin: `PUT` answered `403`.

### The decision

`FileVerdict` carries a **ledger issue**, and the claim for a marker is a
property on it.

1. `GET /rest/api/3/issue/{ledger}/properties/{marker}`. Direct, immediate.
2. A claim carrying `filed` is the ticket. Create nothing.
3. A claim carrying none is an unknown outcome. Resolve it by searching for the
   marker: one match answers it, none answers `JiraError::Claimed`, which the
   executor reports as unresolved. Never a second create.
4. No claim: write the claim, create, then give the claim the key.

The claim is written **before** the create, or a process that dies between them
orphans an issue no later run can recognise. A create the site definitely
refused releases the claim, so a create that never happened does not wedge every
later run.

### Where the ledger issue comes from: configuration, and it is never created

It is named by the deployment and must exist before a run. `Filing` carries it
and `TicketProposal` passes it to `FileVerdict`.

Creating it on demand was rejected. Creating the anchor is itself a create that
needs exactly-once, and finding it again after an interruption needs a search —
the lagging read the ledger exists to escape. Recording it outside Jira moves
the problem to a second store. A configured issue has neither problem, and it is
never a ticket the lane closes.

A ledger issue the site does not hold is named and refused **before** the create.
`a_ledger_issue_the_site_does_not_hold_is_named_and_no_create_is_sent` pins it.

### On the anchor or in the create call: both, and they do different work

`POST /rest/api/3/issue` accepts a `properties` array, so the marker can be
atomic with the create. That cannot replace the ledger, because it cannot be read
before the issue exists, which is exactly when the decision is made.

So the claim on the ledger is the mechanism, and the create also stamps the same
claim on the issue it makes. The second one lets a candidate a later search
offers be confirmed by a direct read rather than trusted from a lagging index.

### What this buys, stated exactly

It closes the **interruption** window and nothing wider. A run arriving after an
interrupted one reads a claim that was written before the create and files
nothing.

Two **concurrent** processes can still both read absent and both claim. There is
no compare-and-set on a Jira issue property. The scope in "What the marker does
not promise" is unchanged: exactly-once across an interruption, not across
concurrent invocations.

One window remains inside the interruption case. A process that dies between the
claim and the create leaves a claim naming no issue and nothing to find. That is
honestly unknown: a search answering nothing cannot tell "never created" from
"created and not yet indexed", and the lag is unmeasured, so no wait resolves it.
The effect refuses and names the ledger issue and the claim for a person. The old
design's answer to the same interruption was a duplicate ticket. This is a
refusal instead, and a refusal stops the work where a duplicate does not.

### The stub can now tell the two designs apart

`crates/fiddle-runtime/tests/support/stub_jira.rs` answers `id` alone from a
search unless `fields=key` is asked, models issue properties as immediately
consistent on a direct read and absent from JQL, and refuses a create carrying no
`fields.issuetype`. Its lost-answer control names the route it loses, so the
claim written before a create is not lost with it.

`the_search_then_create_protocol_files_a_second_issue_where_the_claim_ledger_files_one`
drives both protocols against one stub inside one lag window and counts two
creates against one. Before these three changes the stub answered the same for
both.

### What is fixed and what is not

Fixed, and pinned by hermetic tests: the search asks `fields=key` on every page;
the create names `fields.issuetype`; a run inside the lag window reads a claim
instead of a stale index.

Not fixed: the indexing lag is still unmeasured. `scripts/live-jira-write.sh` now
refuses to print a lag unless the search it read agrees with the number of creates
it made, so a future run measures it or says it did not.

Not verified at the time of writing: none of this had been driven against a real
site. That changed on 2026-08-29; see "FileVerdict has now reached a real site"
below.

> Amended by 080. The run path and the `[jira.filing]` table now exist: a
> mitigate run calls `ticket_proposals` and executes each `FileVerdict` through
> the executor. The first sentence stands. `crates/fiddle-runtime/tests/cve_filing.rs`
> measures the claim ledger against the loopback stub and nothing against a real
> site, so this record's grading of the milestone's central claim is unchanged.

### The sweep the bean asked for, with its denominator

`routed` in `crates/fiddle-runtime/tests/support/stub_jira.rs` serves **eleven**
method-and-route pairs. The denominator is a count of the branches in that
function and not a hand-written list.

| Route | Compared with `snplow.atlassian.net` |
| --- | --- |
| `GET /rest/api/3/myself` | yes, 2026-08-27: 200 and 401 tell a bad credential from an absent issue |
| `GET /rest/api/3/search/jql` | yes, 2026-08-28: `isLast`, `issues`, `id` alone, `fields=key` |
| `POST /rest/api/3/issue` | yes, 2026-08-29: 201 with a `key`, `createmeta` for the required fields, and a `properties` array accepted on the create and readable on the new issue |
| `GET /rest/api/3/issue/{key}` | yes, 2026-08-26: ISP-267, including the `updated` offset with no colon |
| `PUT /rest/api/3/issue/{key}` | **no** |
| `POST /rest/api/3/issue/{key}/comment` | **no** |
| `GET /rest/api/3/issue/{key}/transitions` | yes, 2026-08-28: ids, `to.name`, `to.statusCategory` |
| `POST /rest/api/3/issue/{key}/transitions` | yes, 2026-08-28: 204 on ISP-272 and ISP-273 |
| `GET /rest/api/3/issue/{key}/properties/{name}` | yes, 2026-08-28: 200 with `{key, value}` |
| `PUT /rest/api/3/issue/{key}/properties/{name}` | yes, 2026-08-29: 201 on a new property and 200 on a replacement, which is the pair the stub answers |
| `DELETE /rest/api/3/issue/{key}/properties/{name}` | yes, 2026-08-28: 204 |

**Nine of eleven compared, none in part, two never. 9 + 0 + 2 = 11**, corrected on 2026-08-29 when the live filing lane drove a create carrying a `properties` array and a replacement property write. The issue edit and
the add-comment routes carry `jira.comment_added` and the work-item port, and no
live run has touched either. One divergence is deliberate and recorded in
`a_start_at_offset_is_refused_because_this_endpoint_pages_by_token`: the site
answers 200 to `startAt` and ignores it, and the stub refuses.

## FileVerdict has now reached a real site — MEASURED 2026-08-29

`crates/fiddle-runtime/tests/live_jira_filing.rs` is an `#[ignore]`d case run by
`scripts/live-jira-file-verdict.sh`. It drives this build and writes no request
of its own in the filing path: `ticket_proposals` builds the proposal,
`TicketProposal::operation` builds the `FileVerdict`, and the same `Executor`
`CveMitigate::file` uses executes it. It ran against `snplow.atlassian.net`,
project `ISP`, ledger `ISP-272`.

MEASURED, about `FileVerdict`, the executor and Atlassian together:

- Run one filed `ISP-276`. Run two, over the same invocation reference and sent
  immediately, answered `ISP-276` from the executor's `inspect` and created
  nothing. **One create over two runs**, where the search-then-create protocol
  filed two on 2026-08-28.
- The claim was then removed from the ledger and `FileVerdict::inspect` was
  called again. With nothing to read on the ledger it fell through to the
  search, asked `fields=key`, and read `ISP-276` off the site. The `fields=key`
  repair is measured against Atlassian and no longer against the stub alone.
- The create carried `fields.issuetype` and a `properties` array, and the site
  answered 201.
- The indexing lag is **more than 634 ms and at most 1940 ms**: the search at
  634 ms disagreed with the number of creates this run had made and the search
  at 1940 ms agreed. A first search that already agrees gives an upper bound
  only, and the lane now says which of the two it holds rather than printing the
  resolution of its own polling as a lag.
- `ISP-272` reads as `Won't Do` and answered a property write, an immediate read
  and a delete. A closed issue holds properties, so a ticket an earlier run
  filed and closed is usable as a ledger.

NOT MEASURED, and the run says nothing about these:

- The deployment route. `filing_client` reading `[jira.filing]` and
  `CveMitigate::file_tickets` are not driven by this lane, which builds a
  `TicketFiling` directly. The mapping from `fiddle.toml` to `TicketFiling` is
  measured against the loopback stub only.
- A `fiddle` binary writing to a real site. The only run path to `FileVerdict`
  through the binary is a full CVE sweep, which needs a scanner, an agent and a
  GitHub repository and which opens a pull request.
- Concurrent duplicate invocations, and a page boundary shifting under a walk.
  One process cannot arrange either.
- A genuinely fresh process. The two runs are two executions in one process,
  each rebuilding the proposal from the configuration. `effect_id` digests four
  strings the process holds none of, so the marker cannot depend on process
  state, but that is argued from the derivation and measured only hermetically.

Cleanup, to the operator's ruling of 2026-08-28: `ISP-275` and `ISP-276` were
closed as `Won't Do` through transition 51, each verified by a second read, and
the claims were released. A read this lane did not make afterwards confirms both
are `Won't Do`, the ledger holds no claim, and a JQL for either marker with
`statusCategory != Done` returns 0 issues.
