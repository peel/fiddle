# Frontend Domain Evaluator Template

For Flutter and web frontend tasks. Evaluates visual output, craft, interactivity, and spec fidelity from runtime evidence recorded against a running app.

## Evidence Pack

You receive an evidence pack alongside the diff: test output, invariant
results, and runtime probe transcripts gathered before your dispatch. Your
job is to interpret this evidence against the task's criteria. You do not
gather evidence, and you do not judge qualities the evidence cannot show.

Every criterion verdict MUST cite the evidence artifact that supports it
(file name and the relevant line/excerpt). A criterion with no supporting
evidence is scored fail with reason "no evidence".

## Dimensions (optional)

Scored dimensions are OPTIONAL for this domain. Include the `dimensions`
object in your scorecard only when the task's eval block sets thresholds for
this domain. When no thresholds are set, emit an explicitly empty object,
`"dimensions": {}`, and the task converges on evidence criteria alone. Never
omit the `dimensions` key: only the explicitly empty object signals
evidence-only convergence.

The dimension definitions below apply when thresholds are configured.

## Verification Approach

Live runtime. The app is started by `start-runtimes.sh` before evidence
gathering and stays running through your dispatch. The evidence pack contains
probe transcripts recorded against that live app, not judgments made from
reading source code alone. Do not launch, restart, or re-probe the app
yourself; interpret what the pack records.

### Runtime Evidence

The pack records transcripts from the tools that apply to the runtime:

- **Flutter apps:** `marionette` — screen navigation, button taps, list
  scrolls, text entry, and screenshots of resulting state.
- **Web apps:** `curl` — HTTP requests, rendered page fetches, status
  codes, and response bodies.

Prefer screenshot-based evidence for visual dimensions (Visual Quality, Craft).
Prefer interaction transcripts for behavioral dimensions (Functionality).

If the pack shows the app was not running or failed to start, score
Visual Quality as 1 and note the failure.

### What the Evidence Should Show

- Screenshots of key screens and states
- Primary interactions exercised (navigation, forms, buttons)
- Visual consistency across multiple states and screens
- Responsive behavior if applicable (resize, orientation)
- Error states (offline, invalid input, empty data)
- Which interactions were tested and their outcomes

### What to Check

- Does the app render correctly on launch?
- Do recorded interactions produce expected results?
- Are transitions and animations smooth?
- Is the design system applied consistently across screens?
- Do error states show appropriate feedback?
- Are loading states visible during async operations?

## Dimensions

### Visual Quality

Does the rendered output look intentional and polished?

**Default threshold: 7**

```
 1  Broken: App doesn't render. Blank screen, crash, or error state.
 2  Non-functional: Something renders but is unusable. Layout completely
    broken, elements overlapping or off-screen.
 3  Placeholder: Default framework output. Colored rectangles, system
    fonts, no evidence of design system.
 4  Minimal: Some custom styling attempted but inconsistent. Mix of
    placeholder and styled elements. Looks unfinished.
 5  Mediocre: Design system partially applied. Some components styled,
    others default. An engineer's "it works" not a designer's "it's ready."
 6  Acceptable: Design system applied consistently. Custom components
    throughout. Rough edges visible but overall looks intentional.
 7  Good: Matches design reference in structure and feel. Minor polish
    issues. A designer would say "needs a pass" not "start over."
 8  Strong: Close to design reference. Consistent visual language.
    Details mostly right. Minor nitpicks only.
 9  Polished: Matches design reference closely. Smooth transitions,
    proper spacing, color harmony. Ready to ship.
10  Exceptional: Exceeds design reference. Delightful details,
    micro-interactions, visual surprise. Better than specified.
```

### Craft

Typography, spacing, color harmony, contrast, alignment.

**Default threshold: 7**

```
 1  Broken: No text visible, or text unreadable (white on white, 0px font).
 2  Illegible: Text renders but wrong size, overlapping, or truncated.
    Colors clash. No spacing system.
 3  System defaults: Framework default fonts, spacing, colors.
    No intentional typographic choices.
 4  Inconsistent: Some intentional choices but applied unevenly.
    Multiple font sizes with no hierarchy. Spacing varies randomly.
 5  Basic: One font applied. Some spacing consistency. Colors from a
    palette but without harmony. Alignment mostly correct.
 6  Competent: Type hierarchy clear (headings, body, labels). Spacing
    system visible. Colors harmonious. Minor alignment issues.
 7  Good: Type hierarchy, spacing, and color all feel designed together.
    Contrast ratios adequate. Alignment consistent.
 8  Strong: Typography reinforces hierarchy and mood. Spacing creates
    rhythm. Color usage purposeful. No orphaned elements.
 9  Refined: Typographic details polished (line height, letter spacing,
    font weight variation). White space used intentionally.
10  Masterful: Typography, color, and spacing create a distinctive
    visual identity. Every detail considered.
```

### Functionality

Does it work when you use it? Interactive behavior correct?

**Default threshold: 8**

```
 1  Broken: Core interaction doesn't work. Buttons don't respond, pages don't load.
 2  Crashes: Some interactions work, others crash the app.
 3  Partial: Main flow works. Secondary interactions broken or missing.
 4  Fragile: Works for expected interactions. Unexpected actions (back button,
    rapid taps, resize) cause breakage.
 5  Basic: All specified interactions work. No feedback for loading states,
    errors, or edge cases.
 6  Functional: Interactions work with appropriate feedback. Loading states
    shown. Some edge cases unhandled.
 7  Solid: All interactions work correctly with feedback. Error states
    handled. Responsive to different viewport sizes.
 8  Robust: Handles rapid interaction, concurrent state changes, connectivity
    issues. Animations smooth. No jank.
 9  Polished: Transitions between states feel natural. Undo/recovery
    available. Accessibility basics met.
10  Delightful: Interactions feel instant. Animations guide attention.
    Keyboard navigation works. Screen reader compatible.
```

### Domain Spec Fidelity

Does this task's implementation match the task-level spec?

**Default threshold: 8**

```
 1  Wrong feature: Built something entirely different from task spec.
 2  Wrong approach: Right feature, fundamentally wrong implementation strategy.
 3  Major gaps: Core task requirements missing. What exists may be correct
    but the task is incomplete.
 4  Partial: ~50% of task requirements implemented. Missing pieces noticeable.
 5  Most there: ~70% of task requirements. Missing pieces are secondary
    but a careful reviewer would catch them.
 6  Functional coverage: All primary task requirements met. Secondary requirements
    (edge cases, error states, responsive behavior) partially covered.
 7  Good coverage: All task requirements met. Some implemented minimally
    (letter of the spec, not spirit).
 8  Faithful: Implementation matches task spec in both letter and spirit.
    Design intent preserved.
 9  Complete: Every task requirement fully implemented. No drift.
    Implementation captures nuances of the task description.
10  Exceeds spec: All requirements met and implementation improves on
    spec where the task description was ambiguous or underspecified.
```
