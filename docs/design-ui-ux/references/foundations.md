# Visual foundations and deliberate art direction

## Contents
- Decision hierarchy
- Perception translated into design operations
- Art direction
- Typography
- Layout and spacing
- Color
- Shape and elevation
- Imagery and icons
- Motion and surface effects

## Decision hierarchy

Resolve conflicts in this order: user requirements and verified product constraints; task completion and prevention of harm; accessibility; established platform and product conventions; coherent visual hierarchy; decorative expression. Preserve brand character wherever it can coexist with usable behavior. Do not label a taste preference as a usability defect.

Treat measurements below as starting heuristics in CSS pixels for web, not universal platform requirements. In native products use logical platform units and supported text styles. Follow an existing sound system before introducing a new scale.

## Perception translated into design operations

| Principle | Implement | Verify | Boundary |
|---|---|---|---|
| Familiarity / mental models | Reuse expected symbols, naming, navigation, and input behavior. | A control's appearance predicts its actual result. | Do not copy a harmful pattern merely because a competitor uses it. |
| Target acquisition | Increase usable hit area, reduce travel between related steps, anchor contextual menus to their trigger. | Activate with touch and keyboard; inspect edge collisions. | A small icon can have a large hit area; do not enlarge every visible glyph. |
| Proximity and common region | Put related values closer together; use a shared region only when it adds grouping. | Every heading and label belongs unambiguously to its content. | Whitespace can replace a card; a border cannot rescue unrelated content. |
| Choice complexity | Surface frequent actions; group infrequent actions by purpose. | Users can identify a next step without reading every control. | Do not hide essential actions merely to reduce visible count. |
| Aesthetic-usability effect | Improve typographic rhythm, alignment, and coherence. | Separately test successful task completion. | Positive reactions to appearance do not prove usability. |
| Progress toward a goal | Show meaningful completed work, remaining steps, and the next attainable milestone. | Progress reflects real state and survives interruption where supported. | Do not invent percentages or use coercive streak loss. |
| Attention and scanning | Make importance visible through position, size, grouping, and restrained contrast. | Inspect first viewport, full page, and muted/grayscale hierarchy. | Z/F patterns are hypotheses affected by content, language, and task, not predicted eye tracking. |

## Art direction

Before polishing, state a compact visual thesis: “For [audience/task], use [composition/type/material] to convey [relevant qualities], with [one distinctive motif].” Translate each adjective into a decision. “Precise” could mean aligned columns, tabular numerals, explicit units, and restrained borders; “warm” could mean human photography, warm neutrals, and conversational labels. Do not equate professional with monochrome or playful with inaccessible.

For a new product, compare two plausible directions briefly and select one using audience, content, brand, reading density, implementation cost, and accessibility. Do not generate an options presentation unless requested. Choose distinction in composition, typography, imagery, or a specific motif before adding effects. Use one or two expressive moves consistently; avoid unrelated novelty on every component.

| Context | Strong starting direction | Avoid |
|---|---|---|
| Operational or technical tool | Compact aligned structure, calm neutral base, visible controls, numeric clarity | Giant hero, ornamental gradients, every metric in a large card |
| Editorial or cultural site | Distinctive type hierarchy, controlled reading measure, strong image rhythm | Template feature grids unrelated to the content |
| Consumer utility | Familiar navigation, generous touch areas, concise language | Tutorial screens explaining basic controls |
| Premium catalog | High-quality product imagery, disciplined spacing, clear specifications | Low-contrast text, hidden price, decorative page transitions |
| Learning / wellness | Supportive progress, approachable shapes, clear daily task | Meaningless points, shame, distracting celebration during entry |

Preserve a distinctive existing aesthetic when repairing usability. A dark dense interface does not need to become a light spacious marketing page. Brutalist, flat, rounded, editorial, and expressive systems can all be well designed.

### Turn the direction into composition

Write a small composition contract before styling: primary region and purpose; supporting regions; navigation footprint; content rails and width limits; scale relationship; density; surface hierarchy; imagery/crop rule; one purposeful expressive treatment. For a bounded polish request, derive it from the working layout. For redesign, evaluate alternatives using structural-redesign rather than freezing inherited geometry.

Make hierarchy visible through composition and type before effects. A discovery surface might differentiate a primary recommendation region from thematic sampling; an operational table should retain consistent comparison rows. Neither varied card spans nor a giant heading automatically creates useful hierarchy. Protect the task's first viewport and enough content capacity for recognition/comparison.

Assign canvas, content surface and overlay their actual purposes. Use quiet tonal depth, edges, restrained ambient color, or flat separation according to the direction; do not require glass or gradients. Specify how outer shape, inset, media corners and control geometry relate, then inspect optically. Carry the same relationships into empty, selected, overlay and narrow states. Replace generic slogans with useful content rather than adding prose to make a composition look finished.

## Typography

1. Start with the existing licensed family. Otherwise use one legible family with sufficient weights and language coverage; add a display family only when it serves the concept. Use reliable fallbacks and inspect their wrapping.
2. Establish semantic roles before sizes: display, page title, section title, body, label, supporting text, numeric data. Usually four to six sizes suffice for one screen family. Do not force an entire application into three sizes.
3. For an ordinary web UI, start body at 16px with line-height around 1.45–1.6, supporting text at 14px, and controls at 14–16px with adequate line-height. Use 12px sparingly for nonessential compact metadata after inspection. Never treat 8–10px as an accessibility baseline. Dense expert products may justify different values; allow zoom and text scaling.
4. Use approximately 45–75 characters per line for sustained Latin-script prose, adapted for the language. Avoid long centered paragraphs. Use start alignment for lists, forms, and prose; center short headings or empty-state messages only when composition benefits.
5. Establish hierarchy using size and weight before making secondary text faint. A muted value still has to be readable. Avoid excessive all-caps labels, letter spacing on long text, heavy bold everywhere, and overly tight line-height in multiline headings.
6. Keep button text, headings, and control labels as real text. Do not bake them into images. Keep readable strings whole where possible; break long identifiers safely and offer full values when truncating.
7. Align numeric columns by magnitude; use tabular numerals for changing values. Display units, currency, signs, decimal precision, and date/time context consistently. Do not silently change locale.
8. Test long translated labels, mixed script, fallback fonts, zero/negative/large numbers, and 200% text enlargement. Let containers grow. Shrinking text to fit is the last resort, not the default.

## Layout and spacing

Define page/container width, column logic, reading order, and density before styling individual cards. Use alignment rails shared by headings, content, and actions. A desktop workspace can use width for useful parallel information; a reading page should constrain its text measure.

Use a small spacing scale, for example 4, 8, 12, 16, 24, 32, 48, 64. The transcript's eight-unit rhythm becomes a consistent base scale, with smaller increments for optical corrections and dense controls. Do not require type sizes, icon geometry, or every distance to be divisible by eight.

Establish a visible relationship: intra-group gap < inter-group gap < section gap. Example: label-to-value 4–8; between fields 16–24; between sections 32–48. A section heading should be closer to the content it introduces than to the previous section. Prefer padding on the container and gap on the layout over arbitrary margins on every child.

Choose compact, comfortable, or spacious density based on repeated work, device, and content. More whitespace is not inherently better: if increasing row height hides useful comparison or forces repeated travel, retain a compact option. Do not solve crowding with smaller text before reducing redundancy or improving grouping.

Keep related actions near their content. Align repeated action rows. Allow buttons to wrap or stack before colliding. Use full-width actions on narrow forms when useful; do not stretch every desktop button across its card.

Use grids for comparable peers. Use varied spans only when content importance and amount justify them. A bento arrangement is useful for a finite overview with real hierarchy, not an automatic page template. Check the reading order when columns collapse.

## Color

Create role tokens: canvas, surface, raised surface, primary text, secondary text, border, accent, accent text, focus, danger, success, warning, and information. Give each interaction state an explicit role. Separate brand color, interaction emphasis, status, and data-series categories; they are not interchangeable.

Reserve strong contrast and saturation for important meaning. A primary action may use accent while quieter navigation uses neutral selection. A status label may also use color without being clickable: differentiate it by shape, language, and absence of interactive behavior. Do not ban decorative brand color; limit its competition with tasks.

Define meaning with text or icon shape as well as hue. Use error / warning / success colors consistently within the product and relevant culture. If brand red resembles danger, use clear action labels, a separate destructive treatment, and neutral surfaces where needed. Do not recolor all destructive controls to the brand's success green.

Specify foreground/background pairs, not isolated hex values. Check real composite backgrounds for translucent surfaces. Never lower the opacity of a parent containing text just to soften its background. Set surface alpha separately.

Build dark mode as a separate role mapping: preserve hierarchy, reduce glare, avoid automatic color inversion, and check disabled, border, focus, and status states. Do not add dark mode solely to demonstrate extra work.

## Shape and elevation

Choose a small radius family matching brand and density. Rounded corners are an option, not evidence of better cognition. Keep content clear of the curved boundary. For a uniformly inset rounded rectangle, begin with inner radius = max(0, outer radius − inset), then inspect optically. Example: outer 24, inset 8, inner 16. This geometry does not apply unchanged to pills, irregular shapes, or unequal insets.

Continuous corners/squircles can be used when supported. A standard border-radius is a valid portable fallback; do not simulate elaborate curves at the cost of layout or rendering reliability. Do not detach native component behavior merely to imitate a shape.

Use elevation to express layering and surfaces. A soft shadow alone neither creates nor proves clickability. Pair interactive content with a recognizable link/action, hover and focus behavior, and semantics. Keep static tiles visually distinct from buttons. For a light surface, a low-opacity broad shadow plus a subtle edge often works better than a hard dark halo. Adjust for scale and background; the tutorial's 40px blur/10% opacity is an example, not a universal recipe.

Avoid ambiguous nesting: a card containing independent actions is usually a container with explicit targets. Do not nest buttons in links. If the whole card navigates, give secondary actions separate hit areas, focus stops, and clear behavior. Decorative interface illustrations should be inert; tilting them does not automatically solve ambiguity.

## Imagery and icons

Use imagery to communicate product, place, process, identity, or evidence. Match crop, lighting, color treatment, and aspect ratio across peers. Protect focal points at narrow widths. Preserve logos' aspect ratios and identity. Do not fabricate customer logos, certifications, testimonials, product capabilities, or performance statistics for credibility.

Use available authorized assets; verify licensing when sourcing new ones. If no asset is available, choose an intentional typographic/vector composition or a clearly identified placeholder. Never deliver broken remote images. Give informative images useful alternatives and decorative images empty alternatives.

Use one icon family with consistent optical weight, stroke, corner language, and bounding boxes. Align optically with the text baseline. A 24px box does not guarantee equal perceived size across glyphs. Keep familiar meanings: search, back, close, share, overflow. Use labels when ambiguity remains. Do not add an icon to every line for decoration, or use emoji as an inconsistent replacement for a system.

## Motion and surface effects

Animate a change of state or spatial relationship, not the existence of every element. Start around 100–200ms for microfeedback and 180–300ms for short transitions; larger transitions may need longer. These are adjustable heuristics. Do not apply the tutorial's 800ms demonstration duration everywhere. Never delay actual state updates until animation finishes.

Use easing suited to entry, exit, and continuity. Linear motion is valid for constant progress; custom curves are not universally superior. Keep object identity stable across transitions; in design tools preserve corresponding layer names/structure, and in code use stable keys. Provide reduced-motion behavior that retains feedback without spatial movement.

For gradients, choose direction by purpose: linear for an image scrim or directional transition; radial for a local glow; layered soft shapes for ambient depth. Avoid visible banding and distracting noise. Grain is optional and should not interfere with text or inflate payload.

Use glass/blur only if layered content adds useful context. Keep readable surfaces underneath controls, test worst-case scrolling backgrounds, provide opaque fallbacks, and avoid glass on glass. A scroll-edge scrim can protect sticky controls but must not hide focus or intercept input. Do not make a fashionable material mandatory on native or web platforms.

Use masks for clipping or genuine transparency rather than painting a fade in the assumed background color. Restrict masks to the intended visual layer; preserve text, hit areas, shadows, and focus indicators. Benchmark expensive filters and large animated blurs on constrained devices; simplify when needed.
