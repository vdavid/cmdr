# UI primitives details

Pull-tier docs for `lib/ui/`: architecture, component APIs, and decision rationale. Must-know invariants and gotchas
(the rules that prevent silent breakage) live in [CLAUDE.md](CLAUDE.md).

## Key files

- **`ModalDialog.svelte`**: Central modal container: overlay, dragging, Escape, focus, MCP tracking
- **`focus-trap.ts`**: `use:trapFocus` action: Tab wrapping, focus-leak guard, Escape fallback, trap stack
- **`dialog-registry.ts`**: `SOFT_DIALOG_REGISTRY` array: single source of truth for all dialog IDs
- **`Button.svelte`**: Styled button with variant and size props
- **`Select.svelte`**: Presentational Ark `Select`: items-driven single-pick, the house dropdown (native-`<select>`
  replacement)
- **`Combobox.svelte`**: Presentational Ark `Combobox`: text-field-with-suggestions, async list, free text (model
  picker)
- **`Popover.svelte`**: Generic positioned floater: frosted glass, auto-flip, focus trap, Esc-scoped close
- **`FilterPopover.svelte`**: `Popover` + a labelled section header; the query dialogs' Size / Modified / Search-in
  surface
- **`Chip.svelte`**: Small pill button: filter chip (popover trigger + × clear) or recent pill (badge + truncate)
- **`LinkButton.svelte`**: Link-styled `<button>` (default) or `<a>` (with `href`); the only sanctioned
  `cursor: pointer`
- **`CommandBox.svelte`**: Copyable terminal command (monospace + Copy button)
- **`LoadingIcon.svelte`**: Animated spinner with progressive status text
- **`AlertDialog.svelte`**: Single-action confirmation dialog built on `ModalDialog`
- **`ProgressBar.svelte`**: Reusable progress bar (just the bar, no labels or layout)
- **`Size.svelte`**: Canonical inline byte-count renderer: human-friendly + rainbow tier color
- **`SectionCard.svelte`**: macOS-style grouped card with optional label above; used for Debug/Settings groupings
- **`ToggleGroup.svelte`**: Generic segmented-control primitive: tabs ARIA shape or Ark toggle-group ARIA shape
- **`DateLabel.svelte`**: Canonical inline modified-date renderer: format + per-component age-tier coloring
- **`ShortcutChip.svelte`**: Canonical keyboard-shortcut renderer: live `commandId` mode (clickable) or literal `key`
  mode
- **`StatusBadge.svelte`**: Uppercase stability pill (ALPHA / BETA) for early-stage features; fed by
  `feature-status.json`
- **`toast/`**: Centralized toast notification system: store, container, item

## Not part of this module: soft sheets

`OnboardingWizard.svelte` (in `$lib/onboarding/`) is the canonical soft-sheet implementation: ~90% viewport coverage,
frosted backdrop, no drag / Escape / × button, body owns the close gesture. It's NOT a `ModalDialog` variant — sheets
break almost every `ModalDialog` constraint (full-bleed sizing, no title bar, no Escape, no draggable). Adding sheet
variants to `ModalDialog` would dilute its contract; sheets get their own shell and their own `--sheet-*` design tokens
(see [`docs/design-system.md`](../../../../../docs/design-system.md) § "Soft sheets"), while focus trapping comes from
the same shared `use:trapFocus` action (§ "Focus trapping"). They still plug into the same dialog registry
(`'onboarding'`) so MCP tracking works through the same id-based surface.

Reach for a sheet when you have a multi-step flow the user must commit to. Reach for `ModalDialog` for everything else.

## ModalDialog

Props:

| Prop             | Type                          | Notes                                                                 |
| ---------------- | ----------------------------- | --------------------------------------------------------------------- |
| `titleId`        | `string`                      | Used for `aria-labelledby`                                            |
| `title`          | Snippet                       | Rendered as `<h2>` in the title bar (left-aligned)                    |
| `children`       | Snippet                       | Dialog body                                                           |
| `footer`         | Snippet?                      | Action buttons, rendered in a right-aligned `.modal-footer`           |
| `dialogId`       | `SoftDialogId?`               | Auto-calls `notifyDialogOpened`/`notifyDialogClosed` on mount/destroy |
| `onclose`        | `() => void`?                 | Renders × button; also called on Escape                               |
| `draggable`      | `boolean`                     | Default `true`. Title bar drag moves the dialog.                      |
| `blur`           | `boolean`                     | `true` → 0.6 opacity + `backdrop-filter: blur(4px)` overlay           |
| `containerStyle` | `string`                      | Inline style appended to the dialog element (for sizing, colors)      |
| `role`           | `'dialog'` \| `'alertdialog'` | Default `'dialog'`                                                    |

**Layout convention (macOS-style).** Title and body text are LEFT-aligned; action buttons are RIGHT-aligned with the
primary action last (rightmost). Pass buttons via the `footer` snippet — `ModalDialog` renders them in a `.modal-footer`
that owns the right-alignment, gap, and the dialog's bottom padding, so callers don't hand-roll a button-row. The title
bar's top padding matches the footer's bottom padding (`--spacing-xl`) for vertical balance; bodies use
`0 var(--spacing-xl)` side padding so title, body, and buttons line up flush at the same left inset. A dialog with a
custom button layout (multiple rows, a left-side helper, equal-width buttons) keeps its buttons in `children` and
right-aligns them itself; genuinely centered content (spinners, progress bars, numeric readouts, hero panels like
`AboutWindow`) stays centered.

The overlay element receives `tabindex="-1"` and is focused on mount so Escape/keydown events are captured without a
visible focus ring on the scrim. The overlay also carries `use:trapFocus={{ onEscape: onclose }}` (see § "Focus
trapping" below), so every `ModalDialog` consumer gets Tab containment and the Escape fallback for free.

## Focus trapping (`focus-trap.ts`)

`use:trapFocus` is the one mechanism that keeps keyboard focus inside a modal surface. `aria-modal="true"` is
assistive-tech semantics only — the browser happily tabs focus out of an overlay, into a background where the global
shortcut dispatch is suppressed while the dialog flag is up. That's a full keyboard lockout (the command palette's
Tab-Tab bug: Esc, ⌘⇧P, and Tab all dead, mouse-only recovery). The action does three jobs:

1. **Tab wrapping**: Tab on the last tabbable wraps to the first; Shift+Tab mirrors. The tabbable list is queried fresh
   on every keypress, so dialogs whose controls mount and unmount (the onboarding wizard, filter popovers) stay trapped.
2. **Leak guard**: a document-level capture `focusin` listener pulls focus back if it lands outside the container anyway
   (a programmatic `.focus()` from background code). The pull-back is deferred by one microtask so a closing dialog's
   own focus-restore (`onDestroy` → `previousActiveElement.focus()`) wins — the action's destroy unregisters the trap
   before the microtask runs.
3. **Escape fallback**: if Escape fires while focus is outside the container (the broken state the guard exists for),
   `onEscape` runs. When focus is inside — the healthy state — the action stays out of the way and the dialog's own
   Escape handler works as usual.

Traps **stack**: with several mounted (a `FilterChipPopover` inside `QueryDialog`), only the most recently mounted one
enforces; closing it hands enforcement back down. That's what gives nested popovers their "Esc closes only the popover"
semantics on the leaked-focus path.

Usage: `use:trapFocus={{ onEscape: <close callback> }}` on the dialog's outermost element (the one carrying
`role="dialog"`). Omit `onEscape` only for dialogs that must swallow Escape (the onboarding wizard). All listeners run
in the capture phase, so inner `stopPropagation()` calls (which every dialog makes to shield the file explorer) can't
starve the trap.

**Enforced by**: `cmdr/dialog-needs-focus-trap` (ESLint) — any element with a static `role="dialog"` /
`role="alertdialog"` must carry `use:trapFocus` on the same element. Opt out with an
`<!-- eslint-disable-next-line cmdr/dialog-needs-focus-trap -- <reason> -->` comment above the element; the one
sanctioned case today is `NetworkLoginForm` (an in-pane, non-modal form where the rest of the app stays interactive).
Components passing `role` to `ModalDialog` as a prop don't repeat the directive — the primitive owns it. Tier-2
Playwright coverage lives in `test/e2e-playwright/focus-trap.spec.ts`; the action's unit tests sit next to it in
`focus-trap.test.ts`.

## Dialog registry

`dialog-registry.ts` exports `SOFT_DIALOG_REGISTRY` (a `const` array) and the derived `SoftDialogId` union type. Using a
`dialogId` not in the registry produces a TypeScript error. The registry is sent to the Rust backend at startup so the
MCP "available dialogs" resource stays in sync.

To add a new dialog:

1. Add an entry to `SOFT_DIALOG_REGISTRY` in `dialog-registry.ts`.
2. Pass the new id as `dialogId` to `ModalDialog`. MCP tracking is then automatic.

### Generic close (`dialog-close-registry.ts`)

The MCP `dialog` tool's generic `close` action closes any registered soft dialog by id. `dialog-close-registry.ts` holds
a `Map<SoftDialogId, () => void>` that `ModalDialog` (when it has an `onclose`) and `QueryDialog` (search / go-to-path —
not a `ModalDialog`, so it registers itself) populate on mount and clear on destroy. The backend emits
`mcp-close-dialog { id }`; the main-window router (`listener-setup.ts`) calls `closeDialogById(id)`, which runs the
dialog's own close, unmounting it (→ `notifyDialogClosed` → the backend `SoftDialogTracker` → the tool's
`SoftDialogDisappeared` ack). A dialog rendered without an `onclose` isn't in the map, so `closeDialogById` returns
`false` and the tool reports an honest failure rather than silently closing nothing. `unregisterDialogClose` only clears
an entry that's still its own registration, so a rapid remount can't have the outgoing instance evict the incoming one.

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

## Select

`Select.svelte`: the house dropdown. Presentational, items-driven single-pick built on Ark UI's `Select`. Every
native-`<select>` replacement in scope (settings via `SettingSelect`, viewer encoding / view-mode, transfer volume,
debug panels) renders through it so the macOS-y look lives in one place.

**Scope rule — exactly two value-picker primitives.** `Select` (pick one of a fixed list) and `Combobox` (pick from a
list OR type your own, async list) are the ONLY value pickers; both are Ark-based so keyboard a11y and ARIA come for
free. Converge any new picker onto one of them — don't hand-roll a native `<select>` or a bespoke listbox. Deliberately
NOT value pickers: popovers/menus (`Popover`/`FilterPopover`, context menus, the swatch picker) are a different
primitive, and `CommandPalette` is a bespoke fuzzy+recents launcher — forcing either onto Ark buys risk, not
maintainability.

Props:

- `items: SelectItem[]` — `{ value, label, description?, group? }`. `description` renders as quieter inline text after
  the label (used by `SettingSelect`); `group`, when present on any item, buckets items under Ark `ItemGroup` /
  `ItemGroupLabel` headings (used by the viewer's `EncodingPicker` for Unicode / Western).
- `value: string` — the selected item's `value` (empty string → nothing selected, shows `placeholder`).
- `onChange: (value: string) => void`.
- `onHighlightChange?: (highlightedValue: string | null) => void` — fires on keyboard / pointer highlight.
  `SettingSelect` uses it to apply on highlight.
- `disabled?`, `placeholder?` (default `Select...`), `ariaLabel` (lands on the trigger).
- `contentClass?: string` — extra class on the `.select-content` element (`SettingSelect` sets `custom-highlighted` to
  suppress the checked state on other items while its "Custom…" row is highlighted).
- `portal?: boolean` (default `false`) — teleport the open menu to `document.body`. See "Portal" below.

**macOS pop-up-button look.** The trigger is borderless and hugs its content: value text + a rounded chevron square
(`chevrons-up-down`), not a full-width bezel. The menu is a frosted-glass surface (shared `--color-bg-glass` /
`--color-border-glass` tokens with tooltips and filter-chip popovers; blur dropped under `html.reduce-transparency`).
The checkmark marks the current value on the LEFT (`.select-item-text` is the flex label cell after it); the accent fill
follows the keyboard / pointer highlight (`[data-highlighted]`), so a checked-but-not-highlighted row is plain with just
its checkmark — matching macOS, and distinct from the old "checked = accent bg" behavior.

**macOS overlap positioning (the menu opens _over_ the trigger).** Zag positions the _positioner_ just below the trigger
(`bottom-start`, `gutter: 0`, `flip: false`, `slide: true`); we then translate the _content_ (a child of the positioner,
so the transform never fights zag's own) to land the checked row's label on the trigger's value text, clamped to the
viewport so it stays on screen. The shift is an inline `transform` on the content (`contentStyle`), which does NOT
trigger a zag reposition, so there's no feedback loop. The geometry is the pure, unit-tested `computeOverlapShift`
(`select-positioning.ts`); `Select.svelte` only measures rects and applies the result.

The reveal is driven by the open state through an `$effect` on `isOpen` (set from `onOpenChange`), NOT zag's
`onPositioned` — that callback never fires in this zag version (1.41.x), which is why an earlier `onPositioned`-only
wiring left the menu stuck at `opacity: 0`. The effect retries the measurement across a few `requestAnimationFrame`s
(content mounts and zag places it asynchronously after open) and a `setTimeout` fallback guarantees the content can
never stay invisible if rAF is throttled (unfocused window) or the rows aren't found. The measurement is self-correcting
(it folds the residual gap into the already-applied shift). Content is `opacity: 0` until the first measurement lands,
so it never flashes at the default below-trigger spot. The measurement reads the trigger value via
`rootEl.querySelector` (always in the subtree) and the content via its own `bind:ref` (`contentEl`), so it works whether
or not the menu is portaled.

**Portal (`portal` prop).** Because the menu opens _over_ the trigger, a bottom-of-list selection pushes the top rows
well above the trigger — into whatever chrome sits there. When the menu isn't portaled it's a descendant of its scroll
container, so an ancestor `overflow` clips it and, worse, an ancestor `mask-image` fades its top rows regardless of
z-index (no z-index escapes an ancestor mask). The settings page's `.settings-content-wrapper` has both, which left the
top rows shaded and un-clickable. `portal` teleports the `Positioner` (via Ark's `Portal`) to `document.body` so the
menu floats above all of it, macOS-style; zag still anchors to the trigger, and the design tokens live on `:root` so
body-level content keeps full theming. `SettingSelect` sets `portal`. **Leave it `false` in the viewer window**, whose
restricted capability set assumes no portal-to-body (`ViewModePicker` / `EncodingPicker`); `Combobox` is non-portaled
for the same reason.

**Stable class contract (load-bearing, don't rename):** `.select-trigger`, `.select-item`, `.select-content`,
`.option-description`. `SettingSelect`'s `handleCustomSubmit` focuses `.select-trigger` via `querySelector`, and the
a11y-contrast checker (`scripts/check-a11y-contrast/dropdown_states.go`) keys on the literal
`.select-item[data-highlighted] .option-description` selector + the `--color-accent` / `--color-accent-fg` tokens. The
highlighted item colors must stay on those accent tokens or the contrast matrix breaks. No entrance animation; any
future polish anim must gate behind `prefers-reduced-motion`.

## Combobox

`Combobox.svelte`: presentational text-field-with-suggestions built on Ark UI's `Combobox`. Pick from the list OR type
your own; the list can be empty (cold start) or load async, and the field stays usable throughout. The AI model picker
(settings + onboarding) is the consumer.

**The value model is the whole point (and a trap to get wrong).** This is NOT a value-bound select. Ark's default
`selectionBehavior: "replace"` runs `stringifyMany` on every `value` change, which DROPS any value not in the collection
and writes `inputValue = ""` — blanking the field on an empty / mid-fetch list and on a custom name not in `/models`. So
the component drives the displayed text off `inputValue` (controlled separately from `value`, never derived from
collection membership), sets `selectionBehavior="preserve"`, and passes `allowCustomValue` so a typed value is accepted
on close. `value` is left uncontrolled.

**`preserve` has a flip side that bit us (issue #29): it also stops a list selection from updating the input text**, so
wiring only `onInputValueChange` (the typing event) made clicking a suggestion a silent no-op. The component bridges
Ark's selection event (`onValueChange` → `onInputValueChange(value[0])`) to restore click-to-select while keeping the
custom-value / empty-list protection. Don't drop that handler. `Combobox.svelte.test.ts` pins the click path.

Props:

- `items: ComboboxItem[]` — `{ value, label }` suggestions.
- `inputValue: string` + `onInputValueChange: (inputValue: string) => void` — the controlled displayed text. The
  consumer holds the saved / typed model string here.
- `loading?: boolean` — OUR in-field spinner overlay (a `<Spinner size="sm">` positioned over the input); Ark has no
  loading prop.
- `disabled?`, `placeholder?`, `ariaLabel`, `emptyText?` (shown as a non-actionable `role="option"` row when `items` is
  empty so the `role="listbox"` content satisfies axe's `aria-required-children`).

Open state is left uncontrolled (owned by Ark) with `openOnClick`: clicking the text opens it, the chevron
`Combobox.Trigger` toggles it, typing opens it (`openOnChange`). Don't reintroduce a controlled `open` driven from the
input's focus — the trigger focuses the input on click, so a focus-open handler races the trigger's toggle and flashes
the popup shut. Same standardized Lucide chevron as `Select`. No `Portal`, no entrance animation. Covered by
`Combobox.svelte.test.ts` (the empty-list / custom-value / list-arrives-after-fetch invariants, plus the click-select
path) and `Combobox.a11y.test.ts`.

## Popover

Generic positioned floater anchored to a trigger element. Frosted-glass material (the tooltip's), small radius, hairline
border, soft shadow. Positions itself below the anchor and auto-flips above when there isn't room; clamps horizontally
to the viewport; re-runs on resize. Owns a focus trap (Tab cycles inside, focus returns to the anchor on close) and an
Esc-scoped close that `stopPropagation`s so a host dialog's capture-phase Escape doesn't also fire. Click-outside closes
(on `mousedown`, so a drag that starts inside and ends outside doesn't). Controlled: the parent owns `open`.

Props:

- `anchor: HTMLElement` (required): trigger element for positioning + focus return.
- `open: boolean` (required): controlled visibility.
- `onClose: () => void` (required): fired on Esc / click-outside.
- `ariaLabel?: string`: region label (default "Options").
- `children: Snippet`: the floating content.

The rendered element carries the `.ui-popover` class. Host dialogs that must defer Escape to an open popover detect it
by that class (the query dialog's capture-phase guard checks `dialogElement.querySelector('.ui-popover')`); the E2E
overlay-dismissal helper and `search-filters.spec.ts` use it as a stable selector too. Don't rename it without updating
those.

## Menu

Controlled action menu built on Ark UI's `Menu` (the app's first — context menus are otherwise native/muda). An
items-driven shell: the caller controls `open`, supplies `items`, and reacts to `onSelect`; Ark owns the keyboard
contract (arrow nav, Enter/Space select, Escape dismiss, typeahead) and focus management. Frosted-glass surface with the
shared glass tokens (drops its blur under reduced transparency). The first user is the archive Enter-behavior popup
(`file-explorer/pane/enter-menu.svelte.ts`).

Props: `open` + `onOpenChange` (controlled), `items: MenuItem[]`, `onSelect(value)`, `ariaLabel`, `anchorPoint?` (a
viewport point — the context-menu shape; omit for trigger anchoring), `defaultHighlightedValue?` (the row highlighted on
open), `portal?` (teleport the open menu to `document.body`).

- **`MenuItem` lives in `menu-types.ts`, NOT the component's module script** (unlike `SelectItem`): non-Svelte glue (the
  pane's enter-menu helpers) imports it, and a type imported from a `.svelte` file resolves to `any` under the
  plain-TypeScript lint service. Inline `onSelect`/`onOpenChange` arrows in a `.svelte` consumer hit the same `any`
  inference — pass a named, typed handler (like `SettingToggleGroup`) instead.
- **Point-anchored + no trigger**: opened programmatically via `open` + `anchorPoint`, so there's no trigger element for
  Ark to restore focus to on close. A keyboard-invoked caller (the Enter popup) should `portal` it out of any host with
  an `onfocusin` focus guard and restore focus itself on close.

## FilterPopover

A thin composition of `Popover` plus a labelled section header, for the query dialogs' Size / Modified / Search-in
filter popovers. It's a separate component (not a `variant` prop on `Popover`) so the generic `Popover` stays free of
filter-specific markup. The header is a `<span>` heading over a radio grid by default, or a real `<label for=…>` when
`labelFor` is set (the Scope textarea). The `.popover-section` / `.popover-label` / grid classes live in
`query-ui/filter-chips/filter-popover.css` (a shared global stylesheet, because the grid classes also style the popover
children, which a component-scoped `<style>` can't reach).

Props: `anchor`, `open`, `onClose` (like `Popover`), plus `label: string` (header text), `ariaLabel: string`,
`labelFor?: string` (renders `<label for>`), `sectionClass?: 'size-grid-section' | 'scope-popover'` (widens the
section), `children: Snippet`.

## Chip

A small pill button with two variants:

- `filter` (default): a popover trigger. Default state shows just the label ("Size"); configured state shows "Size: >
  100 MB" plus a decorative `×` clear marker. Carries `aria-haspopup="dialog"` + `aria-expanded`. Activates on click /
  Enter / Space; Backspace on a focused configured chip clears it (the `×` is mouse-only, by design — a nested
  `<button>` would trip axe's `nested-interactive`).
- `recent`: a denser history pill with a leading mode badge (via the `leading` snippet) and a middle-truncated label.
  Activates on click; `onContextMenu` handles right-click "remove from history". No popover ARIA, no clear.

Props: `variant?`, `label` (required), `value?`, `configured?`, `isOpen?`, `disabled?`, `highlighted?`, `onActivate`
(required), `onClear?`, `onContextMenu?`, `ariaLabel?`, `tooltipContent?` (a `TooltipParam`), `leading?` (Snippet),
`chipElement?` (bindable button ref). The two variants render through `class:chip-filter` / `class:chip-recent`
directives (not a `chip--{variant}` interpolation, which the `css-unused` checker can't resolve, and the `--` form trips
its var-definition regex against `:not(...)`). `chip-recent` is also the layout-measurement hook in
`RecentItemsFooter.svelte`.

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

`showCancelHint` adds "Press [Esc] to cancel and go back" (the key rendered as a literal `ShortcutChip`) below the
spinner. The container uses a 400ms `fadeIn` animation where the first 50% is invisible (effectively 200ms before fade
begins), avoiding flash for fast loads.

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

Toast action buttons use `Button` mini primitives in a right-aligned `.actions` row (`justify-content: flex-end`,
`gap: var(--spacing-sm)`, `margin-top: var(--spacing-md)`), with the default action at the far right (macOS
default-button-bottom-right convention) and the alternative to its left. Don't hand-roll bespoke `<button>`s in toast
content components. `DownloadToastContent` is the reference.

Pick the variant by what you want the user to do, not by button position:

- **`variant="primary"`** (filled accent) is reserved for a genuinely affirmative action the user likely wants and that
  moves them forward: "Jump to file", "Keep it on", "Open System Settings", "Restart". At most one per toast, at the far
  right.
- **`variant="secondary"`** (bordered) for everything else: the dismiss/cancel alternative ("Turn it off", "Later",
  "Stop showing these"), AND a lone soft opt-out even when it's the only button ("Disable these notifications", "Don't
  show again"). A muted opt-out rendered as a filled accent button reads as a loud "do this!" on a warn or educational
  toast, which is the opposite of the intent, so keep those secondary. Reserve the filled accent for actions worth
  nudging toward.

Dismissal: `transient` (4s timeout + nav-dismiss, default) or `persistent`.

### Origin pane and scoped dismissal

A toast carries an optional `originPane?: 'left' | 'right'`. It marks a toast as describing THAT pane's directory or a
pane-local action (rename validation/errors, navigate/paste refusals, paste-as-file feedback). Undefined means
app-global: the toast belongs to no pane and no pane's navigation can dismiss it (updater, transfer, downloads,
indexing, licensing, and — deliberately — the clipboard set/cut confirmations, which describe the SHARED clipboard the
other pane consumes, and the SMB reconnect toast, whose own `loadDirectory` would otherwise wipe it instantly).

Two dismissers, both skip persistent toasts:

- `dismissTransientToastsForPane(pane)` removes only transient toasts with `originPane === pane`. This is what pane
  navigation (`listing-loader.ts` `loadDirectory`) and per-keystroke rename validation (`rename-flow`
  `handleRenameInput`) call. A background navigation in one pane (for example an SMB reconnect retry) therefore can no
  longer eat the other pane's or the app's feedback — the incident this design fixes.
- `dismissTransientToasts()` removes every transient regardless of origin. Only the debug panel calls it now.

To make tagging impossible to forget, pane-owned code adds its toasts through `addToastForPane(pane, content, options)`
(which injects `originPane`) rather than `addToast`. `FilePane` closes over its `paneId` to feed the pane-local
controllers it owns (rename flow); DualPaneExplorer-level focused-pane actions (paste refusals, paste-as-file, the
tab-limit refusal) resolve the focused pane at call time. Plain `addToast` stays for app-global toasts. `originPane` is
independent of `toastGroup` (an eviction axis) — don't conflate them. On a same-id re-add, `replaceExisting` keeps the
FIRST toast's `originPane` (consistent with its partial replace of other fields).

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
- `widthPx?: number`: per-toast max-width override (default 360). For a toast whose content needs more room (the
  downloads toast carries a keyboard illustration, so it opts into 432). The container is right-aligned
  (`align-items: flex-end`) and its own `max-width` (440) caps the widest opt-in, so a wider toast just extends leftward
  while default-width toasts keep hugging the screen edge. Plumbed `ToastOptions` → `Toast` → `ToastContainer` →
  `ToastItem` (inline `max-width`).

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
is `FullList.svelte`: it renders the segments straight into its own virtual-scroll grid cell, but it uses the same
`formattedDate(...)` data directly. Keep it that way.

The wrapper sets `font-variant-numeric: tabular-nums` and `white-space: nowrap` so dates align vertically in lists.

See the parent `lib/settings/CLAUDE.md` § "Date display" for the full one-source-of-truth chain (`formatDateForDisplay`,
the per-component tier rules in `age-tier-utils.ts`, and the HTML-string variant for tooltips / MCP responses).

## ShortcutChip

`ShortcutChip.svelte`: the one component that renders a keyboard shortcut anywhere in the UI, so the look stays uniform
and new call sites can't hand-roll a divergent style. Two mutually exclusive modes:

| Prop        | Type            | Notes                                                                                               |
| ----------- | --------------- | --------------------------------------------------------------------------------------------------- |
| `commandId` | `CommandId?`    | Dynamic mode. Renders the command's first shortcut via `getFirstShortcutReactive`, reactively.      |
| `key`       | `string?`       | Literal mode. A fixed key string (toast snapshots, fixed interaction keys). Never clickable.        |
| `clickable` | `boolean?`      | Default `true` in `commandId` mode; ignored (forced non-clickable) in literal mode.                 |
| `size`      | `'sm' \| 'md'?` | Visual density. `md` (default) is the standalone pill; `sm` tightens padding/radius for dense rows. |

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
nested click target would double-activate (it's a competing click target, not a focus-nesting problem — don't "fix" it
with focus management; the non-clickable chip is the fix). Non-clickable chips render a bare `<kbd>`.

**Lazy-import constraint (load-bearing, don't break it).** The chip must NOT statically import
`openShortcutCustomization`. That helper pulls in `@tauri-apps/api/webviewWindow` and window-positioning, which (1) must
stay out of the literal-mode chip's module-eval surface so the chip is importable in the capability-restricted viewer
window with zero Tauri surface, and (2) would reject at runtime in the viewer (no window-creation permission). The chip
loads it via dynamic `import()` inside the click handler only. Keep it that way.

**Visual.** Neutral pill modeled on the Settings `.shortcut-pill` (`--color-bg-tertiary` background, 1px
`--color-border`, `--radius-sm`, `--font-size-xs`), NOT the tooltip's accent chip (`.cmdr-tooltip-kbd` stays its accent
look — different context). The clickable variant adds an accent border + `--color-accent-text` on hover; cursor stays
`default` per the app-wide convention (only `LinkButton` opts into `cursor: pointer`). `size="sm"` shrinks padding to
`0 var(--spacing-xs)` and the corner radius to `--radius-xs` for dense rows where several chips sit side by side — the
command palette (up to three chips per row) is the first consumer.

The `shortcut-<commandId>` anchor-id convention (shared with the Settings section the deep link targets) lives as the
exported `shortcutAnchorId(commandId)` in `lib/settings/settings-window.ts` so it can't drift.

**Where literal chips render the fixed interaction keys (Class B).** Beyond the live `commandId` sites, literal-mode
chips give the uniform key look to fixed (non-customizable) interaction keys: the search dialog's empty-state tip (`⌘N`
/ `⌘H` / `⌘Enter`), the run button's `⏎`, the scope popover's `⌥C` / `⌥V`, the recent-items footer's `⌘H` and popover's
`↑↓` / `Enter`, the viewer's binary-warning `⇧Space` / `Enter`, `LoadingIcon`'s `Esc` cancel hint, the
`PtpcameradDialog` `Ctrl+C`, and the network browser's `⌘R` refresh hint. These keys are static by nature (no registry
command, never clickable); the chip only unifies their appearance.

**Class B sites kept un-migrated (deliberate exceptions):**

- **`ModeChips` / `ToggleGroup` `.tg-hint` glyphs** (`⌥A` / `⌥F` / `⌥R`): these are whisper-quiet tertiary mono text
  baked into a segmented-control cell. A boxed chip reads heavier than the surrounding cell label and fights the
  control's calm rhythm, so the hint style stays. (Pre-decided in the plan.)
- **The `QueryDialog` footer action-button hints** (`.shortcut-hint` / `.shortcut-on-primary`, e.g. `Go to file ⏎`,
  `Show all in main window ⏎`): the key reads as a suffix fused into the button's own label ("Go to file" + "⏎" is one
  phrase). Boxing it fragments the label from the key and the neutral pill clashes on the accent primary button. Kept as
  integrated microcopy, same call as the F-key bar's.
- **`ViewerStatusBar`'s shortcut line** (`W wrap · F tail · ⌘A select all · …`): a dense single tertiary line of six
  key+description pairs. Boxing each key would make the calm status bar loud and risk overflow; the run-on prose form is
  the right treatment here.
- **`KeyboardShortcutsSection`'s "Press ESC to clear"**: lives inside the Settings shortcuts editor, which is out of
  scope for chip migration (it IS the editor).

The litmus test for a future Class B site: migrate when the chip reads same-or-better than the current treatment; keep
the current treatment (and note it here) when the boxed pill genuinely reads worse in a dense or label-fused context.

## StatusBadge

`StatusBadge.svelte`: the small uppercase stability pill (ALPHA / BETA) rendered next to a feature's title. One prop:
`status: 'alpha' | 'beta'` (the `BadgeStatus` type from `$lib/feature-status`). Don't hardcode the status at call sites:
derive it via `getBadgeStatus(featureId)` from `$lib/feature-status`, which reads the repo-root `feature-status.json`
(single source of truth shared with the website; see `docs/feature-status.md`). Stable features return no badge from
that helper, so a graduated feature loses its pill with a one-line JSON edit.

Visual: same token recipe as `ToggleGroup.svelte`'s `.tg-badge` (the "AI" chip): `--font-size-xs` mono, weight 600,
`--color-accent-subtle` background, `--radius-xs`, uppercase via CSS. The class is `feature-status-badge` (NOT
`status-badge`: the Debug window has a `:global(.status-badge)` for the drive-index panel that would leak onto it). The
tooltip text is the canonical definition from the JSON's `statusDefinitions` (exported by `$lib/feature-status`), shared
with the website's pills so the two surfaces can't drift.

Consumers: `QueryDialog`'s title strip (via `QueryDialogConfig.badge`, set by the Search + Selection wrappers) and the
command palette's result rows (via the optional `Command.status` field).

## SectionCard

The canonical "grouped card" primitive that mirrors macOS System Settings on Tahoe: an optional label sitting on its own
line above a rounded card with a soft background. Use it anywhere you'd reach for "a section with a faint, rounded
background" — Settings panels, the Debug window's Components catalog, anywhere we want the native macOS grouping look.

Props:

| Prop       | Type       | Notes                                                                                         |
| ---------- | ---------- | --------------------------------------------------------------------------------------------- |
| `label`    | `string?`  | Rendered as a sentence-case `<h3>` above the card. Omit for an unlabelled grouping.           |
| `id`       | `string?`  | Set on the outer `<section>` element. Use for scroll-to anchors (`#components-foo`).          |
| `gated`    | `boolean?` | Default `false`. `true` dims the card (see below) to signal a closed gate (e.g. FDA-pending). |
| `children` | Snippet    | Slot for whatever goes inside the card                                                        |

`gated` emits `data-gated="true"` on the outer `<section>` and the card owns the dimming rule
(`.section-card-wrap[data-gated='true'] .section-card { opacity: .5 }`), so consumers stop hand-rolling a wrapper div
for it (`FileSystemWatchingSection`'s two FDA-gated cards). It owns only the visual cue — inner controls keep their own
`disabled` state. Omitted when `false` (no attribute), so `[data-gated]` selectors and tests stay clean.

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
effect. The trade-off is manually managing focus trapping and Escape handling: the overlay `tabindex="-1"` + `focus()`
on mount captures keydowns, and the shared `use:trapFocus` action (§ "Focus trapping") keeps Tab inside. Don't rely on
the mount-focus alone — an overlay-tabindex-only setup leaks Tab to the underlying app.

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

## i18n

User-facing copy in these primitives lives in the `ui.*` catalog (`$lib/intl/messages/en/ui.json`), resolved through
`tString()` / `<Trans>`; `cmdr/no-raw-user-facing-string` is enforced on `lib/ui/`. Defaulted copy props (`AlertDialog`
`buttonText`, `Select` `placeholder`, `Popover` `ariaLabel`, `Combobox` `emptyText`) keep the prop optional and fall
back to the catalog value via a `$derived`, so a consumer can still override. `LoadingIcon`'s cancel hint is a `<Trans>`
with an empty `<key></key>` tag: the snippet renders `<ShortcutChip key="Esc" />` and ignores (renders) its children.
Runtime + key rules: [`$lib/intl/CLAUDE.md`](../intl/CLAUDE.md).
