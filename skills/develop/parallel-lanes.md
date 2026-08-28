# Parallel lanes

Run independent beans at the same time, each in its own worktree. Merge them one
at a time and gate the merged result.

Sequential execution costs one full build per bean. An epic of sixteen beans
spends most of its wall clock waiting for the same 317 dependencies to compile
again. Lanes remove that wait. They do not remove the need to prove the parts
integrate.

## When a wave is legal

Two beans may share a wave only when both hold:

1. **No dependency edge.** Neither is in the other's `blocked_by`, directly or
   transitively.
2. **Disjoint files.** Read each bean's `## Files` section. Two beans that name
   one file belong in different waves, whatever the tracker says.

Derive the waves from the edges rather than from the plan's task numbers. The
plan orders by narrative; `blocked_by` orders by fact.

When a bean's `## Files` is absent or vague, treat it as overlapping everything
and run it alone. A guess here is paid for at merge time.

## Setting up a lane

```bash
BASE=$(git -C <epic-worktree> rev-parse HEAD)
git worktree add ".worktrees/lane-<bean>" -b "lane/<epic>-<bean>" "$BASE"
```

Every lane in one wave forks from the same `BASE`. Record it: each lane
reconciles its test-count delta against its own base, and a later wave forks
from a different commit with a different baseline. One global number is wrong
the moment the second wave starts.

## What a lane agent must be told

- The worktree path, the branch, and the base commit.
- **The baseline totals for its own base**, not for the epic branch tip.
- A **private scratch directory**. Not the shared session scratchpad.
- That it must never `cd` outside its worktree to run a build.
- Which shared files its siblings will also touch, and what to leave alone.

The last point prevents most conflicts. When three beans each add one `pub mod`
line to one module, say so, and say which other shared file belongs to which
sibling this wave.

## The isolation that is not enforced

A lane worktree isolates the build. It does not isolate anything else.

Agents that share a scratch directory overwrite each other's files. A lane once
executed a sibling's probe script by a shared path; that script began `cd` into
the sibling's worktree and ran `cargo test --workspace`. It did nothing only
because an unset variable broke its redirects. One variable stood between that
and two builds against one working tree, and the resulting gate numbers would
have looked ordinary.

Give each lane its own directory. Quarantine loose scripts at the shared root.

## Disjoint files are necessary and not sufficient

Two lanes can pass the wave-legality test above and still collide, because the
collision is in a name rather than in a file. Both defects below came from one
merged wave, and neither is attributable to any lane: each was correct alone.

**A name defined twice in one shared file.** Two lanes each added an identical
test helper to different regions of a file both legitimately touched. Git merged
them with no conflict, because textually there was none. The result did not
compile: `E0428` the name is defined multiple times, and `E0119` conflicting
trait implementations.

**A process-global claimed twice.** `install()` is a `OnceLock`, one extension
set per test binary. Each lane had its own install site, alone in its own
binary and correct there. Merged into one binary, the first won, the second got
`AlreadyInstalled`, panicked, and **poisoned the `Once`** — so eight tests failed
with `Once instance has previously been poisoned`, seven of them innocent.

Run `cargo check --workspace --all-features --all-targets` on every merged
result before the gate. Plain `cargo check` does **not** build test targets, so a
duplicate inside a test file passes it while the tree cannot compile its tests.
That check costs seconds and catches the first defect.

Nothing short of running the tests catches the second. That is the argument for
gating every merged wave rather than trusting arithmetic across green lanes.

## Gates prove what they ran on

A lane's gate proves that lane. It says nothing about the wave, because the lane
does not contain its siblings' commits.

Merge one lane at a time into the epic branch. Run one full gate on the merged
result. That number is the milestone's; the per-lane numbers are evidence about
parts.

Merge only what a lane has **reported** as done. A lane still working may amend,
and an amend cannot reach a merge that already happened.

## Failure modes to expect

| Symptom | Cause | What to do |
|---|---|---|
| Agent returns one sentence, no report | It backgrounded a build and its turn ended | Inspect the worktree; uncommitted work is usually there; resume it |
| Provider dispatch writes an empty file and exits 0 | The provider CLI reads its prompt from stdin, and a backgrounded process has no stdin | Dispatch providers in the foreground |
| Lane reports numbers it cannot defend | It raced its own gate, or read a log with no completion marker | Re-run one gate on a quiet tree |
| Merge conflicts in one shared module | Expected when siblings each add a line | Merge sequentially and re-gate; do not batch |
| Merged tree compiles but tests do not | `cargo check` skips test targets | Use `--all-targets` on the merged result |
| Merged tests fail on a process-global | A `OnceLock` or similar claimed by two lanes | One claim site per binary; only a test run finds it |
| Every lane fails at once, linker says `No space left on device` | Accumulated `target/` directories | Reclaim merged lanes; a lane needs ~8 GB |

An agent that stops without a report is the dangerous one. Its work exists and
is uncommitted, and a completion notification looks the same as a real one. Read
the worktree before believing the turn ended cleanly.

## Making lanes affordable

Each worktree carries its own `target/`. Seven lanes measured at roughly 31 GB
of duplicated artifacts, and each paid a cold compile of identical dependencies.

The devshell sets `RUSTC_WRAPPER` to `sccache` with `CARGO_INCREMENTAL=0`, so
lanes share compilation across worktrees. Measured: a worktree with an emptied
`target/` rebuilt at a 100 percent cache hit rate and zero misses.

A shared `CARGO_TARGET_DIR` is the wrong answer. Cargo locks the target
directory, so lanes would serialise on it, and the one-build-per-tree rule would
be broken invisibly rather than made impossible.

**sccache removes the recompilation, not the duplication.** Every lane still
carries its own `target/`, and a full gate grows one to 6-8 GB. Three concurrent
lanes therefore need about 21 GB of headroom, and a milestone that opens waves
without reclaiming finished ones accumulates all of them.

Measured, and the reason this paragraph exists: a milestone reached 100 percent
disk with roughly 62 GB across eleven worktrees. Three lanes had finished their
work; one had not yet committed it. Every build failed, and so did the harness
itself, so the failure could not be cleared from inside the session.

Two rules follow.

- **Delete a lane's `target/` when its branch merges.** Better, remove the whole
  worktree: `git merge-base --is-ancestor lane/<name> HEAD` proves the commits
  are in the branch first, and `git worktree prune` tidies the metadata.
- **Check free space before opening a wave**, and size it against 8 GB per lane.

A lane that fills the disk fails in a way that looks like a code failure. The
gate reported `0 binaries of an unknown total` and a linker error reading
`No space left on device`; the refusal was correct, but the cause was
housekeeping and nothing in the output said so.

## What lanes do not change

The evaluator loop is unchanged. Each bean still runs `fiddle:develop-loop`,
still needs its eval log initialised against its own base, and still converges on
its own criteria. Parallelism changes when implementers run, not what counts as
evidence.
