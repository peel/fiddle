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
| `JIRA_USER_EMAIL` | the live Jira read lane | opt-in, reads one issue, writes nothing |
| `JIRA_API_TOKEN` | the live Jira read lane | the same account's classic unscoped API token |

The two `WIZ_` variables are the only ones `fiddle.toml` never names. `setup-wiz`
runs `wizcli auth --id --secret` with them, and fiddle passes the scanner no
credential of its own (ADR 042). A caller who skips that step gets the scanner's
own exit: `the scanner produced no report (exit 1)`, and exit 11.

A local lane reads its variable from `.env` in the worktree you run from.
`.envrc` is tracked, so `dotenv_if_exists` resolves the `.env` beside it. `.env`
is gitignored; `.env.example` is the template.

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

`scripts/live-jira-observe.sh` reads one issue two ways and compares them. It
calls `/rest/api/3/issue/KEY?fields=status,updated` itself, then runs
`fiddle inspect jira:KEY --json` against a generated document that names
`JIRA_USER_EMAIL` and `JIRA_API_TOKEN` and carries neither value. It refuses when
any of `JIRA_SITE`, `JIRA_ISSUE`, `JIRA_USER_EMAIL`, `JIRA_API_TOKEN` or
`FIDDLE_BIN` is absent, because a lane that skipped for want of a credential
would read exactly like one that passed. It also refuses a `JIRA_SITE` that is
not an `https://` origin. Set the scheme. A bare `snplow.atlassian.net` reaches
the site over http, and the site answers **301** to every request, which reads
like a refusal and measures nothing.

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

The lane records evidence and does not gate. `scripts/gate.sh` references no
`live-*.sh` script. It reads an issue and writes nothing, so it needs no
disposable target; do not point it at an issue whose `status` you mind being
printed to your terminal.

The lane prints the issue it read. It requests `fields=status,updated` and no
other field, so no ticket prose crosses the boundary, and nothing it prints is
committed here.

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
