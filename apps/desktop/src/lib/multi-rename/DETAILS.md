# Multi-Rename sheet: details

- **Keyboard-first**: the name mask has focus on open; Tab walks the fields; Enter in a text field starts (TC's Start!),
  a button or menu keeps its own Enter; ⌘⏎ (`multiRename.rename`, the chip on the Rename button) starts from anywhere, a
  checkbox or the preview included; Esc closes. The placeholder buttons (house `Chip`s, `variant="insert"`,
  `size="field"`, mono) insert at the name mask's caret.
- **Placeholder tooltips**: each button's tooltip (`PlaceholderTip`, adopted as the house tooltip's `contentEl`, so it
  shows on hover AND keyboard focus and is the button's `aria-describedby`) gives the meaning, an example on the sample
  file, a few other forms from `mask.rs` with their examples, and for a range the part it takes in bold with what the
  field keeps around it quiet. The lead line names what the example ran on: `Beach day.jpg`, its path with the two
  folders above it (`[P]`), the date and time (`[YMD]`, `[hms]`), or "the first three files" (a counter, whose numbers
  come from `counterSamples`, no engine call). The `[N]`, `[E]`, and `[P]` tooltips end on how to open the range editor
  (`rangeEditHint`), the counter's on its editor (`counterEditHint`). § Tooltip examples.
- **The list's heading slot** (`.message-line`): one line tall above the preview, so nothing coming or going moves the
  list. An error wins it (ellipsis, overflow tooltip), then Results' notice, else a "Preview" label styled like the
  field labels (`multiRename.preview`, the same key that names the table). Both live regions (`role="alert"` for the
  error, `role="status"` for the notice) stay mounted, empty when quiet, so a message arriving is announced; the label
  is `aria-hidden` because the table already carries the name (`ColumnList` takes an `ariaLabel`, no `labelledby`).
- **Stale preview**: under a spec error the last good preview stays, dimmed with the house disabled opacity (0.5) under
  an `is-unavailable` class, and so does the footer's summary of it. The class names it as inactive content, the WCAG
  1.4.3 case the contrast checker exempts (`scripts/check-a11y-contrast/README.md` § Opacity); both fade with
  `--transition-base` unless the user asked for less motion.
- **Preview**: reruns `PREVIEW_DELAY_MS` (120 ms) after the last edit. The answer carries the counts (the footer and the
  Rename button) and the first page of rows. The list is the house `ColumnList` (`lib/ui/DETAILS.md` § ColumnList), the
  same look as Search's results, fed the state's windowed `source`: `count` is the rows listed, `getRow` reads the held
  rows, and `onRangeChange` is `show`, which pages the missing ones in from the same preview. Far rows are dropped past
  `HELD_ROWS`. A row still on its way is `ColumnList`'s placeholder row.
- **Columns**: file icon (the row's `iconId` from the icon cache, else a file or folder glyph), old name, a quiet arrow,
  new name (emphasis; quiet when unchanged), and a status glyph. Names mid-truncate as Search's do (`useShortenMiddle`).
  Every track is fixed or `share`d: a windowed source can't be measured, so the two names split what the glyphs leave.
- **Cursor**: the list is a Tab stop with one cursor row (`ColumnList`'s `grid` semantics, `lib/ui/DETAILS.md` §
  ColumnList): ↑/↓, PageUp/PageDown, Home/End, and a click move it, in the house cursor color; VoiceOver reads the row
  through `aria-activedescendant`. No multi-selection. It opens on the top row, and the state holds it (`cursorIndex`,
  `cursorRow`, `setCursor`): it stays on its row (`PreviewRow.row`) across previews, clamped when the count shrinks.
  With every row listed an index IS the row, so that's free; among the problems the state finds the row again in the
  first page that lands. "Problems only" on puts it on the first problem; off puts it back on its own row. A cursor the
  state moves is scrolled into view (`PreviewList`'s effect). **Decision/Why: hover doesn't move it**
  (`hoverMovesCursor` off, unlike Search): the cursor row drives the range editor's examples, which mustn't jump while
  the pointer passes over. The list's keys are bare (`list-cursor.ts`), so ↓ there never meets the fields' history
  (`multiRename.fieldHistory` answers only in a text field).
- **Rename order** (⇧↑ / ⇧↓, `multiRename.moveRowUp` / `.moveRowDown`, fixed keys): in the list, the cursor row moves
  one place in the session's order (what `[C]` counts) and the cursor goes with it (`moveCursorRow`; the backend's
  `move_row`, `src-tauri/src/multi_rename/DETAILS.md` § Session, preview, and apply). `PreviewList` answers them through
  `ColumnList`'s `onKeydown` (the grid's keys that aren't cursor keys), so in a text field ⇧↑ / ⇧↓ stay the selection
  keys. The move retires the preview on screen, and the state previews again at once; it's `pending` from the key on, so
  a Start right after waits for the new order's preview. Moves queue, each from where the last left the row. Off while
  "Problems only" is on (claimed, nothing moves): a move in a filtered view is ambiguous. Off under a preview error too:
  the dimmed last good rows can't show the new order, so the cursor would land on another file. A move is announced
  through a house `LiveAnnouncer` ("Moved to 3 of 48", `multiRename.movedRow`); a move at an end says nothing.
  **Decision/Why:** ⇧↑ / ⇧↓ are David's pick; the house reorder key elsewhere is ⌥↑ / ⌥↓ (the `Menu`, the queue window).
  Neither meets another sheet key or `Main window/Multi-rename`'s ancestors (`registry-conflicts.test.ts`).
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
  the operation log's Undo reverses it.
- **Undo rename** (⌘⌥Z, `multiRename.undoRename`): a started run becomes the session's last run (`last-run.svelte.ts`,
  module state that outlives the sheet, not persisted), so the flow is rename, look at the pane, ⌃M, ⌘⌥Z. It calls
  `rollbackOperation`, as the operation log's Roll back does; the page toasts `multiRename.undoStarted` and closes the
  sheet (its session holds the names the undo is taking away). A typed refusal is worded by `rollbackRefusalNotice` in
  the error line; `alreadyRolledBack` forgets the run. The footer shows it as a house `Button` beside Cancel, its key
  chip on it and its count in the tooltip, only while there's a run. No ellipsis: it acts at once, with no question
  first (`docs/style-guide.md` § Writing). **Decision/Why:** ⌘⌥Z, not ⌘Z: ⌘Z / ⇧⌘Z stay the fields' text undo and redo,
  and ⌥ is the sheet's option-key family. Free in the registry and the native menu (checked 2026-10-10). Ported from PR
  #386.
- **Results** (⌥⏎, `multiRename.results`, TC's "edit names"): `results.svelte.ts` asks the state to write the shown
  preview (`writeNames`, waiting for the settled preview as Start does), opens the file through `openFileInEditor`
  (which toasts a launch that didn't start), and reads it back on the window's next `focus` until "Use the settings
  again" (`clearNames`) or a read that fails (`namesFileGone`, worded in the error line). The line under the fields says
  quietly what's going on (`results.editing`, then `results.edited` with the count) with "Read names now" and "Use the
  settings again" links; an error wins the line. A row with a typed name shows an accent pencil where its arrow is
  (`PreviewList.svelte`). The footer offers it as a house `Button`, "Results…", its key chip on it. The ellipsis follows
  `docs/style-guide.md` § Writing: the editor is where the user changes what the rename will do, so the next step steers
  the command. **Decision/Why:** ⌥⏎, the PR's key: Enter stays Rename, and Enter's handler skips any modifier, so the
  two never meet. The matching is the backend's (`multi_rename/DETAILS.md` § Session); the sheet holds no names. Ported
  from PR #386.
- **Field history** (↓, `multiRename.fieldHistory`, TC's): in the name mask, extension mask, search, or replace field, ↓
  or the chevron at the field's end opens one house `Menu` (`field-history-menu.svelte.ts`, rendered inside the dialog
  so it portals into the modal's layer) under that field, listing what it held in earlier renames, newest first (at most
  `HISTORY_SHOWN`); a pick fills the field and the menu hands focus back. The backend records it
  (`multi_rename/DETAILS.md` § Field history); the sheet reads it once on mount. **↓ routing:** with the caret inside a
  mask's token, or right after a `[C…]`, MaskInput claims ↓ first (into its editor), so the sheet never sees it;
  anywhere else the sheet's keydown opens the history, unless the field has none, when ↓ is left to the field.
  **Decision/Why:** the chevron (`FieldHistoryChevron.svelte`) is `Combobox`'s 16 px tertiary glyph in `TextInput`'s
  trailing slot, so it reads as the field's own list, apart from the small ▾ markers under tokens. It's disabled while
  the field has no history (the affordance stays put, and an empty list would only explain itself), out of the Tab order
  (↓ is the keyboard road), and carries `aria-haspopup="menu"`, `aria-expanded`, and the name "History". Ported from PR
  #386, whose chevron buttons it follows.
- **Target**: the focused pane's selected rows in row order (backend numbers, `..` offset removed), or `null` for the
  whole folder when nothing or everything is selected, plus `getLastSequence()` and `nothingSelected`. The sheet's title
  names that scope after the tool's name (`multiRename.scope.selection` / `.wholeFolder`): taking the whole folder is a
  convenience (nothing renames before Start), but a user who expected "selection only" was surprised. A pane with no
  backend listing (servers, search results), or a selection whose rows are still settling (`isRowStateReady`), opens
  nothing. `routes/(main)/+page.svelte` reads it (`getFocusedPaneRenameTarget`) and opens the session
  (`openMultiRename`) before the sheet mounts. A read-only pane (any archive, the `.git` portal, a read-only volume)
  gets `refuseMultiRename`’s alert instead; the backend’s `ReadOnly` at Start is the safety net.
- **Start's toast** says how many files are renaming; a batch that ran as a move on S3 adds a warning for the swaps it
  left out (`swapsLeftOut`), as Ask Cmdr does.
- **Gallery**: `not-triggerable`, since the preview is computed from a real listing.
- **Presets** (TC's F2 "Load/save settings"): the footer's `[Presets ▾ <name>]` button opens a house `Menu`: the presets
  under a "Your presets" heading, in the user's order, numbered 1–9 by position (a digit picks one), then Reset all
  fields (TC's `<Default>`) and Save current as… (⌘S chip). Each preset has a submenu (→, hover, or right-click):
  Rename…, Update with current fields (greyed when nothing would change), Move up, Move down (greyed at the top and
  bottom; the numbers follow the places), Delete. Picking a preset loads it; nothing runs. Every preset command answers
  the backend's list after the change, so the sheet takes it from there.
  - **Seeds**: `loadPresets` hands the backend `PRESET_SEEDS` (`spec.ts`) named in the UI language; a list that never
    took them gets them once, after its own presets (`src-tauri/src/multi_rename/DETAILS.md` § Presets). From then on
    they're ordinary presets: there's no built-in kind.
  - **Last settings** (TC keeps them too): closing the sheet, any way, saves the fields and the loaded preset (`persist`
    in `onDestroy` → `save_multi_rename_last_settings`, one entry in `multi-rename-last.json`), and the next sheet's
    first preview waits for them. **Decision/Why:** the preset travels with the fields, so a reopened sheet's button
    still names it and `edited` stays honest (fields alone would make an edited preset look like nothing was loaded). A
    remembered preset deleted since is dropped (`loadedId` reads the live list), its fields kept. `touched` makes an
    edit (or a preset pick) made before they arrive win.
  - The button reads `<name> (edited)` once a field differs from the loaded preset (`specsEqual`), and just "Presets"
    with nothing loaded. Saving or updating makes that preset the loaded one; deleting it leaves nothing loaded and the
    fields as they are.
  - ⌘S (and Save current as…, and Rename…) open a `Popover` anchored to the button, with one name field prefilled with
    the loaded preset's name. Enter saves; a name another preset has (`presetNameClash`, the backend's trimmed-lowercase
    rule) asks inline first, and a second Enter or Replace confirms. Escape closes the popover only (it stops the key
    before the sheet sees it), and focus goes back to the button. The field's Enter is claimed, so it never reaches the
    sheet's Enter-starts rule.
  - The menu lives in the sheet's overlay (`Menu` portals through `providePortalTarget`), so it sits above the scrim and
    inside the focus trap; its capture listener takes every key while open, so the sheet's Enter can't start a rename
    from it.
  - **F2 vs File > Rename's accelerator.** wry hands a key equivalent to WKWebView first for a top-level webview
    (`WryWebView::performKeyEquivalent` defers to super), and WebKit lets the menu fire only when the page didn't handle
    the key, which the sheet's `claimKey` does; an unhandled keyDown bubbles to `WryWebViewParent`, which runs the main
    menu's key equivalent (wry 0.57.0 source, 2026-10-09). But this repo has recorded the menu firing first (⌘A) and
    both firing (⇧Space), and a disabled item's accelerator still fires (`routes/(main)/DETAILS.md` § Native-menu and
    input-focus interactions). Not verified in the running app. So both roads land in one place: the sheet claims the
    `file.rename` menu command (`#lib/commands/menu-claims.ts`, run by the dispatch core for the menu road ahead of the
    dialog gate), its keydown and its Presets menu's `onKey` call the same `pressOpenKey`, and `createKeyRoadEcho` drops
    the other road's fire within 300 ms, so one F2 toggles the menu once whichever arrives first.

## Layout and option keys

- **Layout**: three full-width rows, in the order a rename runs them (`plan.rs` → `CompiledTransform::apply`: mask,
  search & replace, case, Greek to Latin, diacritics, then the Unicode form), a `--spacing-lg` gap between them. The
  first two share one grid (`.field-grid`: two shared field columns and one `auto` chip column), so their edges line up:
  the name mask (grows) and the extension mask (140 px) span the field columns, so Extension ends where Replace with
  does, with the placeholder chips right of them; then Search for, Replace with, and the search option chips. The chip
  groups share the last column, right edges level; both are house `Chip`s at `size="field"`, as tall as the text boxes,
  with the same padding. Each group spans the column like a segmented bar: one `--spacing-xs` gap, and each chip grows
  from its label's width by an equal share of the spare room (`flex: 1 1 auto`; equal widths would let `Chip`'s field
  `min-width` clip `[YMD]`), so both rows start and end on the same edges. Then Letter case, Greek to Latin, Remove
  diacritics, and Normalize Unicode (`WHOLE_NAME_TOGGLES`), each with its dim key chip. Each `Checkbox` sits in a
  wrapper span, since it renders more than one element.
- **Minimum width** (`MIN_WIDTH_PX`, 880 px, under the app window's 950 px floor): the grid and the footer's four
  buttons still fit; the footer's buttons sit in one non-shrinking `.footer-actions`, so at the floor with Undo rename
  showing, the summary beside Presets wraps (to three short lines) rather than a button's label leaving its chip.
- **Search option chips** (`SearchOptionChips`): Match case `Aa`, First match only `1×`, Include extension `.ext`,
  Regular expression `.*`, Replace whole name `^$` (the regex way to say "the whole string"). Code-editor style find
  toggles: house `Chip`s, `variant="toggle"` (`aria-pressed`, the full name as `aria-label`), mono glyphs, tinted when
  on like a set filter chip (the chip family's one "on" look, `#lib/ui/DETAILS.md` § Chip), and `size="field"` so they
  stand as tall as the text fields. Each tooltip names the option, shows its key chip, and gives one tiny replace on a
  made-up file with the option on and off: "Replacing `photo` with `pic` in `Photo photo.jpg`:", then
  `On  Photo pic.jpg` / `Off  pic pic.jpg`, the text the replace put in bold, the rest quiet. Each option's file name is
  chosen so the option changes the result (`search-option-help.ts`).
- **Option keys**: ⌘⌥U opens Letter case (focus + click on its `.select-trigger`, `Select`'s stable class, so the menu
  opens on the checked row as a click would), and ⌘⌥ G/N/P/I/F/E/R/W flip Greek to Latin, Remove diacritics, Normalize
  Unicode (P for precomposed, the form it renames to), Match case, First match only, Include extension, Regular
  expression, and Replace whole name. Fixed-key registry commands in `Main window/Multi-rename`
  (`sources/file-list.ts`), so Settings and the Help window list them and `registry-conflicts.test.ts` guards the
  defaults. The sheet's keydown asks `optionKeyOf` → `eventMatchesCommand` and claims the key.
  - ⌥ composes a character in `key` (`®`, `ƒ`, `∑`, or `Dead`), so the match runs on the key position:
    `physicalKeyCombo` names a letter by `code` while ⌘ / ⌃ is held (`lib/shortcuts/DETAILS.md` § Key capture). Verified
    in unit and component tests with US-layout events; a real keypress in the running app isn't verified (the MCP driver
    sends synthetic events).
  - None of U/G/N/P/I/F/E/R/W with ⌘⌥ is a Cmdr command or a native menu accelerator (`menu_bar.rs` holds ⌘⌥
    C/O/T/V/Q/L/A, the registry adds H, and macOS takes ⌘⌥D for the Dock; checked 2026-10-10), so no menu command needs
    claiming.
  - Letter case's and the whole-name toggles' keys show as a dim `ShortcutChip` (`commandId`, not clickable: the keys
    can't be rebound) beside the option, `aria-hidden`: decoration for sighted users, the same keys listed in the Help
    window. A search chip shows its key in its tooltip.

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
- **Keys**: ArrowDown with an editor open goes into its first field; with none, ArrowDown with the caret inside an
  editable token (`tokenInside`), or right after a kind with `downRightAfter` (the counter; `tokenAtCaret`:
  `from < caret <= to`), opens it there. **Decision/Why:** not right after a range: `[N]` and `[E]` are the default
  masks, so the caret sits right after one whenever the sheet opens, where ↓ is TC's field history. Both are claimed
  (`claimKey`); anywhere else ArrowDown isn't touched. Escape in the field closes an open editor and is claimed, so the
  sheet stays. In the editor, ArrowDown / ArrowUp walk the fields (rows, in the range editor: § Range editor), and Enter
  or ArrowUp from the first field closes it; both are claimed, so the sheet's Enter-starts never sees them. Escape is
  the `Popover`'s, which stops it before the sheet closes. All three put focus back in the field with the caret after
  the token (Escape through the anchor span's `onfocus`); a click elsewhere closes the editor and leaves focus where it
  landed.
- **Markers**: an absolutely placed ▾ `<button tabindex="-1">` ("Edit counter", "Choose characters",
  `aria-haspopup="dialog"`) centered under each token, measured with a canvas in the input's computed font, net of
  `scrollLeft`; one scrolled out of view isn't drawn. Mousedown is prevented, so a click never moves focus out of the
  field. Re-measured on mask change, input scroll, resize, and `selectionchange` (the caret can scroll the input).
- **Token kinds** (`mask-token-kinds.ts`): each is a grammar (`parse(inner)` → value or `null`, `format(value)` → the
  shortest inner text, in a pure module), an editor component taking `TokenEditorProps` (`value`, `onChange`, `onDone`),
  and two message keys. The text is the single source of truth: an editor edit is `format` + `replaceToken`, never state
  of its own. A new kind (a date) is one entry there plus its grammar's tests. `MaskInput` renders whichever kind a
  token is through `anyKind` (TS can't pair a union of kinds with their values in markup).
- **Counter** (`counter-token.ts`): parses step for step like `counter()` in `mask.rs`, quirks included (`[C++5]` is
  step 5); both sides test against `counter_token_vectors.json`. Writes back in minimal form: defaults (start 1, step 1,
  digits 1) left out, so all-default is a bare `[C]`; a negative start always writes its step (`[C-5+1]`: a lone `[C-5]`
  is a step of -5) and clamps at `MIN_COUNTER_START`; digits clamp to 1–`MAX_COUNTER_DIGITS`. An emptied editor field
  means that part's default; a half-typed `-` waits.

## Range editor

`RangeTokenEditor.svelte`, the inline editor of a range token (`[N…]` `[E…]` `[P…]` `[G…]`, the full-name `[2-5]`): TC's
"select the characters to insert" dialog, done inline. Three parts, top to bottom.

- **What to take**: "Take from" (a house `ToggleGroup`: Name, Extension, Folder, Grandparent folder, Full name) and the
  ribbon, the field's text on the list cursor's file, one cell per character (what `Array.from` splits, as the engine
  counts), positions from the start above and from the end below, the taken run tinted (`--color-accent-selection`) with
  a bar on each edge (`--color-accent-pop`, the mark color that reads on tinted surfaces). The number each edge counts
  from is set in bold. Drag to pick, ⇧-click to extend from the far edge. There is no file switcher: the example is
  always the cursor row (`sampleRows`), so moving the list's cursor changes it. Switching the field keeps the range.
- **How to count**: Start and End each offer "from the start" / "from the end", labelled with the edge's real place on
  that file ("1st" / "9th from the end"), and Write as offers "Start to end" (`[N2-5]`) / "Start and length" (`[N2,4]`);
  the length form hides End. With Start counted from the end, End's "from the start" is off, its tooltip saying why: the
  grammar reads a positive end after a negative start from the end too (`[N-8-5]`). A plain-words line reads the token
  (`rangeReading`, ICU `selectordinal` positions: "From the 1st to the 5th-to-last character"), and a warning says when
  it takes nothing from the cursor's file.
- **On your files**: at most five rows, the cursor's and up to two either side (`rowsAround`, shifted at the list's ends
  so it stays five), each the field's text with the taken part marked, then what it gives, or "nothing" in the warning
  color. The cursor row is set apart (`aria-current`).
- **Which edges count from where**: a drag (or ⇧-click) is a fresh pick, each end counting from the edge it's nearer
  (`autoAnchors`: a tie goes to the start, and an end on the last character counts from the end, so it runs to the end
  of every name); keys and the Start / End rows keep the token's own choice (`anchorsOf`), so a manual choice sticks
  until the next drag. A whole field writes as the bare `[N]` (the full name as `[1-]`). `rangeForSelection` turns a
  pick into the token; `range_token_vectors.json`'s `selections` prove each one takes exactly what was picked.
- **The engine decides what a range takes.** The editor asks `rangeSamples(token, rows)` (the state's, through Svelte
  context, `range-sample-source.ts`, so `MaskInput` stays free of the session), answered by
  `get_multi_rename_range_samples` from `mask.rs`'s own `bounds` (`src-tauri/src/multi_rename/DETAILS.md` § Session). It
  asks on every token change and every new preview, a generation counter dropping overtaken answers; a refusal (a moved
  row's preview still on its way) keeps what's shown. The pick the last edit wrote stands in until the answer for its
  token lands (`picked`), so a key pressed again right away moves on from it. TS only splits the answer's text at the
  engine's indices to draw it.
- **Keys and focus**: the editor's first stop is the start handle (`data-initial-focus`, which `Popover` honors; the
  ribbon itself while nothing is taken, and the start handle once the first answer lands). On the ribbon ← / → move the
  selection, ⇧ ← / → its end, ⌥ ← / → its start (`ribbonMoveOf`, `moveSelection`); a move by one edge focuses that
  edge's handle, so VoiceOver reads its new place. ↑ / ↓ walk the rows (Take from, ribbon, Start, End, Write as), ↑ from
  Take from and Enter anywhere go back to the mask; the editor claims ↑ / ↓ / Enter in the capture phase, so the toggle
  groups and the sheet never see them. **Decision/Why: the two edges are `role="slider"` handles**, labelled Start and
  End, `aria-valuenow` their 1-based place and `aria-valuetext` how the token counts them ("character 1, B", "5th from
  the end, h", a space read as "space"), described by the reading and the keys hint. A range is two values, and the ARIA
  multi-thumb slider is the pattern VoiceOver knows; the cells themselves are `aria-hidden`, and "On your files" reads
  each file in full.
