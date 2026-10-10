# Multi-Rename sheet

The ⌃M sheet over `src-tauri/src/multi_rename/` (the engine and its rules: that module's `CLAUDE.md` / `DETAILS.md`).

- `MultiRenameDialog.svelte` the sheet; `routes/(main)/+page.svelte` opens the backend session first and closes it with
  the sheet. `PreviewList.svelte` its preview (a `table`-semantics `ColumnList` over the windowed `source`).
- `multi-rename-state.svelte.ts` the spec, the debounced preview (a generation counter drops stale answers), the rows in
  view, presets (`loaded`, `edited`), the last settings (`persist`), Results' names, Start.
- `results.svelte.ts` Results (⌥⏎): whether a names file is out in the editor; window focus reads it back.
- `field-history-menu.svelte.ts` the fields' history (↓): one house `Menu` under the field that asked;
  `FieldHistoryChevron.svelte` the chevron at each field's end.
- `PresetsControl.svelte` the Presets button, its house `Menu`, and the name popover. `preset-menu.ts` (rows, name
  clash) and `preset-keys.ts` (F2 / ⌘S) are pure.
- `spec.ts` the default spec, built-in presets, `specsEqual`, placeholder insertion; `row-status.ts` a row status →
  glyph, label, reason. Pure.
- `MaskInput.svelte` both mask fields, `[C…]` tokens with a ▾ marker and an inline `CounterTokenEditor`
  (`mask-token-kinds.ts`); `mask-tokens.ts` / `counter-token.ts` / `token-editor-rules.ts` are its pure logic.
- `PlaceholderTip.svelte` / `placeholder-help.ts`, `SearchOptionChips.svelte` / `search-option-help.ts`: tooltips and
  chips; `rename-examples.ts` renders the examples.
- `last-run.svelte.ts` the last run, which Undo rename (⌘⌥Z) rolls back. Outlives the sheet.
- `option-keys.ts` the ⌘⌥ option keys (`TOGGLE_COMMANDS`) and the whole-name toggles in pipeline order. Pure.

## Must-knows

- **Names stay in the backend session**, Results' typed names too. The selection goes over ONCE, with the pane's applied
  sequence (stale: `selectionChanged`, a toast). Then the sheet sends the spec and, at Start, the `previewId`; it holds
  only the counts and the rows in view. "Problems only" pages from the backend (`PreviewFilter`), ❌ never by filtering
  rows it holds.
- **Start waits for the preview of the last edit** (`pending`), so it never runs a spec nobody saw; a failed Start
  (`applyError`) doesn't block a retry. Results waits the same way.
- **Enter starts from a mask or search field** (⌘⏎ from anywhere), saves from the preset-name popover, and never fires
  mid-composition.
- **Every sheet key is a fixed registry command** in `Main window/Multi-rename`, read through `eventMatchesCommand` and
  claimed, from a text field too. ❌ No raw key tests. An open Presets menu owns every key (its `onKey` answers F2 and
  ⌘S). ↓ at a mask's `[C…]` token is the counter editor's (MaskInput claims it first), elsewhere the field's history.
- **F2 is also File > Rename's menu accelerator.** The sheet claims that menu command while mounted, so both roads end
  in `PresetsControl.pressOpenKey`, which toggles once per press (`createKeyRoadEcho`). ❌ Don't call `openMenu` from a
  key path: the echo would close the menu it opened.
- ⌘⌥ C/A/H/L/O/Q/T/V belong to app commands, D to the Dock. DETAILS § Layout and option keys.
- **`counter-token.ts` must read `[C…]` as `mask.rs` does**: both test against
  `src-tauri/src/multi_rename/counter_token_vectors.json`; change the grammar there first.
- **Tooltip examples are the engine's render of made-up files**, the marked part set apart. ❌ No slicing or replacing
  in TS. DETAILS § Tooltip examples.
- **A spec error keeps the last good preview** under the message; any other error clears it.
- **A caret-opened editor is `passive`** (no focus, no trap): typing stays in the field. DETAILS § Mask input.
- **The sheet opens on what the last one closed with**: fields AND loaded preset, so "Mine (edited)" survives. An edit
  made before they arrive wins. DETAILS § Presets.
- `edited` compares against the preset as it is in the list now, so a rename, update, or delete needs no syncing.

More: `DETAILS.md`.
