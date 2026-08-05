# Drift analysis, documentation, and product artifacts

## Drift analysis

If providers are configured, read `hooks/dispatch-provider.sh` and dispatch each provider as a drift analyst. Put the design and implementation diff in temporary files and ask the provider to identify implemented-as-designed work, drift, missing work, and scope beyond the design. Dispatch in parallel when supported; otherwise sequentially. Collect results in attended mode.

If no provider is available, compare the design document and full diff directly. Present the result before continuing:

```text
Drift analysis complete:
- Implemented as designed: [list]
- Drift detected: [list with explanations]
- Missing from design: [list]
- Added beyond design: [list]

Proceed with documentation update?
```

Wait for confirmation.

## Documentation update

Invoke `fiddle:deliver-docs --epic <epic-id>`. Present its results and wait for approval before continuing.

## Product artifacts

Skip this section when `deliver.product_artifacts` is absent or has no artifacts.

For each configured artifact:

1. Read `<templates_path>/<artifact-type>.md`; warn and skip only that artifact if it does not exist.
2. Gather the design spec from the epic body (`Design:` first, then a sibling `-design.md` next to `Plan:`), the drift result, a diff summary, and optional `docs/product/VISION.md` and `docs/product/GTM.md`.
3. Generate the artifact with the template's voice, audience, and format instructions.
4. Write `<output_path>/YYYY-MM-DD-<epic-id>-<artifact-type>.md`, overwriting the same dated output if it exists.

Present every generated artifact and wait for confirmation or edits before evaluator evolution.
