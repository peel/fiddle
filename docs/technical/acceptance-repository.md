# External acceptance repository

The M0 milestone is proved twice: once in-tree, and once from outside this
repository entirely. This document records the external half — what
`peel/fiddle-acceptance` is, why it is public, and what running it asserts.

## The repository

| | |
| --- | --- |
| Repository | [`peel/fiddle-acceptance`](https://github.com/peel/fiddle-acceptance) |
| Visibility | **public** |
| Default branch | `main` — the only branch |
| Scenario | `scenarios/m0_skeleton.sh`, mode `100755` |
| Contract | `FIDDLE_BIN` names the compiled `fiddle` binary; `jq` must be on `PATH` |
| Secrets | none — no repository secret, no deploy key, no token of any kind |

It exists because a proof that can only be run from inside this repository, by
someone holding these sources and a Rust toolchain, is a weaker proof than one
anybody can run against a released binary. `scenarios/m0_skeleton.sh` links
against nothing: it observes exit codes, `--json` payloads, and files on disk,
which is exactly the surface a caller at a shell has.

## Why public

The repository is public **so that checking it out needs no credential**.

M0's hard constraint is that the acceptance lane is never gated on a secret. A
private acceptance repository would have to be reached from CI with a cross-repo
personal access token, which contradicts that constraint outright and, worse,
would make the external lane unverifiable by anyone except the token holder —
including on a fork, where the secret does not exist. Public keeps
`actions/checkout` credential-free, so the lane is reproducible by anyone.

The repository holds no secrets and never will: it contains black-box scenarios
and a README. That is a standing rule for the repository, not an accident of its
current contents.

The credential-free claim is proved rather than asserted — this clone carries no
credential helper and succeeds:

```sh
git -c credential.helper= clone https://github.com/peel/fiddle-acceptance.git
```

## Running it

```sh
cargo build --release
FIDDLE_BIN="$PWD/target/release/fiddle" bash /path/to/fiddle-acceptance/scenarios/m0_skeleton.sh
```

On success the scenario prints `m0_skeleton: PASS` and exits 0. On failure it
prints `m0_skeleton: FAIL: <what was expected>` on stderr and exits non-zero.

`.github/workflows/acceptance-repo.yml` runs exactly that, as the named step
*M0 acceptance scenario (external, credential-free)*: it builds the release
binary, checks out `peel/fiddle-acceptance` into `acceptance/` with no `token:`
and no `ssh-key:`, and invokes the scenario.

## What it asserts

The scenario is the *same proof* the in-repo lane runs
(`cargo test -p fiddle-acceptance --test m0_skeleton`, documented in
[SYSTEM.md](SYSTEM.md)), not the happy path alone. It is one ordered walk
sharing a single fixture project from the first step to the last, so each step
observes the world its predecessor left:

1. **Configuration.** `config check --json` reports `status: "valid"` and echoes
   the project identity. A document with an unknown key exits **2** and the
   diagnostic names the key, says `unknown field`, and points at its line.
2. **Read-only inspection.** `inspect --json` echoes the invocation reference and
   scheme, observes the work item as available and the change set as unmarked,
   assesses `not_started`, and derives `execute stub_mark`. The stub root's
   path-and-digest snapshot is unchanged afterwards, and `<report.dir>` does not
   exist — looking neither mutates fixture state nor publishes evidence.
3. **Failing closed.** With the stub root moved away — moved rather than emptied,
   because an emptied root is still readable and would exercise "the world is
   empty" instead of "I cannot see the world" — `run` exits **20**, reports
   `observations.work_item` as `unavailable` rather than degrading it to an empty
   value, carries a typed `failed` outcome whose error names the unavailable
   source, derives a `blocked` next action with a non-empty reason, and executes
   nothing: `capability_executions` and `progress` are both empty. The root is
   put back afterwards, so the steps that follow observe the same fixture.
4. **Execution and evidence.** `run --json` completes, executes `stub_mark`, and
   names a bundle that exists on disk. The bundle declares
   `schema: "fiddle.report.v0"`, carries build identity
   (`fiddle.package_version` matching `x.y.z`, `fiddle.source_revision` a
   40-hex sha or `unknown`), records `invocation_ref`, `work_ref`, `attempt_id`,
   `mode`, the typed outcome, the derived next action, the capability execution,
   and a `progress` entry at stage `mark`. The 16-hex marker the bundle reports
   is the one present in the change set on disk.
5. **Stability across a fresh process.** A second `run` leaves the stub bytes
   identical, executes nothing, reports empty `progress`, and still completes.
   Exactly **one** marker file exists. The second bundle carries a **distinct**
   `attempt_id` and the **same** `work_ref` — a real second attempt over the same
   work, not a replay.
6. **Credential independence.** Steps 1–5 run with `GITHUB_TOKEN`, `GH_TOKEN`,
   `ANTHROPIC_API_KEY`, and `JIRA_API_TOKEN` unset, showing fiddle does not
   *need* them. The same command with all four present returns the same answer
   and leaves the same bytes, showing it does not *consult* them either.

## Cleanup and residue

The scenario creates one `mktemp -d` directory and removes it in a
`trap ... EXIT INT TERM`, so a failing or interrupted run leaves no more residue
under `$TMPDIR` than a passing one. It writes nothing outside that directory and
never touches the network.

M0 creates no pull request and no branch in the acceptance repository. After a
run, all of the following hold:

```sh
gh repo view peel/fiddle-acceptance --json name,visibility   # visibility PUBLIC
gh pr list --repo peel/fiddle-acceptance --state all         # empty
gh api repos/peel/fiddle-acceptance/branches --jq '.[].name' # exactly: main
gh api repos/peel/fiddle-acceptance/actions/secrets --jq '.total_count'  # 0
gh api repos/peel/fiddle-acceptance/keys --jq 'length'       # 0
```

## Keeping the two lanes together

The in-repo scenario (`crates/fiddle-acceptance/tests/m0_skeleton.rs`) and the
external one assert the same six properties, in the same order, by design. The
numbered list above is that shared contract: each entry describes one step both
lanes walk, and the two lanes are checked against it — and against each other —
by hand.

A change to the CLI surface that alters what one lane observes must be reflected
in the other, or the lane left behind silently becomes the weaker proof — which
is the one failure mode this arrangement exists to prevent. That is not a
hypothetical: the in-repo lane once lacked step 3 entirely, so the command CI
names as the M0 acceptance scenario, and later milestone seeds run as their
regression baseline, never exercised the fail-closed rule at all.

The expression of a property may differ where the two languages differ — `jq`
regexes against hand-rolled character checks, a `sed`-inserted key on line 3
against a `replacen`-inserted one on line 2 — as long as the property asserted
is the same one. What must never differ is the *set* of properties.
