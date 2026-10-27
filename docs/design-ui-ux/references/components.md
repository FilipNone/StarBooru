# Component selection and behavior contracts

## Contents
- Universal contract
- Actions and buttons
- Cards and lists
- Navigation and disclosure
- Choice controls
- Forms and search
- Feedback and overlays
- Collections and progress

## Universal contract

For each component specify: purpose; content anatomy; visual priority; input trigger; state; resulting effect; keyboard/focus behavior; responsive adaptation; accessible name; unavailable/error behavior. Prefer existing accessible primitives. Native semantics reduce work but do not remove the need to test.

Do not use appearance as a substitute for behavior. Links navigate; buttons perform actions. Selected, focused, pressed, expanded, loading, disabled, and error are different states and may coexist. Hide unavailable content only when it is irrelevant; explain disabled controls when users need to understand the prerequisite.

## Actions and buttons

| Choice | Use | Required detail |
|---|---|---|
| Primary | Dominant next action in a task region | Clear verb + object; visible completion or transition |
| Secondary | Meaningful alternative | Visually subordinate but still discoverable |
| Tertiary / text | Low-priority utility or contextual action | Clear hit area and focus treatment |
| Destructive | Delete, discard, revoke, cancel with loss | Consequence-specific wording; suitable recovery or confirmation |
| Icon only | Familiar compact action | Accessible name, sufficient hit area, tooltip where useful |
| FAB | Frequent primary action in a compatible layout | Stable location, descriptive name, does not cover content |

Usually give a task region one dominant action. Multiple independent panels may have local primaries; repeated products can each have an action. A page need not have a primary button when its purpose is reading, monitoring, or exploration. Do not manufacture a CTA to obey a formula. Secondary buttons can stand alone.

Build size from text, line-height, padding, and hit target. Start horizontal padding greater than vertical (e.g. 16px/10px), with 8px icon gap and a comfortable minimum height. Do not enforce a fixed 2:1 aspect ratio or three-times-text rule. Long labels should grow or wrap without losing meaning. Avoid unnecessary underlined text inside a filled button.

Provide default, hover where supported, visible focus, pressed, unavailable, and pending states. Maintain width during pending; preserve an accessible loading label; prevent duplicate submission while allowing recovery after failure. Use a state change, not a hover scale that displaces adjacent content. Set non-submit buttons explicitly inside forms.

For reversible deletion, provide reliable undo or trash access. For consequential irreversible deletion, describe the exact object, scope, and loss before final confirmation. Do not use “Are you sure?” alone, preselect the destructive option, or require typed confirmations for trivial actions. Keep safe cancellation available and return focus appropriately. The word “Cancel” must distinguish leaving a dialog from canceling a subscription or transaction.

## Cards and lists

Choose a card for a coherent unit with media or mixed content; choose a row/list for rapid scanning and a table for aligned comparisons. Avoid putting every heading, value, and paragraph inside a separate box.

Structure a card: identity/media → title and important facts → supporting detail → applicable action. Cluster metadata by meaning, preserve a consistent image ratio, and align useful comparison fields across repeated cards. Use content-driven height unless matching row height helps scanning; do not crop essential text to make a neat grid.

For image text overlays, use a scrim or solid text region and test the actual crop. A text shadow alone rarely gives a reliable contrast guarantee. If an inset image creates awkward narrow borders, make it edge-to-edge or increase the inset coherently.

Do not nest scrolling carousels inside another horizontal carousel. For a product card with image browsing, make navigation and inner controls unambiguous and provide a simple detail destination. Static badges must not look identical to filter chips or buttons.

## Navigation and disclosure

| Component | Meaning | Behavior and limits |
|---|---|---|
| Global navigation | Major destinations | Persistent identity and current location; stable ordering |
| Mobile navigation bar | Frequent top-level destinations | Labeled items, adequate hit areas, safe area; often 3–5, not a universal numeric law |
| Local tabs | Peer sections of one context | Selected tab + matching panel; do not confuse with sequential steps |
| Breadcrumb | Ancestor hierarchy | Links to ancestors, current item identified; does not represent browsing history |
| Back | Previous context/history | Preserve filters, scroll, and entered data where supported |
| Drawer / hamburger | Secondary or space-constrained navigation | Descriptive trigger, grouped destinations, clear dismissal |
| Profile menu | Identity/account operations | Account, settings, switching, sign in/out; do not hide unrelated primary navigation here |
| Overflow / ellipsis | Extra actions for a specific object/context | Label the context, anchor nearby, group related actions |
| Accordion | Expand content in place | Button heading, expanded state, adjacent panel |
| Menu / dropdown | Reveal choices or actions | Differentiate from a link that directly navigates |

Use familiar platform placement as a starting point; mirror directional conventions for RTL where appropriate. In a redesign, relocate or consolidate destinations when task reasoning justifies it, preserving findability, location semantics and important routes. Do not move a familiar destination solely for symmetry. Keep labels concise but intelligible; shortening “Refreshments” into an unexplained abbreviation is not a fix.

Show selection with more than color when possible: indicator, fill, weight, label, or shape. Avoid mixing icon styles accidentally so an unselected destination looks selected. A current destination and disabled action need different styling.

Keep primary destinations discoverable. Use a drawer when navigation depth, screen space, or established convention warrants it; do not universally ban hamburgers. Use breadcrumbs for meaningful hierarchy even on ordinary websites. Do not replace a hierarchy trail with a history-back button and claim they are equivalent.

For accordions and dropdowns, use a consistent disclosure cue and expanded state. A right-pointing disclosure triangle is valid in many tree/platform patterns; follow the actual system rather than banning it. Avoid hiding required prerequisites or critical errors in collapsed sections without an indication.

Prefer a platform/library menu implementation with expected arrow-key behavior, Escape, initial focus, and focus return. Do not add `role=menu` to an ordinary site navigation list without implementing the associated interaction model. For ordinary navigation links, native tab navigation may be correct. Anchor a popover to its trigger, flip near viewport edges, keep it reachable when scrolling, and distinguish modal drawers from nonmodal popovers.

For true in-page tabs implement tablist/tab/tabpanel semantics and the platform's keyboard pattern. Automatic activation is appropriate only when panel display is immediate; otherwise provide explicit activation. Route navigation that looks like tabs may remain links with current-page semantics. Keep selected content visible after resize; long lists of tabs may need scroll controls, wrapping, or a different navigation structure.

## Choice controls

- **Radio group:** Choose one option from a set. Use a group label, distinct choices, one checked value, and group keyboard navigation. Do not build unrelated independent toggles and call them radios. Allow an explicit “None” option when that is meaningful.
- **Checkbox:** Select independent values; make the associated label clickable. Support a mixed/indeterminate parent state for partially selected children. Explain whether “Select all” means current page, filtered results, or every record.
- **Switch:** Change a binary setting immediately. Name the setting (“Email alerts”), show checked state semantically, save with feedback, and roll back or explain failure. If choices only apply after Submit, a checkbox often expresses the deferred effect better.
- **Chip:** Represent a filter, selected value, suggestion, or input token. Define its type; do not use the same style for static tags and interactive filters. For multi-filter toggles expose pressed/checked state, keep selected filters visible, and provide clear removal or reset. Give each dismiss action its own name.
- **Segmented control:** Choose a small set of peer modes. Use radios or tabs according to whether it changes a setting or the displayed panel. Do not make color imply multiple active modes when only one is active.

Place choice controls consistently at the leading edge for easy scanning, or use a platform-consistent trailing control with an explicit full-row label. Increase the hit area without letting adjacent labels activate the wrong option. Keep selected and unselected outlines discernible.

## Forms and search

Use persistent visible labels, correct input types, appropriate autocomplete/input mode, required/optional indicators, and specific help text. A placeholder is an example, not the only label. Keep instructions adjacent to the field and connect them programmatically.

Validate at useful moments: avoid errors while an untouched field is empty; after leaving a completed field or submitting, explain what failed and how to fix it. On submission failure, preserve all safe inputs, provide a summary for long forms, link to affected fields, and focus the summary or first invalid field as appropriate. Expand hidden erroneous sections. Do not show only “Invalid form.”

Do not disable Submit with no explanation. An enabled Submit that reveals actionable errors is often clearer. If disabling prevents a known incomplete action, make the prerequisite visible and ensure it is discoverable without hovering a disabled element.

Support paste and password managers. Provide a named show/hide-password control. Format forgivingly, accepting common spaces and separators where appropriate; do not rewrite sensitive values unexpectedly. Explain limits before expensive input or upload. For date/time input include locale, time zone, and allowable range when relevant; offer typing as well as a picker.

Place a search-identifying icon at the leading edge; a trailing icon can be an explicit submit action. Distinguish clear, submit, filter, and search-scope controls. Label the scope (“Search invoices”). Enter should submit when appropriate. Clear should remove the query without triggering unrelated navigation.

For autocomplete, support highlighted suggestion, keyboard movement, acceptance, Escape dismissal, loading, and no matches. Keep typed value separate from committed selection when the data model requires it. Debounce remote requests where useful and ignore stale responses; never fabricate a backend search. Present popular/recent suggestions as such, not as matches that were never searched.

## Feedback and overlays

| Pattern | Use | Avoid |
|---|---|---|
| Inline status | Field-specific or contextual information | Toasting every keystroke or small saved setting |
| Toast | Brief nonblocking confirmation | Only copy of a critical error; stealing focus |
| Persistent banner | Ongoing page-level problem or condition | Dismissing a condition that still blocks work without another route |
| Dialog | Focused decision or contained task | Routine information that can remain inline; nested modal chains |
| Tooltip | Optional short clarification | Required instructions, interactive form contents, hover-only access |
| Popover | Contextual interactive details | Treating it like a tooltip with no focus support |
| Loading indicator | Actual pending work | Artificial progress or indefinite activity with no failure state |

For a toast, name what happened and any available next action. Use a suitable noninterruptive live announcement; avoid announcing the same message repeatedly. Keep actionable notices available long enough and expose recovery elsewhere when a short timeout would remove the only route. Use urgent announcements sparingly.

For a modal dialog, provide an accessible title, deliberate initial focus, containment of interaction/focus, a clear close/cancel route, appropriate Escape behavior, and return focus to a valid trigger/context. Make the background inert while modal. Large dialogs must fit the viewport and scroll with actions reachable; do not lock people behind an offscreen close button. Backdrop dismissal must not silently discard meaningful edits.

For tooltips, expose the same information on focus, permit dismissal, and avoid relying on them on touch devices. If information is necessary to complete the task, keep it visible. Interactive content belongs in a popover or dialog.

Distinguish pending from empty. Use skeletons only when the structure is known; preserve dimensions to avoid jumps. When progress is measurable show completed/total; otherwise show an honest activity indicator. Allow retry, cancellation, or background continuation when supported.

## Collections and progress

For carousels, provide an affordance that more content exists: partial next item, explicit controls, or both. Make controls keyboard/touch operable; swipe cannot be the only route. Do not auto-rotate by default. If rotation exists, provide pause and appropriate focus behavior. Do not hide a unique critical action in a later slide.

Use page indicators for position; use labeled tabs for semantically different sections. For many slides use a bounded indicator plus “3 of 12,” not dozens of identical dots or three dots falsely implying three pages. Use actual pagination for long result sets when stable positions and comparison matter. Provide current page, next/previous availability, result counts, and retention of sort/filter state. Infinite loading needs an accessible load route, recoverable position, and a reachable footer when applicable.

For a stepper, show step names, current/completed/future status, and honest progress. Permit returning to edit completed steps; preserve inputs. Do not show every step as clickable unless it is. If the path branches, avoid misleading fixed totals; use named stages or update the denominator clearly.

For gamification, connect the immediate task to an attainable milestone and meaningful outcome. Show earned progress and next action. Handle missed days, offline sync, and completion without punishment or invented achievement data. Provide a quiet alternative to motion-heavy celebration.
