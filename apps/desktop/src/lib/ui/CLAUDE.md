# UI primitives

Reusable components; only silent-breakage rules live here. Ark UI backs the complex ones, thin in-house wrappers the
simple ones.

## Module map

- Dialogs: `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- Primitives: `Icon`, `Spinner`, `Button`, form controls (`ToggleGroup` is segmented, ≠ `RadioGroup`), `Select`,
  `Combobox`, `TextInput` + `TextArea` (+ `text-field-types.ts`), `ShortcutChip`, `toast/`, more in DETAILS § Key files.
  Tooltip is the sibling `../tooltip/tooltip.ts`.

## Must-knows

- **A missing primitive is the cue to add a wrapper here** (`@ark-ui/svelte` and lucide imports are allowlisted to this
  dir; the rules live in `src/CLAUDE.md`). A new primitive owes a tier-3 a11y test, a Debug > Components row, and a
  `design-system.md` § Component patterns entry, all check-enforced. Router: `docs/guides/building-ui.md`.
- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the SAME element**
  (`cmdr/dialog-needs-focus-trap`), else Tab leaks into the shortcut-suppressed background: a full keyboard lockout.
  `ModalDialog` owns the directive, so `role`-prop callers don't repeat it.
- **Adding a dialog** (soft sheets too): register its id in `SOFT_DIALOG_REGISTRY`, pass it as `ModalDialog`'s
  `dialogId`, add a gallery row (type error + `dialog-gallery-coverage`). The registry feeds MCP's "available dialogs".
- **`ModalDialog`'s overlay starts at `inset: var(--titlebar-height) 0 0 0`**, keeping the macOS title bar's window-drag
  region live. Any full-window backdrop must too.
- **A dialog that shows a path is `resizable`** (`"horizontal"` unless something inside absorbs height), and its
  shortened text carries `use:tooltip={{ text: full, overflowOnly: true }}` — never a native `title`. ❌ Don't put the
  drag offset back on the `style` ATTRIBUTE: the resize grip writes the user's size there, so a re-render wipes it.
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle`; one-offs need
  `/* allowed-btn-restyle: <reason> */`). `LinkButton` is the ONLY `cursor: pointer` opt-in; don't hand-roll one.
- **Toasts**: pick a level by feedback kind, not wording; a full all-persistent stack silently drops new ones;
  pane-local transient toasts need `addToastForPane(pane, …)`, or that pane's navigation won't clear them.
- **Tooltip: keep BOTH detached-trigger guards** (`destroy()` cancels the timer; `showTooltip` / `positionTooltip` bail
  on `!el.isConnected`, not a zero-rect heuristic), else a recycled virtual-scroll row fires the 400 ms timer on a
  detached node.
- **`ShortcutChip`**: never statically import `openShortcutCustomization` (it drags `@tauri-apps/api/webviewWindow` onto
  a module-eval surface the capability-restricted viewer can't have); dynamic `import()` in the click handler. Set
  exactly one of `commandId` / `key`; a `commandId` chip with no binding renders NOTHING.
- **`Select`'s `.select-*` class contract is stable** (DETAILS § Select): `SettingSelect`'s `querySelector` and
  `dropdown_states.go`'s contrast matrix depend on it. Don't rename or recolor off the accent tokens without both.
- **`Combobox` is a text-field-with-suggestions, not a value-bound select**: drive its text off `inputValue`, never off
  `value` / `items` (blanks the field on an empty list or a custom name).
- **Text-field chrome lives in `app.css` § "Text fields"**, not in `TextInput` / `TextArea`: both render the same
  `.text-field*` classes, so restyling every field is ONE edit. `Combobox` / `NumberInput` can't delegate (Ark renders
  their input) and read the same `--radius-input` / `--spacing-input` / `--font-size-input` / `--shadow-focus-solid`
  tokens; keep all five in sync. `.text-field` / `.text-field-control` are a stable selector contract (E2E + the
  settings focus-restore); don't rename.
- **`TextInput` is one-way `value` + `oninput`, never `bind:value` internally**: `type` is dynamic (the password field
  flips on focus) and Svelte forbids the pair. `bind:value` still works for callers. Shared prop types live in
  `text-field-types.ts`, NOT a `<script module>` (a type from a `.svelte` file is `any` to the lint service).
- **Single-component traps**, each with its DETAILS §: `StatusBadge`'s class is `feature-status-badge`, NOT
  `status-badge` (Debug's `:global(.status-badge)` would leak onto it), status via `getBadgeStatus(featureId)`; `Slider`
  never renders `Slider.HiddenInput` (axe nested-interactive), and its readout, ticks, and end labels stay
  `aria-hidden`; `containerStyle` is one-off layout sizing only, never token-worthy styling.

Catalogs, prop tables, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
