# Multi-Rename sheet

The ⌃M sheet over `src-tauri/src/multi_rename/` (the engine and its rules: that module's `CLAUDE.md` / `DETAILS.md`).

- `MultiRenameDialog.svelte` the sheet and its preview, a `table`-semantics `ColumnList` over the state's windowed
  `source`; `routes/(main)/+page.svelte` opens a backend session over the focused pane’s selection
  (`getFocusedPaneRenameTarget` → `openMultiRename`) first, and closes it with the sheet. A read-only pane (any archive,
  the `.git` portal, a read-only volume) gets `refuseMultiRename`’s alert instead; the backend’s `ReadOnly` at Start is
  the safety net.
- `multi-rename-state.svelte.ts` the spec, the debounced preview (a generation counter drops stale answers), the rows in
  view (`source` / `show` / `rowAt`, every row or `problemsOnly`), presets (which one is `loaded`, and whether the
  fields are `edited` off it), Start.
- `PresetsControl.svelte` the footer's Presets button, its house `Menu`, and the name popover (save / rename).
  `preset-menu.ts` builds the menu's rows and finds a name clash; `preset-keys.ts` reads F2 / ⌘S. Both pure.
- `spec.ts` the default spec, built-in presets, `specsEqual`, placeholder insertion. Pure.
- `row-status.ts` a row status → its glyph, short label, and tooltip reason (message keys). Pure.
- `MaskInput.svelte` both mask fields: `[C…]` tokens get a ▾ marker and an inline editor (`CounterTokenEditor`), via
  `mask-token-kinds.ts`; `mask-tokens.ts` / `counter-token.ts` read and rewrite tokens, `token-editor-rules.ts` says
  when the editor opens on its own. Pure.
- `PlaceholderTip.svelte` a placeholder button's tooltip, from `placeholder-help.ts` and backend-rendered examples.
- `option-keys.ts` reads the ⌘⌥ option keys (`TOGGLE_COMMANDS`). Pure.

## Must-knows

- **Names stay in the backend session.** The selection goes over ONCE, with the pane's applied sequence (a stale one is
  `selectionChanged`: a toast, no sheet). After that the sheet sends the spec and, at Start, the `previewId` it shows;
  it holds only the counts and the rows near the view, keyed by place in the list. "Problems only" (the summary's "N
  problems" toggle) pages the problem rows from the backend (`PreviewFilter`), ❌ never by filtering rows it holds.
  `previewOutOfDate` re-previews.
- **Start waits for the preview of the last edit** (`pending`), so it never runs a spec nobody saw; a failed Start
  (`applyError`) doesn't block a retry.
- **Enter starts from a mask or search field**, saves from the preset-name popover, and never fires mid-composition.
- **F2 and ⌘S are registry commands** (`multiRename.openPresets` / `multiRename.savePreset`, fixed keys in the
  `Main window/Multi-rename` scope), read through `presetKeyOf` → `eventMatchesCommand`, and claimed. ❌ No raw key
  tests. An open Presets menu owns every key, so its `onKey` answers F2 (close) and ⌘S (save) itself.
- **F2 is also File > Rename's menu accelerator.** The sheet claims that menu command while mounted
  (`claimMenuCommand('file.rename', …)`), so both roads end in `PresetsControl.pressOpenKey`, which toggles once per
  press (`createKeyRoadEcho` drops the other road's echo). ❌ Don't call `openMenu` from a key path: the echo would
  close the menu it opened. ⌘S isn't a menu accelerator.
- **The ⌘⌥ option keys are registry commands too** (fixed, same scope), read through `optionKeyOf` and claimed, from a
  text field as well. ⌘⌥ C/A/H/L/O/Q/T/V belong to app commands. DETAILS § Layout and option keys.
- **`counter-token.ts` must read `[C…]` as `mask.rs` does**: both test against
  `src-tauri/src/multi_rename/counter_token_vectors.json`; change the grammar there first.
- **A spec error keeps the last good preview** on screen under the message; any other error clears it.
- **A caret-opened editor is `passive`** (no focus, no trap), so typing stays in the field. DETAILS § Mask input.
- Built-in preset names are message keys (translated); saved ones are the user's text. `edited` compares against the
  preset as it is in the list now, so a rename, update, or delete is followed with no syncing.

More: `DETAILS.md`.
