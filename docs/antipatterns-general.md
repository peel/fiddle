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
