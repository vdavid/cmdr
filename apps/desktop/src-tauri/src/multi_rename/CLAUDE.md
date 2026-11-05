# Multi-Rename Tool

Total Commander's ⌃M renamer: masks, search & replace, the case and character steps, presets, and a live preview. The
sheet is `src/lib/multi-rename/`.

## Module map

The engine is `crates/cmdr-multi-rename` (the pure modules, plus `MultiRename`: sessions in `session.rs`, media reads
in `media.rs`, apply in `run.rs`), re-exported here so the app names it `crate::multi_rename::…`. This folder is its
host:

- `app_host.rs` the app's engine (`app_multi_rename`, a `LazyLock` on Tauri's runtime), its `MultiRenameHost` answers
  (volume reads, Results offered, `rename_place`, `io_budget`, the write engine), and the `MultiRenameMediaProgress` event.
- `app_files.rs` the app's files: `LAST_SPEC` (last settings), `FIELD_HISTORY`, and `results_path`.
  `preset_store.rs` the presets file (`PRESETS`).

## Must-knows

- **The selection becomes file names ONCE, when the sheet opens** (`MultiRename::open`, under the listing's sequence
  guard). ❌ Never map row numbers through the live listing again: a file appearing while the sheet is open shifts the
  rows, and Start would rename files the user never picked. A session file that's gone is `Missing`.
- **Names stay in the backend.** The sheet gets counts and the page of rows it draws; apply takes `(session, preview)`
  ids, recomputes, and refuses with `PreviewOutOfDate` unless its ready rows are EXACTLY that preview's.
- **A Results name is matched by its OLD name, never its line**, and passes every status check. The session
  reads back only the file it wrote, and apply renames with the edits its preview showed.
- **Sessions are bounded** (`MAX_SESSIONS`, idle eviction on open). The app's test build raises the cap in
  `app_multi_rename` (its tests share one engine in parallel); ❌ never in the crate.
- **❌ No file read under the sessions lock or the media cache's lock**; `Session`'s `Drop` stops the loader.
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
  counter fields fold into the masks on load. Its width is capped (`MAX_COUNTER_DIGITS`); its arithmetic saturates.
- **A preset's number is its place**: a new one goes last; save-over, rename, and update keep it. ❌ Not on
  `crate::recents`, whose `add` moves an entry to the top.
- **Statuses compare folded names** (`name_fold`, as the Mac does): a name a staying file holds is `TargetExists`,
  one a batch row leaves is free; a block cascades through a worklist, so a long chain stays linear.

Decisions and the TC semantics in detail: `DETAILS.md`.
