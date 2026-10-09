# Multi-Rename Tool: details

Total Commander's Multi-Rename Tool, the semantics taken from its help (TOTALCMD.CHM, TC 11.58) and wiki. Proposed
upstream in vdavid/cmdr#372.

## Pipeline

Per row, in rename order (`position` counts from 0 and is what `[C]` counts):

1. `Mask::render` the name mask and the extension mask over `RowFacts` (name, extension, parent, grandparent, modified
   time in local time, position).
2. `CompiledTransform::apply`: search & replace on the name (and the extension with `include_extension`), then the case step
   (lower and upper on both, first-upper and words on the name only: `.Jpg` is never wanted), then `remove_diacritics`
   on both. The search compiles once per preview (`Transform::compile`), never per row.
3. `name` + `.` + `extension`, or just `name` when the extension renders empty.

## Placeholders (`mask.rs`)

- Fields with ranges: `[N]` `[E]` `[P]` `[G]`, and a bare range (`[2-5]`) on the full name. `[N1]` one character,
  `[N2-5]`, `[N2,5]` (start, length), `[N2-]`, negative starts count from the end; with a negative start a positive end
  counts from the end too (`[N-8-5]` is 8th-last to 5th-last), as TC documents.
- Counter `[C]` with the sheet's start / step / digits, or inline `[C10+5:3]`, `[C10]`, `[C+5]`, `[C:3]`, `[C100-10]`.
- Date and time of the last modification: `[Y]` `[y]` `[M]` `[D]` `[h]` `[m]` `[s]`, combinable (`[YMD]`, `[hms]`),
  `[d]` ISO date, `[t]` `hh.mm.ss` (TC's country-specific forms would put `:` in names, which macOS shows as `/`).
- Case switches `[U]` `[L]` `[F]` `[n]` apply from where they stand.
- `[[` is a literal `[`.

Not in v1, all planned in #372: `[T4]` EXIF date, alphabetic counters, `[=plugin.field]` / tag fields, `\` to move into
subfolders (the executor's one-parent rule refuses it today), "next step" chaining, and editing names in an editor.

## Search & replace (`transform.rs`)

- Plain search: case-insensitive unless `case_sensitive`, `*` / `?` wildcards (`*` is lazy), `a|b|c` lists paired with
  `x|y|z` (one replacement serves them all). **Decision: one alternation regex, ONE pass.** Why: chaining the pairs ran
  `a` → `b` → `c` and made a swap a no-op; TC replaces each match once.
- Regex: `$1` groups, braced before expanding (`$1_` is group 1 then `_`, as TC users type it, where the regex crate
  would read a group named `1_`); `$$`, `${…}`, and `$name` pass through. `substitute` makes the whole name the
  expanded replacement when the search matches.
- A broken regex is a spec error (`SpecError::BadRegex`), checked once in `Compiled::new`, never per row.
- `remove_diacritics`: NFD with the combining marks dropped, a table for the letters that don't decompose (`ł` `đ` `ø`
  `ß` `æ` `œ` `þ` `ð` `ı` `ħ` `ŧ`), then NFC. Like foobar2000's `$ascii()`.

## Session, preview, and apply (`session.rs`, `plan.rs`, `run.rs`)

- Row statuses: `Ready`, `Unchanged` (same name), `InvalidName` (via `validate_filename`, plus `.` / `..`),
  `Duplicate` (two rows get one folded name), `TargetExists` (a sibling that STAYS holds the folded name; siblings
  include hidden entries), and `Missing` (the session's file left the folder; set by `session.rs`, never by `plan`). A
  batch row renaming away frees its name, so chains and swaps preview as ready.
- **Decision: a backend session holds the files, by name.** `open_multi_rename(listing, includeHidden, rows | null,
  sequence)` reads the pane's rows (`null`: every row the pane shows, which is what "all selected" and "nothing
  selected" both mean) under `reconciled_cache`, refuses with `SelectionChanged` when the sequence or the hidden-files
  setting moved (the `get_selection_snapshot` guard F5 uses), and stores the names. Every preview looks them up in the
  listing's entries by name. Why: the sheet first sent row numbers and every preview mapped them through the LIVE
  listing, so a file landing above the selection shifted it onto files the user never picked, in the preview and at
  Start. A row's `row` is its place in the session, so it's stable across previews.
- **Decision: names stay in the backend.** A preview answers `previewId`, the counts, and the first `FIRST_PAGE` rows;
  `get_multi_rename_preview_rows` serves the rest of the stored preview a page (≤ `MAX_PAGE`) at a time, so a
  200,000-file folder never ships its names to the frontend and back. Only the latest preview is stored (ids are
  handed out at request time, so a slow older one never replaces it); paging a replaced one is `PreviewOutOfDate`.
  A page counts through every row or, with `PreviewFilter::Problems`, the problem rows alone (`RowStatus::is_problem`),
  so the sheet's "Problems only" list never needs the whole preview. Each row carries its entry's `icon_id` and
  `is_directory` for the sheet's file glyph (`None` for a `Missing` row); they're part of the apply proof's equality,
  which is right: a name that turned from a file into a folder isn't the row the user saw.
- **Decision: apply recomputes, then requires the preview the user saw.** `apply_multi_rename(session, previewId)`
  reruns that preview's spec over the session's files and refuses with `PreviewOutOfDate` unless its ready rows are
  exactly the stored ones (same row, names, status, icon, and kind). Why: the folder can change between the preview and Start.
- Sessions live in a process-wide map, at most `MAX_SESSIONS`; opening one drops sessions idle past `IDLE_LIMIT`,
  then the least recently used. The sheet closes its session (`close_multi_rename`) when it closes.
- Plain search: a `*` is lazy (`IMG_*_` ends at the first `_`) except a trailing one, which runs to the end. Search
  and names are composed (NFC) first, so `é` typed finds the decomposed `é` an SMB share stores.
- A single search takes its replacement literally (`|` included); only a list pairs with a list. A list of nothing
  (`|`) searches for nothing.
- Apply captures a `SourceFingerprint` per ready row: locally on `root`, through the volume elsewhere, eight at a time
  under one deadline (`REMOTE_FINGERPRINT_TIMEOUT`, stretched by `io_budget` on a live-session volume). It calls
  `start_renames(.., Initiator::User)`: the executor orders chains, swaps through temp names, rechecks fingerprints,
  journals every hop (so `undo_operations` reverses it), and runs a copying rename (S3) as one move, which can't swap
  names: those rows come back as `swapsLeftOut` and get their own toast (`multiRename.swapsSkipped`).
- The preview runs off the IPC thread with a 5 s deadline, two at a time (`BlockingBudget`), since the sheet re-issues
  it on every edit.

## Presets (`presets.rs`)

`RecentsFile<MultiRenamePreset>` in `multi-rename-presets.json`, keyed by the trimmed, lowercased name: saving under a
taken name replaces it (the sheet asks first). The one built-in preset (Remove diacritics) and "Reset all fields" live
in the frontend (`spec.ts`) so their names are translated.

- **Save** (`save_multi_rename_preset`) is `RecentsFile::add`: the preset goes on top.
- **Rename** and **Update with current fields** (`rename_multi_rename_preset`, `update_multi_rename_preset`) change the
  preset where it stands, through `RecentsFile::edit`: one locked step, one temp+rename write. **Decision/Why:** the
  sheet's Presets menu numbers saved presets 1–9, so moving one to the top on a rename would renumber the rest under the
  user's fingers. A rename onto a taken name drops the other preset in that same step (`rename_in`), so no reader ever
  sees two presets with one name, or neither.
