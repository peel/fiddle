# Holistic Review

Cross-domain evaluation that assesses the full system as an integrated whole, after all domain evaluators have scored their individual dimensions. Produces a holistic scorecard, a spec coverage matrix, and remediation beans for any gaps.

## Runtime Interaction

Interact with every domain runtime before scoring any holistic dimension, and cite what you observed from that interaction. Neither source code nor the per-domain scorecards can show whether the domains work together — the per-task scores were each produced in isolation, which is the gap this review exists to close. If a runtime is not running, score Runtime Health 1 and note the failure.

The runtimes are started by `start-runtimes.sh` before holistic review begins. You receive runtime state for each domain including ports and domain names. Use the appropriate MCP tools (marionette for Flutter, curl for HTTP, go-dev-mcp for Go) to interact with each runtime.

### Evidence Gathering

- Launch each domain's runtime and verify it responds
- Exercise the primary user flows end-to-end across domains
- Take screenshots of frontend states reached via backend data
- Verify API calls from frontend reach backend and return correct data
- Check console output for errors, warnings, or unhandled exceptions
- Record which cross-domain flows were tested and their outcomes

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
