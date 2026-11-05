# Multi-Rename sheet

The ⌃M sheet over `src-tauri/src/multi_rename/` (the engine and its rules: that module's `CLAUDE.md` / `DETAILS.md`).

- `MultiRenameDialog.svelte` the sheet; `routes/(main)/+page.svelte` opens the backend session first (selection, else
  cursor file) and closes it with the sheet. `PreviewList.svelte` its preview (a `grid` `ColumnList` over the windowed
  `source`, one cursor).
- `multi-rename-state.svelte.ts` the spec, the debounced preview (a generation counter drops stale answers), the rows in
  view, the cursor, presets (`loadedId`, `edited`), the last settings (`persist`), Results' names, Rename and its queue,
  the media read's progress.
- `results.svelte.ts` Results (⌥⏎): whether a names file is out in the editor; window focus reads it back.
- `field-history-menu.svelte.ts` the fields' history (↓), one house `Menu`; `FieldHistoryChevron.svelte` its chevron.
- `PresetsControl.svelte` the Presets button, its house `Menu`, and the name popover. `preset-menu.ts` (rows, name
  clash) and `preset-keys.ts` (F2 / ⌘S) are pure.
- `spec.ts` the default spec, the preset seeds, `specsEqual`, placeholder insertion; `row-status.ts` a row status →
  glyph, label, reason; `error-text.ts` the error line's words. Pure.
- `MaskInput.svelte` both mask fields; `[C…]`, range, and `[=…]` tokens get a ▾ marker and an inline editor
  (`mask-token-kinds.ts`), over pure `mask-tokens.ts`, the `*-token.ts` grammars, and `token-editor-rules.ts`.
- `PlaceholderChips.svelte` the placeholder buttons and `media ▾`, whose `MediaFieldPicker.svelte` inserts fields
  (`media-fields.ts`: their words, the picker's groups, spaced inserts). `PlaceholderTip.svelte` /
  `placeholder-help.ts`, `SearchOptionChips.svelte` / `search-option-help.ts`: tooltips and chips; `rename-examples.ts`
  renders the examples.
- `last-run.svelte.ts` the last run, which Undo rename (⌘⌥Z) rolls back. Outlives the sheet.
- `option-keys.ts` the ⌘⌥ option keys (`TOGGLE_COMMANDS`) and the whole-name toggles in pipeline order. Pure.

## Must-knows

- **Names stay in the backend session**, Results' too. The selection goes over ONCE (stale: `selectionChanged`); then
  the sheet sends the spec and, at Rename, the `previewId`, holding only the counts and the rows in view. "Problems
  only" pages from the backend (`PreviewFilter`), ❌ never by filtering held rows.
- **Rename and Results wait for the preview of the last edit** (`pending`), so neither runs a spec nobody saw; a failed
  Rename (`applyError`) doesn't block a retry.
- **A Rename pressed while a mask's media fields are still read queues** ("Starting…") and runs on the final preview as
  a fresh press would; a second press or any edit cancels it. DETAILS § Media fields.
- **Enter renames from a mask or search field** (⌘⏎ from anywhere), saves from the preset-name popover, and never fires
  mid-composition.
- **Every sheet key is a fixed registry command** in `Main window/Multi-rename`, read through `eventMatchesCommand` and
  claimed, from a text field too. ❌ No raw key tests. An open Presets menu owns every key (its `onKey` answers F2 and
  ⌘S). ↓ strictly inside a mask token is its editor's, elsewhere the field's history.
- **F2 is also File > Rename's accelerator**: both roads end in `PresetsControl.pressOpenKey`, one toggle per press
  (`createKeyRoadEcho`). ❌ Don't call `openMenu` from a key path: the echo would close the menu it opened.
- ⌘⌥ C/A/H/L/O/Q/T/V belong to app commands, D to the Dock, M to macOS. DETAILS § Layout and option keys.
- **The `*-token.ts` grammars read tokens as `mask.rs` does** (`crates/cmdr-multi-rename/src/*_token_vectors.json`);
  change one there first.
- **Tooltip and editor examples are the engine's render.** ❌ No slicing or replacing in TS. DETAILS § Tooltip examples.
- **A spec error keeps the last good preview** under the message; any other error clears it.
- **A caret-opened editor is `passive`** (no focus, no trap). A token editor reads `shown`, ❌ never `editing`: a gone
  token blanked the window. DETAILS § Mask input.
- **The sheet opens on what the last one closed with**: fields AND loaded preset. An edit made before they arrive wins.

More: `DETAILS.md`.
