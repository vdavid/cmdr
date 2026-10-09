# Multi-Rename sheet

The ⌃M sheet over `src-tauri/src/multi_rename/` (the engine and its rules: that module's `CLAUDE.md` / `DETAILS.md`).

- `MultiRenameDialog.svelte` the sheet and its windowed table; `routes/(main)/+page.svelte` opens a backend session over
  the focused pane's selection (`getFocusedPaneRenameTarget` → `openMultiRename`) first, and closes it with the sheet.
- `multi-rename-state.svelte.ts` the spec, the debounced preview (a generation counter drops stale answers), the rows in
  view (`show` / `rowAt`), presets, Start.
- `spec.ts` the default spec, built-in presets, placeholder insertion. Pure.

## Must-knows

- **Names stay in the backend session.** The selection goes over ONCE, with the pane's applied sequence (a stale one is
  `selectionChanged`: a toast, no sheet). After that the sheet sends the spec and, at Start, the `previewId` it shows;
  it holds only the counts and the rows near the view. `previewOutOfDate` re-previews.
- **Start waits for the preview of the last edit** (`pending`), so it never runs a spec nobody saw; a failed Start
  (`applyError`) doesn't block a retry.
- **Enter starts from a mask or search field**, saves from the preset-name field, and never fires mid-composition.
- **A spec error keeps the last good preview** on screen under the message; any other error clears it.
- Built-in preset names are message keys (translated); saved ones are the user's text.

More: `DETAILS.md`.
