# Multi-Rename Tool

Total Commander's ⌃M renamer: masks with placeholders, search & replace, case, Greek to Latin, diacritics, Unicode
normalization, a counter, presets, and a live preview. The sheet is `src/lib/multi-rename/`.

## Module map

- `mask.rs` the placeholders (`[N2-5]`, `[C10+5:3]`, `[YMD]`, `[U]`…), parsed once, rendered per row. Pure.
- `transform.rs` search & replace, the case step, Greek to Latin, `remove_diacritics`. Pure.
- `transliterate.rs` Greek to Latin (ELOT 743). Pure.
- `plan.rs` the preview over a folder's entries: each row's new name and status; `render_examples` runs the sheet's
  tooltip examples (made-up files) through the same `Compiled::render`. Pure.
- `session.rs` one sheet's files in rename order (`move_row`), its latest preview, paging, and Results names.
- `names_file.rs` Results (⌥⏎): the preview as `old<TAB>new` lines, and the user's edits merged back by old name.
- `run.rs` apply: proves the ready rows against the preview shown, then runs `start_renames` (Ask Cmdr's executor).
- `error.rs` `MultiRenameError`, shared by `session` and `run` so neither imports the other (`module-cycles`).
- `history.rs` the text fields' history on `crate::recents`, recorded when a rename starts.
- `presets.rs` named presets in the user's order, as pure ops on the list (`save_in`, `rename_in`, …).
  `LAST_SPEC` keeps the last sheet's settings. `preset_store.rs` (their file, `PRESETS`) builds on it.

## Must-knows

- **The selection becomes file names ONCE, when the sheet opens** (`session::open`, under the listing's sequence
  guard). ❌ Never map row numbers through the live listing again: a file appearing while the sheet is open shifts the
  rows, and the preview and Start would rename files the user never picked. A session file that's gone is `Missing`.
- **Names stay in the backend.** The sheet gets counts and the page of rows it draws; apply takes `(session, preview)`
  ids, recomputes, and refuses with `PreviewOutOfDate` unless its ready rows are EXACTLY that preview's.
- **A Results name is matched by its OLD name, never its line**, and goes through every status check. The session
  reads back only the file it wrote; a stored preview keeps the edits it showed, so apply renames with exactly those.
- **Sessions are bounded** (`MAX_SESSIONS`, idle eviction on open).
- **`a|b` → `x|y` replaces in ONE pass** (`Replacement::Pairs`): `a|b` → `b|c` turns `a` into `b`, `one|two` →
  `two|one` swaps. ❌ Never chain the pairs: that's how `a` became `c`.
- **TC's order is fixed**: mask, then search & replace, case, Greek to Latin, diacritics, and Normalize Unicode last.
  Positions count from 1; a range past the end is empty, never an error.
- **A folder has no extension** (`RowFacts::split_name`); a leading or trailing dot belongs to the name. Names are
  composed (NFC) before the mask, so a range never splits an accent off its letter.
- **Diacritics go only on Latin and Greek letters**: kana dakuten, Indic vowel signs, Cyrillic `й`/`ё` are letters.
- **Frontend grammars mirror `mask.rs`** (`*_token_vectors.json`; change them there first); only `bounds` says
  what a range takes.
- **The counter's settings live in the mask** (`[C10+5:3]`); a bare `[C]` counts 1, 2, 3, unpadded. Old presets'
  sheet-wide counter fields fold into the masks on load (`presets.rs`). Its width is capped (`MAX_COUNTER_DIGITS`) and
  its arithmetic saturates.
- **A preset's number is its place**: a new one goes last; save-over, rename, and update keep it. ❌ Not on
  `crate::recents`, whose `add` moves an entry to the top.
- **Statuses compare folded names** (`name_fold`, as the Mac does): a name held by a file that stays is `TargetExists`,
  one a batch row leaves is free, and a block cascades through a worklist, so a long chain stays linear.

Decisions and the TC semantics in detail: `DETAILS.md`.
