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
  - **F2 vs File > Rename's accelerator.** wry hands a key equivalent to WKWebView first for a top-level webview
    (`WryWebView::performKeyEquivalent` defers to super), and WebKit lets the menu fire only when the page didn't handle
    the key, which the sheet's `claimKey` does; an unhandled keyDown bubbles to `WryWebViewParent`, which runs the main
    menu's key equivalent (wry 0.57.0 source, 2026-10-09). But this repo has recorded the menu firing first (⌘A) and
    both firing (⇧Space), and a disabled item's accelerator still fires (`routes/(main)/DETAILS.md` § Native-menu and
    input-focus interactions). Not verified in the running app. So both roads land in one place: the sheet claims the
    `file.rename` menu command (`$lib/commands/menu-claims.ts`, run by the dispatch core for the menu road ahead of the
    dialog gate), its keydown and its Presets menu's `onKey` call the same `pressOpenKey`, and `createKeyRoadEcho` drops
    the other road's fire within 300 ms, so one F2 toggles the menu once whichever arrives first.

## Mask input

`MaskInput.svelte` is the mask field with inline token editors. Not wired into the sheet yet; to wire it, swap each
mask's `TextInput` for it.

- **Props**: `value` (the mask), `onValueChange(next)` (every keystroke AND every token edit; feed it to
  `tool.update({ nameMask })`), `ariaLabel`, `invalid`, and bindable `inputElement` (the `<input>`, for focus and
  `insertPlaceholder`'s caret). It renders `TextInput mono` itself.
- **Keys**: ArrowDown with the caret inside or right after an editable token (`tokenAtCaret`: `from < caret <= to`)
  opens its editor and is claimed (`claimKey`); anywhere else ArrowDown isn't touched. In the editor, ArrowDown /
  ArrowUp walk the fields, and Enter or ArrowUp from the first field closes it; both are claimed, so the sheet's
  Enter-starts never sees them. Escape is the `Popover`'s, which stops it before the sheet closes. All three put focus
  back in the field with the caret after the token (Escape through the anchor span's `onfocus`); a click elsewhere
  closes the editor and leaves focus where it landed.
- **Markers**: an absolutely placed ▾ `<button tabindex="-1">` ("Edit counter", `aria-haspopup="dialog"`) under each
  token's end, measured with a canvas in the input's computed font, net of `scrollLeft`; one scrolled out of view isn't
  drawn. Mousedown is prevented, so a click never moves focus out of the field. Re-measured on mask change, input
  scroll, resize, and `selectionchange` (the caret can scroll the input).
- **Token kinds** (`mask-token-kinds.ts`): each is a grammar (`parse(inner)` → value or `null`, `format(value)` → the
  shortest inner text, in a pure module), an editor component taking `TokenEditorProps` (`value`, `onChange`, `onDone`),
  and two message keys. The text is the single source of truth: an editor edit is `format` + `replaceToken`, never state
  of its own. A new kind (a range, a date) is one entry there plus its grammar's tests.
- **Counter** (`counter-token.ts`): parses step for step like `counter()` in `mask.rs`, quirks included (`[C++5]` is
  step 5); both sides test against `counter_token_vectors.json`. Writes back in minimal form: defaults (start 1, step 1,
  digits 1) left out, so all-default is a bare `[C]`; a negative start always writes its step (`[C-5+1]`: a lone `[C-5]`
  is a step of -5) and clamps at `MIN_COUNTER_START`; digits clamp to 1–`MAX_COUNTER_DIGITS`. An emptied editor field
  means that part's default; a half-typed `-` waits.
