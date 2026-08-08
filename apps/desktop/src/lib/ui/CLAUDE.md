# UI primitives

Reusable components; only silent-breakage rules live here. Ark UI backs the complex ones, in-house wrappers the rest.

## Module map

- Dialogs: `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- Primitives: `Icon`, `Spinner`, `Button`, form controls (`ToggleGroup` is segmented, ≠ `RadioGroup`), `Select`,
  `Combobox`, `TextInput` + `TextArea` (+ `text-field-types.ts`), `ShortcutChip`, `toast/`, more in DETAILS § Key files.
  Tooltip is the sibling `../tooltip/tooltip.ts`; `../utils/shorten-middle-action.ts` mid-truncates through it.

## Must-knows

- **A missing primitive is the cue to add a wrapper here** (`@ark-ui/svelte` and lucide imports are allowlisted to this
  dir; rules in `src/CLAUDE.md`). A new one owes a tier-3 a11y test, a Debug > Components row, and a `design-system.md`
  § Component patterns entry, all check-enforced. Router: `docs/guides/building-ui.md`.
- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the SAME element**
  (`cmdr/dialog-needs-focus-trap`), else Tab leaks into the shortcut-suppressed background: a keyboard lockout.
  `ModalDialog` owns the directive, so `role`-prop callers don't repeat it.
- **Adding a dialog** (soft sheets too): register its id in `SOFT_DIALOG_REGISTRY`, pass it as `ModalDialog`'s
  `dialogId`, add a gallery row (type error + `dialog-gallery-coverage`). The registry feeds MCP's "available dialogs".
- **`ModalDialog`'s overlay starts at `inset: var(--titlebar-height) 0 0 0`**, keeping the macOS title bar's window-drag
  region live; any full-window backdrop must too. A dialog showing a path is `resizable` (`"horizontal"` unless
  something inside absorbs height) and tooltips its shortened text (`overflowOnly`), never via `title`. ❌ Keep the drag
  offset and the dragged size OFF the `style` attribute: that one is `containerStyle`'s, and rewriting it mid-drag snaps
  the panel back.
- **`resizable` grabs on bands that HANG OVER the panel edge**, so ❌ never put `overflow: hidden` back on
  `.modal-dialog`: it halves every band. The clip lives on `.modal-content` (DETAILS § resizable), which also keeps the
  opposite edge pinned by paying back the centering drift.
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle`; one-offs need
  `/* allowed-btn-restyle: <reason> */`). `LinkButton` is the ONLY `cursor: pointer` opt-in.
- **Toasts**: pick a level by feedback kind, not wording; a full all-persistent stack silently drops new ones;
  pane-local transient toasts need `addToastForPane(pane, …)`, or that pane's navigation won't clear them.
- **Tooltip: keep BOTH detached-trigger guards** (`destroy()` cancels the timer; `showTooltip` / `positionTooltip` bail
  on `!el.isConnected`, never a zero-rect heuristic), else a recycled virtual-scroll row fires the 400 ms timer on a
  dead node.
- **`ShortcutChip`**: import `openShortcutCustomization` dynamically in the click handler, never statically (it drags
  `@tauri-apps/api/webviewWindow` onto a module-eval surface the capability-restricted viewer can't have). Set exactly
  one of `commandId` / `key`; a `commandId` chip with no binding renders NOTHING.
- **`Select`'s `.select-*` class contract is stable** (DETAILS § Select): `SettingSelect`'s `querySelector` and
  `dropdown_states.go`'s contrast matrix depend on it. Don't rename or recolor off the accent tokens.
- **`Combobox` is a text-field-with-suggestions, not a value-bound select**: drive its text off `inputValue`, never
  `value` / `items` (blanks the field on an empty list or a custom name).
- **Text-field chrome lives in `app.css` § "Text fields"**, not in `TextInput` / `TextArea`: both render the same
  `.text-field*` classes, so restyling every field is ONE edit. `Combobox` / `NumberInput` can't delegate (Ark renders
  their input) and re-read the same four `--*-input` / `--shadow-focus-solid` tokens; keep all five in sync.
  `.text-field` / `.text-field-control` are a stable selector contract (E2E + settings focus-restore).
- **`TextInput` is one-way `value` + `oninput`, never `bind:value` internally**: `type` is dynamic (the password field
  flips on focus) and Svelte forbids the pair. `bind:value` still works for callers. Shared prop types live in
  `text-field-types.ts`, NOT a `<script module>` (a `.svelte` type is `any` to the lint service).
- **Single-component traps**, each with its DETAILS §: `StatusBadge`'s class is `feature-status-badge`, NOT
  `status-badge` (Debug's `:global(.status-badge)` would leak onto it); `Slider` never renders `Slider.HiddenInput` (axe
  nested-interactive), its readout, ticks, and labels staying `aria-hidden`; `containerStyle` is one-off sizing only.

Catalogs, prop tables, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
