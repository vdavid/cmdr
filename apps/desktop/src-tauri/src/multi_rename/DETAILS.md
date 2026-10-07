# Multi-Rename Tool: details

Total Commander's Multi-Rename Tool, the semantics taken from its help (TOTALCMD.CHM, TC 11.58) and wiki. Proposed
upstream in vdavid/cmdr#372.

## Pipeline

Per row, in rename order (`position` counts from 0 and is what `[C]` counts):

1. `Mask::render` the name mask and the extension mask over `RowFacts` (name, extension, parent, grandparent, modified
   time in local time, position).
2. `Transform::apply`: search & replace on the name (and the extension with `include_extension`), then the case step on
   both, then `remove_diacritics` on both.
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
- Regex: `$1` groups; `substitute` makes the whole name the expanded replacement when the search matches.
- A broken regex is a spec error (`SpecError::BadRegex`), checked once in `Compiled::new`, never per row.
- `remove_diacritics`: NFD with the combining marks dropped, a table for the letters that don't decompose (`ł` `đ` `ø`
  `ß` `æ` `œ` `þ` `ð` `ı` `ħ` `ŧ`), then NFC. Like foobar2000's `$ascii()`.

## Preview and apply (`plan.rs`, `run.rs`)

- Row statuses: `Ready`, `Unchanged` (same name), `InvalidName` (via `validate_filename`, plus `.` / `..`),
  `Duplicate` (two rows get one folded name), `TargetExists` (a sibling that STAYS holds the folded name; siblings
  include hidden entries). A batch row renaming away frees its name, so chains and swaps preview as ready.
- **Decision: apply recomputes, then requires the user's preview.** The frontend sends listing id + row numbers + spec
  + the ready rows it showed; apply recomputes from the listing and refuses with `PreviewOutOfDate` unless its ready
  rows are exactly those. Why: row numbers shift when a file appears above them, and renaming "row 7" would then rename
  a file the user never saw; names come from the backend, the frontend's only confirm them.
- Plain search: a `*` is lazy (`IMG_*_` ends at the first `_`) except a trailing one, which runs to the end. Search
  and names are composed (NFC) first, so `é` typed finds the decomposed `é` an SMB share stores.
- A single search takes its replacement literally (`|` included); only a list pairs with a list. A list of nothing
  (`|`) searches for nothing.
- Apply captures a `SourceFingerprint` per ready row (local on `root`, the volume's elsewhere) and calls
  `start_renames(.., Initiator::User)`: the executor orders chains, swaps through temp names, rechecks fingerprints,
  journals every hop (so `undo_operations` reverses it), and runs a copying rename (S3) as one move.
- The preview runs off the IPC thread with a 5 s deadline.

## Presets (`presets.rs`)

`RecentsFile<MultiRenamePreset>` in `multi-rename-presets.json`, keyed by the trimmed, lowercased name: saving under a
taken name replaces it. Built-in presets (No change, Remove diacritics) live in the frontend (`spec.ts`) so their names
are translated.
