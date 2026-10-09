# Multi-Rename Tool

Total Commander's ⌃M renamer: a name mask and an extension mask with placeholders, search & replace, a case step,
removing diacritics, a counter, presets, and a live preview. The sheet is `src/lib/multi-rename/`.

## Module map

- `mask.rs` the placeholders (`[N2-5]`, `[C10+5:3]`, `[YMD]`, `[U]`…), parsed once, rendered per row. Pure.
- `transform.rs` search & replace, the case step, `remove_diacritics`. Pure.
- `plan.rs` the preview over a folder's entries: each row's new name and status; `mask_examples` renders the sheet's
  placeholder-tooltip examples for the first file through the same `RowFacts`. Pure.
- `session.rs` one open sheet's files (resolved once from the pane's selection), its latest preview, and paging.
- `run.rs` apply: proves the ready rows against the preview shown, then runs `start_renames` (Ask Cmdr's executor).
- `error.rs` `MultiRenameError`, shared by `session` and `run` so neither imports the other (`module-cycles`).
- `presets.rs` named presets on `crate::recents`; rename and update edit one in place (`rename_in`, `update_spec_in`).

## Must-knows

- **The selection becomes file names ONCE, when the sheet opens** (`session::open`, under the listing's sequence
  guard). ❌ Never map row numbers through the live listing again: a file appearing while the sheet is open shifts the
  rows, and the preview and Start would rename files the user never picked. A session file that's gone is `Missing`.
- **Names stay in the backend.** The sheet gets counts and the page of rows it draws; apply takes `(session, preview)`
  ids, recomputes, and refuses with `PreviewOutOfDate` unless its ready rows are EXACTLY that preview's.
- **Sessions are bounded** (`MAX_SESSIONS`, idle eviction on open): a sheet that never closed can't hold a big
  folder's names for long.
- **`a|b` → `x|y` replaces in ONE pass** (`Replacement::Pairs`): `a|b` → `b|c` turns `a` into `b`, `one|two` →
  `two|one` swaps. ❌ Never chain the pairs: that's how `a` became `c`.
- **TC's order is fixed**: mask, then search & replace, then case, then diacritics. Positions count from 1; a range past
  the end is empty, never an error.
- **A folder has no extension** (`RowFacts::split_name`); a leading or trailing dot belongs to the name. Names are
  composed (NFC) before the mask, so a range never splits an accent off its letter.
- **Diacritics go only on Latin and Greek letters**: kana dakuten, Indic vowel signs, Cyrillic `й`/`ё` are letters.
- **The counter's settings live in the mask** (`[C10+5:3]`); a bare `[C]` counts 1, 2, 3, unpadded. Old presets'
  sheet-wide counter fields fold into the masks on load (`presets.rs`). Its width is capped (`MAX_COUNTER_DIGITS`) and
  its arithmetic saturates.
- **Statuses compare folded names** (`name_fold`, as the Mac does, so over-cautious on a case-sensitive volume): a name
  held by a file that stays is `TargetExists`, one a batch row leaves is free, and a block cascades (a blocked row
  stays, which can block the row renaming into it) through a worklist, so a long chain stays linear.

Decisions and the TC semantics in detail: `DETAILS.md`.
