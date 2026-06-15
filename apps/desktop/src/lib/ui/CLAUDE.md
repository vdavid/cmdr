# UI primitives

Reusable UI components used across the whole desktop app. Because nearly every frontend session touches this directory,
only the rules that prevent silent breakage live here. Usage catalogs, prop tables, the toast guide, and decisions are
in [DETAILS.md](DETAILS.md).

## Module map

- Dialogs: `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- Primitives: `Icon` (every inline glyph, via `icons/icon-map.ts`), `Spinner` (the one loading spinner), `Button`,
  `LinkButton`, `CommandBox`, `LoadingIcon`, `ProgressBar`, `Size`, `DateLabel`, `ShortcutChip`, `StatusBadge`,
  `SectionCard`, `ToggleGroup`, `Popover`, `FilterPopover`, `Chip`, plus the `toast/` system. Tooltip lives in
  `../tooltip/tooltip.ts`.
- Ark UI (`@ark-ui/svelte`) is the headless library for complex interactive components; simple ones are our own thin
  wrappers.

Full architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.

## Must-knows

- **Render glyphs via `Icon`, spinners via `Spinner`; don't import `~icons/lucide/*` or hand-roll a ring.** Add new
  glyphs to `icons/icon-map.ts` (the one place lucide is imported, enforced by `cmdr/no-raw-lucide-import`). See
  `docs/guides/icons.md`.
- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the same element** (enforced by
  `cmdr/dialog-needs-focus-trap`). Without it, Tab leaks focus into the suppressed-shortcut background, a full keyboard
  lockout. `ModalDialog` owns the directive, so `role`-prop callers don't repeat it. Opt out only with
  `eslint-disable-next-line cmdr/dialog-needs-focus-trap -- <reason>` (today only `NetworkLoginForm`, a non-modal
  in-pane form). Omit `onEscape` only for dialogs that must swallow Escape (the onboarding wizard).
- **Adding a dialog: add its id to `SOFT_DIALOG_REGISTRY` and pass it as `ModalDialog`'s `dialogId`.** An unregistered
  `dialogId` is a TypeScript error; the registry is sent to the Rust MCP backend at startup, so MCP "available dialogs"
  silently drifts if you skip it. Soft sheets (`OnboardingWizard`) are NOT `ModalDialog` but still register
  (`'onboarding'`) for MCP tracking.
- **The `ModalDialog` overlay starts at `inset: var(--titlebar-height) 0 0 0`, not `inset: 0`**, so the scrim never
  covers the macOS overlay title bar (the OS window-drag region stays live). Any new full-window backdrop outside
  `ModalDialog` must do the same.
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle` flags it; a one-off
  needs `/* allowed-btn-restyle: <reason> */`). `LinkButton` is the ONLY place that opts back into `cursor: pointer`
  (app-wide `cursor: default`); don't roll your own link-styled button.
- **`ShortcutChip` must NOT statically import `openShortcutCustomization`** (it pulls in
  `@tauri-apps/api/webviewWindow`, which must stay out of the chip's module-eval surface so the chip works in the
  capability-restricted viewer window). Load it via dynamic `import()` in the click handler only. Exactly one of
  `commandId` / `key` must be set; a `commandId` chip renders NOTHING when the command has no binding, so conditionalize
  the surrounding prose. DETAILS § ShortcutChip.
- **Tooltip detached-trigger gotcha (corner tooltip)**: a recycled virtual-scroll row removed while hovered never fires
  `mouseleave`, so the 400 ms timer can fire against a detached node. Two guards must both stay: the action's
  `destroy()` cancels its timer, and `showTooltip` / `positionTooltip` bail on `isTriggerDetached(el)`
  (`!el.isConnected`). Don't swap that for a zero-rect heuristic (happy-dom reports zero rects for connected elements
  too).
- **Toasts (full guide in DETAILS § Toast system)**: five levels carry meaning by feedback kind, not wording, so pick
  the lowest-intensity fitting level (`default` is rare on purpose). A full stack of all-persistent toasts silently
  drops new ones (intentional, they hold important state). Action buttons use `Button` mini in a right-aligned
  `.actions` row (default action far right; `DownloadToastContent` is the reference), filled `variant="primary"` only
  for the one genuinely affirmative action, everything else `secondary`.
- **`StatusBadge` class is `feature-status-badge`, NOT `status-badge`** (a Debug-window `:global(.status-badge)` would
  leak onto it). Derive status via `getBadgeStatus(featureId)`, never hardcode it.
- **`containerStyle` is for one-off layout sizing (width/max-width)** only (it exists because stylelint blocks non-token
  CSS custom properties); never for anything that belongs in the design-token system.
- **`Select` has a stable `.select-*` class contract** (`.select-trigger`, `.select-item`, `.select-content`,
  `.option-description`): `SettingSelect` focuses `.select-trigger` by `querySelector` and `dropdown_states.go` keys on
  the literal selector + accent tokens. Don't rename/recolor without both (DETAILS § Select).
- **`Combobox` is a text-field-with-suggestions, NOT a value-bound select**: its text is `inputValue`-driven, decoupled
  from collection membership (`selectionBehavior="preserve"` + `allowCustomValue`). Driving it off `value` / `items`
  blanks the field on an empty/mid-fetch list and on custom names. DETAILS § Combobox.
- **When adding a primitive**, add it to the Components catalog (`routes/dev/components/`) and a tier-3 a11y test (full
  checklist in DETAILS.md).
