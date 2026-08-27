# Antipatterns — general

## assertion-weaker-than-its-message (2026-08-25)

**Pattern:** A test whose message claims more than its comparison can deliver.
The test passes, a reader believes the message, and the property is unheld. It
is worse than no test, because the message stops anyone looking again.

**Example:** Six from epic `fiddle-qcch`. Five were caught before they shipped
and one shipped.

1. `assert_eq!(cases.len(), 6, "an arm was added without a case here")` over a
   six-entry literal array. It asserts a literal has its own length and passes
   at any enum size. Both enums had eleven variants.
2. Proving "no path returns `AwaitingDecision`" by grepping one source file for
   that string. `CapabilityError::Effect(#[from] EffectError)` propagates
   through `?` and `EffectError::HumanDecisionRequired` maps to
   `Recurrence::Awaiting`, so a workflow suspends while the string is absent.
   Measured: the inversion fails two behavioural tests and `grep -c` still
   returns 0.
3. A ```compile_fail doctest asserting `AuthorizedEffect` cannot be forged. It
   passes when the snippet fails to compile for **any** reason. Measured
   against the same break, trybuild reported `EXPECTED E0451` versus
   `ACTUAL E0432` and failed; the doctest reported `ok`.
4. A pinned payload written from the bean body rather than read off the build.
   Two of three pinned values were wrong, and making them pass would have
   silently moved the wire payload of a live effect.
5. `EffectDescriptor::PartialEq` comparing `name` and `minimum` while the test
   message said "the registry entry is the generated descriptor, not a second
   hand-written one". A hand-written twin with a different constructor compared
   equal. **This one shipped** and was found by holistic review.
6. `merge-scorecards.sh` computing `"pass": (all(.pass))`, where a null `.pass`
   is falsy in jq. A criterion that does not carry the field became a failure,
   and six criteria scored 9 against a threshold of 8 arrived as failures.

**Fix:** Write the case that fails if the check matches everything, and run it.
Prefer a mechanism that cannot be vacuous: an exhaustive `match` with no
wildcard makes a new variant a compile error; a `.stderr` pinned by trybuild
makes the reason part of the assertion; a round trip through one value cannot be
satisfied by two values written to agree. When a check compares over a
collection, pair each negative case with one that corrects only the named fault,
so a case cannot pass for another case's reason. Treat "absent" and "false" as
different, in tooling as much as in tests.

## a-record-corrected-in-isolation-overshoots (2026-08-27)

**Pattern:** A bean fixes a document, its per-task evaluator scores the change high, and
holistic review then fails the epic on the very text the bean wrote. The per-task
evaluator judges a bean against its own criteria and cannot see that the new wording
contradicts a record elsewhere in the same epic. Correcting an understatement in
isolation tends to land past the target rather than on it.

**Example:** From epic `fiddle-gyyo`, M5a.

The record said no Atlassian run had happened while the tree already contained one.
`fiddle-lzl5` fixed that at `ecde6a5` and, correcting the understatement, wrote into
ADR 077 that "Jira Cloud answers 404 for a private issue read with a bad credential ...
so no issue read reaches `JiraError::Unauthorized` or `JiraError::Forbidden`" — stated as
fact.

The epic's own iteration-1 record called exactly that an inference, not a measurement,
whose only observation was a 404 on `/rest/api/3/project/ISP` against the **wrong
tenant**, and `fiddle-2n67` existed because the measurement was untaken. ADR 077 sorts
its other claims into "now a measurement" and "still an argument" in the same section,
and mis-sorted this one.

The per-task evaluator scored `fiddle-lzl5` correctness 10, domain_spec_fidelity 10,
code_quality 9, and did not catch it. Holistic review did, and failed coherence 6 against
a threshold of 7. `fiddle-bsow` then re-graded the claim as an argument, and coherence
cleared to 7. Six probes on 2026-08-27 later measured the behaviour, so `fiddle-2n67`
re-graded it a third time, upward to a measurement.

Three corrections to one passage, in one epic. The first two were both right on the
evidence available when written.

**Fix:** When a bean corrects a record, give its evaluator the neighbouring claims the
record already grades, not only the bean's own criteria. Ask specifically whether the new
sentence claims more than its evidence, since the failure mode of fixing an understatement
is overshoot rather than a return of the original fault. Where a document sorts its claims
by evidence class — measured, argued, inferred — a change to one claim must state which
class it now belongs to and why, and the evaluator must check that grading rather than
only that the text changed.
