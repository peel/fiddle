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
4. **It is written to by exactly two scripts, enumerated here, and by nothing
   else.** They are `scripts/live-github.sh` — the M2 live lane — and
   `scripts/verify-graphql-ready.sh`, ADR 018's probe. Each carries its own
   `FIDDLE_EFFECTS_REPO` default naming this repository
   (`live-github.sh:85`, `verify-graphql-ready.sh:68`), and those two defaults
   are the only places the name appears **as a target**.
   `.github/workflows/github-effects.yml` is a *caller* of the first script
   (line 262) rather than a third writer: it sets no target of its own.

   **Exactly two things violate this rule, and both are checkable.** One: a third
   `FIDDLE_EFFECTS_REPO` assignment — `grep -rn FIDDLE_EFFECTS_REPO` outside
   `docs/` must return those two lines and no other. Two: an occurrence of this
   repository's name under `crates/` that is *neither* in a file under a `tests/`
   directory, *nor* after its file's `#[cfg(test)]` line, *nor* on a comment
   line — that is product code holding the name, which is what *"no product code
   points at it"* forbids. See *Rule 4 was false from `253a7de`* for that check
   run at this commit and what it found.
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

### Rule 4 was false from `253a7de`

Recorded rather than smoothed over, for the same reason as *The second row read
200 until 2026-08-10* below. Until this edit rule 4 read *"it is written to by the
M2 live lane and by nothing else"*, and named that lane's `FIDDLE_EFFECTS_REPO`
default as *"the only place its name appears outside this document"*. Commit
`253a7de` added `scripts/verify-graphql-ready.sh` with a second default at its
line 68, identical in shape to the lane's, and the rule was false from that
commit until it was corrected here.

**Its two clauses were false for different reasons, and by different margins.**
The writer count was off by one, which `253a7de` explains. The second clause was
off by far more and no commit explains it: the name appears in **20** places
outside `docs/`, not one. A reader who fixes only the writer count will believe
the rest of the rule, which is why this is stated separately — the first clause
was falsified by a change, the second was **never true** in the form it was
written.

Those 20 break down as the two `FIDDLE_EFFECTS_REPO` defaults, **11** occurrences
under `crates/`, and 7 in `.env.example`, `github-effects.yml` and the two
scripts' own comments. Running the rule's boundary check over the 11 at this
commit: `orchestration.rs:1228` is inside the `#[cfg(test)]` opening at `:725`;
`human/mod.rs:580`, `:593` and `:604` are inside the one opening at `:574`, which
runs to the file's end at 608; `github/checks.rs:41` is a `//!` doc comment; and
the remaining six are in `crates/fiddle-acceptance/tests/`, where the whole file
is a test target. **Zero are product code**, so the *"no product code points at
it"* half survives intact — which is the half worth keeping, and the reason the
clause was narrowed to "as a target" rather than deleted.

The counts above are a measurement at this commit and are not themselves the
rule — the boundary check is. A count in a standing rule goes stale the first time
someone adds a fixture, which is the failure this section exists to record.

Both were found by **grepping the tree**, not by reading the bean that assigned
this correction: the bean named the second writer and did not state that the
second clause was false at all, and an earlier draft of this edit undercounted the
occurrences at nine by missing two acceptance fixtures, the two script comments
and `.env.example`. That is the same class of error as the rule itself — a set
asserted from memory rather than enumerated from the tree.

It was found by the confirming **evaluation** of the bean that added the script,
not by review of this document — which is the part worth keeping. A rule
asserting an exhaustive set does not fail when the set grows; it just quietly
stops being true, and nothing mechanical notices. The rule is left as an
exhaustive enumeration anyway, because a set of writers that can be counted is
the whole reason a destructive sweep against a standing repository is
defensible. Weakening it to something unfalsifiable — "it is written to only by
scripts that mean well" — would be worse than the brief falsehood: the next
writer would then be compliant by construction.

## The credential

A fine-grained personal access token, held in `FIDDLE_GITHUB_TOKEN` — the same
environment variable name the `[github]` table names, so the lane and the product
read the credential from one place. It expires 2026-11-07.

Its permissions, and what each one is for:

| Permission | | For |
| --- | --- | --- |
| Contents | read and write | pushing the branch |
| Pull requests | read and write | opening the pull request |
| Actions | read and write | `POST .../actions/workflows/<file>/dispatches` — **the dispatch** |
| Metadata | read | mandatory on every fine-grained token, and what answers the selection probe below |
| Secrets | none | asserted on every run — see *How rule 1 is established* |
| Issues | **not in this list, and yet an issue was created** | unexplained and **unresolved** — see *A success this table does not account for* |

`Actions: write` is the permission the dispatch requires, so a 403 on the dispatch
is that permission missing. It is **not** `Workflows`, which is a different
permission governing pushes that touch `.github/workflows/**` — that is how this
repository's own `fiddle-check.yml` was installed (commit `73b480a`), and it is
not something the lane ever does: the only file the lane pushes is a one-line
probe at the repository root. A credential granted `Workflows` in place of
`Actions` still 403s on the dispatch *and* can rewrite the target's CI, which is
the worst of both. `.env.example` and `.github/workflows/github-effects.yml`'s
remediation text name this same list.

### A success this table does not account for

On 2026-08-10, during `fiddle-w0xt`, a GraphQL `createIssue` against this
repository **succeeded** under this credential and opened issue #25. Nothing in
the list above grants it.

Four places enumerate that same grant — Contents read and write, Pull requests
read and write, Actions read and write, Metadata read, Secrets none — and under
every one of them the mutation should have been refused:

| Where | |
| --- | --- |
| `.env.example` | lines 19-26 |
| `docs/evaluator-calibration-general.md` | line 809, which adds *"`Issues` is absent"* in as many words |
| `.github/workflows/github-effects.yml` | its remediation text, lines 153-154 |
| this table | above |

The calibration is the pointed one: it does not merely omit `Issues`, it names the
absence and builds a design decision on it — GitHub routes an issue comment
through **Issues** and a pull request comment through **Pull requests**, so M2's
conversation was deliberately put on a pull request *"so the credential would not
have to be widened"*. A grant that permits `createIssue` is the thing that choice
was made to avoid needing.

Four sources agreeing is not evidence when the wire disagrees with all four; it is
a measure of how often one was copied.

**And a fifth place enumerates a different grant, which is worse than a fourth
agreeing one.** `docs/technical/RUNBOOKS.md` § *Minting the GitHub token* — the
procedure an operator actually follows to create this credential, scoped to this
repository — prescribes *"these five, and no others"* and lists **`Workflows`
read and write**, with no `Secrets` row at all. This table argues the opposite
directly: `Workflows` is *"not something the lane ever does"*, and a credential
holding it *"can rewrite the target's CI, which is the worst of both"*. So the
document that mints the token and the document that describes it do not agree
about what the token holds.

That does **not** explain the issue: `Workflows` is not `Issues`, and no reading of
it permits `createIssue`. It is recorded here because it is the same question —
*what is actually granted?* — and because it means the count of documents to
reconcile is five with two distinct answers, not four with one.

Every issue-*modifying* operation was refused in the same session — 403 on REST
`PATCH state=closed`, `FORBIDDEN` on both `closeIssue` and `deleteIssue`, as
tabulated under *An issue is residue, and it is worse than a branch*. So the
observation is not "this credential
holds `Issues: write`"; a token with `Issues: write` would have closed the issue.
It is narrower and stranger than that, and it is **not** resolved here.

**This edit records the discrepancy and does not restate the grant.** The rule
this table is built on is that scope is proven by a 403 and never by a successful
read — a public repository reads with any credential, so a success proves nothing
about authority. This is that rule's mirror case: a success proving the *presence*
of some authority nobody documented, which is exactly as unresolved as a read
would leave the absence of one. Writing an `Issues` row with a permission level in
it would be inventing the grant to match the observation, in a table whose value
is that every row was measured.

Closing it means reading the token's actual permission set at GitHub and
re-running the probe table against it — both operator actions, on a credential
this document does not widen and no lane should. `docs/BACKLOG.md`'s
2026-08-11 entry carries it. Until then, treat the effective grant as **wider than
this table in an unknown direction**, and note that this is the second time in
this milestone that a set of agreeing documents was wrong about this credential —
the first is *The second row read 200 until 2026-08-10* below, where four of them
said the selection was one repository while it was two.

### The selection, verified by probe rather than assumed

Re-run 2026-08-10, after the selection was narrowed:

| Probe | Result |
| --- | --- |
| `repos/peel/fiddle-effects-acceptance/collaborators` | **200** — the repository is in the token's selection |
| `repos/peel/fiddle-acceptance/collaborators` | **403** — M0's acceptance repository is not |
| `repos/peel/fiddle/collaborators` | **403** — the product repository is not |
| `repos/*/actions/secrets` | **403** for all three — no `Secrets` permission anywhere |
| `repos/*/keys` | **403** for all three — same reason |

```console
$ gh api repos/peel/fiddle-effects-acceptance/collaborators -i | head -1
HTTP/2.0 200 OK
$ gh api repos/peel/fiddle-acceptance/collaborators -i | head -1
HTTP/2.0 403 Forbidden
$ gh api repos/peel/fiddle/collaborators -i | head -1
HTTP/2.0 403 Forbidden
```

So the selection is **one repository**. The token that performs M2's effects
cannot reach the permission-gated surface of either other repository, and a live
lane cannot damage them however wrong it goes.

Public-repository *reads* work regardless of selection, which is why every row is
a Metadata-gated endpoint rather than a `GET` of the repository. `/collaborators`
is the row that carries the weight precisely because it **discriminates**: 200 for
one repository, 403 for two. `/actions/secrets` answers 403 for all three, so on
its own it could not tell them apart — it is evidence of an absent permission, not
of a selection, and a probe that cannot discriminate is not evidence of the thing
it is offered for.

`scripts/live-github.sh` re-reads that 200 on every run and refuses a target the
selection does not include — see *The target guard*.

### The second row read 200 until 2026-08-10

Recorded rather than smoothed over, because for the length of M2's implementation
the selection was **two** repositories wide while four documents said it was one.
The credential performing M2's effects could write to `peel/fiddle-acceptance` —
M0's external acceptance repository, whose whole argument is that reading it needs
no credential, and whose standing rules include
`gh pr list --repo peel/fiddle-acceptance --state all` being empty. A closed pull
request cannot be deleted, so residue there would have falsified that rule
permanently. Nothing in M2 ever wrote to it, and nothing mechanically stopped it
from doing so.

The operator narrowed the selection on 2026-08-10, and the write that was
permitted is now refused:

```console
$ gh api repos/peel/fiddle-acceptance/git/refs --method POST -f ref=refs/heads/scope-probe \
    -f sha="$(gh api repos/peel/fiddle-acceptance/git/ref/heads/main --jq .object.sha)"
{"message":"Resource not accessible by personal access token","status":"403"}
$ gh api repos/peel/fiddle-acceptance/branches --jq '.[].name'
main
```

That is the discriminating form: a *write* attempt against the exact endpoint the
danger ran through, refused. The credential is now structurally incapable of it
rather than merely not pointed at it — the difference between a rule and a habit.
[acceptance-repository.md](acceptance-repository.md) records the same episode from
the other side.

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

`FIDDLE_EFFECTS_REPO` overrides the target, and a target the lane was not built
for is refused before anything destructive is armed — see *The target guard*. On
success the lane prints `live-github: PASS` and exits 0; on failure it prints
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

1. **Preconditions.** The target is the one this lane was built for — see *The
   target guard*, which runs before the cleanup trap is armed — and the remote
   holds **zero** `fiddle/` branches, zero open `fiddle/` pull requests and zero
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

## The target guard

`FIDDLE_EFFECTS_REPO` overrides the target, and cleanup below performs a
pull-request-close and ref-DELETE sweep. Until 2026-08-10 the override was free
text and the trap arming that sweep was set **before** the first thing that
happened to notice a wrong repository — the incidental fetch of the dispatch
target's workflow file. Between those two points a mistyped or hostile value had a
destructive sweep armed against a repository nobody had checked. The file's own
comment said the knob was "never so it can be pointed at a repository somebody
works in"; the code did not say it.

It says it now. Before the traps are set, and before the `git push` that is this
lane's first write anywhere, the target must answer all six of these — every one a
read:

1. **The name is a bare `owner/name`.** The value is interpolated into URL paths,
   so `a/b/../../c` addresses a repository other than the one the operator wrote,
   and the sweep would follow it there. Each half must begin with an alphanumeric,
   which is what refuses `..` as a path component.
2. **It is public.** The lane's committed argument is that reading its target
   needs no credential — it clones with `credential.helper=` disabled — and a
   private repository is by construction one somebody was trusted with.
3. **Its default branch is `main`**, the branch the push must fast-forward.
4. **It holds no branch that is neither `main` nor under `fiddle/`.** Standing
   rule 2 is what makes a ref-DELETE sweep here defensible, so it is checked on
   the way *in* and not only asserted on the way out. Branches under `fiddle/`
   are deliberately tolerated: they are this lane's own residue from an
   interrupted run, the preconditions still refuse to *start* on them, and
   refusing them here would strand them forever — cleanup would never arm to
   sweep them.
5. **The credential's repository selection includes it**, read as 200 on
   `/collaborators`. This is the probe that discriminates (200/403/403 above), and
   the rule it enforces is that the lane may only arm a sweep against a repository
   somebody deliberately selected for the credential — a stronger statement than
   "the write turned out to be permitted".
6. **It carries the dispatch target, echoing the effect id through `run-name`.**
   A repository without `fiddle-check.yml` is not one this lane can complete
   against at all. This check is not new; it is the one that used to sit *after*
   the trap and do this job by accident.

The guard is the **conjunction**, not any single line, and it is deliberately not
a denylist. "Not `peel/fiddle`" would be worth nothing: the value this
milestone's review found dangerous was `peel/fiddle-acceptance`, which is not the
product repository and is still somebody's, and the next dangerous value is one
nobody has thought of. `peel/fiddle` fails 4 and 5; `peel/fiddle-acceptance`
passes 2, 3 and 4 and fails 5 and 6; a typo fails 2, because no such repository
can be read; a traversal fails 1.

A refusal removes the scratch directory itself, since the trap that would have
removed it does not exist yet, and says which side of the trap it is on:

```console
$ FIDDLE_EFFECTS_REPO=peel/fiddle-acceptance scripts/live-github.sh
live-github: target repository: peel/fiddle-acceptance
live-github: visibility=public default_branch=main
live-github: no branch outside main and fiddle/… at the remote
live-github: FAIL: peel/fiddle-acceptance is not in this credential's repository selection (/collaborators is not 200); this lane only sweeps a repository its credential was deliberately given
live-github: refused before arming cleanup and before any mutation; nothing was created, nothing was deleted
```

Note what is **absent** from that transcript: no `cleaning up (exit 1)` line, no
`residue after cleanup` line. Those come from `cleanup`, and `cleanup` is not armed
when the guard refuses — which is the whole point, and is why the refusal is
checked by running it rather than asserted in prose.

The pre-change script, run against a nonexistent repository so its sweep could
delete nothing, printed the opposite and is why this section exists: it reported
`cleaning up (exit 1)` and then issued the whole close-and-DELETE sweep at a
target it had just failed to read. Against a repository that *did* exist under a
mistyped name, those calls would not have 404'd.

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
residue anyone needs to act on. There were sixteen, all closed, when the target
guard above was added.

So "zero residue" means precisely: **no `fiddle/` branch, no open pull request,
no `fiddle-` workflow run, no issue beyond #25, and `main` the only branch.**
Not "no pull request has ever existed" — that is unachievable, and a lane that
claimed it would be lying. For the same reason it is "beyond #25" and not "no
issue": #25 cannot be deleted either, so a definition demanding its absence would
be one more unachievable claim, and the honest form names the one entry that is
grandfathered rather than pretending the count is zero.

### An issue is residue, and it is worse than a branch

This class was missing from the list above until 2026-08-11, and its absence had
a consequence: during `fiddle-w0xt` an exploratory GraphQL `createIssue` against
this repository succeeded and left issue **#25** open, and that bean's residue
check reported **clean** — correctly, against the definition as it then stood,
because no rule told it to look at issues. The lead closed #25 with the operator
principal. A residue definition that enumerates classes is only as good as its
enumeration, and this is what a gap in it looks like from the inside: a green
check over an object nobody had thought to count.

**The asymmetry, measured 2026-08-10.** The lane credential can create an issue
and cannot remove one:

| Operation | Result |
| --- | --- |
| GraphQL `createIssue` | **succeeded** — issue #25 opened |
| REST `PATCH .../issues/25` with `state=closed` | **403** |
| GraphQL `closeIssue` | **200 with `FORBIDDEN`** — the shape [ADR 018](decisions/018-a-graphql-200-is-not-a-success.md) quotes |
| GraphQL `deleteIssue` | **200 with `FORBIDDEN`** |

Every direction that would clear the object is refused, and only the direction
that creates one is permitted. That makes an issue **worse than a branch, not
better** — and worse than a closed pull request too. A `fiddle/` branch is residue
the lane made and the lane removes. A closed pull request is residue the forge lets
nobody remove, but no credential was needed to reach that state. An issue is both
at once: a *lane* can create one, only an *operator* can even **close** it, and
**nobody can delete it** — `deleteIssue` is refused to this credential and GitHub
offers no path that would leave the repository as though the issue had never
existed. So it accumulates, with a person's attention required for each one, in a
repository whose whole argument is that its residue is the lane's own problem
rather than a person's. #25 is now permanent; see *After a run*.

**So the rule is abstention, not cleanup: a lane must not create an issue at
all.** Not "must clean up any issue it creates" — that is a promise this
credential provably cannot keep, and a cleanup step written against it would fail
on every run or, worse, be written to tolerate its own 403 and report clean. The
honest rule is the one that can actually be held: nothing here opens an issue, and
`createIssue` is not in any lane's vocabulary. Cleanup therefore has no issue
sweep, and its absence is deliberate rather than an omission — a sweep could not
have removed #25 either, so adding one would buy nothing and would imply a
capability the credential does not have.

### After a run

```sh
gh repo view peel/fiddle-effects-acceptance --json visibility           # PUBLIC
gh api repos/peel/fiddle-effects-acceptance/branches --jq '.[].name'    # exactly: main
gh pr list --repo peel/fiddle-effects-acceptance --state open           # empty
gh api repos/peel/fiddle-effects-acceptance/actions/runs --jq '.total_count'  # 0
gh issue list --repo peel/fiddle-effects-acceptance --state all         # exactly: #25, closed
gh api repos/peel/fiddle-effects-acceptance/actions/secrets            # 403, as above
```

The issue line is `--state all` rather than `--state open` on purpose: the rule
being checked is that no lane ever opened one, and a closed issue is evidence that
one did — closing it is what an operator had to do, which is precisely the residue
the rule exists to prevent. So `--state open` would answer "empty" and prove
nothing.

**The one entry it lists is expected, and a second one is a violation.** #25 is the
issue `fiddle-w0xt` opened on 2026-08-10 and an operator closed, described under
*An issue is residue*. It cannot be removed — `deleteIssue` is refused — so it
stays in this listing permanently, in the same way the closed pull requests above
do, and it is this rule's one grandfathered entry rather than a passing state. Any
issue numbered other than 25 means a lane created one after the rule said it must
not, which is the thing being checked.

Expecting "nothing" here would be the same mistake as a residue check that reports
clean because it was told to look at the wrong thing. A check whose stated expected
output is already false on the day it is written teaches its reader to stop reading
the comment.

The lane also creates one `mktemp -d` directory holding the whole disposable
project — the configuration document, the fixture roots, the clone, the reports —
and removes it in the same `trap`. It writes nothing outside that directory and
nothing outside this repository.
