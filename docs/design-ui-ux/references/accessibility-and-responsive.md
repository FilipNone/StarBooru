# Accessible and adaptive interfaces

## Contents
- Baseline and scope
- Visual checks
- Input, semantics, and focus
- Responsive layout
- Resilience matrix
- Reference sources

## Baseline and scope

Use WCAG 2.2 AA as the default web design target unless the project specifies another standard. Follow platform accessibility APIs for native products. Treat this file as practical design guidance, not a complete conformance audit or statement of legal applicability. Do not repeat the transcript's five-rule summary, fixed tiny text minimum, EU date, or blanket legal claims. Current legal obligations require separate verification if requested.

Never say “fully accessible” based only on contrast, a screenshot, or an automated checker. Report tested behaviors and remaining limits. Accessibility shapes the initial design; it is not an optional decoration pass at the end.

## Visual checks

- For ordinary text, require contrast at least 4.5:1; for large text, 3:1. Large means at least 18pt (24 CSS px) regular or 14pt (about 18.67 CSS px) bold, with equivalent rendering considered. Do not round a near-pass upward. Logos, incidental/decorative text, and inactive controls have specified exceptions; do not treat important helper text as exempt. [S1]
- For visual information needed to identify active controls and meaningful graphical objects, check at least 3:1 against adjacent colors where the criterion applies. A decorative border need not pass merely because it exists. [S2]
- Express error, success, selected state, and chart identity through text, shape, or pattern in addition to color. A grayscale inspection helps expose dependence on hue; it does not replace contrast measurement.
- Check actual composited colors. On photos, gradients, glass, hover, and scroll states, measure the worst relevant region or add a reliable surface/scrim. Inspect both light and dark modes when supported. A static token-pair calculation cannot certify dynamic backgrounds.
- Allow text enlargement to 200% without losing content or function (subject to the criterion's exceptions). Use growing containers and wrapping; do not disable zoom. There is no universal WCAG 8px, 10px, or 16px minimum font-size rule. [S3]
- Test resilience to increased text spacing: line-height 1.5 times font size, paragraph spacing 2 times, letter spacing .12 times, and word spacing .16 times where applicable. These are override tolerances, not required default styling. [S4]

Use `scripts/contrast.py` for known solid sRGB hex pairs when Python is available. Quote hex colors in shell commands. Example: `python3 scripts/contrast.py '#334155' '#FFFFFF' --minimum 4.5`. It reports pair contrast only and rejects unsupported input; do not mistake it for a screen or accessibility audit.

## Input, semantics, and focus

Prefer semantic HTML/native controls. Provide a useful accessible name, role, value, and state. Connect field labels, errors, and help; use headings and landmarks in a coherent hierarchy. Announce important asynchronous changes without excessive interruption. Alternative text should communicate the image's purpose in context, not merely list pixels.

Walk the primary task with keyboard only. Check logical focus order, visible focus, no unintended traps, operable menus/dialogs, and focus return after dismissal. Use Tab/Shift+Tab for general navigation and expected arrow-key patterns inside composite widgets. Do not make every wrapper a focus stop or use positive tabindex to repair a disordered structure.

Keep focused controls visible. WCAG 2.2 AA's Focus Not Obscured (Minimum) prevents total obscuration by author-created content; this skill's usability target is stronger: reveal the complete control and focus indicator whenever possible. Sticky bars, bottom sheets, cookie notices, and virtual keyboards require explicit checks. [S5]

For web pointer targets, the WCAG 2.2 AA minimum is 24×24 CSS px with spacing, equivalent-control, inline, user-agent, and essential exceptions. Prefer comfortable touch areas around 44–48 CSS px when feasible as a design heuristic. Measure the hit area, not only the icon. Keep adjacent targets distinct and do not claim an exemption without checking it. [S6]

Make drag operations available through a single-pointer alternative unless dragging is essential, and provide keyboard access as applicable. Use move-up/down, selection plus destination, or another direct action for reordering. Do not make swipe, hover, precise dragging, or multi-touch the only path. [S7]

Respect reduced motion. Remove nonessential large spatial transitions and autoplay; preserve immediate state feedback. Provide pause/stop for applicable moving content and avoid flashing. For media, plan captions/transcripts and accessible controls when the deliverable includes it. Do not mark media accessibility tested without appropriate assets and playback checks.

Support accessible authentication: allow password managers and paste, and avoid requiring cognitive tests without an allowed alternative or assistance. Follow the criterion's actual scope rather than declaring every security challenge forbidden. [S8]

## Responsive layout

Design adaptation by content pressure, not named device models alone. Choose breakpoints where labels collide, columns become unreadable, or controls cease to work. A mobile layout is a reordered/reflowed composition, not a uniformly scaled desktop screenshot.

For ordinary web content, check reflow at 320 CSS px wide without loss or two-dimensional page scrolling. This commonly corresponds to 400% zoom from a 1280 CSS px viewport. Necessary two-dimensional content such as comparison tables and maps can have localized exceptions; contain its scrolling instead of making the entire page overflow. [S9]

Test at least a narrow width, an intermediate width near a layout change, and a wide width appropriate to the product. Useful starting samples: 360, 768, and 1440 CSS px, plus 320 reflow when targeting web accessibility. Include short-height/landscape conditions and real zoom if available. Test continuous resize around breakpoints, not only three ideal screenshots.

On narrow screens:

- Preserve the order of meaning: identity/context, task-critical content, related action. Do not visually reorder content into a sequence that conflicts with screen-reader or keyboard order.
- Collapse columns using layout constraints and flexible widths. Apply min-width:0 / equivalent to shrinkable children and safe wrapping to long strings where needed. Do not hide overflow globally to disguise a broken layout.
- Adapt navigation intentionally. Keep important destinations findable and the current location visible. A labeled menu is better than squeezed unlabeled icons.
- Stack action groups before shrinking hit areas. Keep destructive and safe actions distinct after stacking.
- Fit dialogs within available height; let content scroll and keep close/submit reachable. Respond to the on-screen keyboard without covering the active field.
- Account for safe-area insets and fixed/sticky overlays. Reserve content space so the last row and its focused action can scroll above a bottom bar.
- Preserve useful table comparisons; choose horizontal scrolling or a purposeful detail pattern rather than dropping columns silently.

Use logical start/end properties for internationalized layouts. Mirror directional navigation when appropriate; do not automatically mirror logos, media playback, or every icon. Test RTL with realistic content, mixed-direction numbers, long translations, and non-Latin text. Avoid flags as a substitute for language names.

Distinguish CSS pixels, device pixels, native points, and density-independent units. A screenshot exported at 2× cannot be measured as if its bitmap pixels were layout units. Record scale assumptions when only images are available.

## Resilience matrix

| Stress case | Inspect | Fix family |
|---|---|---|
| Long heading / localization | Wrap, title-action collision, clipped labels | Flexible layout, concise wording, appropriate line-height |
| 0, 1, many records | Empty meaning, awkward gaps, performance, pagination | Explicit states, stable structure, scalable collection |
| Very large / negative values | Units, sign, alignment, overflow | Numeric formatting and sufficient width |
| Slow / failed image | Blank region, shifting layout, lost meaning | Reserved aspect ratio, fallback, alt text |
| Slow / failed request | Duplicate submit, false success, lost work | Pending state, safe retry, persistence boundaries |
| Keyboard / screen reader | Focus order, names, announcements | Semantics and behavior, not CSS alone |
| Enlarged text / zoom | Fixed-height clipping, horizontal page scroll | Intrinsic sizing, wrapping, responsive constraints |
| Reduced motion / high contrast | Hidden states, removed focus, motion dependence | Solid visual fallbacks and semantic state |
| Touch / coarse pointer | Hover dependence, small targets, accidental taps | Explicit controls and comfortable hit areas |
| Background variation | Glass text losing contrast | Opaque/frosted surface or controlled scrim |

## Reference sources

Use these primary references to resolve criterion-specific questions. The operational guidance above is self-contained; reading external sources is not a dependency for ordinary design work. Recheck current platform documentation when a task depends on a particular version or API.

- S1: W3C, [Contrast (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
- S2: W3C, [Non-text Contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).
- S3: W3C, [Resize Text](https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html).
- S4: W3C, [WCAG 2.2, Text Spacing](https://www.w3.org/TR/WCAG22/#text-spacing).
- S5: W3C, [Focus Not Obscured (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html).
- S6: W3C, [Target Size (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).
- S7: W3C, [Dragging Movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html).
- S8: W3C, [Accessible Authentication (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/accessible-authentication-minimum.html).
- S9: W3C, [Reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html).
- Behavior patterns: W3C [ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/patterns/), including [modal dialogs](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/) and [tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/). Use matching native/library primitives when possible; ARIA roles alone do not implement interaction.
