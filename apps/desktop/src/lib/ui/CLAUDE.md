# UI primitives

Reusable UI components used across the entire desktop app.

## Key files

| File                  | Purpose                                                                                        |
| --------------------- | ---------------------------------------------------------------------------------------------- |
| `ModalDialog.svelte`  | Central modal container: overlay, dragging, Escape, focus, MCP tracking                        |
| `dialog-registry.ts`  | `SOFT_DIALOG_REGISTRY` array: single source of truth for all dialog IDs                        |
| `Button.svelte`       | Styled button with variant and size props                                                      |
| `LinkButton.svelte`   | Link-styled `<button>` (default) or `<a>` (with `href`); the only sanctioned `cursor: pointer` |
| `CommandBox.svelte`   | Copyable terminal command (monospace + Copy button)                                            |
| `LoadingIcon.svelte`  | Animated spinner with progressive status text                                                  |
| `AlertDialog.svelte`  | Single-action confirmation dialog built on `ModalDialog`                                       |
| `ProgressBar.svelte`  | Reusable progress bar (just the bar, no labels or layout)                                      |
| `Size.svelte`         | Canonical inline byte-count renderer: human-friendly + rainbow tier color                      |
| `SectionCard.svelte`  | macOS-style grouped card with optional label above; used for Debug/Settings groupings          |
| `ToggleGroup.svelte`  | Generic segmented-control primitive: tabs ARIA shape or Ark toggle-group ARIA shape            |
| `DateLabel.svelte`    | Canonical inline modified-date renderer: format + per-component age-tier coloring              |
| `ShortcutChip.svelte` | Canonical keyboard-shortcut renderer: live `commandId` mode (clickable) or literal `key` mode  |
| `toast/`              | Centralized toast notification system: store, container, item                                  |

## Not part of this module: soft sheets

`OnboardingWizard.svelte` (in `$lib/onboarding/`) is the canonical soft-sheet implementation: ~90% viewport coverage,
frosted backdrop, no drag / Escape / × button, body owns the close gesture. It's NOT a `ModalDialog` variant — sheets
break almost every `ModalDialog` constraint (full-bleed sizing, no title bar, no Escape, no draggable). Adding sheet
variants to `ModalDialog` would dilute its contract; sheets get their own shell, their own `--sheet-*` design tokens
(see [`docs/design-system.md`](../../../../docs/design-system.md) § "Soft sheets"), and their own focus-trap. They still
plug into the same dialog registry (`'onboarding'`) so MCP tracking works through the same id-based surface.

Reach for a sheet when you have a multi-step flow the user must commit to. Reach for `ModalDialog` for everything else.

## ModalDialog

Props:

| Prop             | Type                          | Notes                                                                 |
| ---------------- | ----------------------------- | --------------------------------------------------------------------- |
| `titleId`        | `string`                      | Used for `aria-labelledby`                                            |
| `title`          | Snippet                       | Rendered as `<h2>` in the title bar                                   |
| `children`       | Snippet                       | Dialog body                                                           |
| `dialogId`       | `SoftDialogId?`               | Auto-calls `notifyDialogOpened`/`notifyDialogClosed` on mount/destroy |
| `onclose`        | `() => void`?                 | Renders × button; also called on Escape                               |
| `draggable`      | `boolean`                     | Default `true`. Title bar drag moves the dialog.                      |
| `blur`           | `boolean`                     | `true` → 0.6 opacity + `backdrop-filter: blur(4px)` overlay           |
| `containerStyle` | `string`                      | Inline style appended to the dialog element (for sizing, colors)      |
| `role`           | `'dialog'` \| `'alertdialog'` | Default `'dialog'`                                                    |

The overlay element receives `tabindex="-1"` and is focused on mount so Escape/keydown events are captured without a
visible focus ring on the scrim.

## Dialog registry

`dialog-registry.ts` exports `SOFT_DIALOG_REGISTRY` (a `const` array) and the derived `SoftDialogId` union type. Using a
`dialogId` not in the registry produces a TypeScript error. The registry is sent to the Rust backend at startup so the
MCP "available dialogs" resource stays in sync.

To add a new dialog:

1. Add an entry to `SOFT_DIALOG_REGISTRY` in `dialog-registry.ts`.
2. Pass the new id as `dialogId` to `ModalDialog`. MCP tracking is then automatic.

## Tooltip (`../tooltip/tooltip.ts`)

Global tooltip system via Svelte action. Apple-style frosted glass appearance, 400ms show delay, auto-flips above/below
viewport.

Usage:

```svelte
import { tooltip } from '$lib/tooltip/tooltip'

<!-- Plain text -->
<span use:tooltip="Simple tooltip">...</span>

<!-- Rich HTML (supports <br>, <span class="...">, etc.) -->
<span use:tooltip={{ html: 'Line one<br>Line two' }}>...</span>

<!-- Text + keyboard shortcut badge -->
<span use:tooltip={{ text: 'Save', shortcut: '⌘S' }}>...</span>

<!-- Only show when text overflows -->
<span use:tooltip={{ text: longText, overflowOnly: true }}>...</span>

<!-- Live rich content: the caller owns a hidden host, the action adopts its CONTENT child on show -->
<span use:tooltip={{ contentEl: tooltipContent }}>...</span>
<div hidden>
  <div bind:this={tooltipContent}>
    <ProgressBar value={progress} size="sm" />
    <span>{label} · {eta}</span>
  </div>
</div>

<!-- No tooltip (pass undefined or '') -->
<span use:tooltip={undefined}>...</span>
```

`TooltipParam` type: `string | { text?, html?, shortcut?, overflowOnly?, contentEl? } | null | undefined`.

The tooltip element has `white-space: pre-line` and uses global CSS classes, so `<span class="size-mb">` etc. work
inside `{ html }` tooltips. The `html` variant renders via `innerHTML`; only use with trusted content.

### Live rich content (`contentEl`)

For a tooltip whose content updates while it's shown (a ticking counter, a `ProgressBar` whose width transition must
survive), pass `contentEl: HTMLElement`. Precedence is `contentEl` > `html` > `text`/`shortcut`. On show the action
**reparents** that element into the shared tooltip; on hide / destroy / param swap it moves it back. Because the DOM
node persists, Svelte keeps updating it in place: transitions glide instead of resetting, counters don't flicker, and
all the existing tooltip machinery (delay, positioning, glass styling, a11y) comes along.

Rules for callers:

- **The element must stay owned by a hidden host you render.** Wrap the live content in a `<div hidden>` host and pass
  the inner content element (not the host) as `contentEl`: `<div hidden><div bind:this={content}>…</div></div>`. The
  action borrows the inner element while shown and returns it to the hidden host on hide, so your reactive bindings keep
  working. Don't hand it a one-off element you then drop. **Pass the content element, not the hidden host itself — an
  adopted element keeps its own `hidden` attribute and would render invisible inside the tooltip.**
- **Give the content a stable `min-width` (no `ResizeObserver`).** The action positions once after attaching, then can't
  see later content mutations, so growing text would push out of the tooltip without repositioning. Fix the width with
  CSS on the content element (a fixed `min-width`, like `IndexingStatusIndicator`'s 200px tooltip column) so the
  measured size stays steady as counters tick.
- **`aria-describedby` reads the tooltip's text**, so the content must carry the real label (and ETA) as text, not only
  a decorative bar.

The reparenting is singleton-safe: the shared tooltip element is app-wide, so if another trigger's tooltip shows (or the
live update path re-renders) while your element is adopted, the action returns yours to its host first instead of
orphaning it. If the host unmounted mid-show, the element is just detached (guarded by `isConnected`). Covered by
`tooltip.test.ts`.

**Gotcha (detached trigger → corner tooltip)**: the show is deferred by a 400ms timer. A virtual-scroll row recycled
while hovered is removed from the DOM **without** firing `mouseleave`, so the timer would otherwise survive and later
fire against a detached node — whose `getBoundingClientRect()` is all-zero, dumping the tooltip in the window's top-left
corner. Two guards prevent this and must both stay: (1) the action's `destroy()` cancels a pending timer it owns
(tracked via `timerNode`), since `activeElement` is still null during the delay window; (2) `showTooltip` /
`positionTooltip` bail when `isTriggerDetached(el)` (`!el.isConnected`). Don't replace the `isConnected` check with a
zero-rect heuristic — happy-dom reports zero rects for every connected element, so it false-positives the whole test
suite. Covered by `tooltip.test.ts`.

## Button

Variants: `primary` | `secondary` (default) | `danger`. Sizes: `regular` (default) | `mini`. Extends
`HTMLButtonAttributes` so all native button attributes pass through.

`.btn-primary` renders `color: var(--color-accent-fg)` on `background: var(--color-accent)`. Both are derived at runtime
by `lib/accent-color.ts`:

- `--color-accent`: the active macOS system accent (or Cmdr gold if the user picked "App" color).
- `--color-accent-fg`: picked dynamically as `#000000` or `#ffffff` (whichever wins WCAG contrast on the active accent
  via `readableFgOn` in `lib/utils/srgb-mix.ts`). Apple Purple is the only system accent today that takes white text;
  every other system accent reads black. The static `app.css` value (`#1a1a1a`) is a first-paint fallback only.
- `--color-accent-hover`: lightens by 15% (light mode) or 10% (dark) for accents that take black text, and **darkens**
  by the same amount for accents that take white text (Purple). That keeps the readable-fg contrast on hover.

The contrast checker (`scripts/check-a11y-contrast`) mirrors all of this in its accent matrix and runs against the 9
runtime variants. **Don't restyle `.btn-*` colors from a scoped feature component** — `scripts/check-btn-restyle` will
flag it. If you genuinely need a one-off variant, add the rationale via a `/* allowed-btn-restyle: <reason> */` comment.

## LinkButton

Use this for anything that should look and behave like a link. Renders a `<button>` by default (in-app actions like
"Open settings", "Show format help"), or an `<a>` when you pass `href` (for external URLs like `mailto:`, `https://`
that your `onclick` intercepts and routes through `openExternalUrl()`). It is the **only** place in the app that opts
back into `cursor: pointer`; Cmdr globally sets `cursor: default` on `html` and `<a>` for native macOS feel
(`app.css:363-366`), and stylelint blocks `cursor: pointer` everywhere else (`.stylelintrc.mjs:38`). Don't roll your own
link-styled button or anchor with raw CSS; the cursor opt-in stays in one place by convention.

Hover keeps the resting accent-text color (the lighter `--color-accent-hover` doesn't meet 4.5:1 contrast on white). The
underline is enough affordance.

The `href` mode includes a per-line eslint disable for `svelte/no-navigation-without-resolve`. That rule wants
SvelteKit's `resolve()`, which is for internal routes; we route external URLs through `openExternalUrl()` after
`event.preventDefault()` in `onclick`. The `<a href>` is decorative: it gives screen readers the right semantics and
preserves "right-click → Copy link." For SvelteKit-internal navigation, don't use `LinkButton`; use `<a>` with
`resolve()` directly.

## LoadingIcon

Progressive status text driven by props (mutually exclusive, evaluated top-down):

1. `finalizingCount` set → "All N file/files loaded. Sorting your files, preparing view..."
2. `loadedCount` set → "Loaded N file/files..."
3. `openingFolder` true → "Opening folder..."
4. Default → "Loading..."

`showCancelHint` adds "Press ESC to cancel and go back" below the spinner. The container uses a 400ms `fadeIn` animation
where the first 50% is invisible (effectively 200ms before fade begins), avoiding flash for fast loads.

## ProgressBar

Reusable progress bar component: just the bar, no labels or layout. Consumers arrange their own labels around it.

Props:

| Prop        | Type           | Notes                                                                                       |
| ----------- | -------------- | ------------------------------------------------------------------------------------------- |
| `value`     | `number`       | 0–1 fractional progress                                                                     |
| `size`      | `'sm' \| 'md'` | Bar height + radius. `sm` = 4px / `--radius-xs`, `md` = 8px / `--radius-sm`. Default `'md'` |
| `ariaLabel` | `string?`      | Accessible label for screen readers                                                         |

Uses `role="progressbar"` with `aria-valuenow` / `aria-valuemin` / `aria-valuemax`. Fill transitions via
`transition: width 0.15s ease-out`.

Consumers: `IndexingStatusIndicator` (size `sm`, in the indexing tooltip), `TransferProgressDialog` (size `md`, dual
bars for size + file count).

## Toast system (`toast/`)

Centralized toast notifications with stacking, levels, and two dismissal modes.

- **Store** (`toast-store.svelte.ts`): Module-level `$state` array. `addToast(content, options?)` accepts a `Snippet` or
  plain `string`. Optional `id` for dedup (replace in place). Max 5 visible.
- **Container** (`ToastContainer.svelte`): Mounted once in `(main)/+layout.svelte`. Fixed top-right, stacks vertically.
- **Item** (`ToastItem.svelte`): Frame, close button, auto-dismiss timer for transient toasts.

Five levels. Pick by what kind of feedback the toast carries, not by how the message reads:

- **`default`** (no color, the fallback): factual neutral status with no action needed and no value judgement.
  In-progress indicators that get replaced on completion (`Connecting directly…`), "nothing happened" reports
  (`No mounted shares from ${host}` after a disconnect that had nothing to disconnect). Rare in practice — most toasts
  carry some signal.
- **`info`** (blue): notices the user should attend to, including action confirmations. Restart hints
  (`Restart Cmdr to apply…`), instructional cues triggered by a wrong move (`Use F5 to copy files from MTP devices`),
  soft explanations of unexpected UI state (`Your file disappeared from view because hidden files aren't shown.`),
  background activity the user opted into (`Error report sent`), routine action confirmations (`Copied N items`,
  `N items ready to move`), the Quick Look Space-key educational hint, and "operation completed but nothing actually
  changed" outcomes (`Copy complete: skipped all 5 files, nothing was copied`).
- **`success`** (green): one-shot confirmations that something meaningful succeeded. Host removed, share disconnected,
  password forgotten, direct SMB upgrade succeeded, transfer completed with at least one file actually transferred.
- **`warn`** (amber): the user tried something that didn't go through, but no operation failed and no data is at risk.
  Soft refusals and limits hit: `Tab limit reached`, `Can't remove discovered hosts`, `Share 'X' not found on Y`,
  `No files on the clipboard. Copy files first with ⌘C.`, `No recently closed tabs in this pane.`, rename-conflict
  notices that don't abort the rename.
- **`error`** (red): an attempted operation actually failed. Examples: `Couldn't remove ${host}`,
  `Direct connection failed: …`, `Couldn't delete saved password`. Inline "Send error report…" button auto-attaches for
  string-content errors.

Tiebreaker: when unsure between two adjacent levels, pick the lower-intensity one. Frequent feedback should be quiet;
the user can read the text. Color is for the few cases where attention is warranted. Note that `default` is rare on
purpose — if the toast carries any meaning at all (an attempted action, a refusal, a completed operation), one of the
other four levels usually fits.

Common mistakes to avoid: don't pick `default` for soft refusals (those are `warn`); don't pick `success` for "completed
but nothing changed" outcomes (those are `info`); don't pick `info` for in-progress spinners (those are `default`);
don't pick `warn` when an op actually failed (that's `error`); don't pick `error` for soft refusals like "tab limit
reached" (that's `warn`).

Dismissal: `transient` (4s timeout + nav-dismiss, default) or `persistent`.

Call `dismissTransientToasts()` on pane navigation to clear stale feedback.

`ToastOptions` extras for component-content toasts that have their own action buttons:

- `closeTooltip?: string`: tooltip text shown on hover/focus over the X button. Set this when the toast also has its own
  buttons (for example, an inline "Cancel"), so users can tell what each control does. Without it, no tooltip renders.
- `onDismiss?: () => void`: fires only when the user clicks X (or the inline "Send error report…" link). Auto-dismiss on
  timeout and programmatic `dismissToast()` calls do NOT trigger it. Use this when the caller needs to remember "the
  user closed this," for example to avoid re-adding a toast that's tied to long-running background work.
- `toastGroup?: string`: opt into a per-group cap so a burst of homogeneous notifications can't push unrelated toasts
  off the screen. When set, the new toast counts against a per-group cap BEFORE the global cap of 5 applies. On a full
  group, the oldest transient in that same group is evicted first (FIFO-in-group), even if the global cap hasn't been
  hit. Persistent toasts in the group block group-level eviction the same way they block global eviction.
- `maxInGroup?: number`: per-group cap. Defaults to 5 when `toastGroup` is set, ignored otherwise. A higher value than
  the global cap (5) is silently clamped by the global cap kicking in second.

### Hover behavior

All transient toasts pause their auto-dismiss timer while the pointer is over them. On pointer leave, the timer either
resumes with the remaining time or starts a 2-second grace window, depending on whether the user got any unhovered time
to read the toast:

- If the timer had made any progress before the hover started, leaving resumes the timer with the captured remainder so
  the user gets the rest of the natural visibility window they would have had without the hover.
- If the pointer entered before the toast had any unhovered visibility (the only reading window was during hover),
  leaving starts a `HOVER_LEAVE_GRACE_MS` (2-second) grace timer. This catches accidental cursor exits and gives the
  user a beat to actually read the toast before it disappears.

`HOVER_LEAVE_GRACE_MS` is exported from `toast/index.ts` for any future tuning. Persistent toasts have no timer and the
hover handlers no-op for them.

## CommandBox

`CommandBox.svelte`: monospace terminal command with a one-click Copy button and 2-second "Copied!" feedback. Takes a
single `command` string prop. Handles clipboard internally (`copyToClipboard` with `navigator.clipboard` fallback).
Parent controls spacing via its own wrapper. Used in `PtpcameradDialog`, `MtpPermissionDialog`, and `ShareBrowser`.

## Size

`Size.svelte`: canonical inline byte-count renderer. Takes `bytes: number | null | undefined` and optional `fallback`
(default `''`). Always human-friendly (`"1.02 MB"`), always colored with the active rainbow tier class
(`size-bytes`/`size-kb`/`size-mb`/`size-gb`/`size-tb`). Respects the `appearance.fileSizeFormat` setting (binary vs.
decimal) and follows palette swaps via the `data-size-colors` attribute on `<html>` automatically.

Use this in Svelte templates: `<Size bytes={entry.size} />`. For HTML string contexts (tooltips, error messages, prose
that goes through `{@html}`), use `colorizeSizeString(text)` from
`$lib/file-explorer/selection/selection-info-utils.ts`: pass an already-formatted size string (for example, from
`formatFileSizeWithFormat` or the legacy `formatBytes` in `$lib/tauri-commands`) and it wraps the value in the right
tier span.

Free-space displays (volume picker, status bar, usage-bar tooltip, transfer-dialog destination info) intentionally DON'T
tier-color the numbers — for "free space" big-is-good, and red GB would falsely signal "low space". They use the plain
formatters from `disk-space-utils.ts` with `formatFileSizeWithFormat` for the inner formatter. The usage-bar itself
stays color-coded (driven by `getDiskUsageLevel`, which is the right signal for free space).

The `<Size>` component always renders the friendly dynamic form regardless of the user's `listing.sizeUnit` choice
(bytes / dynamic / kB / MB / GB). That setting governs the file-list size column where apples-to-apples comparison
matters; tooltips, dialogs, breadcrumbs, and inline `<Size>` callouts read more clearly with the self-describing dynamic
format. The file-list column renders `formatSizeForDisplay` directly (passing the active unit) because it also needs the
mismatch-warning + cursor-row neutralization treatment.

## DateLabel

`DateLabel.svelte`: canonical inline renderer for a file's modified date. Wraps `formattedDate(modifiedAt)` from
`lib/settings/reactive-settings.svelte.ts` so it picks up the current `appearance.dateTimeFormat` and
`appearance.dateColors` automatically. Each segment carries its own age-tier class (year / month / day / time) so the
active date palette colors components independently; literals (separators) and tier-less segments render plain. Empty
input (`null` / `undefined`) renders nothing.

Props:

| Prop         | Type                          | Notes                                                                                   |
| ------------ | ----------------------------- | --------------------------------------------------------------------------------------- |
| `modifiedAt` | `number \| null \| undefined` | Unix timestamp in seconds (matches `FileEntry`)                                         |
| `class`      | `string?`                     | Optional class on the outer `<span>` wrapper, in case the parent needs to scope spacing |

Use this anywhere you'd otherwise reach for `formatDateTime` or hand-roll a date string. The one consumer that opts out
is `FullList.svelte`: its column-alignment story needs the two halves rendered into specific elements (`.date-left` /
`.date-right`), so it uses the same `formattedDate(...)` data directly. Keep it that way.

The wrapper sets `font-variant-numeric: tabular-nums` and `white-space: nowrap` so dates align vertically in lists.

See the parent `lib/settings/CLAUDE.md` § "Date display" for the full one-source-of-truth chain (`formatDateForDisplay`,
the per-component tier rules in `age-tier-utils.ts`, and the HTML-string variant for tooltips / MCP responses).

## ShortcutChip

`ShortcutChip.svelte`: the one component that renders a keyboard shortcut anywhere in the UI, so the look stays uniform
and new call sites can't hand-roll a divergent style. Two mutually exclusive modes:

| Prop        | Type         | Notes                                                                                                    |
| ----------- | ------------ | -------------------------------------------------------------------------------------------------------- |
| `commandId` | `CommandId?` | Dynamic mode. Renders the command's first effective shortcut via `getFirstShortcutReactive`, reactively. |
| `key`       | `string?`    | Literal mode. A fixed key string (toast snapshots, fixed interaction keys). Never clickable.             |
| `clickable` | `boolean?`   | Default `true` in `commandId` mode; ignored (forced non-clickable) in literal mode.                      |

Exactly one of `commandId` / `key` must be set (a dev-time error otherwise).

**Truthfulness rule.** A `commandId` chip is a _claim about live app behavior_ ("pressing this does X"), so it reads the
reactive store and updates live when the user rebinds. It renders **nothing** when the command has no binding — callers
embedding it in prose must conditionalize the surrounding sentence. A `key` chip is just typography. Keeping both modes
in one component is what guarantees the uniform look, while the prop split keeps the rule mechanical: customizable →
`commandId`; fixed → `key`.

**Clickable variant.** In `commandId` mode (and `clickable` not set to `false`) the chip is a real
`<button type="button">` wrapping the `<kbd>`, with `aria-label="Customize the {command name} shortcut"` and a
"Customize this shortcut" tooltip. Clicking deep-links to Settings > Keyboard shortcuts (`openShortcutCustomization`).
Set `clickable={false}` when the chip sits inside another interactive control (palette rows, F-key bar buttons) where a
nested click target would double-activate. Non-clickable chips render a bare `<kbd>`.

**Lazy-import constraint (load-bearing, don't break it).** The chip must NOT statically import
`openShortcutCustomization`. That helper pulls in `@tauri-apps/api/webviewWindow` and window-positioning, which (1) must
stay out of the literal-mode chip's module-eval surface so the chip is importable in the capability-restricted viewer
window with zero Tauri surface, and (2) would reject at runtime in the viewer (no window-creation permission). The chip
loads it via dynamic `import()` inside the click handler only. Keep it that way.

**Visual.** Neutral pill modeled on the Settings `.shortcut-pill` (`--color-bg-tertiary` background, 1px
`--color-border`, `--radius-sm`, `--font-size-xs`), NOT the tooltip's accent chip (`.cmdr-tooltip-kbd` stays its accent
look — different context). The clickable variant adds an accent border + `--color-accent-text` on hover; cursor stays
`default` per the app-wide convention (only `LinkButton` opts into `cursor: pointer`).

The `shortcut-<commandId>` anchor-id convention (shared with the Settings section the deep link targets) lives as the
exported `shortcutAnchorId(commandId)` in `lib/settings/settings-window.ts` so it can't drift.

## SectionCard

The canonical "grouped card" primitive that mirrors macOS System Settings on Tahoe: an optional label sitting on its own
line above a rounded card with a soft background. Use it anywhere you'd reach for "a section with a faint, rounded
background" — Settings panels, the Debug window's Components catalog, anywhere we want the native macOS grouping look.

Props:

| Prop       | Type      | Notes                                                                                |
| ---------- | --------- | ------------------------------------------------------------------------------------ |
| `label`    | `string?` | Rendered as a sentence-case `<h3>` above the card. Omit for an unlabelled grouping.  |
| `id`       | `string?` | Set on the outer `<section>` element. Use for scroll-to anchors (`#components-foo`). |
| `children` | Snippet   | Slot for whatever goes inside the card                                               |

Spacing between adjacent `SectionCard`s is built in (`var(--spacing-xl)` bottom margin); consumers don't have to manage
it. Stack them top-to-bottom and they read correctly.

Anatomy:

- Label: `<h3>` styled at `var(--font-size-sm)`, weight 500, `var(--color-text-secondary)`, sentence case (style guide).
- Card: `var(--color-bg-secondary)` background, `var(--radius-lg)` corners, `var(--spacing-lg)` padding, 1px
  `var(--color-border-subtle)` border in both themes.

This is intentionally minimal: it's a wrapper, not a layout. Inner content is whatever you want — a grid of buttons, a
label-and-control row, a paragraph of text. For a Settings-style label-left + control-right row inside the card, compose
with the existing setting components (or, later, a dedicated `SectionRow` primitive when the pattern needs codifying).

## Component catalog

Every primitive listed above also has a section in the in-app, dev-only Components catalog at
`apps/desktop/src/routes/dev/components/+page.svelte`. The catalog renders matrices of states (all `Button` variants ×
sizes × states in one grid, every toast level, every loading message, etc.) so agents and humans can see the visual
contract of a primitive at a glance. It's reachable in the running app via Debug window (`⌘D`) → "Components", or
directly in a browser tab at `http://localhost:<port>/dev/components`.

When you add a new primitive to `lib/ui/`:

1. Add a row to the "Key files" table above.
2. Add a dedicated section in this file describing the API and key decisions.
3. Add a section file at `apps/desktop/src/routes/dev/components/sections/<Name>.svelte` showing the primitive's states
   flat (no toggles, just everything visible at once). Import it from the catalog page and add a sidebar entry in
   `apps/desktop/src/routes/debug/+page.svelte` under the "Components" parent.
4. Add a Vitest behavior test (`<Name>.test.ts`) and a tier-3 a11y test (`<Name>.a11y.test.ts`) colocated next to the
   component.

## ToggleGroup

Generic segmented-control primitive used by Settings (`SettingToggleGroup`) and the search / selection mode chips. One
visual contract, two ARIA shapes selected via the `semantics` prop.

Pick the shape from the user's perspective, not the visual:

- `semantics: 'tabs'` when the active option drives a UI mode (the user hears "tab 2 of 4, Filename, selected"). Renders
  `<div role="tablist">` with `<button role="tab" aria-selected>` children. Arrow keys cycle through interactive options
  skipping disabled ones; the active option carries `tabindex=0` and the rest `tabindex=-1`, so Tab from a sibling input
  lands on the active option directly.
- `semantics: 'toggles'` when the active option picks a stored value (the user hears "toggle button, kB, pressed").
  Wraps Ark UI's `ToggleGroup.Root` + `ToggleGroup.Item` in single-select mode.

Both shapes share visual chrome (`.tg-root`, `.tg-item`, `.tg-badge`, `.tg-label`, `.tg-hint`) so they render
identically.

Props:

| Prop        | Type                      | Notes                                                                      |
| ----------- | ------------------------- | -------------------------------------------------------------------------- |
| `semantics` | `'tabs' \| 'toggles'`     | Picks the ARIA shape (see above)                                           |
| `value`     | `string`                  | The currently active option's value                                        |
| `options`   | `ToggleGroupOption[]`     | Per-option config (see below)                                              |
| `onChange`  | `(value: string) => void` | Fires on activation; does not fire when clicking the already-active option |
| `ariaLabel` | `string`                  | Accessible name for the tablist / toggle-group root                        |
| `disabled`  | `boolean?`                | Default `false`. Short-circuits all clicks and disables every option       |

`ToggleGroupOption` shape:

| Field       | Type       | Notes                                                                                           |
| ----------- | ---------- | ----------------------------------------------------------------------------------------------- |
| `value`     | `string`   | Identity                                                                                        |
| `label`     | `string`   | Visible text                                                                                    |
| `badge`     | `string?`  | Small uppercase pill rendered before the label (for example `AI`)                               |
| `hint`      | `string?`  | Mono tertiary text after the label (for example `⌥A`); `aria-hidden`                            |
| `disabled`  | `boolean?` | Per-option disable. Combine with `tooltip` for "visible-disabled" affordances                   |
| `tooltip`   | `string?`  | Tooltip text; stays active on hover/focus even when `disabled` is set (the "Coming soon" idiom) |
| `ariaLabel` | `string?`  | Override the accessible name when the visible label alone isn't enough                          |

When to add a wrapper (like `SettingToggleGroup`) versus using `ToggleGroup` directly: wrap when the options come from a
single source of truth that the consumer already owns (the settings registry, a config object). Otherwise, use the
primitive directly.

## Ark UI

Uses `@ark-ui/svelte` as the headless component library for complex interactive components (Dialog, Tabs, Select,
Checkbox, Switch, Slider, Radio Group, etc.). Chosen over Bits UI and Melt UI for: 45+ components (vs ~20-25), clean
tree-shaking (1.33 MB unpacked), Zag.js FSM robustness (prevents invalid states), full focus/escape control (disable FSM
defaults with `={false}`, implement custom logic in callbacks), and scoped CSS selectors
(`[data-scope="dialog"][data-part="content"]`) that work with vanilla CSS. Team-maintained by Chakra UI team. Simple
elements like `<Button>` are our own thin wrappers (a button needs no headless library).

## Adding a component-level a11y test (tier 3)

Cmdr runs a three-tier a11y strategy; see `docs/design-system.md` § "Automated contrast checks" and
`apps/desktop/test/e2e-playwright/accessibility.spec.ts` for tiers 1 and 2. Tier 3 runs axe-core against a component
mounted in Vitest/jsdom, covering structural a11y (ARIA, labels, focusable-when-enabled) in milliseconds. Contrast is
tier 1's job; focus traps and Escape-return-focus are tier 2's.

Helper: `$lib/test-a11y` exports `expectNoA11yViolations(container)`. Same axe ruleset as E2E, minus `color-contrast`
and `region` (both misfire in jsdom; see the helper's comments).

Template: colocate `ComponentName.a11y.test.ts` next to the component.

```ts
import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import MyComponent from './MyComponent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('MyComponent a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MyComponent, {
      target,
      props: {
        /* default props */
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
```

Write one test per meaningful state (default, disabled, error, open/closed, etc.). For components that hit Tauri IPC,
mock `$lib/tauri-commands` at the top of the file. Set `CMDR_A11Y_DEBUG=1` to log pass/violation counts per call when
investigating why a test passes silently.

Current coverage: ~60 component-level `.a11y.test.ts` files covering dialogs, file explorer panes, settings components
and sections, search, command palette, toasts, crash/licensing/onboarding, and MTP dialogs. Skipped tests (marked
`it.skip` with a `TODO:` + axe rule ID) flag real a11y findings the team hasn't fixed yet. Do NOT remove those skips
without fixing the underlying component. Each skip has a concrete fix noted in the test file.

## Key decisions

**Decision**: Custom `ModalDialog` with manual overlay + drag logic instead of the native `<dialog>` element. **Why**:
Native `<dialog>` doesn't support drag-to-reposition, and its `::backdrop` is not style-customizable enough for the blur
effect. The trade-off is manually managing focus trapping and Escape handling, but the overlay `tabindex="-1"` +
`focus()` on mount approach is simpler than a full focus-trap library.

**Decision**: Dialog registry is a `const` array with `satisfies` (not an `enum` or `Record`). **Why**:
`as const satisfies` gives a union type for `SoftDialogId` that TypeScript can narrow, while also letting the array be
iterated at runtime to register with the Rust MCP backend. An `enum` can't be iterated without extra transformation, and
a `Record` would split the ID from its metadata.

**Decision**: `containerStyle` prop for one-off sizing instead of CSS custom properties or class names. **Why**: The
project's stylelint config blocks custom properties that don't match the `(color|spacing|font)-` prefix convention.
Inline style strings bypass this restriction for layout-only overrides (width, max-width) that don't belong in the
design token system.

**Decision**: Toast content accepts both `string` and `Component<any>` (Svelte component). **Why**: Simple notifications
are strings. Interactive toasts (update restart, AI download) need buttons and state, so they're full Svelte components.
The toast item renders strings as `<span>` and components via `{@const}` + render. No wrapper needed.

**Decision**: Toast dedup uses an optional `id` key with in-place replacement rather than preventing duplicates.
**Why**: The update toast and AI toast need to update their content as state changes (e.g. download progress) while
keeping the same slot in the stack. Replacing in place avoids the visual flicker of remove-then-add.

## Key gotchas

- The Svelte 5 snippet named `title` shadows any prop also named `title`. In `AlertDialog` this is handled by
  destructuring as `title: dialogTitle`.
- `containerStyle` exists because stylelint blocks non-standard CSS custom properties (any not matching
  `(color|spacing|font)-` prefix). Use it for one-off sizing instead of CSS vars.
- `blur` prop applies `backdrop-filter` which triggers GPU compositing; use sparingly.
- The overlay starts at `inset: var(--titlebar-height) 0 0 0`, **not** `inset: 0`, so the scrim never covers the macOS
  overlay title bar — the OS window-drag region stays live while a dialog is open. Any new full-window backdrop (a
  bespoke overlay outside `ModalDialog`, like the command palette or query dialog) must do the same. `--titlebar-height`
  is per-window (27px default in `app.css`; the viewer overrides it to its taller toolbar via a `display: contents`
  wrapper in `viewer/+layout.svelte`).
- When the toast stack is full (5 toasts) and all are persistent, new toasts are silently dropped. This is intentional:
  persistent toasts represent important state (update ready, AI installing) and should not be evicted by transient
  feedback.

## Dependencies

- `$lib/tauri-commands`: `notifyDialogOpened`, `notifyDialogClosed`
- `apps/desktop/src/app.css`: all CSS variables used here must be defined there
