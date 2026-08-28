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

## What lanes do not change

The evaluator loop is unchanged. Each bean still runs `fiddle:develop-loop`,
still needs its eval log initialised against its own base, and still converges on
its own criteria. Parallelism changes when implementers run, not what counts as
evidence.
