# Evidence discipline

Rules for making and checking claims in this repository, each one earned by a specific defect. **Dispatches
should point at this file rather than restate it** — M3 transmitted these by copy-paste into every dispatch,
roughly 700 lines of it, and the lead's own transcription errors propagated three times: a step count, a crate,
and an accessor claimed absent that existed.

Every rule below has cost something. The evidence is named so a reader can judge the rule rather than obey it.

## 1. Measurement

**A truncated log is not a small log; it is a different log.** M3: a baseline captured through `| tail -120`
discarded 34 of 42 result lines and reported `binaries=8 passed=0` — a catastrophe that was an artefact of the
pipe. Write an `EXIT=` marker after every run and check for it before reading any figure. The lead read a
partial log as final **six times** and was caught by a missing marker each time.

**The marker goes into the log, not into the console.** Of five inversion drivers in M3, two echoed the exit
code to stdout instead, and **158 of 194 inversion-shaped logs carry no `EXIT=` marker** as a result. That
figure was first recorded here as "79 of 132" — wrong on both numbers, because the count globbed `inv-*.log`
while the same directory held 53 `inv_*.log` and 6 `8vpm-inv-*.log`, all equally markerless. **A hyphen
against an underscore**, which is §2's own rule about patterns and structure, committed into the file that
states it. For a single-lane run the
absence is recoverable, since one `test result:` line is the whole run; for a 19- or 42-binary run it is not,
and one such log measured nothing at all — zero results, ending in `warning: build failed`. A status that
survives only in a transcript is not in the evidence pack.

**An inversion's result is the lane's exit code, not the driver's.** A driver that runs several lanes and
then exits cleanly writes `EXIT=0` for itself while individual lanes exited 101. M3: the lead grepped `^EXIT=`
across eight inversion logs, got zero from all eight, and would have reported **every one as a null** —
the exact opposite of what they found. Write the per-lane code under a distinct name (`LANE_EXIT[<lane>]=`),
and say in the report which field is the result, because a reader who greps the obvious one gets the
inverse of your finding.

**Capture exit codes from the command itself, never through a pipe.** `echo $?` after a pipe reports the last
stage's status.

**A negative check must print its denominator.** "Found nothing" and "examined nothing" must not render
identically. M3: four checks returned an empty result because they ran from the wrong directory, and only the
denominator distinguished a clean bill from a broken check.

**"Found nothing" fails hardest when you looked in one place.** M3's audit reported one bean's inversion
artefacts as "none"; they were on disk under `scratchpad/usp7/` as `inv-I1`…`inv-I8` with a manifest and a
restore script, while the audit looked for `inv-usp7/`, the naming its own lane used. The absence was
published inside the section auditing other beans for exactly this, and the artefacts **supported** the
conclusion it had already reached another way.

**A filtered count reports both numbers.** *"5 hits, of which 4 are `String::as_str` on a different type"* —
never the filtered count alone. M3: a lane reported "exactly one hit" from a grep returning five, one message
after warning the lead about exactly this.

**Re-derive the figure that goes in the record; do not re-derive the three before it.** Intermediate gate runs
are the largest avoidable cost in the loop.

## 2. Claims

**Agreement is not verification.** When a lane proposes a mechanism and the lead confirms it, the claim has
been examined by two readers and tested by nobody — and it now *reads* as corroborated. M3: a moved-head
mechanism was claimed, agreed, and wrong; `panic!` at the entry of the function the claim named refuted it in
one line, failing 7 of 22 tests while the test in question passed.

**A mechanism claim ships with the mutation that would refute it.** `panic!` at a function's entry, then see
which tests notice. It is the cheapest check available and it settled two agreed-but-false claims.

**An assertion that reduces to one already made above it cannot fail.** M3: a helper asserted
`reads + writes + graphql == total()` where `total()` is defined as those three plus `unclassified` — so it
reduced to `unclassified == 0`, which the line immediately above already asserted. It read as a completeness
check and was decoration. The replacement that *can* fail is the one comparing the partition to its source:
`total() == world.requests().len()`, which catches a dropped entry — and nothing else in that test could,
because dropping one bucket's entries leaves every other bucket correct. **Ask what mutation this assertion
fails on that its neighbours do not.** If there is none, it is a restatement.

**Cite ranges, not lines, and open what you cite.** A single-line citation of a multi-line sentence points at
something true while the claim around it is false. Seven stale or misaimed citations occurred in M3, four of
them the lead's.

**A pattern matched against a token cannot see the structure the token sits in.** In M3 this failed four ways:
a grep could not tell a **renamed** test from a deleted one (twice), counted a criterion id mentioned in order
to be **excluded**, matched `m3-design` inside the filename `agentic-factory-m3-design.md`, and conflated
`Ignored::as_str` with `String::as_str`. **Grep for what would be wrong rather than for what should be right**
— the negative form has no false negatives — **and grep for the receiver, not the method.**

**Name the sha you checked when you report state.** Then a stale observation announces itself in one line
instead of two readers reasoning about a tree neither is looking at. M3: the lead ruled on superseded reports
**six times**.

## 3. Inversion

**The rule, in two forms. The second was found late and caught what the first missed.**

> **Any fixture value that only appears where its value cannot matter is not tested.** It is merely *consistent
> with* the tests. A stub returning `vec![]` satisfies it forever.
>
> **Any outcome two different causes produce identically is not an assertion about either of them.**

The first form found eight nulls in one bean. The second found three the first missed — a struct field, a
constant, and a whole matrix: five rows asserted a shape (`Unclear` → `AwaitingDecision` → exit 10, nothing
mutated) that **every transport failure also produces**, so a correct refusal and a broken adapter were
bit-for-bit indistinguishable and 21 tests passed over it.

**Fixing an assertion's units does not fix an input that cannot exercise the difference.** M3: a test asserted
`chars().count() <= 2_048` against a cap documented in bytes; the lead corrected it to `len()`, which is
strictly better — and the rows it asserts over are pure ASCII, so the byte form and the character form agree on
every one of them. Two caps of 2048 exist, `REDIRECT_INSTRUCTION_LIMIT` in bytes and `PUBLISHED_TEXT_LIMIT` in
characters, and deleting the byte truncation entirely left **the acceptance lane** green — it was written up
here as "left the suite green", and that is this document breaking the filtered-count rule stated one section
above it. `-p fiddle-runtime --test interpretation` fails **2 of 8** under that same mutation, measured twice
at two shas by two lanes. **A per-lane null is not a suite null, and the sentence that says otherwise was in
the file that forbids it.**

**And the runtime-tier half of that cap is held by an accident of spelling.**
`a_redirect_instruction_is_capped` feeds `"z".repeat(10_000)` — pure ASCII, so both caps agree on it — and
fails by **2 bytes**, 2050 against 2048, only because the truncation marker `Published::of` appends contains
one `…` (U+2026, three bytes) inside a bound counted in characters. **Respell that marker `"..."` and the
assertion goes green under the mutation it exists to catch.** Its sibling `a_cap_never_splits_a_character`
fails by 6070 against 2048 and is the real guard. An assertion whose margin comes from another crate's
constant should say so at the assertion.

The lead's own correcting comment named the discriminating case — *"3,000 star characters truncate to 2,046 bytes, which is 682
characters"* — and the row was never written. **When a fix turns on a distinction, the input has to make the
distinction visible; otherwise the assertion is a value appearing only where its value cannot matter, and the
fix's own justification is the test it is missing.**

**A mutation that moves a constant without moving the relation proves nothing.** M3: to demonstrate a tripwire
over two colliding comment ids, the lead suggested shifting the base constant from 9000 to 9500. The collision
is relative arithmetic, so every id shifted together and the collision survived — the lane nearly recorded that
as a pass. Mutate the relation the claim depends on, not a number the claim is expressed in.

**When an inversion is masked by an earlier assertion, neutralise the mask and declare it.** M3: a lane needed
to show a receipt-list assertion was load-bearing, but four assertions above it failed first and would have
taken the credit. It neutralised those four in the mutation script and said so in the script, *"because
otherwise I would have reported the receipt claim load-bearing on no evidence."* An inversion attributes its
catch to whichever assertion fires first, so a mutation that reaches an assertion only through others proves
something about the others.

**An accessor asserted only *empty* needs a positive case beside the negative.** Three in M3 were read solely
by a negative assertion, so a version answering "nothing" unconditionally passed everything.

**Closing a null with a table you wrote yourself closes half of it.** A table driving every arm of a
partition proves the sorter behaves as the table says; it cannot prove the table's expectation matches
reality, because the same hand wrote both. Pair it with one assertion over a **real** invocation — M3's lane
re-sorted its live log after dispatching a genuine GraphQL mutation by hand. Correspondingly, one real call
cannot cover four arms. **Neither surface alone is the property.**

**A null is only as wide as the lanes behind it.** M3: one mutation ran green in four lanes a reader would
name — the library, the protocol lane, the capability lane, the acceptance lane — and red in a fifth, 2 of 26.
The record and the bean filed from it both reported the null without naming the lane that refutes it. **Report
the lanes a null was taken over, not only its count.**

**A null result is a finding to report, not a problem to hide.** M3's beans reported 8, 7, 4 and 0 nulls; every
one was either closed or recorded with its reason. Two were found by attacking a *proposed fix* rather than the
code.

**A null reported unclosable must name its construction *and its surface*.** *"Unclosable while `SEEDED_AT` is
the only value `comment_from` can read"* is checkable by the next lane; *"unclosable"* is not. M3: a second
value was added three beans later, the lead concluded the null was closable, and it was not — the value reached
a different reader. Two files then gave opposite answers about one field name, neither false.

**An unreachable fail-closed guard is worth keeping when its absence would default to a behaviour forbidden
elsewhere** — but it must say so, rather than documenting a case that cannot occur.

### Hygiene

**Pin fresh immediately before each mutation and never reuse a pin.** A reused pin silently reverted committed
work while the `cmp` guard passed, because the guard compares the tree to the pin and says nothing when the pin
is stale.

**Delete a pin after a verified restore, or mark it `SPENT`.** A pin that outlives its run is a loaded gun
with no safety: M3's shared scratchpad still holds `inv-565u/pin/` and `inv-565u/pristine/` where
`human_direction.rs` is 63,185 bytes against 176,941 in the tree, and `inv-9krm/` is an entire copy of the
repository. A restore loop pointed at any of them today reverts five files with `cmp` passing.

**Restore from a recorded manifest, not a directory listing.** A listing-based restore wrote back three files a
neighbouring lane had left in a shared scratchpad. Note also that `mkdir -p` cannot distinguish a directory it
created from one it found.

**A cleanup counts what is left, not what it deleted.** M3's live lane proved this by mutating its own
sweep to select the author's user id instead of the comment's: the walk still passed, and the sweep printed
`found 2, deleted 0, left 2` and exited 1. A sweep that reports its deletions cannot distinguish a successful
delete from a delete of nothing; a sweep that asserts the remaining count can. The same lane also found that
the mechanism its own bean gave for this defect was wrong — the payload's key order puts `id` *before*
`user.id`, so scraping the first id-shaped field happens to be right — while the consequence was real and
re-measured, `DELETE` by the user id answering 404 with the comment still listed. **A claim's consequence can
be real while its stated mechanism is false, and only the consequence is worth carrying forward.**

**Verify the restore with both `cmp` against the pin and `git diff --quiet`.**

**One worktree, one lane, and a bean is not finished until its lane stops editing.** M3's worse instance: the
lead dispatched an auditing lane into a worktree, then asked a supposedly-finished lane for four more edits in
the same tree. The second lane sampled `git status` every three seconds, caught product files cycling
mutate → test → restore, and **refused to edit until told the tree was its own** — correctly, and its reasoning
is the general statement: *a scratchpad collision costs a driver, which is replaceable; a working-tree collision
reverts committed work while both `cmp`-against-pin guards pass*, because each guard compares the tree only to
its own pin and is silent about another writer. **Reopening a bean means reassigning its worktree, or giving it
a new one.**

**The two directions are asymmetric, and only one of them has a guard.** A neighbour's edit that your restore
would revert **is** caught — by `git diff --quiet` after the restore. A neighbour's *commit* that captures
your live mutation is caught by **nothing**: after the commit the mutated file *is* `HEAD`, so `cmp` against
the pin passes and `git diff --quiet` passes. Defending against it means re-reading `git rev-parse HEAD` after
every restore and comparing it to the sha you pinned at. The lane that found this records the sha per
inversion and does not compare it — which is how it knew the gap was there.

**`git commit --only <path>` is necessary and not sufficient.** It prevents *staging* a neighbour's
mutation, which is why four docs commits landed inside two running inversion exercises in M3 without
corrupting either. It does **not** stop the pre-commit hook: `prek` stashes every unstaged change, runs the
hooks, and restores. The lead committed a docs file while a lane had **189 uncommitted lines** in the same
worktree and watched `Unstaged changes detected, stashing` scroll past — that work spent the interval in
`.devenv/state/prek/patches/*.patch` and came back only because nothing failed. **The rule with no caveat: do
not commit in a worktree where another lane has uncommitted work.** A bare `git commit -a` is worse still,
staging the mutation outright so it becomes `HEAD` where no pin-guard can see it. This rule was stated as
sufficient twenty minutes before being demonstrated insufficient; it is the third refinement of the same rule
in one afternoon, and each came from overstating the previous.

**Namespace your scratchpad by bean, and never write a driver to its root.** M3: an evaluator was given its own
detached worktree but not its own scratchpad, wrote `invert.sh` to the shared root, and replaced the
implementation lane's driver — a file that bean's record cites as evidence. Its logs and mutation scripts
survived because their names differed; the driver did not. **A worktree is not isolation on its own.** The lane
that got this right namespaced everything under `inv-<bean>/` and collided with nobody.

**The scratchpad path is keyed by worktree, so one session has different scratchpads in different worktrees.**
M3: the lead assessed this very collision in the wrong directory — same session id, different worktree key —
found an untouched file belonging to a third lane, and concluded no collision had occurred. Then a
time-filtered `find` returned nothing and was read as absence. **Both are the same error the measurement
section already names: a check that could not evaluate, reported as a negative result.**


### Granularity

**A cold `target/` and a cold `sccache` are different costs, and conflating them made isolation look
unaffordable.** M3's dispatches all carried "a fresh worktree costs a ten-minute cold build", which was the
standing objection to giving every lane its own. Measured in a worktree with no `target/` at all:
`gate.sh --full` ran **fmt 1s, clippy 5s, test 116s**, plus release build and `nix flake check`, and the
sccache counters read **697 hits in 1,959 requests — 50.4% overall, 66.5% on C/C++** with zero errors. So a
fresh worktree costs about **two minutes**, and the ten-minute figure was a cold *sccache*.

This also resolves an earlier measurement that looked like its opposite: sccache served **1 hit in 32** across
two target dirs, and 19 hits with a 5.5× speedup at the same path. Both are right and they compose — dev-profile
debuginfo embeds absolute paths, so **Rust** crates miss across paths while the **native C/C++**
dependencies hit, and those are the expensive half of a cold build. The useful form: *sccache does not help a
Rust rebuild at a new path, and does help the half of a cold build that dominates its wall clock.*
**A private worktree per lane is affordable, which makes it the cheap fix for the collision hazard above
rather than an expensive one.**

**Run each inversion against the binaries that can observe the mutation, and the full gate once at the end.**
M3 ran 47, 27, 22 and 17 inversions per bean, each as `cargo test --workspace`. A mutation to
`capability/propose.rs` cannot affect the other 40 binaries; `human_direction` takes 35s and
`propose_capability` 11s against multiple minutes for the workspace. This was the single largest cost in the
milestone.

## 4. Scope

**A bean does not invalidate a converged sibling's property as a side effect.** Held five times in M3 and right
every time. When a fix genuinely requires it, that is the *purpose* of a separate bean — reword the property
there, do not delete it, and state in its doc comment which half changed and why.

**If the lane adding a seam cannot name a production caller a test can reach, the seam is debt on arrival.**
M3 accumulated four inert surfaces — `RequireHumanDecision`, `execute_decided`, `DecisionTrace`,
`IgnoredReply` — each added one layer above its caller in the same commit as the thing it was for. All four were
later discharged by the bean that actually needed them, which is how it should go.

**A criterion that cannot be satisfied at a bean's tier is a partial discharge, not a failure** — declared with
its reason, not discovered by an evaluator. Accepted three times in M3. Prefer a **tripwire** to a TODO: an
assertion pinning current behaviour with a message naming what to write when it becomes assertable, so it fails
the day the blocker is removed.

**A criterion must not embed an absolute suite count.** The figure moves whenever any lane adds a test. M3
inherited `443 passed / 34 binaries` from the design stage into **twelve** places including a criterion, where
it survived an annotation and had to be rewritten. State the command and a delta against a baseline the bean
measures itself at a named sha.

## 5. The record

**When a change falsifies a comment, grep for the sentence, not the file.** One claim in M3 was written in four
places across two crates when the bean named two. Another survived three rounds of review because a duplicate
of it was **true of a different caller** — the sentence was true of the function and false of the path.

**Show the false version, marked, rather than silently swapping it.** A reader needs to know a claim moved.

**A tally is decoration; the line is the claim.** Three tallies in M3 were wrong while the conclusion they
supported held. Prefer citing the line, which either says what it is cited for or does not, and does not change
when a neighbour commits.

**The bean is the evidence pack.** In M3 an implementer terminated mid-inversion without reporting; its
baseline, justification, reused-versus-built table, stated limitation and fifteen reproducible inversion logs
with pins and manifests all survived, and the lead ran its unrun mutation with its own driver. **A dead agent
and lost work are different things, and the bean is the difference.**

## 6. The lead's own failure modes

Recorded because they cost more wall-clock in M3 than any code defect.

**Do not rule on a report while its author may have moved past it.** **Eight** crossings, three of them
consecutive and all three after this rule was written down — each one a message assuming a lane had not
finished when it had. Writing the rule did not stop it; the lanes caught it every time. Check the tree and name the
sha before ruling; a lane's "done" does not cover a request that crossed it.

**Do not dispatch evaluation against a moving tip.** Four stale dispatches. Give each evaluator its own
worktree at a pinned sha so implementation and evaluation overlap without drifting under each other — waiting
buys the cost of isolation without the benefit.

**A fresh worktree cannot commit until it has entered the dev shell once.** `git commit` fails there with
`config file not found: .pre-commit-config.yaml`, because that file is generated by `devenv:git-hooks:install`
on shell entry. Use `nix develop -c git commit`, which fixes it *and* runs the hooks. `PREK_ALLOW_NO_CONFIG=1`
silences the same error by skipping the hooks, so it is the wrong fix. Say this in the dispatch when handing
out a new worktree; three M3 lanes hit it and each spent turns on it.

**Verify before correcting a lane.** Six lanes corrected the lead's reasoning in M3 and all six were right: a
stale baseline, a table audited against the wrong crate, an accessor claimed absent that existed, a
`created_at` claim propagated into a converged bean's record, a confirmed-but-false mechanism, and a grep that
could not see a receiver's type. **A lane's measurement beats the lead's assertion.** Say so in the dispatch,
because one lane that did not check published the lead's wrong number over its own correct one.
