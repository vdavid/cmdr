# Multi-Rename sheet: details

- **Keyboard-first**: the name mask has focus on open; Tab walks the fields; Enter in a text field starts (TC's Start!),
  a button or menu keeps its own Enter; Esc closes. The placeholder buttons insert at the name mask's caret.
- **Preview**: reruns `PREVIEW_DELAY_MS` (120 ms) after the last edit. The answer carries the counts (the footer and the
  Rename button) and the first page of rows. The list is the house `ColumnList` (`lib/ui/DETAILS.md` § ColumnList), the
  same look as Search's results, fed the state's windowed `source`: `count` is the rows listed, `getRow` reads the held
  rows, and `onRangeChange` is `show`, which pages the missing ones in from the same preview. Far rows are dropped past
  `HELD_ROWS`. A row still on its way is `ColumnList`'s placeholder row.
- **Columns**: file icon (the row's `iconId` from the icon cache, else a file or folder glyph), old name, a quiet arrow,
  new name (emphasis; quiet when unchanged), and a status glyph. Names mid-truncate as Search's do (`useShortenMiddle`).
  Every track is fixed or `share`d: a windowed source can't be measured, so the two names split what the glyphs leave.
  `table` semantics, since it's a preview with no cursor.
- **Status glyphs** (`row-status.ts`): one glyph per problem kind (invalid name, duplicate, name taken, gone) in a
  `StatusGlyph`, its short label the accessible name and the full reason the tooltip, in the error color. Ready and
  unchanged rows show nothing and carry screen-reader-only text, so a screen-reader user still hears every row's status.
- **Problems only**: a checkbox over the list. The state switches its rows to the problem rows alone, paged by the
  backend (`get_multi_rename_preview_rows` with `PreviewFilter::Problems`, offsets counted among the problems), so a
  200k-row preview never ships to find its problems. The preview's first page is every row's, so in this mode a new
  preview pages its rows in rather than showing it. Disabled while there are no problems (unless it's on).
- **Start** calls `applyMultiRename`; the page shows a toast and closes the sheet. The operation is in the queue, and
  the operation log's Undo reverses it. An Undo button in the toast is a follow-up.
- **Target**: the focused pane's selected rows in row order (backend numbers, `..` offset removed), or `null` for the
  whole folder when nothing or everything is selected, plus `getLastSequence()`. A pane with no backend listing
  (servers, search results), or a selection whose rows are still settling (`isRowStateReady`), opens nothing.
- **Start's toast** says how many files are renaming; a batch that ran as a move on S3 adds a warning for the swaps it
  left out (`swapsLeftOut`), as Ask Cmdr does.
- **Gallery**: `not-triggerable`, since the preview is computed from a real listing.
- **Presets** (TC's F2 "Load/save settings"): the footer's `[Presets ▾ <name>]` button opens a house `Menu`: saved
  presets newest first, numbered 1–9 (a digit picks one), then the built-ins, then Reset all fields (TC's `<Default>`)
  and Save current as… (⌘S chip). Each saved preset has a submenu (→, hover, or right-click): Rename…, Update with
  current fields (greyed when nothing would change), Delete. Picking a preset loads it; nothing runs.
  - The button reads `<name> (edited)` once a field differs from the loaded preset (`specsEqual`), and just "Presets"
    with nothing loaded. Saving or updating makes that preset the loaded one; deleting it leaves nothing loaded and the
    fields as they are.
  - ⌘S (and Save current as…, and Rename…) open a `Popover` anchored to the button, with one name field prefilled with
    the loaded saved preset's name. Enter saves; a name another preset has (`presetNameClash`, the backend's
    trimmed-lowercase rule) asks inline first, and a second Enter or Replace confirms. Escape closes the popover only
    (it stops the key before the sheet sees it), and focus goes back to the button. The field's Enter is claimed, so it
    never reaches the sheet's Enter-starts rule.
  - The menu lives in the sheet's overlay (`Menu` portals through `providePortalTarget`), so it sits above the scrim and
    inside the focus trap; its capture listener takes every key while open, so the sheet's Enter can't start a rename
    from it.
  - Unverified in the running app: F2 is also the native File > Rename accelerator. The dialog gate refuses that menu
    command while the sheet is open, and the keydown still reaches the webview, as the Shift+Space double fire shows
    (`routes/(main)/command-handlers/DETAILS.md`).
