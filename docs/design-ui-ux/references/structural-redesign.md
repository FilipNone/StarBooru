# Structural redesign and composition decisions

## Contents
- Select the depth of change
- Separate invariants from arrangement
- Rebuild the task structure
- Choose navigation and collection patterns
- Compose the screen
- Adapt relationships across sizes
- Verify the transformation
- Transfer examples

## Select the depth of change

Classify the requested intervention before editing. Do not silently downgrade a redesign to spacing, radius, and color changes. Do not expand a bounded polish request into a new product.

| Request | Default work | Boundary |
|---|---|---|
| Fix / repair | Correct the identified defect and adjacent causes | Preserve unrelated composition and workflows |
| Polish / refine | Improve type, spacing, alignment, surfaces, and component consistency | Retain sound architecture; make only necessary usability repairs |
| Redesign / reimagine | Evaluate navigation, grouping, content priority, layout, interaction, and visual direction | Preserve essential capabilities; select justified changes, not novelty for its own sake |
| Create | Derive architecture and composition from the actual task and content | Do not invent product requirements to fill a template |
| Match a reference | Reconstruct the authorized reference within the target's constraints | Distinguish reproducing appearance from reproducing verified behavior |

For redesign, consider both a conservative repair and a structural alternative internally. Compare them against task completion, discovery, content capacity, accessibility, existing expectations, and implementation constraints. Select one without turning this into a user interview or multiple full mockups. If the current structure wins, record the specific reason; do not retain it simply because it exists. Changing only styling is valid when the architecture is already sound, but describe the actual extent honestly. Do not impose a quota of changed regions.

## Separate invariants from arrangement

Write a short preservation map before moving controls:

| Protect | May reconsider within a redesign | Verify after changing |
|---|---|---|
| User task, meaningful capabilities, records, permissions, URLs/deep links, integration contracts, undo/recovery | Entry point, navigation grouping, control placement, layout and disclosure | Every required destination and action remains reachable; saved state and contracts survive |
| Brand recognition, supplied logos, semantic names and meanings | Container geometry, type hierarchy, density, surface treatment | The product remains identifiable and understandable |
| Working accessibility and platform behavior | Visual arrangement and adaptation | Logical reading/focus order, operability, reflow and context continuity |

Preserve what a function accomplishes, not automatically its coordinates. A duplicate link to the same destination can be consolidated without deleting the capability. Two similar labels may instead mean different scopes: inspect their actual destinations or mark the distinction unknown. Do not use fewer visible controls as the sole success criterion.

If only a screenshot exists, enumerate visible capabilities and acknowledge unknown behavior. Treat a proposed relocation as a design hypothesis; do not assert that an unseen route is redundant or unused. Preserve a clear route to important tools. Distinguish an implemented change from a specification for a future integration.

## Rebuild the task structure

1. Identify the current role and intent: known-item retrieval, exploratory browsing, exact comparison, creation, monitoring, reading, or communication. Do not assume the business's promotional priority is the user's frequent task.
2. Inventory destinations, objects, actions, account tools, filters, and settings separately. Group items by user meaning. Replace unexplained labels with specific names while preserving meaning.
3. Examine repeated scanning, duplicated destinations, context switches, excessive navigation chrome, uniform treatment of unequal content, and secondary actions that displace the main activity. Explain the consequence of each suspected problem; a long sidebar alone is not proof of failure.
4. Identify tasks that must happen together. If someone checks a reference while editing, comments while watching, or compares records while choosing, preserve visibility of the relevant context. Consider a split view, contextual panel, anchored summary, or purposeful sequential flow. Avoid adding independent scroll regions without a real need.
5. Define the new order of decisions and the destination owning each action. Ensure safe back/cancel routes and retain list/filter/scroll context when applicable.

Record major decisions compactly: **observed condition → task consequence or hypothesis → selected change → retained capability → tradeoff → verification**. This is internal working evidence, not a mandatory presentation or approval stage.

## Choose navigation and collection patterns

| Situation | Consider | Check / counterexample |
|---|---|---|
| A few peer destinations and a wide content task | Top navigation or a compact labeled bar | Labels must fit and adapt; retain location/history semantics; use route links for routes, not false ARIA tabs |
| Many destinations, hierarchy, or repeated switching | Sidebar, grouped navigation, expandable branches | Do not replace a useful expert navigation tree with a crowded top bar |
| Account-specific tools | Coherent account/library area | Keep important frequent tools findable; not every personal task belongs behind an avatar |
| A known item to retrieve | Visible search with scope and submit route | Enter is not a replacement for a usable touch/pointer submit action |
| Exploration across distinct intents or themes | Named sections, categories or thematic rows | Use real grouping; do not invent categories with no content |
| A finite comparable set | Grid, list or table | Equal peers may deserve equal scale; preserve comparison and metadata |
| Curated sampling from several themes | Bounded horizontal rows with a route to the full collection when supported | Show that more exists, support touch and keyboard, preserve position; carousels conceal inventory and are poor for exhaustive comparison |
| A dominant item plus supporting content | Larger primary region with subordinate regions | Priority must come from the brief/content, not a need to manufacture a hero |

Separate **destinations** (where am I?), **filters** (which records?), **sort** (in what order?), and **suggestions** (what could I look for?). A category dropdown can reduce a long chip strip, but hides choices and may increase effort. A short visible filter set may be better. Make the selected scope visible and resettable. Never invent personalized or trending data because the reference displays it.

For role differences, retain an available creator/admin route without assuming it should dominate a viewer/operator surface. Use known roles and permissions; do not fabricate usage statistics to justify priority. Product customization is optional, not a prerequisite to fix poor default organization.

## Compose the screen

Choose composition before effects. Define the primary region, supporting regions, navigation footprint, content rails, scale relationship, and a plausible first viewport with real content. Use the foundations module for detailed typography, spacing, imagery, and materials.

- Give task importance visible form through order, size, placement and grouping. A heading alone cannot make an otherwise uniform page hierarchical. Do not vary equal comparison rows just to appear creative.
- Allocate width deliberately. Removing a sidebar may make cards excessively large or text lines too long; use content constraints and purposeful columns rather than filling every freed pixel.
- Establish a surface hierarchy: page canvas, working/content surface, contextual overlay. Distinguish them only where the grouping helps. A single common surface can unify content; repeated boxes around every element can fragment it.
- Choose coherent relationships among container radii, image radii, avatar shapes, controls, edges and padding. A fancy avatar mask must preserve identity and remain legible at small sizes; it is not a requirement.
- Decide which facts must remain outside media. Compact duration/view badges can reduce repeated text, but need reliable contrast, meaningful separation from artwork, and no obscured captions or sponsorship. Do not move essential titles into images.
- Keep chrome quieter than content. Reserve strong selection treatment for the current destination and relevant actions. Use tonal depth or restrained ambient color only when it supports the chosen visual direction and remains readable.
- Remove filler slogans and explanatory UI copy that merely advertises the design. Use labels and metadata that help people decide or act.

Carry the selected direction through real content, empty results, selected filters, overlays, and mobile; styling one desktop screenshot is insufficient.

## Adapt relationships across sizes

Preserve priorities and context rather than uniformly shrinking desktop geometry. Let labels and content pressure determine when navigation changes form. Keep search reachable and understandable on touch; consider reachability together with visibility, keyboards, safe areas, and platform conventions.

When a wide split view becomes narrow, decide whether to stack, use a sheet, keep an anchored context, or switch between views while preserving state. A media/detail flow may need captions and playback controls readily accessible; do not bury them as decorative secondary settings. Test whether fixed panels leave enough room for the main content at short heights.

Do not add autoplay, random discovery, chat, recommendation algorithms, or customization merely because a reference proposes them. These are product changes, with distinct scope and behavior. If authorized, specify user control, dismissal, sound, reduced motion, keyboard/touch alternatives, and persistence/integration boundaries. Never remove a required feature because a designer personally dislikes it.

## Verify the transformation

Review a matched before/after task and viewport using equivalent content where possible. Separate three conclusions:

1. **Structural:** The new arrangement addresses the stated task or hierarchy issue and retains required capabilities.
2. **Visual:** The rendered composition has coherent scale, grouping, density, imagery and identity. Inspect actual output; source code cannot verify this.
3. **Behavioral:** Main paths, relocation, back/cancel, filters, recovery, keyboard and adaptation work within the declared boundary.

If only token values or syntax were checked, mark the visual conclusion unverified. If only a screenshot was reviewed, mark interaction and responsiveness unverified. Deliver useful work when tools are absent; do not call the result visually validated or professionally proven. Resolve concrete shortcomings without endless ornamental iterations.

## Transfer examples

**Content discovery:** A mixed feed requires scanning unrelated subjects and a large menu repeats verified destinations. Compare a categorized grid with curated thematic rows and a consolidated navigation bar. Keep search, saved content, sponsorship and other required destinations reachable. Give a recommendation region greater scale only if justified. Test hidden inventory, narrow navigation, filtered emptiness and returning from detail. Do not import a video reference's dislike of short-form content or speculative autoplay feature.

**Operational workspace:** An operator must reconcile stock counts with delivery exceptions. Moving everything into carousels to mimic a media app harms comparison. Keep aligned records and the relevant reference visible; group frequent actions by record, consolidate verified redundant routes, and use a restrained density appropriate to repeated work. Test long values, filters, corrections, and audit history.

**Visual polish:** A distinctive catalog already supports finding and comparing products. Repair cramped labels, inconsistent gutters, weak metadata contrast and awkward inset geometry. Retain its navigation and product arrangement unless a concrete defect requires a scoped repair. A professional polish pass need not change architecture.
