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

## a-record-passes-on-its-shape-not-its-sentences (2026-09-04)

**Pattern:** A criterion over a record asks whether a section exists and says the
right kind of thing. The evaluator reads the section, finds that kind of thing,
and passes. Nothing asks whether the sentences are true, and the citation check
cannot: `scripts/check-adr-cites.sh` proves the symbols a sentence names resolve,
not what the sentence says about them. A false count, path or call order then
stands under a green gate until a holistic review re-derives it.

**Example:** Two sentences from `fiddle-ihl1` in epic `fiddle-t1zi`, M5c, both
admitted at `ce29b22` and both corrected by lane `fiddle-xwhz` at `2f2c8dc`. The
bean's eval block held seven criteria and no thresholds, so it converged on
criteria alone, and all seven passed.

1. `docs/technical/RUNBOOKS.md`, in the toil section that satisfied
   `an-operator-procedure-for-the-toil-route-exists`: "A ticket the route
   completed carries the run's correlation marker" and "Nothing on an operator
   surface clears a completion." The tree at that HEAD:
   `WorkflowCapability::record_change_set` writes the marker to
   `self.ports.stub_root.join(format!("changes/{work_id}.json"))`, at
   `crates/fiddle-runtime/src/capability/workflow.rs:445`, so the ticket carries
   nothing and the completion is a file under the deployment's `[stub] root`.
   ADR 083 in the same tree says
   `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail`
   "removes that record first, so the run works the ticket again". The criterion
   listed what the section must name. The section named it.
2. `docs/technical/decisions/083-toil-is-a-document-and-a-gate.md`, in the
   paragraph that satisfied `the-agent-precondition-is-recorded-or-removed`:
   "The thirteen rules are now applied in two halves. `toil::deterministic`
   applies the nine that read the tracker's own fields and needs no model.
   `toil::review_of` applies the three the ambiguity review decides". Nine plus
   three is twelve. `RULES` in `crates/fiddle-runtime/src/toil/qualify.rs` is
   `[&str; 13]`. `toil::deterministic` holds nine rules, `toil::review_of` holds
   three, and `toil::recheck` applies the thirteenth, `TICKET_HELD_ITS_REVISION`,
   which is in neither half. The criterion asked that the precondition be
   recorded or removed. It was removed, and
   `a_ticket_the_deterministic_rules_refuse_is_refused_on_a_deployment_that_configured_no_model`,
   the test it asked for, exists.

Every symbol in both sentences resolves. `2f2c8dc` records that "all three stood
under a green `ADR CITES: 0 unresolved` and a green `TOIL GATE ORDER: ok`".

The rule under Record Changes in `skills/evaluate/evaluator-general.md`, held
against the three earlier instances of the same class:

- `fiddle-eif4`, ADR 083. The clause "`toil::qualify` runs in
  `crates/fiddle-cli/src/main.rs`, in `qualified`, before the document is
  loaded" was written at `e22b212`, graded ARGUED and "read off the source at
  `77b82f6`". At `77b82f6`, `main.rs` calls `selected_workflow` at line 1564 and
  `qualified` at line 1571. Caught: a call order re-derived at the cited
  revision is two line numbers. The correction at `6423bd9` kept "`RULES` holds
  the thirteen rule names it applies" on the changed line, and `toil::recheck`
  had applied the thirteenth since `33fdb4e`. Caught too: thirteen entries in
  `RULES`, twelve applied by `toil::qualify`.
- ADR 077, first correction: `fiddle-bsow` at `762def6` over `fiddle-lzl5` at
  `ecde6a5`. The sentence "Jira Cloud answers 404 for a private issue read with a
  bad credential ... so no issue read reaches `JiraError::Unauthorized` or
  `JiraError::Forbidden`. `docs/technical/RUNBOOKS.md` records that behaviour"
  states a site's behaviour, which no count, path, symbol or call order in the
  tree can settle. Not caught by those four. Caught by the grade clause: the
  sentence stands in a section sorted into measurements and arguments and names
  its artifact, and `RUNBOOKS.md` at `ecde6a5` restates the same sentence and
  names no request, no endpoint, no status code and no date. A restatement is
  not a measurement.
- ADR 077, second correction: `fiddle-2n67` at `c3aa659` over `fiddle-bsow` at
  `762def6`. Not caught, and there was nothing to catch. `762def6` graded the
  claim an argument, named the one observation behind it and the test that pins
  the opposite, `a_refused_credential_and_a_missing_issue_do_not_read_alike`,
  which drives the stub with 401, and each of those holds against the tree at
  `762def6`. The re-grade came from six probes run on 2026-08-27, after admission
  and outside the tree. The rule reads the tree at admission and cannot see a
  probe that has not run. One clause in that text, "It is blocked", states a
  tracker state. `.beans/` is ignored by git, so the rule refuses that clause
  with the reason rather than passing it, and `fc53c42` later found the tracker
  held no `blocked_by` edge.

**Fix:** When a criterion admits a change to a record, re-derive every count,
path, symbol, call order and evidence grade the new lines state, off the tree at
the revision the sentence cites, and report the denominator in the evidence.
`skills/evaluate/evaluator-general.md` carries the rule under Record Changes.
Fail the criterion on a claim that does not hold, and fail it on a claim you
cannot re-derive, with the reason. Put the derivation into the criterion only
when the criterion corrects a sentence it can quote; a criterion written before
the text exists cannot name a derivation for it, and both sentences above were
of that kind. A citation check keeps a record's symbols current. It does not
keep its sentences true, and a green citation line is not evidence that they
are.
