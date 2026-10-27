# Redesign, failure diagnosis, and quality gates

## Contents
- Preserve and diagnose
- Severity and prioritization
- Failure pattern catalog
- Review sequence
- Worked transformations
- Evidence and stopping criteria
- Handoff

## Preserve and diagnose

Before modifying an existing interface, inventory routes/screens, functions, content, data contracts, permissions, user customizations, brand assets, design tokens, component libraries, and important responsive states. Identify what already works. Record a baseline screenshot or exact current artifact if available. Preserve behavior and unrelated user changes.

Map at least the primary task from entry through completion and recovery. Separate flow defects, information architecture, component/interaction defects, visual defects, and implementation defects. A screenshot supports observations about appearance; it does not establish that a button fails, an action is absent from the whole product, or users abandon a task.

Use an issue record:

```
Location / state:
Observed evidence:
User consequence:
Severity and confidence:
Proposed change:
What must remain:
Verification:
```

For ambiguous findings use “risk” or “hypothesis,” not a factual failure claim. Choose minimal repairs for a polish request. For a redesign request or justified architecture change, follow structural-redesign: compare a conservative repair and a structural alternative, then select using explicit task reasoning. Keep broader rebranding within requested scope. Do not replace a functioning stack or delete critical data merely to express taste. Familiar meanings deserve protection; existing coordinates and layout do not receive automatic protection.

Resolve structure first, then visual expression. These are work stages, not automatic approval gates. Move autonomously between them unless the user explicitly requests a review checkpoint. Implement the critical flow and its states before propagating to all screens; then complete the agreed scope.

## Severity and prioritization

- **P0:** Data loss, dangerous misleading outcome, unavailable essential task, or similarly severe issue. Repair or explicitly surface a real blocker immediately.
- **P1:** Main task inaccessible, misleading control, serious contrast/focus/navigation failure, mobile flow unusable, or missing recovery.
- **P2:** Repeated friction, weak hierarchy, inconsistent components, unclear labels, unnecessary steps.
- **P3:** Fine optical polish, decorative inconsistency, optional motion refinements.

Prioritize severity, frequency, reach, evidence confidence, and effort. Do not manufacture exact priority scores from absent analytics. Fix layout/root-token problems before many isolated overrides. Keep legitimate tradeoffs explicit: more readable spacing may reduce simultaneous data; document why the selected density suits the task.

## Failure pattern catalog

For each suspected pattern, inspect the actual context before editing. The remedy column is a diagnostic starting point, not a new universal style.

| Symptom | Likely problem | Concrete repair | Exception / proof |
|---|---|---|---|
| Every control has a filled accent background | No action hierarchy | Identify the task-region primary; quiet utilities; retain visible status | Independent tasks can have separate local primaries |
| Three identical action buttons compete | Priority unspecified | Name intended next step; use contrast/placement to separate alternatives | Equal choices may genuinely deserve equal emphasis |
| Static metric looks like a button | False affordance | Remove hover/cursor/button treatment; use label/value hierarchy | A genuine drill-down must be a real operable target |
| Very polished page has no clear next step | Decoration hides task | Rebuild around user intent and content sequence | Reading/exploration may need orientation rather than a CTA |
| Every section is a rounded card | Grouping is mechanical | Use spacing, rules, typography, lists, and tables where appropriate | Independent objects can appropriately remain cards |
| Same hero and three-column feature grid for every product | Template substituted for understanding | Use real content structure and one relevant visual motif | Use a grid when items truly are peers |
| Redesign changes only gutters, corners and colors | Scope collapsed into polish | Evaluate navigation, grouping, content priority and composition; select a justified arrangement | Retaining sound architecture is valid when supported by explicit reasoning |
| A sidebar remains after verified destinations are consolidated | Layout inherited without evaluating content capacity | Compare a compact bar with the retained sidebar and test labels, hierarchy and reflow | A deep or frequently switched hierarchy can need the sidebar |
| Every theme is placed in a carousel | Reference copied as a universal pattern | Choose grid/list/table/rows by browsing versus comparison; retain inventory access | Bounded curated sampling can benefit from thematic rows |
| Minimalism buries search, captions or required routes | Visual simplicity overrides task access | Restore the usable input/control route and make scope/location clear | Optional rarely used settings may appropriately use disclosure |
| Huge headline and sparse dashboard | Marketing composition on a work tool | Prioritize useful information and operating controls | A summary wall display has different needs |
| Text seems refined only because it is faint | Contrast sacrificed for hierarchy | Increase contrast; create hierarchy with size, weight, and space | Inactive decorative content has different requirements |
| Heading floats halfway between groups | Association unclear | Move it toward its content; increase preceding gap | A centered section separator must still be unambiguous |
| Button label feels cramped or lost | Padding/text/target mismatch | Use line-height + padding + minimum target, inspect long labels | Full-width actions can have large horizontal space |
| Inner and outer curves form awkward wedges | Inset/radius mismatch | Adjust inner radius to inset geometry or go edge-to-edge | Pills and irregular shapes need optical judgment |
| Heavy shadow on every element | Depth lacks meaning | Reduce elevation levels; reserve stronger shadow for overlays | Expressive illustration may intentionally use strong lighting |
| Many arbitrary colors identify no state | Palette has no roles | Assign semantic roles; consolidate duplicates; check contrast | Data categories legitimately need distinct encoding |
| Green error, red confirmation, or brand-colored deletion | Conflicting status signals | Apply stable semantic role + text/icon | Cultural/contextual systems may differ; verify intent |
| Unselected icon looks filled/active | Mixed icon families/states | Normalize family/weight; encode selection consistently | Native system may have documented selected variants |
| Ellipsis hides the most frequent action | Discoverability traded for neatness | Expose frequent action; retain secondary overflow | Expert tools may offer deliberate configurable density |
| Profile menu contains core product navigation | Account metaphor overloaded | Restore destinations to nav; keep identity actions together | Some established products have a clear workspace menu |
| Label predicts navigation but opens a menu | Behavior signaled incorrectly | Distinguish link from disclosure trigger | Follow familiar platform patterns consistently |
| Multi-select presented as radios | Wrong selection semantics | Use independent checkboxes/chips and a group label | Radios remain correct for exclusive choices |
| Tooltip contains required instructions | Hidden knowledge prerequisite | Put critical help inline; use tooltip for optional detail | Compact expert UI still needs touch/focus alternatives |
| Toast says “Saved” while data never persisted | False completion | Connect result to actual response or label simulation | Local-only persistence can be valid when disclosed |
| Form cannot proceed and no reason is shown | Invisible validation | Explain fields/prerequisite, preserve input, guide focus | Prevent invalid action only with discoverable reason |
| Modal opens but keyboard reaches background | Focus/modal model incomplete | Use tested dialog primitive, containment, inert background, focus return | Nonmodal popovers follow a different model |
| Mobile solved by hiding overflow | Layout defect concealed | Find overflowing element; reflow or localize necessary scrolling | Intentional visual clipping can be decorative |
| Sticky CTA hides last item or focus | Overlay space not reserved | Add inset-aware clearance and scroll reveal behavior | Verify short height and virtual keyboard too |
| 800ms animation on routine controls | Demo timing copied into production | Shorten feedback and update state immediately | Deliberate long narrative motion may be appropriate |
| Glass readable only on one background | Dynamic contrast failure | Add sufficient surface opacity/scrim or use solid fallback | Recheck all relevant states; blur alone is not proof |
| Carousel inside another carousel | Gesture/target ambiguity | Flatten structure or move browsing to detail | If retained, provide distinct controls and verified routing |
| “Premium” redesign erases brand and data | Generic aesthetic imposed | Restore identity and essential information, repair specifics | Full repositioning requires actual scope |
| Screenshot review claims measured usability gains | Evidence overreach | Rephrase as hypothesis; propose actual task verification | Only observed results justify quantified claims |
| Repeated “Seamless,” “Elevate,” and invented social proof | Generic or fabricated content | Write specific true value; remove unsupported evidence | Fictional/demo content must be explicitly identified |
| No visible flaw, yet repeated restyling continues | Endless taste loop | Compare to brief and hard gates; finish | Iterate further only for a concrete unresolved issue |

## Review sequence

Run an observe → explain → change → verify loop. Inspect the output after changes; a successful build is not a visual review.

1. **Task/structure pass:** Can the intended person locate, understand, complete, and recover from the primary task? Are important controls discoverable? Does navigation identify the current location? Are domain data and permissions preserved?
2. **Visual pass:** View at actual scale and full-page scale. Check first-attention hierarchy, type, alignment rails, grouping, density, color role consistency, image crops, optical centering, nested radii, shadows, and whitespace. Inspect repeated components together. Use the composition checks below; passing token contrast does not make a bland or poorly composed layout finished.
3. **Interaction pass:** Exercise primary action, alternate route, back, cancel, validation, pending, error, success, and a relevant destructive/recovery path. Verify sequences across states: save/edit/reopen and remove-last/save/reload where persistence exists. Confirm feedback and recovery remain reachable inside overlays. Test only implemented states; record unavailable integration checks.
4. **Adaptive/accessibility pass:** Inspect narrow/intermediate/wide layouts, enlarged text, keyboard, focus, touch, contrast, reduced motion, and applicable assistive semantics. Use the detailed matrix in the accessibility reference.
5. **Regression pass:** Recheck changed routes and high-risk adjacent behavior. Reuse project tests/build/lint if relevant. Do not add broad low-value test suites for a small reversible style fix.
6. **Specificity pass:** Remove unsupported copy, ornamental controls, redundant icon boxes, arbitrary badges, placeholder TODOs, and repeated generic layouts. Keep deliberate character that serves the brand.

Inspect both the default view and the most difficult relevant state: long labels, many rows, empty results, form error, overlay, or narrow width. Do not validate only a perfectly populated desktop screenshot.

### Composition checks

Use observable questions, not numerical self-ratings or a generic beauty checklist:

| Check | Inspect in the rendered artifact | Repair when failing |
|---|---|---|
| Task prominence | The useful content/control is visible in the first viewport; chrome and decoration have subordinate roles | Reallocate area, change content order, remove redundant framing |
| Scale and priority | Important regions read differently; equal comparison peers remain comparable | Establish a clear size/order/grouping relationship rather than making everything larger |
| Coherence | Page surfaces, media, type, controls, corners and spacing express one direction across states | Repair shared tokens and relationships before isolated styling |
| Content fit | Real titles, avatars, metadata, crops and long values remain legible and useful | Adjust measure, crop/focal point, spacing or density; retain needed facts |
| Product specificity | Structure and visual choices follow this product's tasks and identity | Replace inherited template structure and filler with purposeful decisions |
| Transformation | Matched before/after views show what the selected redesign changes and preserves | Revisit architecture when only styling changed without justification; retain sound structure when it wins |

Separate **structural quality**, **visual quality**, and **behavioral verification** in the evidence ledger. Mark each unverified dimension explicitly. A preview that cannot render limits evidence; it does not establish a visually successful result. Finish within scope once concrete defects are resolved; do not keep adding flourishes to chase subjective novelty.

## Worked transformations

### A crowded listing card

**Before:** A 320px-wide card has an image inset 8px, outer radius 24px, image radius 24px, 8px between every text block, a 12px title, and three equally filled orange buttons. The section heading is equally distant from the previous and following cards.

**Change:** Keep the outer radius if it fits the brand. Use image radius about 16px for the 8px inset, or choose an edge-to-edge crop. Give the text region a coherent 16px inset. Make the title a clear heading, for example 18px/24px, use legible metadata, place closely related metadata 4–8px apart, and separate the action region by 16px. Identify the real task: if the card is a browsing result, use a clear detail destination and quieter Save action; if it is an order confirmation, do not add a browsing CTA. Increase space above the section heading while keeping it close to its own list.

**Verify:** Use one short and one three-line title, an absent image, a long price, keyboard focus, and the narrow layout. Inspect whether the repaired curves and spacing actually look balanced. These numbers are one workable composition, not reusable mandates.

### A form with a silent failure

**Before:** An inactive Continue button sits below eight fields. A required date is missing, but there is no indicator. Hover is the only way to discover help.

**Change:** Add persistent labels and visible requirements. Prefer allowing submission and showing “Enter a delivery date” at the relevant field, with an error summary when useful. Preserve the other seven values. If disabling remains appropriate, explain the prerequisite before submission. Connect help and errors semantically and make the next step available by keyboard.

**Verify:** Submit empty and partially completed forms, correct the date, reopen the form, navigate backward, and test long error messages. Check that feedback reflects real validation rather than merely changing the button color.

### A five-step workflow

**Before:** To create a playlist, the user enters Music, opens an existing playlist, chooses Add, finds New playlist, and enters a name. This is observed in the supplied prototype, not asserted about a real brand.

**Change:** Add a findable Create playlist entry in the library's relevant task area. Let the user name it, then add tracks. Preserve the existing contextual add route. Keep a meaningful empty state and creation feedback. Do not place a dominant Create button on every unrelated screen.

**Verify:** Starting from the same state, record the actual decisions/actions for both routes and check discoverability, cancellation, and duplicate creation. Say that the path is shorter if verified; do not infer retention or conversion uplift.

### An interface that already has character

**Before:** An editorial site uses square corners, an expressive serif, a warm red brand, and linear image scrims. Its actual problems are long centered body text and a close button covered at short viewport heights.

**Change:** Retain the shapes, type identity, red, and appropriate scrims. Use a readable start-aligned text column; repair dialog sizing and scroll behavior. Separate destructive meaning through wording and treatment if needed.

**Verify:** Inspect reading measure, actual text contrast, short-height dialog dismissal, and keyboard access. Do not replace the design with rounded blue cards simply because the course demonstrates them.

## Evidence and stopping criteria

Keep a compact evidence ledger:

| Check | Result | Evidence | Limit |
|---|---|---|---|
| Main flow | pass / fail / not run | actions exercised and observed result | integration boundary |
| Layout | pass / fail / not run | rendered widths/states | untested device |
| Accessibility | pass / fail / not run | actual keyboard/contrast/semantic checks | no full audit claim |
| Regression | pass / fail / not run | relevant existing tests | coverage limits |

A self-rating is not evidence. If a check could not run, say so; never convert “looks plausible in code” into a verified pass.

Stop when applicable P0/P1 issues are resolved, the agreed scope is complete, relevant checks pass, and remaining issues are minor or explicitly blocked by unavailable information/tools. Finish with honest scope when a check is impossible. Do not hold delivery for endless ornamental alternatives or ask the user to make routine design choices.

## Handoff

Lead with the result. Briefly connect the major changes to the user's task, name material assumptions, state what was actually tested, and identify meaningful limits. Provide the working artifact or implementation through the environment's normal mechanism. Do not deliver a long process diary unless requested.

For a requested design presentation, use: objective → relevant before state → prioritized issues → proposed experience → matched before/after views → important interactions → verification/next steps. Use equal viewport, scale, and content for comparisons. Device mockups should fit the actual target and expose the interface clearly; decorative framing cannot replace a flat, legible view. Do not invent outcomes to make a case study persuasive.

For implementation handoff, include tokens, component states, breakpoints/constraints, interaction/focus behavior, content, assets, and data/integration boundaries. Keep identifiers semantic and editable. For estimating work, inventory screen families, unique components, states, responsive variants, integrations, review cycles, and QA; do not use the tutorial's dated rates, tax assumptions, or arbitrary screen price.
