# Multi-Rename Tool: details

Total Commander's Multi-Rename Tool, the semantics taken from its help (TOTALCMD.CHM, TC 11.58) and wiki. Proposed
upstream in vdavid/cmdr#372.

## Pipeline

Per row, in rename order (`position` counts from 0 and is what `[C]` counts):

1. `Mask::render` the name mask and the extension mask over `RowFacts` (name, extension, parent, grandparent, modified
   time in local time, position).
2. `CompiledTransform::apply`: search & replace on the name (and the extension with `include_extension`), then the case step
   (lower and upper on both, first-upper and words on the name only: `.Jpg` is never wanted), then Greek to Latin, then
   `remove_diacritics`, both on name and extension. The search compiles once per preview (`Transform::compile`), never
   per row.
3. `name` + `.` + `extension`, or just `name` when the extension renders empty.
4. `Compiled::finish`: composed (NFC) when the spec normalizes.

**Normalize Unicode** (`normalize_unicode`). Off, a new name that differs from the old one only in its Unicode form is
`Unchanged` (`Compiled::same_name`). On, only the exact spelling is, so a decomposed name (macOS and SMB often hand one
over) renames to its composed form, the one Windows, Linux, and the web expect (verified on APFS with a real rename by
`run_test::normalizing_renames_a_decomposed_name_to_its_composed_spelling_on_disk`, 2026-10-10).

**Greek to Latin** (`transliterate.rs`, ELOT 743, close to ISO 843 type 2, what Greek passports use): letter by letter
with the digraphs that read as one sound (`ου`, `αυ`/`ευ`/`ηυ` as `v` or `f` by what follows, `γγ` `γξ` `γχ`), tonos
dropped, a dialytika splitting a pair, capitals cased by their neighbor (`Θέση` → `Thesi`, `ΘΕΣΗ` → `THESI`). It runs
before diacritics, so with both on `Αθήνα Café` is `Athina Cafe`. **Decision/Why:** ELOT 743 over a letter table, ported
from PR #386 (Jiri Slovacek): a plain table reads `ευ` as `eu`, which no Greek reads it as.

## Placeholders (`mask.rs`)

- Fields with ranges: `[N]` `[E]` `[P]` `[G]`, and a bare range (`[2-5]`) on the full name. `[N1]` one character,
  `[N2-5]`, `[N2,5]` (start, length), `[N2-]`, negative starts count from the end; with a negative start a positive end
  counts from the end too (`[N-8-5]` is 8th-last to 5th-last), as TC documents. `bounds` turns a range and a text
  length into the first and last character taken (or none); `slice` and `RangeToken::bounds` both go through it, so the
  range editor's highlight is exactly what a rename takes. The frontend's `range-token.ts` parses and writes the same
  grammar, tested against `range_token_vectors.json` (its `selections` check that each token the editor writes takes
  exactly the characters the user picked).
- Counter `[C]`: 1, 2, 3, in the name mask or the extension mask. Each counter carries its own settings, as TC writes
  them: start, `+`/`-` step, `:` digits (`[C10+5:3]`, `[C10]`, `[C+5]`, `[C:3]`, `[C100-10]`), each defaulting to 1.
  **Decision/Why:** the settings live in the mask, not in sheet-wide fields, so two counters can count differently and a
  preset is its masks; a bare `[C]` never pads to the batch size (David's call: TC doesn't either). A lone leading sign
  is the step (`[C-5]` counts down by five, as in TC), so a negative start needs its step: `[C-5+1]` (Cmdr's extension;
  TC has no negative start).
- Date and time of the last modification: `[Y]` `[y]` `[M]` `[D]` `[h]` `[m]` `[s]`, combinable (`[YMD]`, `[hms]`),
  `[d]` ISO date, `[t]` `hh.mm.ss` (TC's country-specific forms would put `:` in names, which macOS shows as `/`).
  A combination may keep `-` `.` `_` or a space between its letters, each rendered as itself: `[Y-M-D h.m.s]` is
  `2026-07-14 09.05.30`, `[D.M.Y]` is `14.07.2026`. **Decision/Why:** Cmdr's extension (TC has none), so one
  placeholder reads as the name it makes. It must start with a letter, so `[2-5]` and `[-3-]` stay full-name ranges;
  `:` and `/` aren't separators (the slash problem above, and `/` splits paths); `d` and `t` stay whole-placeholder
  only.
- Media fields `[=source.field|modified:format]` (`mask/media_token.rs`, a child of `mask` so it shares the date
  letters, `range`, `slice`, and `pad`): the file's EXIF or tag value, named by `MediaField::name` (`exif.taken`,
  `tag.albumartist`; exact, case matters). The format follows the field's kind: a date takes `date_parts`
  (`[=exif.taken:Y-M-D]`, `:d`, `:t`; none reads as `[d] [t]`), a number a digit count 1–`MAX_COUNTER_DIGITS`, padded
  like the counter and never cut (`[=tag.track:2]` is `03`), text a range (`[=tag.artist:1-10]`). `|modified` (dates
  only) comes before the format. An empty suffix, a suffix out of order, or a format the kind doesn't take is
  `MaskError::Unknown`. **Decision/Why:** `=` is TC's content-plugin shape (`[=plugin.field]`), and no other
  placeholder starts with it, so no mask that parsed before changes meaning.
  - **A field with no text is a `FieldGap`, never an empty string**: `Mask::render` answers `Result<String, FieldGap>`.
    No value (or a file we don't read, folders included) is `NoValue { field }`, a failed read `Unreadable { field }`,
    a file still being read `Reading`. The first gap wins, except `Reading` wins over any other, in either mask
    (`Compiled::render`): a value may still come. `plan::preview` turns a gap into the row's status
    (`Reading` / `NoValue` / `Unreadable`) and keeps the old name as its new one, so the file holds its name in
    `settle`.
  - **`|modified` stands in for a file without its own date: no value, `NotMedia`, or `Unreadable`**, but never for one
    still `Reading`. **Why:** otherwise a row would show the modified date, then flip to the taken one mid-read, and the
    token editor's "N files use the modified date" would count rows that end up with their own. A file with no
    modified date either is `NoValue`.
  - `MediaToken` (`parse`, `of`, `field`, `resolve`, `value_text`) is the placeholder alone, for the session's field
    samples and the picker's examples: `resolve` says whether the text is the file's own (`FieldText::Value`) or the
    fallback (`FieldText::Fallback`). The frontend's `media-token.ts` parses and writes the same grammar, tested against
    `media_token_vectors.json` (which also lists every field's name, IPC name, and kind).
- Case switches `[U]` `[L]` `[F]` `[n]` apply from where they stand, media fields included.
- `[[` is a literal `[`.

Not in v1, all planned in #372: alphabetic counters, `\` to move into subfolders (the executor's one-parent rule refuses
it today), "next step" chaining, and editing names in an editor.

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
  batch row renaming away frees its name, so chains and swaps preview as ready. Names fold as the Mac does, so a
  case-sensitive volume gets over-cautious statuses; a block cascades (a blocked row stays, which can block the row
  renaming into it).
- **Decision: a backend session holds the files, by name.** `open_multi_rename(listing, includeHidden, rows | null,
  sequence)` reads the pane's rows (`null`: every row the pane shows, which is what "all selected" and "nothing
  selected" both mean) through `Listings::selection_names`, refuses with `SelectionChanged` when the sequence or the
  hidden-files setting moved (the `selection_snapshot` guard F5 uses), and stores the names. Every preview looks them up in the
  listing's entries by name. Why: the sheet first sent row numbers and every preview mapped them through the LIVE
  listing, so a file landing above the selection shifted it onto files the user never picked, in the preview and at
  Start. A row's `row` is its place in the session, so it's stable across previews until the user moves it.
- **Rename order** (⇧↑ / ⇧↓ in the sheet's list, `move_multi_rename_row`): the session's names ARE the rename order,
  the one `[C]` counts. `move_row(session, row, direction)` swaps the file at `row` with its neighbor and answers its
  new place (the same at that end; a row past the end is `Internal`). **Decision/Why: a move retires the latest
  preview** (`latest = None`), so paging, Results, and apply refuse it with `PreviewOutOfDate` until the sheet's next
  preview: its rows no longer say what the session renames, and apply must never start from an order nobody saw. The
  sheet sends the cursor's INDEX, which with every row listed is the place, so a second move before the new preview
  lands still names the moved file; it refuses moves while listing the problems alone (an index there isn't a place).
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
- **Range samples** (`get_multi_rename_range_samples(session, token, rows)`, `session::range_samples`): for the sheet's
  range editor, what one range placeholder takes from a few session files (at most `MAX_RANGE_SAMPLES`): each row's
  field text and the taken `first`/`last` character (Unicode scalar indices, what `Array.from` splits in JS), or none.
  **Decision/Why: it reads the latest preview's rows** (old name, folder or not), not the listing: they're the rows
  the sheet lists, the call stays a lookup of a few structs with no listing lock, and a moved row's retired preview
  answers `PreviewOutOfDate` until the next one, which the sheet's cursor row follows anyway. Names are composed first,
  as the preview renders them. A token that isn't a range is a `Spec` error.
- Sessions live in a process-wide map, at most `MAX_SESSIONS`; opening one drops sessions idle past `IDLE_LIMIT`,
  then the least recently used. The sheet closes its session (`close_multi_rename`) when it closes.
- **Results (⌥⏎, TC's "edit names", `names_file.rs`)**: `write_multi_rename_names(session, previewId)` writes that
  preview's rows (a `Missing` one gets no line) as `old<TAB>new` lines to
  `<app cache dir>/multi-rename/<pid>-<session>.txt`; the sheet opens it in the user's editor and calls `read_multi_rename_names` when its window gets focus again. A line
  counts only when its new name differs from what was written, so an untouched row keeps following the settings and
  an earlier edit stays; the merged names (`NameEdits`, by composed old name) become the session's, and every preview
  from then on puts them through `Compiled::finish` and all the status checks (`PreviewRow::edited` marks them).
  `clear_multi_rename_names` drops them. **Decision/Why:** by old name, never line number, so a file appearing
  meanwhile can't shift a name onto the wrong row; the new name is after the LAST tab, so an old name holding a tab
  still splits. Only the path the session wrote is read (`Written`), so the frontend never names a file to read.
  `StoredPreview` keeps the edits it showed, and apply recomputes with those: an edit read back after the preview on
  screen never sneaks into its rename. Dropping a `Written` deletes the file, so close, eviction, and clear tidy up.
  The file is read outside the sessions lock. Ported from PR #386 (Jiri Slovacek), whose read-back was process-wide.
- **Decision/Why: the Results folder is the user's alone.** `names_file::write` makes `multi-rename/` with mode `0700`
  and refuses one that's a symlink, someone else's, or open to others; the file is `create_new` + `O_NOFOLLOW`, `0600`,
  and a stale one is removed first only if it's a regular file. `read` won't follow a link or open a named pipe. Why:
  in a shared `/tmp` on Linux, another account could pre-create the folder, plant a link at the guessable name so the
  write overwrote the user's own file, swap in its own names, or read the user's (issue #546). The app's cache dir
  is per-user and per-instance, and "Delete all Cmdr data" removes it whole.
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
- `plan::preview_names` is the preview BY FILE NAME (a gone name is `Missing` in place; the counter numbers only the
  files still there). The session and the MCP `multi_rename` tool (`../tools/actions/multi_rename.rs`) both call it, so
  an agent's dry run shows exactly the statuses the sheet would. The tool needs no session: it previews against the
  pane's cached listing and, for a real run, stages the ready rows as a rename proposal rather than calling `run.rs`.

## Media fields (`media.rs`, `crates/cmdr-media-meta`)

`[=source.field]` placeholders (EXIF, video capture dates, media tags; issue #657). The crate reads files and names the
fields (`MediaField`, `MediaFacts`; `crates/cmdr-media-meta/CLAUDE.md`); this module caches them per session and
answers the sheet.

- **Rows see media through `RowFacts.media: RowMedia`** (`Reading`, `Read(&MediaFacts)`, `Unreadable`, `NotMedia`).
  `plan::preview` / `preview_names` take a `&dyn MediaLookup`, whose `media(&FileEntry)` answers each row; the entry
  carries size and modification time, so a lookup can tell a file that changed since it was read. `NoMedia` answers
  `NotMedia` for every row: the MCP tool (for a mask without media fields, `Compiled::uses_media`) and the plan tests
  pass it. The session passes its cache's `Lookup`.
- **The MCP `multi_rename` tool shares this cache and loader** with no sheet: it seeds its own `SessionMedia`, takes an
  open session's answers over the same folder (`SessionMedia::adopt`, same listing stamp only, one lock at a time),
  and awaits `SessionMedia::read_all` (the same `run`, inline, no re-queue behind it). Cap and cancel:
  `../tools/actions/DETAILS.md`.
- **The cache** (`SessionMedia`, one per session, `Arc`-shared with its loader): each file's `MediaRead` (`Reading`,
  `Read(Arc<MediaFacts>)`, `Unreadable`, `NotMedia`) with the listing's size and modified time it was read for, the
  read queue, the files in flight, and running counts for `MediaProgress`. `session::open` seeds it from the listing:
  a non-folder `is_media_name` file is `Reading` and queued in rename order, everything else `NotMedia`. A session
  with no media files is `done` from the start and never runs a loader.
- **The loader** (`SessionMedia::run`, a tokio task): `open_multi_rename` starts it right after the session opens
  (`session::start_media`, with the command's `AppHandle` as the emitter). It reads up to `MAX_PARALLEL_READS` (8)
  files at once, each on `spawn_blocking` through `read_media_from` over a `VolumeByteSource` (the volume resolved
  per file from the volume manager, so a reconnect's new volume is the one read) under one file's budget,
  `io_budget_for_volume(10 s)`. A reader that panics on a hostile file makes it `Unreadable` (`catch_unwind`); why
  any read failed is logged at debug, the row only says it couldn't read.
  - **Order**: rename order, so row 0 first; `prioritize_multi_rename_media` (the rows the sheet shows) moves those
    files to the front, behind row 0 while it still waits. Files in flight aren't queued twice.
  - **Progress**: `MultiRenameMediaProgress` at most every 250 ms, plus one when a run ends. `done` is computed
    (`read == total`), never forced: a run that ends while a re-read starts another doesn't claim it. `total` counts
    media files only; an `Unreadable` file counts as read.
  - **A dead volume**: after three timeouts or I/O errors in a row (`GIVE_UP_AFTER_FAILURES`; a read that succeeds
    resets the count, a panicking parser doesn't count), or once the volume is unregistered or not live
    (`Loader::volume_gone`, asked before each read), every file still waiting is `Unreadable` in one step and the run
    ends. So a dropped share costs a few reads, not a 10 s budget per file, and at most
    `parallel + GIVE_UP_AFTER_FAILURES - 1` timed-out reads leave a detached `read_range` behind per run.
- **Freshness**: a preview's `Lookup` compares each entry's size and modified time with what the file was read for. A
  mismatch answers `Reading`, and `Lookup::finish` (after the preview, under the cache's lock) re-queues the
  file at the front and starts the loader if it was idle. A read that finishes for an older stamp is dropped and
  queued again, never cached.
- **Cancel**: `Session`'s `Drop` (close, eviction, idle drop) sets the cache's cancel flag. No new read starts, a read
  in flight stops at its next block fetch (`VolumeByteSource` checks the flag), the loader exits without a final
  event, and the cache goes with the last `Arc`.
- **Locks**: ❌ nothing reads a file under `SESSIONS` or the cache's lock. Callers copy the cache's `Arc` out of
  `SESSIONS` (`with_session`) and release it first. The cache's lock is held only for short steps: storing an answer,
  copying the cache out (a preview's `Lookup` is a copy, keyed by `Arc<str>` so it's cheap), or resolving a few sample
  rows. The overview, field samples, and prioritizing run on blocking threads (`commands/multi_rename.rs`).
- **Field samples and the overview read the cache, not the latest preview**, so the token editor shows values before
  any preview uses the token. `field_samples` resolves the token (`MediaToken::resolve`) per file with the modified
  time the file was read for; `usage` covers every session file and is remembered per token until the cache changes
  (a generation counter), so a keystroke that doesn't change the token doesn't walk every file again. The overview's example is the first file in rename
  order with a value (`MediaToken::value_text`), and `files_with_it` counts read files that have the field.
- **What a read costs**: the header blocks a parser touches, 64 KiB at a time, never the cover art, at most 4 MiB a
  file (`crates/cmdr-media-meta/DETAILS.md` § The byte source).
- **Why the queued Rename lives in the frontend**: apply's proof (`(session, preview)` ready rows equal the preview's)
  stays untouched, and the queue is UI state that a close already ends. A backend queue would need its own cancel and
  a way to report back to a sheet. `prepare` refusing `StillReading` is the backend's safety net.
- Tests: `media_test.rs` (a stand-in reader: order, throttled progress, freshness, cancel on close, a panicking
  reader, a dead volume, samples, `StillReading` versus `PreviewOutOfDate`), `media_real_test.rs` (real files on the
  local disk through the real read path), `session.rs`'s eviction test.
- **Statuses**: `RowStatus::Reading` (not a problem, not ready; `PreviewCounts.reading`), `NoValue { field }`, and
  `Unreadable { field }` (problems). A placeholder's own miss is a `FieldGap` (`Reading`, `NoValue`, `Unreadable`, each
  naming the `MediaField`), which crosses IPC in `FieldSample`.
- **`MediaField` crosses IPC as its camelCase variant** (`exifTaken`); the mask writes its dotted name (`exif.taken`).
- **IPC** (`commands/multi_rename.rs`): `get_multi_rename_media_progress` (for a sheet that subscribed late),
  `get_multi_rename_media_overview` (the picker: progress plus each field's example and file count),
  `get_multi_rename_field_samples` (the token editor: the token alone on the cursor rows, plus `FieldUsage`), and
  `prioritize_multi_rename_media` (fire-and-forget). Events: `MultiRenameMediaProgress` (`multi-rename-media-progress`).
- **`MultiRenameError::StillReading`**: apply's safety net while rows still read; the sheet keeps a queued rename
  waiting on it rather than showing an error. Only a preview that was itself still reading gets it: when a fully read
  preview's recompute finds `Reading` rows (a file changed under it), `prepare` answers `PreviewOutOfDate`, since a
  queue would then run names the user never saw.

## Presets (`presets.rs`, `preset_store.rs`)

One list in the user's order, top to bottom, in `multi-rename-presets.json` (`PresetStore`, its one instance the
`preset_store::PRESETS` static; `presets.rs` stays the pure list logic and types, so the store depends on it and not
the other way around), keyed by the trimmed,
lowercased name: saving under a taken name replaces that preset (the sheet asks first). "Reset all fields" lives in
the frontend (`spec.ts`).

- **Decision/Why: an ordered store, not a recents list.** The sheet's Presets menu numbers presets 1–9 by position, so
  a number may move only when the user moves or deletes something. A recents list puts every add on top, which
  renumbered the menu on each save. Each change is a pure op on the `Vec` (`save_in`, `rename_in`, `update_spec_in`,
  `delete_in`, `move_in`) run by `PresetStore::edit` as one locked step with one durable temp+rename write, and every preset
  command answers the list after, so the sheet never reads it back separately.
- **Save** (`save_in`): a new name goes at the END; a taken name is replaced where it stands. A new one past
  `MAX_PRESETS` is refused with a typed `PresetSaveRefusal::ListFull` (the name popover words it), never trimmed off
  another end: either would lose or renumber a preset.
- **Move up / Move down** (`move_multi_rename_preset`, `move_in`) swap a preset with its neighbor; at that end of the
  list it's a no-op (the sheet greys the item).
- **Rename** and **Update with current fields** change the preset where it stands. A rename onto a taken name drops
  the other preset in that same step (`rename_in`), so no reader ever sees two presets with one name, or neither.
- **Seeds** (`seed_in`, `PresetSeed`): the list starts with Date and time (`[Y-M-D h.m.s]`), Remove diacritics,
  Greek to Latin, and Normalize Unicode, as ordinary presets the user can rename, update, move, and delete. They go in
  ONCE per list, after the presets already there, skipping a name the list has and stopping at `MAX_PRESETS`, so no
  number the user knows moves. `seeded` in the file records it, so a deleted seed stays gone and an emptied list stays
  empty. **Known limit:** an older build rewrites the file without `seeded`, so after a downgrade-then-upgrade the
  seeds are offered again: a deleted one returns, and a renamed one comes back beside the renamed copy. No file (a new install) is unseeded; so is a file from before seeding (no `seeded` field), which is how
  an existing user gets them; a file set aside as unreadable counts as seeded (the person's list is in the copy).
  **Decision/Why: the frontend names them, the backend decides.** `get_multi_rename_presets(seeds)` takes the seeds,
  names already in the UI language (`spec.ts`'s `PRESET_SEEDS` through `tString`) and runs `seed_in` before answering;
  every later call is a no-op for them. The names are message keys the backend can't read, and from then on they're
  plain user data, so nothing re-translates them. A preset saved before the first read keeps its place: the seeds go
  after it.
- **The file** keeps the `crate::recents` envelope (`_schemaVersion: 1`, `entries`) plus `seeded`, so an older build's
  file reads as stored (newest first) and an older build reads ours. A file that won't read goes through
  `state_recovery::read_state_file` (set aside, the person told once), like every state file; the lock discipline is
  `recents`' (two locks, never held at once).
- **Last settings** (`LAST_SPEC`, `LastSpec`): a one-entry `RecentsFile` in `multi-rename-last.json` (fixed
  `dedupe_key`, so a save replaces it) holding the spec and the `presetId` the sheet closed with. Ported from PR #386;
  the preset is Cmdr's addition (sheet side: `src/lib/multi-rename/DETAILS.md` § Presets). `StoredLastSpec` also reads
  the older `preset: { kind, id }`: a `saved` one keeps its id, a `builtIn` one (those are seeds now, with ids of their
  own) names no preset and keeps the fields.
- **Newer options load off.** `greek_to_latin` and `normalize_unicode` are `#[serde(default)]`, so a preset (or last
  settings) saved before they existed loads with both off.
- **Legacy counter fields.** Presets saved while the sheet had counter start / step / digits fields still carry
  `counterStart` / `counterStep` / `counterDigits`. `MultiRenamePreset`'s hand-written `Deserialize` folds them into
  every counter of both masks (`mask::fill_counter_defaults`): a part the counter already has stays, a missing one takes
  the old field unless that's the default 1, so `[C]` with start 10, step 5, digits 3 becomes `[C10+5:3]` and `[C:4]`
  with start 10 becomes `[C10:4]`, the names the preset always made. Nothing writes the fields anymore; the next save of
  that preset drops them.

## Field history (`history.rs`)

TC's per-field history (↓ in a field): `RecentsFile<FieldHistoryEntry>` in `multi-rename-history.json`, all four text fields
(name mask, extension mask, search, replace) in one list of `MAX_FIELD_HISTORY` (200), deduped by field and value, so
using a value again moves it to the top. `apply_multi_rename` reads the spec of the preview it starts from
(`session::spec_of`) and, once the rename has started, adds `history_entries`: a field at its no-change default (`[N]`,
`[E]`) or empty is skipped, and a replacement counts only beside a search; the name mask goes in last, so it lands on
top. **Decision/Why:** recorded on a started rename, never on typing, so the list holds values that renamed something.
`get_multi_rename_history` hands the sheet the whole list; it filters by field. Ported from PR #386 (Jiri Slovacek),
where it lived in `presets.rs`.

- **A new install's history is seeded** (`history_seed`, through `RecentsFile::load_seeded`): the name mask's ↓
  offers `[Y-M-D h.m.s]`, then `[N]` (which a rename never records, being the default). Only a missing file seeds; the
  seed reaches disk with the first rename, so an emptied or set-aside history never gets it back.
