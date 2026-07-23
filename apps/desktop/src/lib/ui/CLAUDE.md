# UI primitives

Reusable components. Only silent-breakage rules live here; catalogs, prop tables, and decisions sit in `DETAILS.md`,
which you read before any non-trivial work here.

## Module map

- Dialogs: `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- Primitives: `Icon`, `Spinner`, `Button`, `LinkButton`, `Checkbox`, `Switch`, `RadioGroup`, `ToggleGroup` (segmented
  toggle, distinct from `RadioGroup`), `Select`, `Combobox`, `Slider`, `NumberInput`, `ShortcutChip`, the `toast/`
  system, and more (full list: DETAILS § Key files). Tooltip is the sibling `../tooltip/tooltip.ts`. Ark UI
  (`@ark-ui/svelte`) backs complex interactive components; simple ones are thin in-house wrappers. `@ark-ui/svelte` may
  be imported ONLY here (`cmdr/no-raw-ark-import`'s allowlist is exactly `lib/ui/`); needing it elsewhere means a
  primitive is missing, so add the wrapper.

## Must-knows

- **Glyphs go through `Icon`, spinners through `Spinner`.** Add a glyph to `icons/icon-map.ts`, the only lucide import
  site (`cmdr/no-raw-lucide-import`); never hand-roll a ring.
- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the same element** (enforced by
  `cmdr/dialog-needs-focus-trap`). Without it, Tab leaks focus into the suppressed-shortcut background: a full keyboard
  lockout. `ModalDialog` owns the directive, so `role`-prop callers don't repeat it. Opt out only via the documented
  `eslint-disable` (just `NetworkLoginForm` today). DETAILS § Focus trapping.
- **Adding a dialog: register its id in `SOFT_DIALOG_REGISTRY`, pass it as `ModalDialog`'s `dialogId`, and add a gallery
  row** (enforced by a type error and `dialog-gallery-coverage`). The registry feeds MCP's "available dialogs", so
  skipping it drifts them silently. Soft sheets too. DETAILS § Dialog registry.
- **The `ModalDialog` overlay starts at `inset: var(--titlebar-height) 0 0 0`, not `inset: 0`**, so the scrim never
  covers the macOS title bar, keeping the OS window-drag region live. Any full-window backdrop must too.
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle` flags it; one-offs need
  `/* allowed-btn-restyle: <reason> */`). `LinkButton` is the ONLY `cursor: pointer` opt-in (app-wide
  `cursor: default`); don't hand-roll a link button.
- **`ShortcutChip` must NOT statically import `openShortcutCustomization`**: it pulls in
  `@tauri-apps/api/webviewWindow`, which must stay off the chip's module-eval surface for the capability-restricted
  viewer. Use dynamic `import()` in the click handler. Set exactly one of `commandId` / `key`; a `commandId` chip with
  no binding renders NOTHING. DETAILS § ShortcutChip.
- **Tooltip detached-trigger gotcha**: both guards must stay, else a recycled virtual-scroll row (removed while hovered,
  no `mouseleave`) fires the 400 ms timer on a detached node: `destroy()` cancels the timer, and `showTooltip` /
  `positionTooltip` bail on `!el.isConnected` (not a zero-rect heuristic). DETAILS § Tooltip.
- **Toasts (guide in DETAILS § Toast system)**: pick a level by feedback kind, not wording. A full all-persistent stack
  silently drops new toasts. Pane-local transient toasts need `addToastForPane(pane, …)`, or that pane's navigation
  won't clear them.
- **`containerStyle` is one-off layout sizing (width/max-width) only** (it bypasses stylelint's non-token-CSS-var
  block), never what belongs in design tokens.
- **`StatusBadge` class is `feature-status-badge`, NOT `status-badge`** (the Debug window's `:global(.status-badge)`
  would leak onto it). Derive status via `getBadgeStatus(featureId)`.
- **`Select` has a stable `.select-*` class contract** (the four classes in DETAILS § Select) that `SettingSelect`'s
  `querySelector` and `dropdown_states.go`'s contrast matrix depend on. Don't rename, or recolor off the accent tokens,
  without updating both.
- **`Slider` never renders `Slider.HiddenInput`** (a focusable input in the thumb trips axe's nested-interactive rule;
  nothing here posts a form). Test hooks ride `thumbProps`; readout, ticks, and end labels stay `aria-hidden`.
- **`Combobox` is a text-field-with-suggestions, NOT a value-bound select**: drive its text off `inputValue`, never off
  `value` / `items` (which blanks the field on an empty list or custom name). DETAILS § Combobox.
- **Adding a primitive is an enforced contract**: the component + a tier-3 a11y test (`a11y-coverage`) + a Debug >
  Components section (`ui-primitive-coverage`) + a `design-system.md` § Component patterns entry. Prefer a primitive
  over raw native controls (`cmdr/prefer-ui-primitive`). Router: `docs/guides/building-ui.md`.
