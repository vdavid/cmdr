# Component catalog (Storybook replacement) — plan

In-app, dev-only catalog of every primitive in `apps/desktop/src/lib/ui/`. Lives at
`apps/desktop/src/routes/dev/components/+page.svelte`, surfaced as a nested "Components" entry in the existing Debug
window sidebar. Also reachable in a browser tab at `http://localhost:<port>/dev/components` for full-screen browsing.

## Required reading for anyone touching this work

Read in full before writing or reviewing code on this task:

- [`/AGENTS.md`](../../AGENTS.md) — repo-wide rules, critical-rules section, workflow.
- [`/docs/architecture.md`](../architecture.md) — subsystem map; only the frontend `lib/` and routes columns are
  load-bearing here, but read the whole thing once for context.
- [`/docs/design-principles.md`](../design-principles.md) — UX values that drive every decision below (platform-native,
  radical transparency, keyboard-first).
- [`/docs/design-system.md`](../design-system.md) — the canonical tokens, component patterns, and the existing
  Cmdr-specific deviations from generic-web defaults. The catalog is the visual mirror of this doc.
- [`/docs/style-guide.md`](../style-guide.md) — writing style for all labels, sentence case, no em-dashes, no
  "kill"/"sanity"/"just" wording.
- [`/apps/desktop/src/lib/ui/CLAUDE.md`](../../apps/desktop/src/lib/ui/CLAUDE.md) — the per-primitive API reference.
  Every section in the catalog must match the primitive's documented props and key decisions.

If a subagent is picking this work up, point them at this spec first, then the docs above. The whole list fits
comfortably in a 1M context window; don't summarize.

## Motivation

We want a place where AI agents and (occasionally) David can see every UI primitive in one view, with all relevant
states laid out flat. The agent-readable side is mostly already covered by the docs above, but a visual catalog closes
the loop: agents can render and snapshot the catalog to verify a primitive looks the way the docs claim.

Storybook was the inspiration. We chose against it because:

- 700+ lines of `design-system.md` plus `ui/CLAUDE.md` already cover the "agent reads tokens and props" use case.
- Storybook adds a separate build pipeline, story files per primitive, decorators to fake Tauri APIs, and a UI library
  that runs in plain Chrome not WKWebView. The fidelity gap and maintenance cost don't pay off for solo development.
- A native in-app route inherits the real WKWebView render, the real CSS tokens, the real density variables, and the
  real Tauri IPC. No stubs, no drift, no second build.

## Architecture

### Routes

- `apps/desktop/src/routes/dev/components/+page.svelte` — the catalog itself.
- `apps/desktop/src/routes/dev/components/sections/*.svelte` — one file per primitive (Buttons, Links, Dialogs, Toasts,
  Progress, Loading, Tooltips, SizeBadges, CommandBox, EmptyStates, Groups). Each section is a self-contained Svelte
  component that the page composes top-to-bottom.

### Dev gate

The route renders only when `import.meta.env.DEV` is true. In production builds the component module returns `null` from
its top-level `if (!import.meta.env.DEV) return null` early-out. The actual route file still ships in dev builds; Vite
tree-shakes the section components out of prod via the dead-code path.

Alternative considered: a build-time route filter via `svelte.config.js` `kit.routes`. Rejected because it complicates
the build for a tiny gain — the dead-code path is already free at runtime.

### New shared primitive: `SectionCard.svelte`

Lives at `apps/desktop/src/lib/ui/SectionCard.svelte`. This is the **macOS System Settings "grouped card"** pattern
shown in David's reference screenshot: a label sitting on its own line above a rounded card with a soft background. Used
by the catalog now, and intended for Settings UI refactors later (so its API has to be flexible enough for label+control
rows, not just freeform content).

API:

```ts
interface Props {
  /** Optional label rendered above the card. Omitted for unlabelled groupings. */
  label?: string
  /** Slot for whatever goes inside the card. */
  children: Snippet
}
```

Visual spec (mirrors macOS System Settings on Tahoe):

- Label: `font-size: var(--font-size-sm)` (12px), `font-weight: 500`, `color: var(--color-text-secondary)`, rendered as
  `<h3>` with `margin: 0 0 var(--spacing-sm) var(--spacing-md)`. Sentence case (per style guide).
- Card: `background: var(--color-bg-secondary)`, `border-radius: var(--radius-lg)` (8px), `padding: var(--spacing-lg)`
  (16px). No border in dark mode; very subtle 1px `--color-border-subtle` border in light mode where the card needs to
  separate from the page background. Tested in both modes via existing a11y-contrast check.
- Spacing between adjacent `SectionCard`s: `var(--spacing-xl)` (24px) bottom margin. Applied by the card itself, not by
  consumers, so they stack consistently.

Tests:

- `SectionCard.test.ts` (Vitest): renders with and without label, slot content visible, label is `<h3>`.
- `SectionCard.a11y.test.ts`: axe-core clean in both label states.

Docs:

- New entry in `apps/desktop/src/lib/ui/CLAUDE.md` under "Key files" and a dedicated section with the API and a
  guideline that it's the canonical wrapper for any "grouped card" pattern (Settings, Debug, anywhere).

### Debug sidebar nesting

`apps/desktop/src/routes/debug/+page.svelte` currently has a flat sidebar of section buttons. The change:

- Extend the `SectionId` union with `'components'` plus per-subsection ids: `'components-buttons'`,
  `'components-links'`, `'components-groups'`, `'components-dialogs'`, `'components-toasts'`, `'components-progress'`,
  `'components-loading'`, `'components-tooltips'`, `'components-size-badges'`, `'components-commandbox'`,
  `'components-empty-states'`.
- Change `SECTIONS` from `{id, label}[]` to support optional `children: {id, label}[]`. Render parent as a normal
  sidebar item; if it has children, render them indented (`padding-left: var(--spacing-lg)`) directly below.
- Clicking the parent ("Components") selects `'components'`, which renders the catalog page from the top.
- Clicking a child selects `'components-<sub>'`, which renders the catalog page AND scrolls to `#<sub>` after mount via
  `scrollIntoView({ block: 'start' })`. The catalog page wires a top-level `$effect` that watches an `anchorId` prop and
  scrolls when it changes.
- Active-section highlight: an `IntersectionObserver` in the catalog page updates the selected child as the user scrolls
  through the panel. This means sidebar state syncs both ways (click → scroll, scroll → highlight).

Existing flat sections (Appearance, Drive index, etc.) remain unchanged.

### "Open in browser" link

A small link at the top of the catalog panel:

```svelte
<a
  href="{browserUrl}"
  onclick={(e) => { e.preventDefault(); void openExternalUrl(browserUrl) }}
>
  Open in browser ↗
</a>
```

`browserUrl` is computed at render time from `window.location.origin + '/dev/components'`. The Vite dev port is
ephemeral (set per-instance by `tauri-wrapper.js`, see `/docs/tooling/instance-isolation.md`), and `window.location` is
the only place where the live port is known — no env var, no hardcoded number. Uses the existing `openExternalUrl()`
helper from `$lib/tauri-commands`.

## Layout and content

### Page shell

```
Components
Catalog of every primitive in lib/ui. Add new primitives here — see lib/ui/CLAUDE.md.
Open in browser ↗

[ SectionCard for each primitive, in this order ]
```

Each `SectionCard` has an `id="<sub-id>"` on its outer `<section>` for scroll anchoring.

### Per-section content

Each section is a flat layout (no controls strips, no toggles) showing the relevant matrix of states. Where a state
requires the real DOM `:hover` / `:focus` pseudo-classes, we fake it via a `.demo-hover` / `.demo-focus` modifier class
that copies the primitive's hover/focus styles. The primitive itself isn't modified.

**Buttons** (`#components-buttons`):

A 4-column grid: variant × size × state. Variants: primary, secondary, danger. Sizes: regular, mini. States rendered as
rows: normal, hover, focused, disabled. Each cell is a real `<Button>` with the appropriate class. Total: 24 buttons
laid out in a clean grid.

**Links** (`#components-links`):

Three rows of `<LinkButton>`:

1. In-app button mode: `Open settings`.
2. External `href` mode: `support@getcmdr.com` (`mailto:`); `onclick` intercepts and goes through `openExternalUrl`.
3. Inline-in-prose mode: a sentence with an inline link.

**Groups** (`#components-groups`):

Self-referential — `SectionCard` showing itself: one labelled, one unlabelled, nested example with two cards inside a
parent card to show how the spacing reads. Demonstrates the visual look from the user's reference screenshot.

**Dialogs** (`#components-dialogs`):

Static preview cards (no portals, no overlay) of `ModalDialog` and `AlertDialog` content rendered inline as plain
`<div class="dialog-preview">`s styled to match the real components. Below each preview, a "Trigger real dialog" button
that opens the actual portal-mounted dialog with `dialogId` from the registry. Two static previews per dialog: default
state, and `blur` overlay state.

**Toasts** (`#components-toasts`):

Inline visual previews of one of each level (default, info, success, warn, error) rendered as a static
`<ToastItemPreview>` (a new local-only component that mirrors `ToastItem` minus the timer logic) so all five colors are
visible at once. Below: a row of 5 buttons that trigger the real transient toast for each level via `addToast()`. Also a
row to trigger a persistent toast.

**Progress** (`#components-progress`):

Two static `<ProgressBar>` instances (size `sm` and size `md`) at 60%. A third `<ProgressBar>` cycling 0–100% on a 3s
loop to show animation. A "Show ProgressOverlay for 5s" button that mounts the overlay, with label/detail/eta/progress
populated.

**Loading** (`#components-loading`):

Four `<LoadingIcon>` instances rendered side-by-side in their four message states (default, openingFolder, loadedCount,
finalizingCount). One extra with `showCancelHint=true` to show the cancel-line.

**Tooltips** (`#components-tooltips`):

Four anchor elements with `use:tooltip` variants: plain string, with shortcut badge, rich HTML (multi-line with size
badges), overflow-only (a deliberately truncated path). Anchor labels say "Hover me" / "Hover this truncated path…".

**Size badges** (`#components-size-badges`):

Two rows of inline `<Size>` renders across the five tiers (bytes, KB, MB, GB, TB) — normal and selected (with
`data-size-colors="selected"` on a wrapper to flip palettes).

**CommandBox** (`#components-commandbox`):

Two examples: a short command and a longer one that demonstrates wrapping behavior.

**Empty states** (`#components-empty-states`):

The canonical "Empty folder" message rendered in its own card, centered, in `--color-text-tertiary` at
`var(--font-size-sm)`.

### Responsive layout

- Wide screens (>= 1100px main-pane width): matrices use their natural columns.
- Narrower: each section card stays whole, but the grids inside reflow to fewer columns via
  `grid-template-columns: repeat(auto-fit, minmax(<minWidth>, 1fr))`. No collapsing, no responsive hiding.

No section is collapsible. Everything is visible by default.

### Active-section indicator

`IntersectionObserver` on each `<section id="components-*">` with `rootMargin: '-30% 0px -60% 0px'` to bias the active
section toward the top of the viewport. When a section becomes active, the catalog page calls back to the parent
(`+page.svelte`) so the sidebar highlight follows. Implemented as a callback prop the page passes down.

## Docs updates

Each of these gets one short paragraph + a code link:

- `apps/desktop/src/lib/ui/CLAUDE.md`:
  - New "Key files" row for `SectionCard.svelte`.
  - New "SectionCard" section between LinkButton and LoadingIcon with the API.
  - Final paragraph: **"Adding a new primitive: also add a section to `routes/dev/components/sections/` and a sidebar
    child in `routes/debug/+page.svelte`."** With a minimal template.
- `docs/design-system.md`: a one-line pointer at the top of "Component patterns" linking to the catalog and to
  `SectionCard` as the canonical grouped-card primitive.
- `docs/architecture.md`: a new row in the frontend table for `routes/dev/components/` ("In-app catalog of all UI
  primitives, dev-only").

## Implementation order

1. `SectionCard.svelte` + tests + docs (smallest, used by everything downstream).
2. Debug sidebar nesting (`SECTIONS` type + render + indent CSS).
3. Catalog page shell (`+page.svelte`, dev gate, anchor scrolling, IntersectionObserver wiring).
4. Each per-section file (can be parallelized across subagents once shell exists).
5. Docs updates.
6. Full check suite.

## Acceptance

- `./scripts/check.sh` clean.
- `./scripts/check.sh --check oxfmt` clean.
- Per-section a11y tests for `SectionCard` clean.
- Manually verified in the running app: opening `⌘D` shows "Components" as a parent with children. Clicking a child
  scrolls. Scrolling updates the sidebar highlight. Every primitive renders. "Open in browser" opens the dev URL.
- Visit `http://localhost:<port>/dev/components` directly in a browser tab: primitives that don't depend on Tauri IPC
  render correctly. Primitives that do (dialogs that call `notifyDialogOpened`, `LinkButton` that calls
  `openExternalUrl`) gracefully degrade with a console warning but don't crash the page.

## Out of scope (explicitly)

- Visual regression / Chromatic / Playwright snapshot tests against the catalog. Discussed and deferred.
- A dedicated marketing-site or CF Pages deploy of the catalog. Discussed and deferred.
- Refactoring Settings sections to use `SectionCard`. The component will be ready for it; the refactor is a separate
  pass.
- Per-primitive controls strips with togglable props ("try it" mode beyond what we get from rendering matrices). The
  flat-matrix approach replaced this.

## Risks and mitigations

- **Risk:** Section content drifts from the actual primitive's API over time. **Mitigation:** the `ui/CLAUDE.md` note
  about "also add a catalog section" plus the fact that every primitive author opens this file when adding new ones.
  Long-term we can add a check that grep-matches `ui/*.svelte` against the catalog sections.
- **Risk:** Faked `:hover`/`:focus` styles diverge from the real ones when a primitive's CSS changes. **Mitigation:**
  the demo modifier classes copy the primitive's classes (`btn-primary-hover` is just `btn-primary` plus the rules from
  `:hover:not(:disabled)`); the primitive author updates both. Acceptable churn for a primitive-level catalog.
- **Risk:** `import.meta.env.DEV` accidentally trips in a production build (e.g. previewed locally with `pnpm build`).
  **Mitigation:** also gate the catalog page exports behind `if (!import.meta.env.DEV) return null`, AND verify in the
  acceptance checklist that a production build does not surface the route.

## File list (everything to be created/modified)

Created:

- `apps/desktop/src/lib/ui/SectionCard.svelte`
- `apps/desktop/src/lib/ui/SectionCard.test.ts`
- `apps/desktop/src/lib/ui/SectionCard.a11y.test.ts`
- `apps/desktop/src/routes/dev/components/+page.svelte`
- `apps/desktop/src/routes/dev/components/sections/Buttons.svelte`
- `apps/desktop/src/routes/dev/components/sections/Links.svelte`
- `apps/desktop/src/routes/dev/components/sections/Groups.svelte`
- `apps/desktop/src/routes/dev/components/sections/Dialogs.svelte`
- `apps/desktop/src/routes/dev/components/sections/Toasts.svelte`
- `apps/desktop/src/routes/dev/components/sections/Progress.svelte`
- `apps/desktop/src/routes/dev/components/sections/Loading.svelte`
- `apps/desktop/src/routes/dev/components/sections/Tooltips.svelte`
- `apps/desktop/src/routes/dev/components/sections/SizeBadges.svelte`
- `apps/desktop/src/routes/dev/components/sections/CommandBoxSection.svelte`
- `apps/desktop/src/routes/dev/components/sections/EmptyStates.svelte`

Modified:

- `apps/desktop/src/routes/debug/+page.svelte` — sidebar nesting + new "Components" entry.
- `apps/desktop/src/lib/ui/CLAUDE.md` — `SectionCard` + "adding a primitive" note.
- `docs/design-system.md` — one-line pointer to the catalog.
- `docs/architecture.md` — new row for `routes/dev/components/`.
