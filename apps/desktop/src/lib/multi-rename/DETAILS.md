# Multi-Rename sheet: details

- **Keyboard-first**: the name mask has focus on open; Tab walks the fields; Enter in a text field starts (TC's Start!),
  a button or menu keeps its own Enter; Esc closes. The placeholder buttons insert at the name mask's caret.
- **Preview**: reruns `PREVIEW_DELAY_MS` (120 ms) after the last edit. The answer carries the counts (the footer and the
  Rename button) and the first page of rows. The table is windowed: spacer rows give it the full height, it draws the
  rows in view plus `OVERSCAN`, measures one drawn row for the height, and `show` pages the missing ones in from the
  same preview. Far rows are dropped past `HELD_ROWS`. A row still on its way draws empty at full height.
- **Start** calls `applyMultiRename`; the page shows a toast and closes the sheet. The operation is in the queue, and
  the operation log's Undo reverses it. An Undo button in the toast is a follow-up.
- **Target**: the focused pane's selected rows in row order (backend numbers, `..` offset removed), or `null` for the
  whole folder when nothing or everything is selected, plus `getLastSequence()`. A pane with no backend listing
  (servers, search results), or a selection whose rows are still settling (`isRowStateReady`), opens nothing.
- **Start's toast** says how many files are renaming; a batch that ran as a move on S3 adds a warning for the swaps it
  left out (`swapsLeftOut`), as Ask Cmdr does.
- **Gallery**: `not-triggerable`, since the preview is computed from a real listing.
