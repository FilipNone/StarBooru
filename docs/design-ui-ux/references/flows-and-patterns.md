# Tasks, flows, and product patterns

## Contents
- Derive a working brief
- Build information architecture
- Model a complete flow
- Select a product pattern
- Content and microcopy
- Research and evidence

## Derive a working brief

Turn “make it professional” or “design the best app” into a concrete working hypothesis using the product and artifacts available. Record:

- User role and relevant conditions: experience, task frequency, device/input, environment, time pressure, accessibility needs.
- Job and trigger: “When [situation], help [person] achieve [outcome].”
- Product goal: task completion, comprehension, successful discovery, fewer mistakes, repeat utility, or another supported outcome.
- Primary flow and success condition; secondary flows within scope.
- Existing brand, content, component, data, permission, technical, and platform constraints.
- What is observed, supplied, inferred, and still unknown.

Prefer task-based proto-personas over invented biographies. Do not invent interviews, demographic needs, analytics, or customer preferences. If a fact is missing, use a reversible default and note it briefly. Ask only if conflicting requirements, missing essential assets, or consequential domain ambiguity prevents a responsible next step. A generic lack of color preference is not a blocker.

When a requested tactic seems disconnected from the goal, reason from the goal: “more playlist creation” could serve sharing or retention. Do not silently replace the user's goal with engagement. Treat alternatives as hypotheses. For a utility, shorter sessions with successful completion may be better than more time in the app.

## Build information architecture

Inventory content objects and actions first: people, orders, documents, products, events, settings, and their relationships. Group by the user's task vocabulary rather than internal teams. Separate destinations from actions and categories from filters. Define primary navigation, local navigation, contextual actions, and account controls.

Map which screen owns each piece of data and action. If the same operation appears in several places, reuse the same state and wording. Keep identity/current workspace visible where mistakes across projects or accounts are costly. Do not require onboarding to explain an architecture that could be simplified.

Before building many screens, solve a representative vertical slice: entry/list → selected detail → meaningful action → feedback/recovery. Include the hardest relevant constraint, such as long data, a form, or small-screen navigation. Then extend the system to secondary screens. A slice is an implementation checkpoint, not permission to leave the requested product incomplete.

## Model a complete flow

Use this compact contract for each important flow:

```
Entry and prior state:
User intent and prerequisites:
Steps: trigger -> resulting state -> next choice
Success and where the result appears:
Back/cancel/undo behavior:
Empty/loading/invalid/failure/permission variants:
Responsive and keyboard route:
Persistence and integration boundary:
Evidence to verify:
```

Count user decisions, repeated entry, travel, waiting, and recovery as well as clicks. Remove duplicate questions and unnecessary pages. Keep useful review, comprehension, consent, and recovery even if they add a step. A single overwhelming screen can be worse than a short sequence.

Choose sensible defaults from known context. Reuse previously provided values where appropriate, while letting users inspect and edit them. Do not preselect paid additions or infer consent. Preserve unfinished work across ordinary navigation when the product supports it; warn before actual loss rather than on every exit.

Derive states conditionally, not as mandatory boilerplate on every screen:

| Condition | Required experience |
|---|---|
| First use, no data | Explain the value and one concrete way to begin |
| No filtered matches | Preserve query/filters; distinguish from empty account; offer clear/reset |
| Pending | Honest status, stable layout, duplicate-action protection |
| Partial data | Show what is available and identify incomplete scope |
| Input invalid | Specific correction near field; retain entered data |
| Request failed | Clear outcome uncertainty and a safe retry route |
| Offline | Explain capabilities; do not promise sync unless implemented |
| Permission denied | Explain restriction; offer an available route; do not fake access |
| Session expired | Preserve safe work and provide reauthentication path |
| Stale/conflicting data | Show current version or conflict resolution; avoid silent overwrite |
| Success | Show changed object/value and next relevant action |
| Destructive result | Reliable undo/trash when available, or explicit finality |

For consequential transactions, distinguish “request failed” from “transaction definitely failed.” Avoid encouraging duplicate payment/booking on uncertain completion. Preserve authentication, authorization, and domain rules when changing appearance.

Test transitions between states, not only isolated examples of each state. For a saved collection, exercise initial empty → add → save → modify → reload, then remove the last item → save → reload. An intentionally cleared collection is a valid persisted state even if first-use guidance suggests adding an item. Do not let validation resurrect stale saved content. Define whether changes autosave or require Save, and what happens when leaving with unsaved changes. Test failed reads/writes without silently discarding current work or replacing recoverable stored data. Keep undo/recovery reachable in the same interaction context, including when a modal makes the background inert.

## Select a product pattern

### Marketing, service, or landing page

Make the first viewport answer what it is, whom it serves, its concrete value, and what the visitor can do next. Use real product/service detail instead of generic “unlock your potential.” Arrange evidence near the claim it supports. Present scope, pricing basis, or process clearly when supplied. Use an honest CTA (“Request a quote,” “View catalog”) instead of an unsupported “Start free.”

Use visual rhythm suited to the content: hero, evidence, explanation, relevant examples, decision information, closing action. This is a reasoning sequence, not a required six-section template. Do not invent reviews, clients, awards, urgency, or conversion numbers. Ensure the contact/purchase route actually works within the deliverable.

### Dashboard / operational workspace

Start with the decisions the user must make. Put exceptions and actionable information ahead of vanity metrics. Each measure needs a label, value, unit, period, and useful comparison when known. Distinguish unavailable from zero. Keep filters and date ranges visibly scoped. Use tables for exact comparison, charts for patterns, and clear drill-down for detail.

Avoid rainbow metric cards. Use a mostly calm base so status and data encoding stand out. Keep status colors separate from arbitrary data-series colors. Charts need units, legends or direct labels, sensible axes, and an accessible summary or data view. Do not display fabricated live data; label examples in prototypes.

### Data table / list

Provide clear headers, coherent alignment, sort direction, filter status, count, and appropriate row actions. Keep identifiers and key comparisons visible. Scope bulk selection precisely. Preserve scroll and selection during safe updates. Confirm batch destructive scope. A click on a checkbox or row action must not also open the row.

On small screens choose by task: retain a scrollable comparison table, prioritize essential columns with a detail view, or transform rows into labeled records. Do not convert to cards when cross-row comparison is the main job. Make horizontal scrolling discoverable and keyboard accessible.

### Commerce / booking

Keep identity, variant, availability, actual total, and next step clear. Explain fees before commitment, preserve the cart, and separate optional upgrades. Keep quantity changes and remove actions distinct. Validate addresses and dates without destroying progress. Show order/booking confirmation only when the integration confirms it; demos must identify simulated behavior.

### Content / catalog / discovery

Offer search when users know what they want and browsable categories when they do not. Use titles and metadata that support recognition. Keep recommendations distinct from user's saved content and sponsored content. Preserve reading/list position after visiting a detail. Avoid endless carousels that conceal the inventory.

Distinguish intent and available time where the content supports that choice: known-item retrieval, exploratory discovery, short sampling, sustained reading/listening, or exact comparison. Choose named groups and layout from actual content rather than assuming one mixed feed suits every intent. Consult structural-redesign when changing navigation or collection arrangement. Compare thematic rows with a categorized grid/list; keep full inventory and filtered-result access when supported. Make a primary recommendation region larger only when its priority is justified. Do not fabricate personalization, trends, duration or consumption preferences.

If reading/viewing and discussion are part of the requested task, retain access to the content while opening contextual discussion/detail. Adapt simultaneous desktop context purposefully on narrow screens and preserve position/state. Treat autoplay previews, random/shuffle discovery, social rooms and customization as separately scoped features, not automatic consequences of a visual redesign.

### Settings / profile / account

Group identity, preferences, notifications, security, and billing by task. Distinguish immediate switches from staged edits. Provide save status and unsaved-change recovery. Keep dangerous actions separated but findable. Cancellation, privacy settings, and sign-out must remain intelligible and accessible.

### Onboarding / learning / progress

Ask for the minimum information needed for initial value. Request permissions in context with an explanation. Make nonessential introductions skippable. Give meaningful progress and an early useful result. Explain the next attainable milestone; do not require decorative gamification in a professional utility.

### Maps, kiosks, native, and unusual environments

For maps, provide a list/detail alternative, named controls, and selected-location state. For kiosks, prioritize large reachable controls, privacy reset, timeouts that warn and can be extended when appropriate, and recovery from abandoned flows. For native apps, follow the installed platform's controls, navigation, safe areas, text scaling, and assistive APIs. For CLI/TUI, preserve the same task/state principles using readable grouping, keyboard commands, text alternatives to color, and explicit statuses. Do not force a website's visual layout onto these environments.

## Content and microcopy

Use actual or realistically shaped content early. Lorem ipsum hides hierarchy and overflow problems. Mark example data as examples when authenticity matters. Write labels as concise verbs or concrete nouns. Prefer “Save address” over “Continue” when it completes a specific action; use “Continue” when the next step is genuinely transitional.

Errors should state what failed, what remains intact, and how to recover. Avoid blame and unexplained codes. Put technical details in an expandable area if they help support. Use consistent names for the same object and operation throughout. Match the user's product language and locale, including decimal separators, date order, units, and grammatical form.

## Research and evidence

Use provided research and analytics before intuition. If comparison is useful and browsing is available, inspect a few relevant current examples for behavior and structure; do not require a paid service or copy protected brand assets. Separate observed behavior from assumed business reasons. If browsing is unavailable, use established patterns and state the evidence limit.

Use a contextual journey when environment or motivation changes the solution: trigger → action → interruption/friction → opportunity. Do not fill a template with invented emotions or an irrelevant hour-by-hour life story.

Distinguish evidence types: usability tasks reveal completion and errors; interviews reveal explanations; surveys collect reported perceptions; analytics record defined events; click maps do not establish where eyes looked. A synthetic walkthrough is a heuristic check, not a user study.

When proposing an experiment, identify the change, outcome, guardrails, audience, and decision rule. Do not claim uplift from a redesign without observed data. An A/B test needs valid assignment and analysis; raw click count is not automatically success.
