# Multi-Rename Tool

Total Commander's ⌃M renamer: a name mask and an extension mask with placeholders, search & replace, a case step,
removing diacritics, a counter, presets, and a live preview. The sheet is `src/lib/multi-rename/`.

## Module map

- `mask.rs` the placeholders (`[N2-5]`, `[C10+5:3]`, `[YMD]`, `[U]`…), parsed once, rendered per row. Pure.
- `transform.rs` search & replace, the case step, `remove_diacritics`. Pure.
- `plan.rs` the preview over a folder's entries: each row's new name and status. Pure.
- `run.rs` preview and apply off the pane's cached listing; apply runs `start_renames` (Ask Cmdr's executor).
- `presets.rs` named presets on `crate::recents`.

## Must-knows

- **Names come from the backend, never the frontend.** Apply recomputes the preview from the listing and renames only
  if its ready rows are EXACTLY the `(row, old, new)` the user saw (`ExpectedRename`), else `PreviewOutOfDate`: a row
  number that shifted never renames another file. The heavy part runs off the IPC thread; a routed (read-only) folder
  is refused.
- **`a|b` → `x|y` replaces in ONE pass** (`Replacement::Pairs`): `a|b` → `b|c` turns `a` into `b`, `one|two` →
  `two|one` swaps. ❌ Never chain the pairs: that's how `a` became `c`.
- **TC's order is fixed**: mask, then search & replace, then case, then diacritics. Positions count from 1; a range past
  the end is empty, never an error.
- **A folder has no extension** (`RowFacts::split_name`); a leading or trailing dot belongs to the name. Names are
  composed (NFC) before the mask, so a range never splits an accent off its letter.
- **Diacritics go only on Latin and Greek letters**: kana dakuten, Indic vowel signs, Cyrillic `й`/`ё` are letters.
- **The counter width is capped** (`MAX_COUNTER_DIGITS`) and its arithmetic saturates.
- **Statuses compare folded names** (`name_fold`, as the Mac does, so over-cautious on a case-sensitive volume): a name
  held by a file that stays is `TargetExists`, one a batch row leaves is free, and blocking repeats until stable (a
  blocked row stays, which can block the row renaming into it).

Decisions and the TC semantics in detail: `DETAILS.md`.
