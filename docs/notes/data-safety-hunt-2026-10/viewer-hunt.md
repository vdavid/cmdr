# Data-safety hunt 2026-10: `viewer` slice (hunter report)

Slice: the file viewer backend (`apps/desktop/src-tauri/src/file_viewer/`), its IPC layer (`commands/file_viewer.rs`),
the viewer route's save flow (`src/routes/viewer/viewer-copy.svelte.ts`), "Open with" for routed files
(`file_viewer/open_with_extract.rs` + `menu/open_with.rs::launch_with`), Quick Look, and the F4 text-editor launch
(`file_system/text_editor*.rs`).

The viewer writes in exactly three places: the "Save as file" selection save (`session.rs::write_range_to_file`), the
preview temp pull (`materialize.rs::pull_to_temp`), and the "Open with" copy + stand-in (`open_with_extract.rs`). The F4
editor launch, Quick Look, and the media protocol only read. The save and the preview temps hold up well (temp + rename,
uuid subdirs, prefix-scoped reapers, `remove_dir_all` that doesn't follow symlinks, a no-escape basename). What survives
is in the "Open with" copy lifecycle and two low-severity save quirks.

### viewer-1. Edits an app saves into an "Open with" copy of an archive entry are silently deleted at the next Cmdr start

- **Severity:** medium
- **Confidence:** plausible (the app-side save into a `0o444` copy depends on the editor; traced end to end on Cmdr's
  side)
- **Where:** `apps/desktop/src-tauri/src/file_viewer/open_with_extract.rs:219`, `launch_paths_in`;
  `open_with_extract.rs:78`, `init_open_with_extract_dir`; `apps/desktop/src-tauri/src/file_viewer/materialize.rs:162`,
  `reap_temps_with_prefix`; `apps/desktop/src-tauri/src/lib.rs:382`
- **Excerpt:**

```rust
219	pub(super) fn launch_paths_in(paths: &[PathBuf], dir: &Path, cap: u64) -> Result<Vec<PathBuf>, RefusedCopy> {
220	    let spot = TempSpot {
221	        dir,
222	        prefix: TEMP_SUBDIR_PREFIX,
223	    };
224	    paths
225	        .iter()
226	        .map(|path| {
227	            if !path_routes_over_its_parent(path) {
228	                return Ok(path.clone());
229	            }
230	            // `None`: the route didn't confirm (a real folder named `foo.zip`), so the
231	            // path is a real file and opens as it is.
232	            let copy = extract_routed_into(path, &parent_volume_id(path), spot, cap).map_err(
...
240	            let Some(copy) = copy else {
241	                return Ok(path.clone());
242	            };
243	            make_read_only(&copy.temp_file);
244	            Ok(copy.temp_file)
```

```rust
162	pub(super) fn reap_temps_with_prefix(dir: &Path, prefix: &str) {
163	    let Ok(entries) = std::fs::read_dir(dir) else {
164	        return; // dir doesn't exist yet (first run) — nothing to reap.
165	    };
166	    for entry in entries.flatten() {
167	        let name = entry.file_name();
168	        if name.to_string_lossy().starts_with(prefix)
169	            && let Err(e) = std::fs::remove_dir_all(entry.path())
```

- **Mechanism:**
  1. Right-click a file inside a zip (or a `.git` snapshot) → Open with → an editor. `menu/open_with.rs::launch_with`
     calls `launch_paths` → `launch_paths_in` → `extract_routed_into` → `pull_to_temp`, which writes the entry to
     `<app_data_dir>/open-with-extract/.cmdr-open-with-<uuid>/<name>`.
  2. `make_read_only` sets permission bits to read-only (best-effort; a failure leaves the copy writable,
     `open_with_extract.rs:261`). That's a permission bit, not the `uchg` "locked" flag.
  3. The app opens the copy. Nothing in Cmdr tells the user it's a temp copy: no toast on success, no "read-only copy"
     wording (the `fileExplorer.openWith.copyRefused.*` strings cover failures only). The app's title bar shows only the
     entry's own basename.
  4. The user edits and saves. Common editors write through a permission-read-only file on request: VS Code offers
     "Overwrite" (clears the bit and writes), BBEdit and TextEdit offer to unlock or to save a duplicate, and a
     duplicate's Save panel opens in the same folder by default. Either way the user's work lands inside
     `.cmdr-open-with-<uuid>/`.
  5. Next Cmdr start: `lib.rs:382` → `init_open_with_extract_dir` → `reap_open_with_temps` → `remove_dir_all` on every
     `.cmdr-open-with-*` subdir, which permanently removes the edited copy and any sibling the user saved beside it. No
     Trash, no prompt, no log above `debug`.
- **Trigger:** A user who browses zips in Cmdr like folders opens `todo.md` inside `notes.zip` with Open with > VS Code
  (or another editor), edits it, answers the editor's read-only prompt with "Overwrite" or "Save a copy", and later
  quits and relaunches Cmdr (or Cmdr updates and restarts itself).
- **Guard ruled out:** The `0o444` bit is the only guard, and it's advisory for the owner: every mainstream macOS editor
  offers a way past it, and `make_read_only` is best-effort. Write-back is deliberately absent (`file_viewer/DETAILS.md`
  § "Open with": "edits an app saves stay in the copy"), and the startup-only reaper is deliberate too, but no step
  informs the user, checks whether the copy changed since the pull (mtime/size vs. the pulled values), or keeps a
  modified copy. Cmdr browses archives as writable folders elsewhere, so a user can reasonably expect the save went back
  into the zip.
- **Impact:** The user's new edits (content that exists nowhere else) are permanently deleted on the next app start. Not
  recoverable from Cmdr; only a Time Machine snapshot of `~/Library/Application Support` would have them.
- **Suggested fix:** Before reaping, skip (or move to the Trash) any copy whose mtime/size changed since the pull, or
  any subdir holding more than the one pulled file, and log it at `warn`. Optionally say "Opened a read-only copy;
  changes won't go back into the archive" in a toast on a successful launch.

### viewer-2. "Open with" copies of archive entries pile up for the whole run, up to 2 GiB each, with nothing reclaiming them

- **Severity:** low
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/file_viewer/open_with_extract.rs:66`, `OPEN_WITH_CAP_BYTES`;
  `open_with_extract.rs:219`, `launch_paths_in`; `apps/desktop/src-tauri/src/menu/open_with.rs:204`, `launch_with`
- **Excerpt:**

```rust
66	/// Max bytes pulled for one launch. Larger than the viewer's 256 MiB preview cap: a
67	/// whole video or a big PDF is what people open in a real app. The declared size is
68	/// checked before a byte is written, so this is also the zip-bomb guard, and each copy
69	/// stays on disk until the next launch, which is what keeps it bounded.
70	#[cfg(target_os = "macos")]
71	pub(crate) const OPEN_WITH_CAP_BYTES: u64 = 2 * 1024 * 1024 * 1024;
```

- **Mechanism:** Each Open with click pulls a FRESH copy into a new uuid subdir (no dedup, by design). Nothing removes a
  copy until the next process start (`init_open_with_extract_dir`). The comment's "until the next launch" means the next
  Cmdr launch, so the bound is per click, not per run. A multi-row launch where row N's pull fails (`RefusedCopy`) also
  leaves rows 1..N-1's copies behind, with no app ever opening them. There is no free-space check before the pull.
- **Trigger:** Over a long-running session (Cmdr runs for days), the user opens videos or large PDFs from inside
  archives a few dozen times, or re-clicks the same one.
- **Guard ruled out:** The per-copy cap and the declared-size refusal bound ONE copy; nothing bounds their sum. The
  viewer's own temps don't have this problem (`PreviewTemp` drops with its last session).
- **Impact:** The boot volume fills silently with hidden copies under Application Support, which can make OTHER apps'
  saves fail with ENOSPC. Cmdr's own data isn't lost; the space comes back on restart.
- **Suggested fix:** Reap copies older than a generous age whose file no process holds open, or cap the dir's total size
  and evict oldest-first; at least clean up a failed multi-row launch's earlier copies.

### viewer-3. "Save as file" of a selection silently replaces undecodable bytes with U+FFFD

- **Severity:** low
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/file_viewer/encoding.rs:427`, `decode_line`;
  `crates/cmdr-viewer/src/session.rs:960`, `write_range_to_file_inner`
- **Excerpt:**

```rust
427	pub fn decode_line(bytes: &[u8], encoding: FileEncoding) -> String {
428	    if matches!(
429	        encoding,
430	        FileEncoding::Utf8 | FileEncoding::Utf8WithBom | FileEncoding::UsAscii
431	    ) {
432	        return String::from_utf8_lossy(bytes).into_owned();
433	    }
...
446	    let (cow, _had_errors) = encoding.as_static().decode_without_bom_handling(bytes);
447	    cow.into_owned()
448	}
```

- **Mechanism:** `write_range_to_file` streams the selection through the same row decode as the screen
  (`read_range_streamed` → backend `get_lines` → `decode_line`), then writes UTF-8. A file detected as UTF-8 that holds
  a few Latin-1 or binary bytes (a typical mixed log) gets each invalid sequence replaced with U+FFFD; `_had_errors` is
  dropped, so the save reports success. The "Save as file" button is offered precisely for the 100 MiB+ selections the
  clipboard refuses, which is the "split a huge log" use case where a byte-exact slice is what the user expects.
- **Trigger:** Open a 500 MB log, ⌘A or select a range, hit ⌘C, take "Save as file" from the refuse dialog.
- **Guard ruled out:** The decode is documented ("a UTF-16 source saves as UTF-8 text"), but the lossy U+FFFD
  replacement isn't surfaced to the user anywhere, and there's no byte-exact path.
- **Impact:** The saved copy differs from the source; the original file is untouched, so nothing irrecoverable is lost.
- **Suggested fix:** Surface `had_errors` (and the `from_utf8_lossy` equivalent) as a typed "some characters couldn't be
  decoded" note on the save result, or save the raw byte range when the decode would be lossy.

### viewer-4. A save the viewer reports as "took too long" can still replace the destination afterwards

- **Severity:** low
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/commands/file_viewer.rs:469`, `SaveWatch::give_up`;
  `crates/cmdr-viewer/src/session.rs:972`, `write_range_to_file_inner`;
  `apps/desktop/src-tauri/src/file_viewer/range_read.rs:352`
- **Excerpt:**

```rust
469	    fn give_up(&mut self) -> Option<ViewerError> {
470	        if self.progress.is_done() {
471	            // It returned a moment ago; the waiter takes its real result instead.
472	            return None;
473	        }
474	        // The waiter detaches the work rather than dropping it, so the save has to be
475	        // told: it sees the flag at its next chunk, removes its temp file, and answers
476	        // `Cancelled` to nobody.
477	        let _ = file_viewer::cancel_read(&self.session_id, self.read_id);
478	        // `TimedOut`, not `StoppedResponding`: that one means "the source stopped
479	        // sending the file", which says the wrong thing about a save's destination, and
480	        // the FE already words this variant for the save ("that took too long").
481	        Some(ViewerError::TimedOut)
482	    }
```

- **Mechanism:** When the LAST 1 MiB write blocks for over 90 s (hung network mount), `give_up` flips the cancel flag
  and the FE toasts "that took too long". But `read_range_streamed` only reads the flag between rows and chunks; once
  the final `write_all` returns, it exits through `ChunkEnd::EndOfFile` → `emit.finish(false)` → `Ok` with no further
  check (`range_read.rs:352–364`), and `write_range_to_file_inner` goes on to `std::fs::rename(tmp_path, dest_path)`.
  The comment's promise ("removes its temp file, and answers `Cancelled`") doesn't hold for a stall in the final write.
  A write that never returns also leaves the `.cmdr-tmp-<uuid>` staging file behind with no reaper.
- **Trigger:** "Save as file" to a folder on a flaky NAS mount where the last write stalls for 90 s or more, then
  completes.
- **Guard ruled out:** The user confirmed the overwrite in the native save panel, so the content that lands is what they
  asked for. The harm is the false "it didn't happen" report: a user who then thinks the old destination file is still
  intact (or saves elsewhere and deletes the source) is misled.
- **Impact:** The destination is replaced after the user was told the save failed. No data the user didn't agree to
  overwrite is lost.
- **Suggested fix:** Check the cancel flag once more right before the rename in `write_range_to_file_inner`, and remove
  the temp instead of renaming when it's set.

## Near misses

- **Save over an existing file**: `rename` overwrites, but the destination comes only from the native save panel
  (`viewer-copy.svelte.ts:322`), which confirms the replace.
- **Save over the file being viewed**: the session reads through a held handle on the old inode (`origin.rs`, one handle
  per session), so the save reads consistent bytes and the rename atomically swaps the file.
- **Save into an archive or `.git` tree**: refused up front with typed `DestinationIsReadOnly`
  (`commands/file_viewer.rs:399`).
- **Save temp collision / leftover on error or cancel**: uuid `StagingTemp::mint` sibling, removed on every `Err` path
  (`session.rs:972–980`), and hidden from panes while in flight.
- **Soft-wrapped rows gaining newlines in the save**: `continues` rows don't get a delimiter (`range_read.rs:340`).
- **Short chunk read as EOF truncating a save**: termination is by `ChunkEnd::EndOfFile`, never by row count
  (`range_read.rs:352`).
- **Whole-file save drops the final newline**: half-open `Eof` semantics (`range_read.rs:364`); cosmetic, intentional.
- **Preview temp cleanup deleting a non-temp**: `PreviewTemp::drop` removes only its own uuid subdir; reapers match only
  their prefix in their own per-instance dir; `remove_dir_all` doesn't follow a symlinked entry.
- **Archive entry name escaping the temp dir (`../`, `/`)**: `temp_basename` keeps only `Path::file_name()`, and
  `..`/`.`/empty fall back to `preview` (`materialize.rs:540`).
- **Stand-in extension escaping its dir**: `Path::extension()` can't carry a separator; the stand-in is only ever shown
  to LaunchServices, never handed to an app.
- **Zip-bomb / huge entry exhausting disk in the viewer**: declared-size refusal before any write plus a streaming byte
  cap (`materialize.rs:450`, `:516`), 256 MiB per window, freed with the last session.
- **Viewer reload/tail on a half-written file leading to a truncating save**: tail mode and reload never write; a save
  reads the session's backend (a held handle), and the source is never written back.
- **Preview temp deleted while a backend still reads it**: unlink keeps the open fd valid on POSIX.
- **F4 editor launch on a routed or device path**: the FE pane guard (`file-explorer/pane/editor-open.ts`, per
  `open-file-in-editor.ts`'s header) admits only real local files, and the argv's file is an absolute path, so no `open`
  option injection.
- **Quick Look**: read-only, gated on `paths_are_os_visible()`.

## Covered

Read in full:

- `apps/desktop/src-tauri/src/file_viewer/materialize.rs`
- `apps/desktop/src-tauri/src/file_viewer/open_with_extract.rs`
- `apps/desktop/src-tauri/src/file_viewer/range_read.rs`
- `apps/desktop/src-tauri/src/file_viewer/media_session.rs`
- `apps/desktop/src-tauri/src/commands/file_viewer.rs`
- `apps/desktop/src-tauri/src/file_system/text_editor.rs`
- `apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`, `apps/desktop/src-tauri/src/quick_look/CLAUDE.md`

Read in part (the write, temp, and lifecycle paths):

- `crates/cmdr-viewer/src/session.rs` (open path 140–440, `with_registered_read`, `SaveProgress`,
  `write_range_to_file*`, `cancel_read`)
- `apps/desktop/src-tauri/src/file_viewer/encoding.rs` (`decode_line`)
- `apps/desktop/src-tauri/src/file_system/text_editor_macos.rs` (choice, `launch_argv`, `open_in_editor`, `launch`)
- `apps/desktop/src-tauri/src/menu/open_with.rs` (`launch_with`, `announce_refused_copy`)
- `crates/cmdr-fs/src/staging.rs` (`StagingTemp`)
- `apps/desktop/src/routes/viewer/viewer-copy.svelte.ts` (save flow), `viewer-menu-actions.ts`,
  `src/lib/text-editor/open-file-in-editor.ts`
- `apps/desktop/src-tauri/src/file_viewer/DETAILS.md` § "Open with"

A repo-wide grep over the slice for `remove_file|remove_dir|fs::write|File::create|rename(|OpenOptions` found no other
write sites beyond the three above.

## Not covered

- Read-only backends not read line by line: `byte_seek.rs`, `line_index.rs`, `full_load.rs`, `rows.rs`, `row_walk.rs`,
  `search_matcher.rs`, `content_kind.rs`, `headless.rs`, `analytics.rs`, `watcher.rs`, `pending_open.rs`, `origin.rs`,
  `source.rs`, `volume_source.rs`, `media.rs`, `media_protocol.rs`, `media_backend.rs`, `session_store.rs`, and
  `session.rs` outside the ranges above (encoding swap, tail extend, reload). The grep shows none of them writes; their
  worst case is a wrong read or memory use, not a write.
- `quick_look/controller.rs` and `commands/quick_look.rs` (read-only by the grep and the `CLAUDE.md`).
- The viewer route's other FE files (rendering, selection, scroll).
