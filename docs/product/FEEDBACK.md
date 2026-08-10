# User Feedback

<!-- Structured feedback log. Each entry captures: Who, Context, Observation, Implication, Confidence.
     Append-only — never edit or delete entries.
     Run /insights periodically to synthesize into personas and insight summaries.
     See /feedback for the entry format. -->

<!-- ### YYYY-MM-DD — <short observation title>

     **Who:** <role, segment, experience level>
     **Context:** <where/when/how this was observed>
     **Observation:** <what happened — facts only>
     **Implication:** <what it means — one sentence>
     **Confidence:** <high|medium|low>
     Tags: #feature-request #bug #confusion #praise #churn-signal #ux #performance #onboarding #docs
-->

### 2026-08-09 — Orientation is a near-fixed cost per bean, and it does not scale down with bean size

**Who:** Lifecycle lead and nine implementer subagents, running `fiddle:develop` over milestone M2 (epic fiddle-srrw, 15 beans).

**Context:** Nine M2 beans measured from their session transcripts, `claude-opus-5` at effort `high`, 16:12Z–21:15Z. Wall clock averaged 23.4 min/bean (range 8.6–44.2).

**Observation:** The phase split is orientation 5.7 min (25%), implementation 12.3 min (53%), final gate 5.3 min (23%). Of total wall clock, 63% is model generation and only 37% is the machine; roughly two thirds of the model's output tokens are extended thinking rather than visible output.

**Bean size does not predict wall clock.** Task 3 spent **8.0 minutes orienting for 2.5 minutes of implementing**. A bean adding a ~40-line pure module with 6 tests cost about the same orientation as one adding 544 lines and 9 tests, because both read the same prior-task source, the same bean body, and the same epic `## Contracts`. Orientation is also the least elastic phase — 5.7 min of which only 1.0 min is machine time.

The lead's own hypothesis, that `nix develop` shell entry dominated, was **refuted**: 2.54 s warm per entry, ~18 entries/bean, 3.2% of wall clock. Two real wastes were found instead, both authored by the lead's prompts rather than by the implementers: ten named regression lanes re-run individually *after* a clean full-workspace run had already printed those same counts (171 s across nine beans), and a 5.7 min median gap between one bean finishing and the next being dispatched, spent writing summaries.

**Implication:** The develop loop pays for a fresh implementer's understanding once per bean, and that cost is nearly independent of how much the bean changes — so decomposing work into smaller beans does not make the milestone faster, and past some point makes it slower.

**Confidence:** high for the phase split and the nix refutation (measured directly); medium for the reasoning-token share (derived by subtraction, ±15%, since thinking content is not retained in transcripts).

Tags: #performance #orchestrate #develop-loop

### 2026-08-10 — The tracker recorded nothing for the hour a bean was being worked

**Who:** Lifecycle lead running `fiddle:develop` over milestone M2 (epic fiddle-srrw), 21 beans across 16 planned tasks and 5 holistic remediations.

**Context:** Each bean is implemented by a fresh subagent taking 20–60 minutes. The operator asked for status seven times over the run and each answer came from the lead polling `ps` for CPU and `git status` for modified paths, because the tracker held nothing else.

**Observation:** Every bean body carries a `## Steps` checklist in `- [ ]` form, and **not one checkbox was ticked by anybody, on any bean, at any point.** All 20 completed beans finished with 100% of their steps unchecked — 110 boxes in total. The project's own instructions say to keep todo items current *as it happens*; the lead never did it and never instructed an implementer to. The only durable record of progress was a `## Summary of Changes` appended at close, so a bean read identically at minute 2 and minute 55 of its implementation, and a reader who was not the lead had no way to distinguish "started" from "nearly done" from "stuck".

Two consequences beyond the missing visibility. The lead answered "status?" with process CPU percentages and file lists — inferring phase from whether `cargo` was running — which is guesswork dressed as reporting. And when the operator asked for detail, the lead had to message the agent and wait, rather than reading it, which is the same latency the polling was meant to avoid.

**Implication:** A loop whose unit of work runs for an hour needs the work-in-progress record to be written *by the worker as it goes*, not reconstructed by the coordinator from process state — otherwise the tracker documents outcomes and nothing about the run, and the coordinator becomes a bottleneck for a question the tracker should answer.

**Confidence:** high — the 110 unticked boxes are directly observable, and the seven status exchanges are in the session record.

Tags: #orchestrate #develop-loop #ux #confusion
