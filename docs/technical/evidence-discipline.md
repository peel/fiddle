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

**Capture exit codes from the command itself, never through a pipe.** `echo $?` after a pipe reports the last
stage's status.

**A negative check must print its denominator.** "Found nothing" and "examined nothing" must not render
identically. M3: four checks returned an empty result because they ran from the wrong directory, and only the
denominator distinguished a clean bill from a broken check.

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
characters, and **deleting the byte truncation entirely left the suite green.** The lead's own correcting
comment named the discriminating case — *"3,000 star characters truncate to 2,046 bytes, which is 682
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

**Restore from a recorded manifest, not a directory listing.** A listing-based restore wrote back three files a
neighbouring lane had left in a shared scratchpad. Note also that `mkdir -p` cannot distinguish a directory it
created from one it found.

**Verify the restore with both `cmp` against the pin and `git diff --quiet`.**

### Granularity

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

**Do not rule on a report while its author may have moved past it.** Six crossings. Check the tree and name the
sha before ruling; a lane's "done" does not cover a request that crossed it.

**Do not dispatch evaluation against a moving tip.** Four stale dispatches. Give each evaluator its own
worktree at a pinned sha so implementation and evaluation overlap without drifting under each other — waiting
buys the cost of isolation without the benefit.

**Verify before correcting a lane.** Six lanes corrected the lead's reasoning in M3 and all six were right: a
stale baseline, a table audited against the wrong crate, an accessor claimed absent that existed, a
`created_at` claim propagated into a converged bean's record, a confirmed-but-false mechanism, and a grep that
could not see a receiver's type. **A lane's measurement beats the lead's assertion.** Say so in the dispatch,
because one lane that did not check published the lead's wrong number over its own correct one.
