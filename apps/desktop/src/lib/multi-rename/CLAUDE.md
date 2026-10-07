# Multi-Rename sheet

The ⌃M sheet over `src-tauri/src/multi_rename/` (the engine and its rules: that module's `CLAUDE.md` / `DETAILS.md`).

- `MultiRenameDialog.svelte` the sheet; opened from `routes/(main)/+page.svelte` with the focused pane's target
  (`getFocusedPaneRenameTarget`), closed back to the pane.
- `multi-rename-state.svelte.ts` the spec, the debounced preview (a generation counter drops stale answers), presets,
  Start.
- `spec.ts` the default spec, built-in presets, counts, placeholder insertion. Pure.

## Must-knows

- **Names come from the backend.** The sheet sends the listing id, backend row numbers, the spec, and at Start the ready
  rows it SHOWED, which the backend only checks against its own (`previewOutOfDate` re-previews).
- **Start waits for the preview of the last edit** (`pending`), so it never runs a spec nobody saw; a failed Start
  (`applyError`) doesn't block a retry.
- **Enter starts from a mask or search field**, saves from the preset-name field, and never fires mid-composition.
- **A spec error keeps the last good preview** on screen under the message; any other error clears it.
- Built-in preset names are message keys (translated); saved ones are the user's text.

More: `DETAILS.md`.
