# Holistic Review

Cross-domain evaluation that assesses the full system as an integrated whole, after all domain evaluators have scored their individual dimensions. Produces a holistic scorecard, a spec coverage matrix, and remediation beans for any gaps.

## Runtime Evidence

You receive an evidence pack recorded against the live runtimes before your dispatch, alongside the diff and the design doc. Read it before scoring any holistic dimension and cite the artifact behind each observation. Neither source code nor the per-domain scorecards can show whether the domains work together — the per-task scores were each produced in isolation, which is the gap this review exists to close. If the pack shows a runtime was not running, score Runtime Health 1 and note the failure.

Interpret what the pack records; do not launch, restart, or re-probe the runtimes yourself. The runtimes are started by `start-runtimes.sh` and the probes are captured before dispatch precisely so a reviewer that cannot execute anything, such as a provider running read-only, evaluates the same evidence as one that can.

### What the Evidence Should Show

- Each domain's runtime responding, with its port and domain named
- Primary user flows exercised end-to-end across domains, and their outcomes
- Frontend states reached via backend data, captured as screenshots or transcripts
- API calls from frontend arriving at backend and returning correct data
- Console output, including any errors, warnings, or unhandled exceptions
- Test-suite and validation-script output for the epic as a whole

Anything the pack does not cover is unevidenced: say so in the dimension's evidence rather than inferring it from the diff.

## Cross-Domain Integration Check

Before scoring, verify that domains work together as a system:

- **API contract compliance:** Does the frontend send requests the backend expects? Does the backend respond with shapes the frontend can parse?
- **Data flow end-to-end:** Trace at least one full user action from UI interaction through API call to backend processing and back to UI update.
- **Error propagation:** When the backend returns an error, does the frontend display appropriate feedback?
- **State consistency:** Does the frontend state reflect backend state accurately after mutations?

Note any integration gaps in the scorecard evidence. Integration failures directly affect the Integration and Coherence dimension scores.

## Dimensions

Score each dimension using the scales defined in: `skills/develop/holistic-dimensions.md`

Score the assembled whole rather than carrying over domain evaluator scores; a system of individually-passing parts is exactly the case these dimensions are meant to catch. Judge Holistic Spec Fidelity against the overall design document, not the individual task specs.

Dimensions and default thresholds:
- **Integration** (7) — Do the pieces work together?
- **Coherence** (7) — Does the whole feel like one system?
- **Holistic Spec Fidelity** (8) — Does the full result match the design vision?
- **Polish** (6) — Would you ship this?
- **Runtime Health** (9) — App launches cleanly, no console errors?

## Output

Produce output following: `skills/develop/holistic-scorecard-schema.md`

This includes the spec coverage matrix, remediation beans, and scorecard JSON.
