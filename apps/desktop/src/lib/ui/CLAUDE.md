# UI primitives

Reusable components; only silent-breakage rules live here. Ark UI backs the complex ones, in-house wrappers the rest.

## Module map

- Dialogs: `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- The primitives themselves (`Icon`, `Spinner`, `Button`, form controls, `Select`, `Combobox`, text fields,
  `ShortcutChip`, `toast/`) are catalogued in DETAILS § Key files. `ToggleGroup` is segmented, ≠ `RadioGroup`; Tooltip
  is the sibling `../tooltip/tooltip.ts`.

## Must-knows

- **A missing primitive is the cue to add a wrapper here** (`@ark-ui/svelte` and lucide imports are allowlisted to this
  dir; rules in `src/CLAUDE.md`). A new one owes a tier-3 a11y test, a Debug > Components row, and a `design-system.md`
  entry, all check-enforced. Router: `docs/guides/building-ui.md`.
- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the SAME element**
  (`cmdr/dialog-needs-focus-trap`), else Tab leaks into the shortcut-suppressed background: a keyboard lockout.
  `ModalDialog` owns the directive, so `role`-prop callers don't repeat it.
- **Adding a dialog** (soft sheets too): register its id in `SOFT_DIALOG_REGISTRY`, pass it as `ModalDialog`'s
  `dialogId`, and add a gallery row (type error + `dialog-gallery-coverage`). The registry feeds MCP's available
  dialogs.
- **`ModalDialog`'s overlay starts at `inset: var(--titlebar-height) 0 0 0`**, keeping the macOS window-drag region
  live; any full-window backdrop must too. ❌ Keep the drag offset and dragged size OFF the `style` attribute
  (`containerStyle` owns it), and ❌ never restore `overflow: hidden` on `.modal-dialog` (the resize bands hang over its
  edge).
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle`; one-offs need
  `/* allowed-btn-restyle: <reason> */`). `LinkButton` is the ONLY `cursor: pointer` opt-in.
- **Per-component traps.** Each has its own section in `DETAILS.md`:
  - `Tooltip`: keep BOTH detached-trigger guards (`destroy()` cancels the timer, `showTooltip` / `positionTooltip` bail
    on `!el.isConnected`), or a recycled virtual-scroll row fires the 400 ms timer on a dead node.
  - `ShortcutChip`: import `openShortcutCustomization` dynamically in the click handler — a static import drags
    `@tauri-apps/api/webviewWindow` onto a module-eval surface the capability-restricted viewer can't have. Set exactly
    one of `commandId` / `key`.
  - `Select`: the `.select-*` classes are a contract (`SettingSelect`'s `querySelector`, `dropdown_states.go`'s contrast
    matrix). Don't rename them or recolor off the accent tokens.
  - `Combobox` is a text-field-with-suggestions, not a value-bound select: drive its text off `inputValue`, never
    `value` / `items`, which blanks the field on an empty list or a custom name.
  - Text fields: the chrome lives in `app.css` § "Text fields", so restyling every field is ONE edit, and `Combobox` /
    `NumberInput` re-read the same tokens (keep all five in sync). `.text-field` / `.text-field-control` are a stable
    selector contract, and `TextInput` is one-way `value` + `oninput`, never an internal `bind:value`.
  - Toasts: pick a level by feedback kind, not wording; a full all-persistent stack silently drops new ones; a
    pane-local transient toast needs `addToastForPane(pane, …)` or that pane's navigation won't clear it.

DETAILS carries the catalogs, prop tables, and the remaining single-component traps: `StatusBadge`'s
`feature-status-badge` class, `Slider`'s a11y shape, and `containerStyle` as one-off sizing only. Read `DETAILS.md`
before any non-trivial work here: editing, planning, reorganizing, or advising.
