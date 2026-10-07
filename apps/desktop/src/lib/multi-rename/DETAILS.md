# Multi-Rename sheet: details

- **Keyboard-first**: the name mask has focus on open; Tab walks the fields; Enter in a text field starts (TC's Start!),
  a button or menu keeps its own Enter; Esc closes. The placeholder buttons insert at the name mask's caret.
- **Preview**: reruns `PREVIEW_DELAY_MS` (120 ms) after the last edit. The table draws the first 1,000 rows; the rest
  still rename, and a line says how many aren't shown.
- **Start** calls `applyMultiRename`; the page shows a toast and closes the sheet. The operation is in the queue, and
  the operation log's Undo reverses it. An Undo button in the toast is a follow-up.
- **Target**: the focused pane's selected rows in row order (backend numbers), or the whole folder when nothing is
  selected; a pane with no backend listing (servers, search results) opens nothing.
- **Gallery**: `not-triggerable`, since the preview is computed from a real listing.
