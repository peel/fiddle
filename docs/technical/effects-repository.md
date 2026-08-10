# Disposable effects repository

M2 gives fiddle its first effects on the outside world: a branch, a pull request
and a requested check on GitHub. The deterministic suite proves the properties
that matter — including exactly-once — offline, against a scripted `gh` and a
recording `git`, and that is the lane that gates. This document records the
*other* half: the real repository those effects are actually performed against,
what it is for, and the standing rules it holds.

It is the sibling of [acceptance-repository.md](acceptance-repository.md), and it
inherits that document's reasoning. The difference is the direction of the
traffic: `peel/fiddle-acceptance` is a repository fiddle is proved *from*, and is
never written to; this one is a repository fiddle is proved *against*, and exists
in order to be dirtied.

## The repository

| | |
| --- | --- |
| Repository | [`peel/fiddle-effects-acceptance`](https://github.com/peel/fiddle-effects-acceptance) |
| Visibility | **public** |
| Default branch | `main` — its only permanent branch |
| Contents | `README.md` and `.github/workflows/fiddle-check.yml`, and nothing else |
| Dispatch target | `.github/workflows/fiddle-check.yml`, installed by commit `73b480a` |
| Lane | `scripts/live-github.sh`, opt-in, never part of the gate |
| Contract | `FIDDLE_BIN` names the compiled `fiddle`; `FIDDLE_GITHUB_TOKEN` holds the credential; `gh`, `git` and `jq` must be on `PATH` |
| Secrets | none — and the lane's credential could not read one if there were |

Created 2026-08-09. Its whole history is four commits: the initial commit, the
two halves of the write probe described below, and the workflow.

## Why a standing repository, and not one created per run

The obvious design is to create a repository, publish into it, and delete it —
residue by construction impossible. It is not available: the credential holds no
`delete_repo` authority, and asking for one would mean holding a credential that
can destroy a repository in order to prove that fiddle can open a pull request.

So the repository stands, and residue becomes the lane's problem rather than the
platform's. That is the trade this document exists to make honest: see
*Cleanup and residue* below for exactly what is removed, and for the one thing
that cannot be.

## Why public

The same reason `peel/fiddle-acceptance` is public: **reading it needs no
credential**.

Anyone can see what fiddle did here — the branch it pushed, the pull request it
opened, the workflow run it requested — without being trusted with anything. The
lane's own worktree is a `git -c credential.helper= clone` of it, which is both
what makes the push a fast-forward on top of the real `main` and a live
demonstration of that rule on every run.

Nothing in it is confidential, because nothing in it is anything: a README and a
workflow that echoes one line.

## Standing rules

These are properties of the repository, not observations about its current
contents.

1. **It holds no secret and never will.** No Actions secret, no Actions
   variable, no deploy key, no token of any kind. There is nothing for a
   workflow here to be given, because the workflow does nothing.
2. **`main` is its only permanent branch.** Everything under `fiddle/` is
   fiddle's, is disposable, and is removed by the lane that made it.
3. **Everything titled `fiddle-…` in the Actions history is disposable** and is
   removed by the lane that dispatched it.
4. **It is written to by the M2 live lane and by nothing else.** No product code
   points at it; the lane's `FIDDLE_EFFECTS_REPO` default is the only place its
   name appears outside this document.
5. **Nobody works in it.** If that ever stops being true, rule 2's scoping is
   what protects them — see *Cleanup and residue*.

### How rule 1 is established

Not by a zero count. The lane's credential **cannot enumerate secrets at all**:

```console
$ gh api repos/peel/fiddle-effects-acceptance/actions/secrets
{"message":"Resource not accessible by personal access token","status":"403"}
gh: Resource not accessible by personal access token (HTTP 403)
```

That 403 is stronger evidence than `total_count: 0` would be, and it is the
reason no broader token was requested to go and check. A credential that cannot
list a repository's secrets cannot leak one either, whatever the repository
holds; and the standing rule that it holds none is then *documented* — by this
list — rather than asserted by a credential that would need more authority than
the lane's purpose justifies. The same 403 comes back for `keys`, for the same
reason.

`scripts/live-github.sh` asserts that 403 on every run, in the direction that
matters: if the credential ever *could* enumerate secrets, the lane fails and
says the token is scoped too broadly.

## The credential

A fine-grained personal access token, held in `FIDDLE_GITHUB_TOKEN` — the same
environment variable name the `[github]` table names, so the lane and the product
read the credential from one place. It expires 2026-11-07.

What was verified, read-only, rather than assumed:

| Probe | Result |
| --- | --- |
| `repos/peel/fiddle-effects-acceptance/collaborators` | 200 — the repository is in the token's selection |
| `repos/peel/fiddle-acceptance/collaborators` | 200 — **also** in the token's selection |
| `repos/peel/fiddle/collaborators` | **403** — the product repository is not |
| `repos/*/actions/secrets` | **403** for all three — no `Secrets` permission anywhere |

The third row is the one the design needed: the token that performs M2's effects
cannot reach `peel/fiddle`'s permission-gated surface, so a live lane cannot
damage the product repository however wrong it goes. The second row is wider than
the design assumed and is recorded rather than smoothed over — `peel/fiddle-acceptance`
holds no secret either (its own standing rules say so) and nothing in M2 writes
to it, but the token's selection is two repositories and not one.

Public-repository *reads* work regardless of selection, which is why row three is
a Metadata-gated endpoint rather than a `GET` of the repository.

The credential never reaches an `argv`. `git push` carries it in
`http.https://github.com/.extraHeader` through `GIT_CONFIG_*`, `gh` carries it in
`GH_TOKEN`, and the lane's own `gh` does the same — with `GH_CONFIG_DIR` pointed
at an empty scratch directory, so the credential it uses is provably the one it
was handed rather than whatever is in the operator's keychain. The lane greps
every byte fiddle writes for the token *before* echoing any of it, and a hit is a
failure rather than a redaction.

## The cross-repository write, exercised

The design carried one open external assumption: that this credential could
actually write to a repository other than `peel/fiddle`. Read-only probing could
not settle it, so it was recorded **blocked** and assigned here.

It is now closed twice over.

**The narrow probe** (2026-08-09) created and deleted a ref:

```console
$ gh api repos/peel/fiddle-effects-acceptance/git/refs --method POST \
    -f ref=refs/heads/probe -f sha="$(gh api .../git/ref/heads/main --jq .object.sha)"
# created
$ gh api repos/peel/fiddle-effects-acceptance/git/refs --method POST -f ref=refs/heads/probe ...
422 {"message":"Reference already exists"}
$ gh api repos/peel/fiddle-effects-acceptance/git/refs/heads/probe --method DELETE
# deleted; branches are again exactly `main`
```

The 422 is worth keeping: it is the exact response `EnsurePullRequest`'s and
`EnsureBranchPublished`'s duplicate handling must resolve *by reading the world*
rather than by retrying the write.

**The whole walk**, which is the stronger closure: `scripts/live-github.sh`
pushes a branch, opens a pull request and dispatches a workflow run against this
repository, and reads all three back. The write is not merely permitted; it is
performed, observed, and removed.

## The dispatch target workflow

The dispatch fiddle issues is answered by a workflow in *this* repository.
Without it the dispatch 404s and the live check path cannot work at all.

```yaml
# .github/workflows/fiddle-check.yml
name: fiddle-check
run-name: fiddle-${{ inputs.fiddle_effect_id }}
on:
  workflow_dispatch:
    inputs:
      fiddle_effect_id:
        description: "Echoed into run-name so a fresh process can locate this run"
        required: true
concurrency:
  group: fiddle-${{ inputs.fiddle_effect_id }}
  cancel-in-progress: false
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo "the check fiddle requested"
```

`run-name` is not decoration. It is **the entire channel** by which a check
request's identity comes back, and that is a fact about GitHub rather than a
design preference:

- `POST /repos/{repo}/actions/workflows/{file}/dispatches` answers **204 No
  Content** with no run id at all;
- the runs listing carries **no `inputs` key**, so the value that was dispatched
  is not readable from the run;
- so `run-name` — free text, set by the target repository's own workflow file —
  is the only place `fiddle-<effect id>` can be recovered from.

`fiddle-runtime::github::checks::run_name` writes the local half of that
spelling and this file writes the remote half, which makes the round trip
**fragile in a specific way**: the two halves live in different repositories and
nothing compiles them together. That is exactly why the lane asserts it on every
run rather than trusting it, and why the workflow's `run-name:` line is grepped
out of the remote file before anything is published.

### Verified against real GitHub

The echo was verified before anything depended on it, and is re-verified every
run. From the run recorded below:

```
live-github: run-name echo present in fiddle-check.yml
live-github: round trip: ref fiddle/c7d872cc2cd95328, pull request #11,
             run 31343171740 titled fiddle-1e57efd5a300af97
```

`fiddle-1e57efd5a300af97` is the name **GitHub** gave the run, and
`1e57efd5a300af97` is the identity **fiddle** derived for its
`ensure_check_requested` effect. They are compared, not assumed.

## Running the lane

```sh
nix develop -c cargo build --release
set -a; . ./.env; set +a          # or export FIDDLE_GITHUB_TOKEN yourself
FIDDLE_BIN="$PWD/target/release/fiddle" scripts/live-github.sh
```

`FIDDLE_EFFECTS_REPO` overrides the target. On success the lane prints
`live-github: PASS` and exits 0; on failure it prints
`live-github: FAIL: <what was expected>` on stderr and exits non-zero.

**It never gates.** Nothing in `.github/workflows` invokes it, and it is not a
`cargo test`, so `scripts/gate.sh` cannot reach it. The gate stays offline and
credential-free.

**It never skips.** With `FIDDLE_GITHUB_TOKEN` or `FIDDLE_BIN` absent it fails
loudly, because a silently-skipped lane is indistinguishable from a passing one —
the rule M1 established for its Tier 1 lane. Interrupted, it cleans up and exits
130 or 143, so a killed lane does not look like a passing one either.

## What it asserts

One ordered walk, each step observing the world its predecessor left. Every count
is read out of GitHub with `gh`; the run's own report is consulted only to
cross-check an *identity* against what the remote answered.

1. **Preconditions.** The repository is public; the dispatch target carries the
   `run-name` echo; the credential cannot enumerate secrets; and the remote holds
   **zero** `fiddle/` branches, zero open `fiddle/` pull requests and zero
   `fiddle-` runs. Zero-then-one is what makes the walk unfakeable: a lane that
   did nothing at all and then asserted "exactly `main`, no open pull requests"
   would otherwise **pass**.
2. **Publishing.** Fresh `fiddle` processes run
   `run beans:fiddle-live-publish --capability publish_change` until one
   completes. Afterwards the remote holds exactly one `fiddle/` branch, one open
   pull request and one `fiddle-` workflow run.
3. **The round trip.** The run GitHub lists is titled with the check effect's
   derived identity; the branch receipt is about the ref the remote actually
   holds, at the commit the worktree actually published; and each receipt's
   external reference — sha, pull request number, run id — is the remote's own
   identifier for the object.
4. **Nothing local survives.** Every bundle, every attempt journal under
   `<report.dir>/.attempts`, and the change-set marker are deleted. A process
   that consulted a local record of what an earlier one did would have nothing to
   consult; and with the marker left in place the next process would decline to
   execute the capability at all, which would prove nothing about
   read-before-write.
5. **The same work again.** More fresh processes run the same invocation. They
   derive the same three identities, and **recognise** the same three objects —
   same sha, same pull request number, same run id.
6. **Exactly one of each, still.** However many processes ran, the remote holds
   one branch, one pull request and one requested check, and they are the same
   three objects throughout. The identity check is what the counts alone cannot
   give: a close-and-reopen or a delete-and-redispatch would leave every count at
   one.

This is the same property `crates/fiddle-acceptance/tests/exactly_once.rs` proves
offline and gates on. The live lane does not re-prove it — it shows it survives
contact with the real thing.

### A process may legitimately not complete, and what that costs

The three-tier model's second rule is that a live lane never asserts GitHub
cooperated. This lane found out why on its first real run, and both cases are
recorded here because they change how it is read.

- **`GET .../actions/workflows/<file>/runs` immediately after a dispatch does not
  yet list the run.** Reliably, not once. `EnsureCheckRequested` therefore
  reports `Unresolved` — correctly: the 204 said nothing, and the world does not
  yet show the write.
- **`GET .../git/ref/heads/<branch>` immediately after the push that created it
  answered 404 once.** `EnsureBranchPublished` reported `Unresolved` rather than
  believing the push's own answer, which is precisely what step 8 of the effect
  protocol exists to do.

Both reach `RunOutcome::Retryable`, **exit 11**, whose whole meaning is "run me
again". So the lane runs fresh processes until one completes — which is what a
caller reading exit 11 is told to do — up to six attempts four seconds apart.
Exit **20**, a settled failure, is never retried, and exhausting the attempts is
a loud failure.

That is not a weakening; it is the opposite. Each extra process is another
opportunity to duplicate an object, and the assertion is unchanged. It is also
worth noticing what these two cases *are*: the ambiguity `exactly_once.rs`
arranges with a fixture that applies a mutation and then dies is not a contrived
shape. GitHub produces it for free.

## Cleanup and residue

Cleanup hangs off a `trap` and runs on **every** exit path — pass, fail, `INT`,
`TERM` — so a failing or interrupted run leaves no more residue than a passing
one. It is registered on `EXIT`, with `INT` and `TERM` turned into an `exit` that
reaches it, rather than as `trap cleanup EXIT INT TERM`: on a signal, `$?` at the
top of a handler is the interrupted command's status, which for a `sleep` that
finished is **0**, and a killed lane that exits 0 is the same failure as a
skipped one.

What it removes, and only this:

- open pull requests whose **head branch starts with `fiddle/`**, closed with
  `--delete-branch`;
- remaining branches whose name starts with **`fiddle/`**;
- workflow runs of `fiddle-check.yml` whose **`run-name` starts with `fiddle-`**,
  cancelled first if in flight, because a run in flight cannot be deleted.

**The scope is the point.** A blanket "delete every branch that is not `main`"
would destroy a colleague's branch the first time this repository stopped being
disposable. `fiddle/` is not a convention chosen for cleanup's convenience: it is
the namespace `fiddle-runtime::github::refs::NAMESPACE` derives every branch name
under, and `fiddle-` is the prefix
`fiddle-runtime::github::checks::run_name` builds every run title from. Cleanup
deletes what fiddle names and nothing else.

Runs are removed as well as refs, and that is not tidiness. Deleting a branch
does **not** delete the workflow runs dispatched against it, so a lane that
cleaned up only refs would accumulate residue invisibly — and its own "exactly
one run" precondition would start failing on the next invocation, which is how
this was noticed.

After cleanup the lane **asserts** zero residue rather than reporting it:

```
live-github: residue after cleanup: fiddle/branches=0 open-prs=0 fiddle-runs=0
live-github: branches at the remote: main
```

Both lines are checked. The first is the scoped claim; the second asserts this
repository's standing rule that `main` is its only permanent branch, so a scoped
cleanup that missed something *outside* its namespace is still caught.

### The one thing that cannot be cleaned up

**A closed pull request is permanent.** GitHub has no API that deletes one, so
every run of this lane leaves behind a closed pull request and consumes a number
that is never reused. That is a property of the forge, not of the lane, and it is
a large part of why this repository is disposable rather than shared: a growing
list of closed pull requests — one per run, all of them fiddle publishing the same
one-line change and then having it closed underneath it — is expected, and is not
residue anyone needs to act on. There were thirteen, all closed, when this
document was written.

So "zero residue" means precisely: **no `fiddle/` branch, no open pull request,
no `fiddle-` workflow run, and `main` the only branch.** Not "no pull request has
ever existed" — that is unachievable, and a lane that claimed it would be lying.

### After a run

```sh
gh repo view peel/fiddle-effects-acceptance --json visibility           # PUBLIC
gh api repos/peel/fiddle-effects-acceptance/branches --jq '.[].name'    # exactly: main
gh pr list --repo peel/fiddle-effects-acceptance --state open           # empty
gh api repos/peel/fiddle-effects-acceptance/actions/runs --jq '.total_count'  # 0
gh api repos/peel/fiddle-effects-acceptance/actions/secrets            # 403, as above
```

The lane also creates one `mktemp -d` directory holding the whole disposable
project — the configuration document, the fixture roots, the clone, the reports —
and removes it in the same `trap`. It writes nothing outside that directory and
nothing outside this repository.
