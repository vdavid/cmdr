# Multi-Rename sheet: details

- **Keyboard-first**: the name mask has focus on open; Tab walks the fields; Enter in a text field starts (TC's Start!),
  a button or menu keeps its own Enter; Esc closes. The placeholder buttons insert at the name mask's caret.
- **Placeholder tooltips**: each button's tooltip (`PlaceholderTip`, adopted as the house tooltip's `contentEl`, so it
  shows on hover AND keyboard focus and is the button's `aria-describedby`) gives the meaning, an example on the sample
  file, a few other forms from `mask.rs` with their examples, and for a range the part it takes in bold with what the
  field keeps around it quiet. The lead line names what the example ran on: `Beach day.jpg`, its path with the two
  folders above it (`[P]`), the date and time (`[YMD]`, `[hms]`), or "the first three files" (a counter, whose numbers
  come from `counterSamples`, no engine call). § Tooltip examples.
- **Error line**: always rendered under the search row, one line tall (ellipsis, overflow tooltip), so the preview never
  shifts.
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
- **Problems only**: the "N problems" in the footer's summary (`multiRename.summary`, a `Trans` `<problemsToggle>` tag)
  is a `LinkButton` with `aria-pressed`: pressed, the state lists the problem rows alone, paged by the backend
  (`get_multi_rename_preview_rows` with `PreviewFilter::Problems`, offsets counted among the problems), so a 200k-row
  preview never ships to find its problems. The preview's first page is every row's, so in this mode a new preview pages
  its rows in rather than showing it. With no problems it's plain text, and a preview with none (or a lost preview)
  switches the list back to every row.
- **Start** calls `applyMultiRename`; the page shows a toast and closes the sheet. The operation is in the queue, and
  the operation log's Undo reverses it. An Undo button in the toast is a follow-up.
- **Target**: the focused pane's selected rows in row order (backend numbers, `..` offset removed), or `null` for the
  whole folder when nothing or everything is selected, plus `getLastSequence()`. A pane with no backend listing
  (servers, search results), or a selection whose rows are still settling (`isRowStateReady`), opens nothing.
  `routes/(main)/+page.svelte` reads it (`getFocusedPaneRenameTarget`) and opens the session (`openMultiRename`) before
  the sheet mounts. A read-only pane (any archive, the `.git` portal, a read-only volume) gets `refuseMultiRename`’s
  alert instead; the backend’s `ReadOnly` at Start is the safety net.
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

## Layout and option keys

- **Layout**: three full-width rows, in the order a rename runs them (`plan.rs` → `CompiledTransform::apply`: mask,
  search & replace, case, diacritics), a `--spacing-lg` gap between them. First the name mask (grows) and the extension
  mask (140 px), with the placeholder buttons under them; then Search for, Replace with, and the search option chips;
  then Letter case and Remove diacritics, each with its dim key chip. Each `Checkbox` sits in a wrapper span, since it
  renders more than one element.
- **Search option chips** (`SearchOptionChips`): Match case `Aa`, First match only `1×`, Include extension `.ext`,
  Regular expression `.*`, Replace whole name `^$` (the regex way to say "the whole string"). Code-editor style find
  toggles: mono glyphs, `aria-hidden`, on the button an `aria-label` with the full name and `aria-pressed`; on is the
  accent fill, as a chosen `ToggleGroup` cell. A plain `<button>` (no house multi-toggle primitive exists; `ToggleGroup`
  is single-select). As tall as the text fields by their frame's own recipe (`app-field.css`: font × tight leading + two
  input paddings + the border). Each tooltip names the option, shows its key chip, and gives one tiny replace on a
  made-up file with the option on and off: "Replacing `photo` with `pic` in `Photo photo.jpg`:", then
  `On  Photo pic.jpg` / `Off  pic pic.jpg`, the text the replace put in bold, the rest quiet. Each option's file name is
  chosen so the option changes the result (`search-option-help.ts`).
- **Option keys**: ⌘⌥U opens Letter case (focus + click on its `.select-trigger`, `Select`'s stable class, so the menu
  opens on the checked row as a click would), and ⌘⌥ N/I/F/E/R/W flip Remove diacritics, Match case, First match only,
  Include extension, Regular expression, and Replace whole name. Fixed-key registry commands in
  `Main window/Multi-rename` (`sources/file-list.ts`), so Settings and the Help window list them and
  `registry-conflicts.test.ts` guards the defaults. The sheet's keydown asks `optionKeyOf` → `eventMatchesCommand` and
  claims the key.
  - ⌥ composes a character in `key` (`®`, `ƒ`, `∑`, or `Dead`), so the match runs on the key position:
    `physicalKeyCombo` names a letter by `code` while ⌘ / ⌃ is held (`lib/shortcuts/DETAILS.md` § Key capture). Verified
    in unit and component tests with US-layout events; a real keypress in the running app isn't verified (the MCP driver
    sends synthetic events).
  - None of U/N/I/F/E/R/W with ⌘⌥ is a Cmdr command or a native menu accelerator (`menu_bar.rs` holds ⌘⌥ C/O/T/V/Q/L/A,
    the registry adds H; checked 2026-10-09), so no menu command needs claiming.
  - Letter case's and Remove diacritics' keys show as a dim `ShortcutChip` (`commandId`, not clickable: the keys can't
    be rebound) beside the option, `aria-hidden`: decoration for sighted users, the same keys listed in the Help window.
    A search chip shows its key in its tooltip.

## Tooltip examples

- **Made-up files, rendered by the real engine.** Every example is a `RenameExample` (a file name and a full spec on
  `DEFAULT_SPEC`), all asked in ONE `render_multi_rename_examples` call on open (`renderExamples`, keyed by mask or by
  `searchExampleKey`) and answered by `plan::render_examples`, which runs `Compiled::render` (what the preview runs) on
  the file in `Trips/Lisbon 2026`, last changed 2026-07-14 09:05:30. So an example can't drift from what a rename does,
  and never depends on what's selected: examples from the batch's first file were fragile (a gone file, no date, a name
  too short for a range). A spec that doesn't run comes back `null` and its example is left out.
- **Marks set apart the part an example is about.** `U+E000` / `U+E001` (private use, never in a real name or mask) wrap
  it inside the spec, and the engine copies them through like any text: a range's mask is `[E1]` + marked `[E2-3]` +
  `[E4-]` (what the field keeps around it, unmarked), and a search option's replacement is marked, so the result marks
  exactly what the replace put in (and nothing, when it matched nothing). `examplePieces` splits the result. ❌ Don't
  diff before and after in TS to find the change: a diff can't tell `pic pic` from `Photo photo`.
- The lead lines' path and date are rendered too (`FOLDER_LEAD_MASK`, `DATE_LEAD_MASK`), so the folder names and the
  date live only in `plan.rs`.

## Mask input

`MaskInput.svelte` is the mask field with inline token editors, used for both masks.

- **Props**: `value` (the mask), `onValueChange(next)` (every keystroke AND every token edit; feed it to
  `tool.update({ nameMask })`), `ariaLabel`, `invalid`, and bindable `inputElement` (the `<input>`, for focus and
  `insertPlaceholder`'s caret). It renders `TextInput mono` itself.
- **Opening on its own** (`token-editor-rules.ts`, a pure reducer over caret, hover, request, engage, tokens, and
  dismiss events; the delays are the component's):
  - The caret resting strictly inside a token (`tokenInside`: `from < caret < to`) for 250 ms opens it, `passive`: focus
    stays in the field and typing goes on. The caret leaving, Tab or a click away, or the token stopping being one
    (`[C1x]`) closes it. Right after `]` doesn't count, so typing a token through never pops its editor.
  - The pointer resting on a token's text (measured extent) or its marker for 300 ms opens it; it stays while the
    pointer crosses into the popover and closes 300 ms after it leaves both. Hover never takes over an editor the caret
    or the user holds.
  - Focus going into the editor (ArrowDown, a click into it, the marker) makes it `focus`-held: only Escape, Enter,
    ArrowUp, or a click elsewhere closes it. A mousedown back in the field hands it to the caret first, `flushSync`ed,
    so the popover's trap is gone before focus moves (else the trap pulls focus back).
  - The popover is `surface="solid"` (opaque `--color-bg-secondary`): the house glass is ~70% opaque, and the numbers
    must read cleanly over the mask and the list.
- **Keys**: ArrowDown with an editor open goes into its first field; with none, ArrowDown with the caret inside or right
  after an editable token (`tokenAtCaret`: `from < caret <= to`) opens it there. Both are claimed (`claimKey`); anywhere
  else ArrowDown isn't touched. Escape in the field closes an open editor and is claimed, so the sheet stays. In the
  editor, ArrowDown / ArrowUp walk the fields, and Enter or ArrowUp from the first field closes it; both are claimed, so
  the sheet's Enter-starts never sees them. Escape is the `Popover`'s, which stops it before the sheet closes. All three
  put focus back in the field with the caret after the token (Escape through the anchor span's `onfocus`); a click
  elsewhere closes the editor and leaves focus where it landed.
- **Markers**: an absolutely placed ▾ `<button tabindex="-1">` ("Edit counter", `aria-haspopup="dialog"`) centered under
  each token, measured with a canvas in the input's computed font, net of `scrollLeft`; one scrolled out of view isn't
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
