---
name: runtime-evidence
description: Use when an evaluator needs to interact with a running application before scoring dimensions
---

# Runtime Evidence

Runtime evidence means interacting with the RUNNING application, not just reading
source code. Observations of actual behavior in a live environment — screenshots,
HTTP responses, console output, interaction results — are runtime evidence. Static
code analysis is not.

## HARD-GATE

```
If a runtime is configured for this evaluation, you MUST interact with the
running app before scoring ANY dimension.

Code-only review is INSUFFICIENT when a runtime is available.

Do not score a single dimension until you have exercised the running app and
recorded what you observed. No exceptions.
```

## Evidence Gathering Methods

Use whatever tools are available to observe the live application:

- **Screenshots** — Capture rendered UI states (screens, components, transitions)
- **HTTP responses** — Status codes, response bodies, headers from live endpoints
- **Console/log output** — Application logs, warnings, errors during interaction
- **Interaction results** — Outcomes of taps, clicks, navigation, form submissions
- **State changes** — Database writes, file system modifications, side effects

Evidence must come from the running app. "The code looks correct" is not evidence.
"I hit GET /users and received a 200 with a JSON array of 3 users" is evidence.

## Evidence Format

For each piece of evidence, record:

1. **Action taken** — What you did (e.g., "Tapped the Submit button")
2. **Expected result** — What the spec says should happen (e.g., "Form submits and shows success toast")
3. **Observed result** — What actually happened (e.g., "Form submitted, success toast appeared after 200ms")
4. **Judgment** — Pass or fail, with reasoning if fail

Structure evidence per dimension. Each dimension's evidence field should reference
specific runtime observations, not code impressions.

## Stack-Specific Patterns

These are guidance for common stacks, not hard requirements. Use whatever tools
are available in your environment.

### Flutter

Use the `marionette` MCP tool to interact with the running app:
- Navigate between screens
- Tap buttons, scroll lists, enter text in fields
- Take screenshots of each state
- Verify visual output matches spec

### Go API

Use `curl` or the `go-dev-mcp` tool against the running server:
- Hit endpoints with valid and invalid inputs
- Verify status codes, response shapes, headers
- Check error responses for proper structure
- Test concurrent requests if relevant

### Web Frontend

Use browser tools or `curl` against the running app:
- Load pages and verify rendering
- Submit forms, click links, test navigation
- Check responsive behavior at different viewport sizes
- Verify client-side state management

## Failure Classification

Not all failures mean the implementation is bad. Distinguish between two categories:

### App Failure — Score It

The application itself is broken. This is evaluation-relevant:

- App crashes on launch → Functionality/Correctness score 1-2
- Endpoint returns wrong data → Correctness impacted
- UI doesn't render spec'd components → Visual Quality / Domain Spec Fidelity impacted

Score these honestly using the domain template's scale definitions.

### Harness Failure — Escalate, Don't Score

The test infrastructure broke, not the app (runtime didn't start, port unreachable,
MCP tool unavailable, environment misconfigured).

Do NOT penalize the implementation for harness failures. Report the failure to
the orchestrator and request re-evaluation with a working runtime.
