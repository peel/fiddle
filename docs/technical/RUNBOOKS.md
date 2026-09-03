# Runbooks

Fiddle is a binary and a skills library, not a service. Nothing is deployed. What
needs operating is the credentials and the two lanes that use them. Everything else
is credential-free.

## Credentials

`fiddle.toml` holds the *name* of an environment variable. A document holding a
literal secret does not load.

| variable | used by | note |
| --- | --- | --- |
| `LITELLM_API_KEY` | the real-model tiers | opt-in, costs money, never gates |
| `FIDDLE_GITHUB_TOKEN` | the local live forge lane | opt-in, writes to a disposable repository |
| `FIDDLE_EFFECTS_TOKEN` | the dispatched forge lane | the same value, as a repository secret |
| `FIDDLE_CVE_TOKEN` | the host's CVE sweep | the host repository's secret; it needs `Checks: read`, which the two above do not carry |
| `WIZ_CLIENT_ID` | the host's `setup-wiz` step | the host repository's secret; the Wiz service account the scan runs as |
| `WIZ_CLIENT_SECRET` | the host's `setup-wiz` step | the host repository's secret; that account's secret |
| `JIRA_USER_EMAIL` | every live Jira lane, and the toil route | opt-in; the account the read or the write is performed as |
| `JIRA_API_TOKEN` | every live Jira lane, and the toil route | the same account's classic unscoped API token |
| `JIRA_SITE` | every live Jira lane | the site origin **with its scheme**, as in `https://snplow.atlassian.net`. Every live lane refuses a value that is not an `https://` origin, so a bare `snplow.atlassian.net` fails before it reaches the site |
| `JIRA_ISSUE` | the live Jira read lane | the one issue it reads. Both write lanes refuse while it names `JIRA_WRITE_PROJECT`, so unset it for those |
| `JIRA_SEARCH_PROJECT` | the live Jira search shape lane | read only; the project it searches and reads workflow statuses from |
| `JIRA_WRITE_PROJECT` | the two live Jira write lanes | it CREATES an issue in this project and then CLOSES it, and never deletes |
| `JIRA_LEDGER_ISSUE` | the two live Jira write lanes | an anchor issue in that project, of the same issue type, whose workflow the lane resolves the closing transition off |

The two `WIZ_` variables are the only ones `fiddle.toml` never names. `setup-wiz`
runs `wizcli auth --id --secret` with them, and fiddle passes the scanner no
credential of its own (ADR 042). A caller who skips that step gets the scanner's
own exit: `the scanner produced no report (exit 1)`, and exit 11.

`.env.example` is the template and names every variable above that a local lane
reads. Six of them are what the lanes in this file need to run at all:
`LITELLM_API_KEY`, `FIDDLE_GITHUB_TOKEN`, `JIRA_USER_EMAIL`, `JIRA_API_TOKEN`,
`JIRA_SITE` and `JIRA_ISSUE`. The three write-lane variables are named too, and
this deployment sets none of them.

A local lane reads its variable from `.env` in the worktree you run from.
`.envrc` is tracked, so `dotenv_if_exists` resolves the `.env` beside it. The
tracked `.gitignore` carries `.env` and `.env.*` with a `!.env.example`
negation, so the claim that a token in `.env` cannot be committed rests on a
tracked file and not on one clone's `.git/info/exclude`. Check it rather than
trust it:

```sh
git check-ignore -v .env .env.example
```

It must name `.gitignore` for `.env` and answer nothing for `.env.example`. A
rule that resolves to `.git/info/exclude` holds on your clone and on no other.

A repository secret is set with `gh secret set` and never appears in `.env`. The
note above says which repository holds each one.

### Mint the GitHub token

Use a **fine-grained** token scoped to one repository. A classic `repo` token
reaches every repository you can push to, including one whose whole argument is
that reading it needs no credential.

github.com/settings/personal-access-tokens/new

```
Resource owner        peel
Repository access     Only select repositories -> peel/fiddle-effects-acceptance
Expiration            90 days
```

Grant these five repository permissions and no others.

```
Contents          Read and write   # push the branch, read refs
Pull requests     Read and write   # look up by head/base, POST /pulls
Actions           Read and write   # POST .../dispatches, GET .../runs
Workflows         Read and write   # write files under .github/workflows/**
Metadata          Read-only        # mandatory, auto-selected
```

`Actions` dispatches a workflow and `Workflows` writes a workflow file. They are
different grants and both are needed. `Contents: write` alone returns 403 for any
path under `.github/workflows/**`.

Store it without letting it reach a shell history or a command line.

```sh
# local: paste after the = in the worktree's .env
FIDDLE_GITHUB_TOKEN=github_pat_...

# CI: reads stdin, so the value never enters argv
gh secret set FIDDLE_EFFECTS_TOKEN --repo peel/fiddle
```

### Verify the scope

A successful read proves nothing, because `peel/fiddle` is public. Use a
permission-gated endpoint and expect `200`, `403`, `403`.

```sh
set -a; . ./.env; set +a
for r in peel/fiddle-effects-acceptance peel/fiddle-acceptance peel/fiddle; do
  printf '%-34s ' "$r"
  GH_TOKEN="$FIDDLE_GITHUB_TOKEN" gh api "repos/$r/collaborators" --jq 'length' 2>&1 | tail -1
done
```

Anything else means the selection is wider than one repository. Do not read
`.permissions` off the repository payload instead: it reports *your* rights as
owner and says `admin=true` for a repository the token cannot write.

### Rotate

```sh
gh auth logout --hostname github.com && gh auth login    # the gh CLI's own token
```

For the fine-grained token: revoke it at
github.com/settings/personal-access-tokens, mint a new one, then update `.env`
**and** the `FIDDLE_EFFECTS_TOKEN` secret together. Permissions are editable
without re-minting, so adding a missing grant does not invalidate the secret.

## Run the lanes

```sh
# the gate: offline, credential-free, this is what must be green
scripts/gate.sh --full

# the local live lane
nix develop -c cargo build --release
FIDDLE_BIN="$PWD/target/release/fiddle" scripts/live-github.sh

# the same walk from a runner; --ref is required, see Common issues
gh workflow run github-effects.yml --repo peel/fiddle \
  --ref ci/github-effects-dispatch-proof

# the live Jira read lane; one issue, read only
JIRA_SITE=https://snplow.atlassian.net JIRA_ISSUE=ISP-267 \
  FIDDLE_BIN="$PWD/target/release/fiddle" scripts/live-jira-observe.sh

# the live Jira search shape lane; searches and workflow statuses, read only
JIRA_SITE=https://snplow.atlassian.net JIRA_SEARCH_PROJECT=ISP \
  scripts/live-jira-search-shape.sh

# the live Jira write lane; it CREATES an issue and then CLOSES it, never deletes
JIRA_SITE=https://snplow.atlassian.net JIRA_WRITE_PROJECT=ISP \
  JIRA_LEDGER_ISSUE=ISP-<anchor> scripts/live-jira-write.sh

# the live Jira filing lane; the same write, driven through FileVerdict itself.
# JIRA_ISSUE belongs to the read lane and both write lanes refuse while it names
# JIRA_WRITE_PROJECT, so unset it for the invocation.
env -u JIRA_ISSUE JIRA_SITE=https://snplow.atlassian.net JIRA_WRITE_PROJECT=ISP \
  JIRA_LEDGER_ISSUE=ISP-<anchor> scripts/live-jira-file-verdict.sh
```

Read the gate's coverage off its `TOTALS` line. It says `N of M binaries`, and
`N < M` means the run stopped short, so the counts beside it are a floor.

Neither forge lane gates. Both write to `peel/fiddle-effects-acceptance`, which
exists to be dirtied and holds no secret. `docs/technical/effects-repository.md`
describes it.

### The live CVE steering lane

```sh
FIDDLE_GITHUB_TOKEN=<token> FIDDLE_CVE_TAG=v0.44.0 scripts/live-cve-steering.sh
```

It dispatches `cve-remediation.yml` on `peel/fiddle-test`, waits for the run to
open a pull request, requests changes on it, and asserts the next attempt's diff
carries what the review asked for. It closes the pull request and deletes the
branch afterwards, so the board is clean for the next run.

**Bump the pin before you run it.** The lane refuses unless
`peel/fiddle-test` `main` pins the tag you gave it, in
`.github/workflows/cve-remediation.yml`:

```yaml
          FIDDLE_TAG: v0.44.0
          FIDDLE_SHA256: 40bbef0a85b58fa2564f5cdc2bf674e0869b71f86f2e8c9191c9e92f138a4d30
```

`FIDDLE_TAG` and `FIDDLE_SHA256` move together, because the job refuses when the
measured checksum and the pinned one disagree. Read the sha off the release's
published `fiddle-linux-amd64.sha256` asset.

Measured 2026-08-26: the pin had drifted to `v0.42.0`, which is **older than the
release M4d measured**. Nothing failed and nothing warned. A dispatch would have
produced a green run against the wrong binary, and no comparison with the previous
milestone would have been possible. Check the pin first, every time.

**`nothing_to_do` is a failure.** It reports `outcome completed` and exits 0, and
it is the failure that most resembles success: a lane that passed by finding
nothing cannot be told from one that passed because nothing was wrong. Its usual
cause is an open pull request whose tip already carries the fix being reused, so
confirm the testbed has none before running.

**The token needs `actions=write`.** A fine-grained token with Contents and Pull
requests is not enough; the dispatch endpoint returns 403 without it. When a
GitHub 403 is puzzling, read the `x-accepted-github-permissions` header on the
failing response — it names the exact permission the endpoint wants, and
`repos/.../actions/permissions` is not a proxy for it because that endpoint
requires `administration` instead.

**The credential this lane has been proved with is not the one the epic
specifies.** The standing result — runs 33010960178 and 33011135515 on
2026-08-26 — was produced with the operator's `gh` keyring token, whose scopes
are `repo` and `workflow` and which reaches every repository that account can
push to. `FIDDLE_GITHUB_TOKEN`, the fine-grained token the section above tells
you to mint, has never been shown to work here: it still answers the dispatch
endpoint with 403 and `x-accepted-github-permissions: actions=write`. The forge
behaviour is the same either way, because both credentials issue the same API
calls. What is unmeasured is whether a credential scoped to one repository
suffices for this lane. An evaluator scored that gap 5 against a threshold of 7;
the operator waived the dimension on 2026-08-27 rather than regenerate the
token, so the lane's result stands and the specified credential stays unproven.

The lane covers the effect path and steering through review. It covers nothing
else, and in particular it is not evidence about Jira, the observation ports, or
the credential boundary.


### The live Jira read lane

**The site is `snplow.atlassian.net`.** `snowplow.atlassian.net` is a different
organisation's tenant and answers `serverTitle: Aspen Digital`. A run against it
spends its rounds on authentication that could never succeed.

`scripts/live-jira-observe.sh` reads one issue three ways and compares them. It
calls `/rest/api/3/issue/KEY?fields=status,updated` itself, calls the same route
again with the field list the port sends, then runs
`fiddle inspect jira:KEY --json` against a generated document that names
`JIRA_USER_EMAIL` and `JIRA_API_TOKEN` and carries neither value. It refuses when
any of `JIRA_SITE`, `JIRA_ISSUE`, `JIRA_USER_EMAIL`, `JIRA_API_TOKEN` or
`FIDDLE_BIN` is absent, because a lane that skipped for want of a credential
would read exactly like one that passed. It also refuses a `JIRA_SITE` that is
not an `https://` origin. Set the scheme. A bare `snplow.atlassian.net` reaches
the site over http, and the site answers **301** to every request, which reads
like a refusal and measures nothing.

The lane holds no copy of the field list. `fields_the_port_asks_for` reads
`const FIELDS` out of `crates/fiddle-runtime/src/jira/work_item.rs` and refuses
when that list does not carry every field the lane grades. That drift is what
this lane was corrected for: the port widened its read, the lane
kept recording three, and a green run said nothing about the two it had stopped
grading.

What it proves, and why each assertion is there:

- `fields.updated` is present. It is the only revision Jira Cloud offers, and the
  whole target identity rests on it (ADR 077).
- the root carries no `version` key, so there is no counter to prefer over
  `fields.updated`. A site that grew one would print a note here.
- `fiddle` reported a status, a projected state and a revision. The revision is
  the load-bearing one: Jira Cloud sends `fields.updated` with a **colonless**
  offset, which is not RFC 3339, so `read_instant` in
  `crates/fiddle-runtime/src/jira/work_item.rs` tries two further format
  descriptions after `Rfc3339`. If the site's shape ever moves outside all three,
  `canonical_revision` returns `None`, the port reports `Unavailable`, and this
  lane is the only thing that would say so — the hermetic suite reads a stub that
  emits the shape it was written to emit.
- the site answers the port's own field list, and `fields.labels`,
  `fields.description` and `fields.comment` arrive in shapes `labels_in`,
  `description_in` and `comments_in` read. Each recorder names the field whose
  shape it refuses, so an answer the port cannot parse stops the lane rather than
  passing through it.
- what `fiddle` parsed agrees with what the direct read carried, field by field.
  A lane that recorded a shape and never compared it with the port's own reading
  would pass while the two disagreed.
- a work item the port reports `Unavailable` fails the lane and carries the
  port's reason verbatim. That reason is the only line that names the field, so
  the lane prints it rather than reporting "no status".

Measured on 2026-09-01 against `snplow.atlassian.net`:

| what came back | ISP-239 | ISP-267 |
| --- | --- | --- |
| `fields.labels` | a list of 2, every entry text | a list of 2, every entry text |
| `fields.description` | a document, type `doc`, version `1`, 3729 bytes | a document, type `doc`, version `1`, 4376 bytes |
| `fields.comment` | 2 of 2 comments, page size 2 | 1 of 1 comments, page size 1 |
| what `fiddle` read off it | 2 labels, 2638 characters of description, 2 comments | 2 labels, 1401 characters of description, 1 comment |

Both comment containers carried `comments`, `total`, `maxResults` and `startAt`,
every comment carried `id` and `author.accountId`, and every body was a document
`written` flattened to something rather than to nothing. `maxResults` equalled
`total` in both reads, so the page an issue read returns is still unmeasured.
`replies_in` refuses a `total` above the number of comments carried, so an issue
whose conversation exceeds one page would report `Unavailable`. No such issue has
been read.

The lane records evidence and does not gate. `scripts/gate.sh` references no
`live-*.sh` script. It reads an issue and writes nothing, so it needs no
disposable target; do not point it at an issue whose `status` you mind being
printed to your terminal.

The lane prints the issue its narrow `curl` read, and that call requests
`fields=status,updated` and no other field, so no ticket prose can reach the
terminal through it. The wide `curl` and `fiddle inspect` both request
`status,updated,labels,description,comment,issuetype,summary`, because
eligibility weighs the label, the issue type, the summary, the description and the
conversation (`crates/fiddle-runtime/src/jira/work_item.rs`). The lane grades the
first five of those and reads the list off `const FIELDS`, so a list that widens
again does not stale this lane. The lane writes both of those
answers to a temporary file and prints types, counts, byte sizes and character
counts off them. No label's text, no description's prose and no comment's body
reaches a line the lane prints.

On the path that passes, none of either wide answer is printed. The lane
reports the three values it read and the three shapes it graded, and
`trap 'rm -rf "$TMP"' EXIT` removes the temporary directory when the run ends.

Two paths do print an answer. When the site's wide answer does not parse as
JSON, or when `fiddle`'s own answer does not, the lane fails and writes that whole
answer to stderr, because a reader who cannot see what arrived cannot say why it
did not parse. Those are the malformed-answer diagnostics, and they are the only
paths on which an inspected answer reaches the terminal. A refused shape is not
one of them: it names the field, the shape word and the counts, and prints none
of the value. A non-zero exit from `fiddle inspect` prints the captured stderr
and not the answer. A credential found in any stream is reported without printing
the stream at all.

So ticket prose stays inside the run directory unless an answer arrives that is
not JSON. Nothing the lane prints is committed here.

The refusals are proved without a credential and without reaching a site.
`scripts/test-live-jira-lanes.sh` sources the lane, hands each recorder a fixture
built from one well-formed answer by breaking exactly one field, and requires the
refusal to name that field. It runs the well-formed answer through the same
recorders first, so a recorder that refused everything would fail the suite
rather than pass it. It also runs the whole lane against a port source declaring
`status,updated`, which must refuse for drift before any request, and against the
source this repository ships, which must reach the site and fail there instead.
It ran 52 cases on 2026-09-01. The
port's own refusals were measured the same day against a loopback stub: with a
`fields.labels` document, a `fields.description` list, a `fields.comment` string
and a container carrying 1 of 3 comments, `fiddle inspect` reported the work item
`unavailable` on all four and the lane failed carrying a reason that named the
field.

### The live Jira search shape lane

`scripts/live-jira-search-shape.sh` sends six GET requests and writes nothing. It
settles what `crates/fiddle-runtime/tests/support/stub_jira.rs` assumed about
`/rest/api/3/search/jql`, which until now was the only definition in this
repository of that endpoint's shape. It refuses when `JIRA_SITE`,
`JIRA_SEARCH_PROJECT`, `JIRA_USER_EMAIL` or `JIRA_API_TOKEN` is absent, and it
exits 2 with a reason.

Before it measures anything it plants the credential in a file of its own and
greps for it. A grep that cannot find a planted credential proves nothing by
finding none elsewhere, so the lane refuses when the planted case does not bite.

**Measured 2026-08-28 against `snplow.atlassian.net`, project `ISP`.**

| what | the stub assumed | the site answered |
| --- | --- | --- |
| top level members | `issues`, `isLast`, `nextPageToken` | `isLast, issues, nextPageToken` |
| `isLast` | emitted, never depended on | present, `false` on a page that is not the last |
| `total`, `startAt`, `maxResults` in an answer | absent | absent |
| default page size | 50, graded medium confidence | 50 |
| `nextPageToken` | present until the last page, and it advances | present, and it advances |
| `startAt` in a request | refused with 400 | **200, and silently ignored** |
| `maxResults=500` | capped at the stub's page cap | 265 matches in one page, no further token |

Two of those need reading rather than glancing at.

**`startAt`.** The stub refuses it with 400 and the site answers 200. That is not
a defect in the stub. The site returns the same first key as an unparameterised
page, so it accepts the parameter and does nothing with it: a caller that asked
for page two is handed page one and cannot tell. The stub's 400 is a deliberate
divergence toward strictness and it stands. `all_search_matches` and
`every_issue_carrying_the_marker` page by `nextPageToken` and send no `startAt`,
so nothing in this build depends on either behaviour.

**The page cap.** `a_page_larger_than_the_cap_is_still_capped_and_still_says_there_is_more`
holds of the stub and is not a fact about a real site. `ISP` holds 265 issues and
the site served all 265 to `maxResults=500`. A caller must still follow pages,
because 265 is not a bound the site published — it is how many matched.

**Now a measurement: real `[jira.workflow]` status names.** ADR 077 recorded this
as the one item its live read left open. Every issue type in `ISP` — Task, Story,
Bug, Epic, Spike, Sub-task — offers the same six statuses:

| status | category |
| --- | --- |
| To Do | new |
| In Progress | indeterminate |
| In Review | indeterminate |
| Blocked | indeterminate |
| Done | done |
| Won't Do | done |

`Blocked` is the one that matters. Its category is `indeterminate`, the same as
`In Progress` and `In Review`, so a deployment that wants blocked work to read as
blocked has to name it in `[jira.workflow]`. The category cannot tell them apart.

### The live Jira write lane

`scripts/live-jira-write.sh` **creates an issue on a real site and then closes
it.** It never deletes. Deletion is refused in `ISP` by project policy, so a
cleanup that depends on it leaves residue on every run.

**Six preconditions, all asserted before anything is written.** The four
variables, the https origin and the bare project key, as before. Then:
`JIRA_LEDGER_ISSUE` must exist, must live in `JIRA_WRITE_PROJECT`, and must be
the issue type the lane files, because a Jira workflow is per issue type and a
transition resolved on an issue of another type says nothing about the ticket
the lane creates. The closing transition must resolve to **exactly one id** from
that issue's state; the lane refuses on any other count and prints every
transition it was offered. And the token must be able to write, read back and
remove a property on the ledger issue. Discovering any of these after a create
is what left `ISP-272` and `ISP-273` behind.

**The closing transition is named, never matched by category.** `Won't Do` by
default, `Done` as the declared fallback, `JIRA_CLOSING_TRANSITION` to spell it
otherwise. ADR 077 measured that `Won't Do` and `Done` share the category `done`,
so a category match would pick the wrong transition. The category **is** used to
exclude closed issues from the marker search, which is what the category means
and is a different question.

**It sends the claim-then-create requests `FileVerdict` sends.** It reads
`/rest/api/3/issue/{ledger}/properties/{marker}` first. A claim naming an issue
means the ticket exists and it creates nothing. A claim naming none is an
unknown outcome, and the lane refuses rather than repeating the write. No claim
means it writes one, creates, and gives the claim the key. It runs that twice
with no wait, which is the interruption case exactly-once is scoped to. Its
searches carry `fields=key` and refuse an answer that omits the key.

**The lag it reports agrees with its own observations.** It waits until the
search shows exactly as many issues as the run created **and** shows the key it
filed. Until both hold, the number would be taken from a stale index. If neither
holds within 300 seconds it says the lag is unmeasured rather than printing a
number. The 2026-08-28 run printed `0 seconds` while reporting one issue where
two existed; that number is unsound and must not be quoted.

**What it leaves behind.** Nothing, when it passes. Every issue it wrote or
matched is closed through the resolved transition and each close is verified by
a second read. The claim is removed from the ledger issue. The ledger issue
itself is never closed, and the lane refuses if the close list names it. When a
close does not take, the lane names the keys on stderr and fails.

**It does not drive `fiddle`.** It sends the requests by hand, so it measures the
site rather than the build. `scripts/live-jira-file-verdict.sh` is the lane that
drives the build; see below. `human::publish`, the other route a Jira write could
take, still has no caller outside tests. The credential census in
`crates/fiddle-acceptance/tests/jira_credential.rs` now carries a write scenario:
`a_sweep_that_files` drives `fiddle run cve --capability cve_mitigate` over a
document carrying `[jira.filing]`, and the site it files into is a loopback stub.
Ten of the census's 42 surfaces are that run's, and `reports/filings.json` is one
of them. No acceptance scenario drives the filing path against Atlassian, and
none is meant to: `scripts/live-jira-file-verdict.sh` is the lane that does.
`the_credential_never_reaches_the_filing_report_through_a_quoted_refusal` holds the
one new surface the filing path writes, which is `filings.json` on the disk.

**It is a human gate and the operator runs it.** It does not gate CI. The
rewritten lane ran against `ISP` on 2026-08-29 with ledger `ISP-272`: two runs,
one create, the ticket closed as `Won't Do` and the claim released. `scripts/test-live-jira-lanes.sh`
holds that it refuses rather than skips, that it sends no delete against an
issue, that it resolves its closing transition by name, and that its marker
search excludes closed issues. Those are checks on the text of the lane, not on
Atlassian.

**Run twice against `ISP` on 2026-08-28, before this rewrite.** It filed
`ISP-272` and `ISP-273`, so two runs left two issues rather than one. Both were
closed to `Won't Do` by hand through transition `id=51`. ADR 079 records what the
run measured and what it refutes.

### The live Jira filing lane

`scripts/live-jira-file-verdict.sh` **drives `FileVerdict` itself against a real
site.** It runs `crates/fiddle-runtime/tests/live_jira_filing.rs`, an `#[ignore]`d
case, so `scripts/gate.sh` never files a ticket. Nothing in its filing path is a
request the lane writes: `ticket_proposals` builds the proposal,
`TicketProposal::operation` builds the `FileVerdict`, and the same `Executor`
`CveMitigate::file` uses executes it.

**It takes the same five variables as the write lane and refuses the same way.**
`JIRA_ISSUE` names the read lane's issue and is set in the operator environment;
unset it for the invocation rather than weakening the guard.

**It proves the two inspects separately.** Run one files. Run two, over the same
invocation reference, is answered by the executor's `inspect` reading the claim
on the ledger. The claim is then removed and `FileVerdict::inspect` is called
once more, which falls through to the search, asks `fields=key`, and must read
the same key back. The first inspect measures the ledger; the second measures the
search.

**Its lag carries the bound it actually observed.** A search that disagrees with
the run's own create count is a lower bound; the first search that agrees is an
upper bound. When the first search already agrees the lane says `at most N ms`
and refuses to call it zero, because it observed no search that disagreed.

**What it leaves behind.** Nothing, when it passes. The ticket is closed through
the resolved transition and verified by a second read, the claim is removed, and
the lane refuses if the close list names the ledger. Keys it could not close are
printed and the case fails.

**Run against `ISP` on 2026-08-29** with ledger `ISP-272`. It filed `ISP-275`
and, after a correction to its lag reporting, `ISP-276`. Both are closed as
`Won't Do`. ADR 079 records what the run measured and what it does not reach.

**It does not drive a `fiddle` binary.** It builds a `TicketFiling` itself, so
the mapping from `fiddle.toml` to `TicketFiling` and `CveMitigate::file_tickets`
are still measured against the loopback stub alone. The only run path to
`FileVerdict` through the binary is a full CVE sweep, which needs a scanner, an
agent and a GitHub repository and which opens a pull request.

**Out of reach and claimed by none of these lanes.** Concurrent duplicate
invocations. The design scopes exactly-once to interruptions, and one process
cannot race itself. Whether a page boundary shifts under a walk is also
unmeasured: it needs an issue indexed between two pages of one walk, which
nothing here can arrange.

## Cut a release

The tag is the trigger. `release.yml` builds `linux-amd64` from the pinned
lockfile and publishes the binary with its SHA256 beside it.

```sh
git tag -a v0.4.0 -m v0.4.0 && git push origin v0.4.0
gh run list --repo peel/fiddle --workflow release.yml --limit 1
gh release view v0.4.0 --repo peel/fiddle --json assets --jq '.assets[].name'
```

Verify what a host would download.

```sh
gh release download v0.4.0 --repo peel/fiddle --pattern 'fiddle-linux-amd64*'
sha256sum -c fiddle-linux-amd64.sha256
```

## The CVE sweep, which runs from the host

**This repository triggers no CVE sweep.** The sweep runs from
`.github/workflows/cve-remediation.yml` in
`snowplow-incubator/snowplow-identities`. Searching here for a workflow that
sweeps finds nothing by design. `docs/technical/host-workflow-m4b.patch` carries
the host's side of it. `docs/technical/cve-repository.md` describes the
disposable target and the token grants.

That workflow runs on `cron: "0 3 * * *"`, and it also takes a dispatch.

```sh
gh workflow run cve-remediation.yml \
  --repo snowplow-incubator/snowplow-identities -f ref=<ref>
```

**A dispatch against any ref but the host's default branch scans and does not
remediate.** The `remediate` job's `if` requires `on_default_branch == 'true'`,
and the `ref` input says the same thing in its own description. Point `ref` at a
release tag to report on that tag, and expect no pull request from it.

Establish this state before a dispatch that is meant to remediate.

```sh
host=snowplow-incubator/snowplow-identities
target=peel/fiddle-cve-acceptance

# 1. the secrets, on the host; no value enters argv
gh secret set FIDDLE_CVE_TOKEN --repo "$host" < token.txt
gh secret list --repo "$host"

# 2. the workflow, and that it carries a dispatch
gh workflow list --repo "$host" | grep cve-remediation

# 3. the target, seeded as cve-repository.md describes
gh api "repos/$target" --jq '"\(.visibility) \(.default_branch)"'
gh api "repos/$target/labels/security%2Fcve" --jq .name

# 4. no residue from an earlier run
gh pr list --repo "$target" --state open
```

The patched step runs `fiddle run cve` once. Reading real CI feedback needs two
runs. The first publishes, the target's own `pull_request` workflow answers, and
the second reads the check runs on the candidate commit. `[agent]
max_capability_attempts` must be at least 2, because a bound of 1 stops the
second run before it reads anything.

## The toil route, which works one labelled Jira ticket

`fiddle run jira:KEY` with no `--capability` flag selects `toil`. It is not a
mark. An eligible ticket becomes one branch, one pull request, one link comment
on the ticket and one transition to `In Review`. A ticket the gate refuses
becomes one comment on that ticket and exit 2.

Nothing in this section has been run against a real forge. One command line has
reached the toil gate against the real Jira site, and it refused the ticket
deterministically.

### What a ticket needs

| what | where it comes from | this deployment |
| --- | --- | --- |
| the trigger label | `[jira.labels] toil_trigger`, and an absent key resolves to the default | `fiddle/toil` |
| the issue type | frozen in Rust, `TOIL_WORKED_ISSUE_TYPES` in `crates/fiddle-cli/src/config.rs` | `Task`, and nothing else |
| the shortest description | frozen in Rust, `TOIL_SHORTEST_DESCRIPTION` in the same file | 20 characters after trimming |
| the repository | `[github] repo`, which is both the repository the ticket is taken to name and the only one in bounds | one repository |

The label and the repository are yours to set. The issue type list and the
description floor are not configurable, and a deployment that wants either
changed changes Rust. `fiddle config check` reports the label it resolved and
both scope bounds below; it reports neither frozen value, so read them here or
in that file.

### Answering an open question in a comment

The gate refuses a ticket whose description asks a question rather than stating a
change, with the rule `the ticket asks for a change and not a product decision`.
The answer can go in the description, and it can go in a comment on the ticket.

A comment counts only when its author is named in `[jira.decision] authorized`,
which is the same table that names who may answer a question a `propose_change`
run asks. It has to be that table: an account in `[github.decision] authorized`
authorizes no comment, whatever its digits look like. A deployment that writes no
`[jira.decision]` table reads no comment at all: the gate reads the summary and
the description, as it did before, and the refusal's remedy names the description
alone. Fill the table in and the remedy names both
routes. `fiddle config check` reports the accounts it holds.

Name your own account. fiddle has no tracker account of its own: it comments
with the credential in `[jira] user` and `token`, so the comments it publishes
read back as yours. That is handled — the gate drops any comment carrying a
marker fiddle writes, before it looks at the author — so authorizing yourself
does not hand the review fiddle's own refusals. It does mean the author check
alone is not what protects you, and ADR 083 records why.

One thing to know before you rely on it: the implementer is given the summary and
the description only, so a ticket admitted because a comment decided the question
still hands the agent a description that leaves it open. Writing the decision
into the description as well is the way to avoid that. ADR 083 records that too.

### The document, and where it is read from

`workflows/toil.toml`, resolved **beside the deployment document**, so a
`--config /srv/fiddle.toml` reads `/srv/workflows/toil.toml`. No other location
is read, there is no search path, and a document whose `stage` is not `toil` is
refused. The shipped copy is this repository's `workflows/toil.toml`; ship it
beside `fiddle.toml` and ship `workflows/prompts/` with it.

A malformed or absent document is refused **before** the ticket is qualified,
because the command line loads the document first. The person who filed the
ticket is told nothing in that case, so `config check` and one dry run over a
ticket you own are worth doing before a document reaches a real board.

### The two tables it needs

```toml
[agent]
model = "..."
base_url = "..."
api_key = { env = "LITELLM_API_KEY" }

[jira]
site = "https://snplow.atlassian.net"
project = "ISP"
user = { env = "JIRA_USER_EMAIL" }
token = { env = "JIRA_API_TOKEN" }

[jira.labels]
toil_trigger = "fiddle/toil"
```

`[jira]` is needed to read the ticket at all, so a run without it stops before
the gate. `[agent]` is needed only once the nine deterministic rules have held:
they read the tracker's own fields and reach no model, so a deployment that
configured no model still answers a label-less ticket with a refusal on the
ticket rather than with a configuration error. The credential each table names
is resolved from the environment, and `fiddle.toml` holds the variable's name
and never its value.

### The four rows deployment policy has for it

`fiddle config check` prints one `github.policy.<name>` row per registered
effect. Four of them belong to this route.

| the row `config check` prints | what it governs | denying it |
| --- | --- | --- |
| `github.policy.jira.comment_added` | the refusal comment, and the decision channel's question | a refused ticket is told nothing, and the run says so loudly and still exits 2 |
| `github.policy.jira.pull_request_linked` | the link comment the sixth step writes | the run fails at that step with the pull request already open |
| `github.policy.jira.issue_transitioned` | the move to `In Review`, the last step | the run fails at that step with the pull request open and linked |
| `github.policy.jira.issue_filed` | filing a new issue, which this route never does | nothing on this route |

In the document the key is quoted, because it carries dots.

```toml
[github.policy]
"jira.comment_added" = "deny"
```

An absent row is `allow`. Deny the comment first if you want to watch the gate
refuse without writing to a board.

### The two bounds you can set, and the two you cannot

```toml
[orchestration.toil]
max_files_changed = 10
max_diff_lines = 500
```

Both are measured in the workspace after each agent step, and a change beyond
either stops the run before any effect. An absent `[orchestration.toil]` table
resolves to 10 and 500 rather than to no bound, and `config check` reports the
resolved numbers, which is where you see a value you did not write. The frozen
pair is the issue type list and the description floor in the table above.

### What a refusal looks like on the ticket

The comment is the whole reason, in four lines and the quoted text the rule
read.

```
fiddle did not take `ISP-239` on, and this comment is the whole reason.
The rule that failed: the trigger label is present
What the gate found: ISP-239 carries 2 labels and none is the trigger label
What would change that: add the label `fiddle/toil` to ISP-239
The text on this issue that the rule read: ...
```

The command line states the same refusal and exits 2.

```
fiddle::toil::ineligible

  × `ISP-239` is not work this build takes on. The rule that failed: the trigger label is present. What the gate found: ISP-239 carries 2 labels and none is the trigger label
  help: add the label `fiddle/toil` to ISP-239
```

That sentence is wrapped to the terminal, with `│` continuing each line, so do
not grep it as one string.

A refusal names the one rule that failed. It does not name which of the other
twelve were reached, nor on what evidence any of them stood.

`fiddle inspect jira:KEY` reports the same refusal in `would_refuse`, before you
run anything, for any of the nine deterministic rules. It resolves no model
credential, so it costs nothing and writes nothing.

### When the refusal itself cannot be published

The run reports it loudly rather than swallowing it, and still exits 2.

```
error: the refusal below was not published on `ISP-239`, so the person who
       filed it has been told nothing
  work_item   = ISP-239
  cause       = policy denied jira.comment_added: deployment policy denies this effect kind
```

Deployment policy and a site error both reach this line. Read the `cause`.

### Working one ticket again

A ticket the route completed carries nothing. The completion is a file:
`WorkflowCapability::record_change_set` writes the run's correlation marker to
`<[stub] root>/changes/<KEY>.json`, where `<[stub] root>` is the `root` this
deployment set under `[stub]`. The next run over the same reference reads that
marker, judges the work done and executes nothing — whatever the ticket now
says. Removing the trigger label and adding it again does not start a second
round.

Removing that file makes the next run work the ticket again. No fiddle command
does it, so it is a file an operator deletes and not an operator surface, and it
is what the tests do to force a second round:
`a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail`
removes the record and reruns, and the rerun pays for its own agent turns and
reaches the effect tail (ADR 083).

Two consequences follow for a deployment. A `[stub] root` on a path that does
not survive the run — a container layer, a temporary directory — forgets every
completion, so the next run works the ticket again. And where `[stub] root`
points decides what a fresh checkout inherits: a root inside the working copy
leaves its completions behind, and a root on an absolute path outside the
checkout carries every one of them across.

## Record what the model was sent and what it returned

Off by default. One run, one variable.

```sh
FIDDLE_TRANSCRIPT=1 fiddle run beans:fiddle-m1-demo --capability fixture_repair
```

The run writes `<report.dir>/transcript/<slug>-<token>.jsonl` and names the file
on stderr. One JSON object per line. The first line is the brief, the offered
tools and the bounds. Read it with `jq -c` or `jq 'select(.record=="received")'`.

`[report] dir` is uploaded as a workflow artifact, so a transcript from CI needs
no further plumbing. Set the variable on the step that runs fiddle.

The file carries the repository's content and the model's replies. The resolved
credential is replaced with `[redacted]`. Do not attach the artifact where the
repository's content may not go.

## Common issues

**`HTTP 404: workflow github-effects.yml not found on the default branch`.**
The file is not on the default branch, so no workflow entity exists. Land it there;
no `--ref` works around this.

**`could not find Cargo.toml` at `cargo build --release`.** The dispatched ref
carries no Rust workspace. Pass a milestone branch. The lane's preflight refuses
this by name and prints the ref to use.

**The lane exits 1 naming `FIDDLE_EFFECTS_TOKEN`.** The secret is unset or empty. A
reference to a missing secret renders as the empty string, which is why the guard
tests for emptiness rather than existence.

**A dispatch 403s.** `Actions: write` is missing. Adding `Workflows` does not fix a
dispatch.

**The host's sweep exits 1 naming a secret.** The secret is unset or empty on
the host. A reference to a missing secret renders as the empty string, so a
guard must test for emptiness rather than existence.

**The host's sweep 403s reading check runs.** `FIDDLE_CVE_TOKEN` carries no
`Checks` permission. `Contents` and `Pull requests` do not imply it, and the
host's own `GITHUB_TOKEN` cannot read another repository at all.

**Residue at the disposable repository.** Expect branches to be exactly `main` and
open pull requests `0`. Closed pull requests are permanent, so a non-zero closed
count is normal.

```sh
gh api repos/peel/fiddle-effects-acceptance/branches --jq '[.[].name]|join(", ")'
gh pr list --repo peel/fiddle-effects-acceptance --state open
gh api repos/peel/fiddle-effects-acceptance/branches --jq '.[].name' | grep '^fiddle/' \
  | xargs -r -I{} gh api "repos/peel/fiddle-effects-acceptance/git/refs/heads/{}" --method DELETE
```

**`config check` exits 2 naming `go` as an unknown field under
`[orchestration.cve]`.** M4c deleted that key and the table is strict, so a
document that loaded yesterday fails today. Delete the key. The table admits
`image`, `severities` and `max_findings` and nothing else.

**A run exits 2 naming `FIDDLE_TRANSCRIPT`.** The variable accepts only `1`.
Unset it to record nothing.

**The Jira lane says `the site holds no issue KEY` for an issue you can open in a
browser.** The credential is good and the key is wrong. The port asks
`/rest/api/3/myself` before it says this, so it says it only after a **200** from
that endpoint. A wrong `JIRA_API_TOKEN` reads as `the site refused the credential
with 401` instead.

This is measured, not inferred. Six probes ran against `snplow.atlassian.net` on
2026-08-27, with an operator's valid token and with that token corrupted by four
appended characters:

| probe | endpoint | credential | status |
|---|---|---|---|
| 1 | `/rest/api/3/issue/ISP-239?fields=status,updated` | valid | 200 |
| 2 | `/rest/api/3/issue/ISP-239?fields=status,updated` | corrupted | 404 |
| 3 | `/rest/api/3/issue/ISP-239?fields=status,updated` | none | 404 |
| 4 | `/rest/api/3/issue/ISP-99999999?fields=status,updated` | valid | 404 |
| 5 | `/rest/api/3/myself` | corrupted | 401 |
| 6 | `/rest/api/3/myself` | valid | 200 |

Probes 2, 3 and 4 are one answer, so an issue read cannot tell a bad credential
from a missing issue and the 401 and 403 arms of `JiraWorkItemPort::read` are
unreachable for an issue read on their own status. Probes 5 and 6 differ, so
`/rest/api/3/myself` tells them apart. It reads the credential alone and does not
depend on issue permissions.

**The Jira lane says the site holds no issue `KEY`, or it refused the credential.**
The credential check itself did not answer. The reason carries the status it got
back. Run the check by hand:

```sh
curl -s -o /dev/null -w '%{http_code}\n' \
  -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
  https://snplow.atlassian.net/rest/api/3/myself
```

`200` means the credential is good and the key is wrong. `401` means the
credential is wrong, whatever the issue read said. Anything else is the site, not
the credential and not the key.

**A run exits 20 rather than 11.** That is a permanent refusal: a `[github.policy]`
deny, a duplicate remote state, a diverged payload, or an unanswerable
human-decision requirement. Repeating the same invocation does the same thing. Fix
what the reason names first.

---
Last reviewed: 2026-08-21
