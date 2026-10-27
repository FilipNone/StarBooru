---
name: design-ui-ux
description: "Design, implement, audit, and improve professional UI/UX for websites, web apps, native/mobile apps, dashboards, portals, forms, commerce, and other digital interfaces. Use for new interface creation, redesign, visual polish, interaction or usability fixes, design systems, responsive adaptation, accessibility improvements, and screenshot-based design review. Make informed design decisions independently, preserve working product behavior, and verify the resulting artifact. Framework- and tool-independent; supports code, design editors, screenshots, and specification-only environments."
---

# Professional UI/UX Design

Create a coherent interface that lets its intended user understand, act, and recover. Make deliberate visual decisions and finish the requested artifact. Translate vague requests into a useful working brief; do not require the user to art-direct routine choices.

Treat this package as self-contained. The original course/transcript, linked exercises, paid services, and external assets are not required. Follow host instructions, available capabilities, and user scope. Do not promise professional results without inspecting what can actually be produced and tested.

## Load the right depth

Resolve all paths relative to this skill directory. Read the applicable files before the corresponding work; do not merely acknowledge that they exist. For a small edit, read only the relevant sections plus quality gates. For a full build/redesign, load all applicable modules in stages.

| Work | Read |
|---|---|
| Every visual design task | [foundations](references/foundations.md): priority, typography, layout, color, shape; [review-and-redesign](references/review-and-redesign.md): review and stopping criteria |
| New product, unclear brief, flow/architecture change | [flows-and-patterns](references/flows-and-patterns.md) |
| Add or change interactive components | Relevant sections of [components](references/components.md) |
| Build or alter an interface's layout/interaction | [accessibility-and-responsive](references/accessibility-and-responsive.md) |
| Redesign / reimagine, or consequential navigation/grouping/layout changes | [structural-redesign](references/structural-redesign.md): scope, preservation map, architecture, pattern selection, composition and tradeoffs |
| Existing design | Preserve/diagnose and failure catalog in [review-and-redesign](references/review-and-redesign.md); for bounded polish, load structural-redesign only if the change affects architecture |
| Tool/stack selection, screenshot-only, limited runtime | [environment-adapters](references/environment-adapters.md) |
| Rigid rule, source interpretation, trend conflict | [source-interpretation](references/source-interpretation.md) |
| Known solid color contrast pair | Optional [contrast calculator](scripts/contrast.py); requires Python standard library only |

Keep requirements distinct from heuristics. Numbers for type, spacing, radius, motion, and density are starting points; accessibility thresholds are specific criteria with defined scope. Existing sound conventions usually outrank generic defaults.

## Operating sequence

### 1. Establish scope and capabilities

Inspect the provided artifact, workspace instructions, existing implementation, design assets, tokens, content, platform, and available tools. Determine whether the task is create, repair, polish, audit, or redesign. Match the output to the request: working implementation when asked to build; findings when asked to audit; design/specification when execution is unavailable.

For an existing product, record working behavior and constraints before changing them. Preserve meaningful capabilities, routes/deep links, data, permissions, integrations, brand recognition, accessibility support, and unrelated user changes. Protect outcomes and contracts without automatically preserving control placement or page geometry. Choose the smallest adequate change for a local repair/polish request; for a redesign, evaluate architecture as well as appearance.

Infer ordinary missing choices from the product. Record material assumptions briefly. Ask only when an unresolved conflict or missing essential input changes correctness, consequences, or feasible scope. Do useful authorized work before asking. Do not ask for font/color preferences merely to transfer design judgment to the user.

### 2. Define the task and acceptance contract

Write a compact working brief in notes or the relevant artifact:

```
Audience and context:
Primary job and success condition:
Required screens/flows and secondary tasks:
Known constraints and preserved behavior:
Assumptions and evidence limits:
Verification appropriate to available tools:
```

Identify the primary task per screen/region and the information needed to make its decision. Do not invent analytics, testimonials, business facts, user research, or product capabilities. Use clearly labeled example data when necessary.

For complex work, inventory screens and map the main flow from entry to outcome, including back, cancel, errors, and recovery. Separate the user's goal from a proposed tactic. Reduce unnecessary decisions and repeated entry; keep review and safeguards when useful.

### 3. Set structure before polish

Group information by user meaning. Establish global navigation, local navigation, page hierarchy, content order, and action priority. Distinguish destinations, account tools, filters, sort and suggestions. Use low-detail layouts or explicit structural notes when they reduce uncertainty; do not produce ceremonial wireframes for a small fix.

For a redesign, compare a conservative repair and a structural alternative internally before styling. Examine navigation footprint, verified duplication, browsing/retrieval intent, grouping, unequal content priority, and tasks that need concurrent context. Select the arrangement that best serves the task and preserves required capabilities. Do not silently turn “redesign” into a spacing/color pass or force structural novelty when the existing arrangement is sound. Record major choices as condition → consequence/hypothesis → change → preserved capability → tradeoff → verification; use the structural-redesign module for the decision procedure.

Build a representative slice that includes the main task, a detail/action state, and the hardest relevant constraint. Connect each structural change to an observed condition or explicit task hypothesis. Keep familiar meanings and clear routes; change the arrangement when that addresses the problem. Do not treat an absent user study as a prohibition on a reasoned, reversible design hypothesis.

Treat structure and styling as internal stages. Continue between them autonomously unless a review checkpoint was explicitly requested. Complete the full requested scope after validating the slice.

### 4. Choose and encode visual direction

Choose a visual thesis tied to audience/content. For new work without brand direction, briefly compare two appropriate directions, select one, and implement it; do not require user selection. Preserve an existing visual language for repair/polish. For redesign, retain meaningful brand cues while deliberately selecting composition, density, type hierarchy, imagery and surface relationships.

Define the primary and supporting regions, navigation footprint, alignment rails and content scale before decorative effects. Make the chosen hierarchy visible in the composition; a large heading over an unchanged uniform feed is not sufficient. Equal peers may rightly remain equal. Carry one or two purposeful visual decisions through components and states; do not rely on gradients, rounded cards, filler slogans or arbitrary asymmetry to create distinction.

Define a compact system: semantic colors, typographic roles, spacing scale, layout width/grid, control sizes, radii, elevation, icon family, imagery treatment, and motion. Reuse a good existing system. Give strong emphasis to the most useful information/action; allow expressive detail without making every element compete.

Implement these minimum visual relationships:

- Align related content on shared rails; keep group gaps smaller than section gaps.
- Keep headings close to their content, text legible, and line length appropriate.
- Distinguish interactive controls from static labels and status.
- Keep component family, icon weight, radius geometry, and color roles consistent.
- Use meaningful content and authentic assets; preserve image/logo proportions.
- Make layout adapt through constraints and reflow, not screenshot scaling.
- Use effects only when they support the chosen direction and preserve readability/performance.

### 5. Complete behavior and adaptation

For each important component, specify and implement purpose, state, trigger, outcome, focus behavior, and error/recovery. Use semantic/native or proven accessible primitives. Never leave decorative controls that imply unsupported actions.

Cover relevant default, focused, selected/expanded, pending, empty, invalid, failure, success, and unavailable states. Do not add irrelevant states just to fill a checklist. Preserve entered data and explain failure. Distinguish real persistence from local/demo behavior.

Adapt to narrow/intermediate/wide widths as applicable. Check long content, enlarged text, keyboard, touch targets, safe areas, overlays, contrast, and reduced motion. Reserve space for sticky controls. Do not hide essential content or page overflow merely to make a screenshot neat.

### 6. Render, exercise, repair

Use the best available authorized renderer, preview, browser, emulator, or design tool. Inspect actual-size views and a relevant difficult state. For redesign, compare equivalent content and viewport with the baseline where available. Run the main interaction and recovery paths; use existing project tests where useful. Syntax, token contrast, builds, DOM harnesses and deployment status verify different things; none substitutes for rendered visual review.

Review in order: task correctness → structure → visual hierarchy → interaction → responsive/accessibility → regression → fine polish. Fix the highest-severity defect, rerender/retest the affected area, and repeat until acceptance criteria are met. Do not endlessly restyle already sound work.

Use the failure catalog for specific diagnosis. Check especially for all-accent buttons, ambiguous nesting, misplaced headings, faint text, mismatched icons, misleading feedback, inaccessible dialogs, sticky overlap, unnecessary cards, generic copy, and unsupported claims.

### 7. Deliver with evidence

Return the requested artifact or scoped changes, not only advice or a plan. Explain the most important choices in terms of the user's task and report the actual depth of change. Separate structural reasoning, rendered visual evidence and behavioral evidence. Keep a compact evidence ledger for substantial work; do not claim measured improvement, accessibility conformance, professional visual validation or cross-device validation from source inspection or an untested specification.

Publish only when authorized by the request/host workflow. Design autonomy does not grant unrelated deployment or external-action permission.

## Hard gates

Do not mark the work complete while an applicable gate is knowingly failing:

1. **Purpose:** Intended users can identify the task, current location, and relevant next action; no fabricated product facts.
2. **Behavior:** Required controls do what they signal; main task, back/cancel, and meaningful recovery work within the declared implementation boundary.
3. **Readability:** Hierarchy and grouping are clear; text and applicable graphical contrast pass; critical meaning does not rely only on color.
4. **Access:** Keyboard/focus and target behavior are usable; no trapped or hidden essential action; relevant semantics exist.
5. **Adaptation:** Required sizes and content variations remain usable; no accidental clipping or overlay-obscured task.
6. **Integrity:** Existing required functions, data contracts, brand constraints, and user edits survive.
7. **Evidence:** Distinguish pass, fail, and not tested. Explicitly identify genuinely unavailable checks or blockers.
8. **Design depth:** For a redesign, evaluate architecture and composition, justify the selected arrangement, and retain important capabilities. For bounded polish, respect its scope. Do not substitute superficial styling or speculative product features for the requested design work.

When capabilities prevent verification, deliver the best concrete result and name the exact unverified part. Do not invent a pass or stop all useful work.

## Abbreviated fallback

If bundled references cannot be loaded, apply this kernel: infer the user/task, intervention depth and constraints; preserve capabilities and contracts; evaluate grouping/navigation/composition for redesign; map the primary flow and recovery; choose a coherent visual direction; establish type/space/color/component roles; implement real controls and appropriate states; adapt to content and device; inspect or clearly mark untested output; repair specific failures; deliver. Use readable body text, semantic controls, visible focus, comfortable hit areas, meaningful labels, truthful feedback, and stable layouts. Never substitute a rigid visual trend for the task. State the missing-reference limitation if it materially affects the work.
