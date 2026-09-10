# UI primitives

Reusable components; only silent-breakage rules live here. Ark UI backs the complex ones, in-house wrappers the rest.

## Module map

- Dialogs: `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- The primitives (`Icon`, `Spinner`, `Button`, form controls, `Select`, `Combobox`, text fields, `ShortcutChip`,
  `InfoTip` (a `<button>`), `StatusGlyph` (❌ never focusable: a tab stop per virtual row wrecks keyboard navigation),
  `toast/`) are catalogued in DETAILS § Key files. `ToggleGroup` is segmented, ≠ `RadioGroup`; Tooltip is the sibling
  `../tooltip/tooltip.ts`.

## Must-knows

- **A missing primitive is the cue to add a wrapper here** (`@ark-ui/svelte` and lucide imports are allowlisted here;
  rules in `src/CLAUDE.md`). A new one owes a tier-3 a11y test, a Debug > Components row, and a `design-system.md`
  entry, all check-enforced. `docs/guides/building-ui.md`.
- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the SAME element**
  (`cmdr/dialog-needs-focus-trap`), else Tab leaks into the shortcut-suppressed background: a keyboard lockout.
  `ModalDialog` owns the directive, so `role`-prop callers don't repeat it.
- **Adding a dialog** (soft sheets too): register its id in `SOFT_DIALOG_REGISTRY`, pass it as `ModalDialog`'s
  `dialogId`, add a gallery row (type error + `dialog-gallery-coverage`). Its `whileOpen` verdict is REQUIRED and won't
  compile until answered: it decides whether a file operation may start behind your dialog.
  `$lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
- **`ModalDialog` registers what it renders in `open-dialogs.svelte.ts`**, keeping that set exhaustive;
  `OnboardingWizard` alone hand-registers. ❌ An unpaired close blocks file operations until restart.
- **`ModalDialog`'s overlay starts at `inset: var(--titlebar-height) 0 0 0`**, keeping the macOS window-drag region
  live; any full-window backdrop must too. ❌ Drag offset and dragged size stay OFF the `style` attribute
  (`containerStyle` owns them), ❌ never restore `overflow: hidden` on `.modal-dialog` (resize bands overhang), ❌ never
  drop `.modal-overlay:focus { outline: none }` (the scrim holds focus, so a UA ring lines the title bar in the SYSTEM
  accent), ❌ keep the MCP close registration in its `$effect` or a mount/destroy pair makes `dialog close` lie. DETAILS
  § ModalDialog.
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle`; one-offs need
  `/* allowed-btn-restyle: <reason> */`). `LinkButton` is the ONLY `cursor: pointer` opt-in.
- **Per-component traps.** Each has its own section in `DETAILS.md`:
  - `Tooltip`: keep BOTH detached-trigger guards (`destroy()` cancels the timer, `showTooltip` / `positionTooltip` bail
    on `!el.isConnected`), or a recycled virtual-scroll row fires the 400 ms timer on a dead node. A keypress hides
    tooltips app-wide; its hover-suppress flag isn't redundant (scrolling slides a fresh row under a still pointer). ❌
    Never a native `title` (`cmdr/no-title-attribute`): it skips keyboard focus. Keep any `aria-label`.
  - `ShortcutChip`: import `openShortcutCustomization` dynamically in the click handler: a static import drags
    `@tauri-apps/api/webviewWindow` onto a module-eval surface the capability-restricted viewer can't have. Set exactly
    one of `commandId` / `key`.
  - `Select`: `.select-*` classes are a contract (`SettingSelect`'s `querySelector`, `dropdown_states.go`); don't rename
    or recolor off the accent tokens. `--z-dropdown` belongs on `.select-positioner`, inert on `.select-content`.
  - `Combobox` is a text-field-with-suggestions: drive its text off `inputValue`, never `value` / `items`, which blanks
    the field on an empty list or custom name.
  - Text fields: chrome lives in `app.css` § "Text fields", so ONE edit restyles all five (keep `Combobox` /
    `NumberInput` in sync). `.text-field` / `.text-field-control` are a selector contract, and `TextInput` is one-way
    `value` + `oninput`, never an internal `bind:value`.
  - Toasts: pick a level by feedback kind, not wording; a full all-persistent stack silently drops new ones; a
    pane-local transient toast needs `addToastForPane(pane, …)` or that pane's navigation won't clear it.

Catalogs, prop tables, and remaining component traps: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
