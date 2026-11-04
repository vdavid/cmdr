# File viewer host details

Pull-tier docs for the desktop app's host of the file viewer. The viewer's core (rows, backends, search, encodings,
sessions, Markdown, media tokens, tail mode) is `crates/cmdr-viewer`, documented in `crates/cmdr-viewer/DETAILS.md`.
Must-knows: `CLAUDE.md`.

## Key files

- `mod.rs`: re-exports the core (`pub use cmdr_viewer::*`), so the app names it as `crate::file_viewer::…`.
- `app_host.rs`: the app's one `Viewer` (`app_viewer()`, on Tauri's runtime, minting into the token table the
  thumbnails share) and its answers to the core's host seams: the volume manager (`AppVolumes`), the materializer
  (`AppPulls`), the Markdown image fetches (`AppImages`), analytics, and the Tauri events and opener, which reach a
  window through the app handle `init_app_handle` stores at startup (until then, in the app's unit tests, they answer
  nothing). Builds the `viewer:file-changed:<sid>` payload.
- `events.rs`: the `tauri_specta::Event` structs for the core's facts, `ViewerMediaLoad` and `ViewerPullProgress`,
  same JSON as the facts (`wire_events_test.rs` pins it).
- `media_protocol.rs`: the `cmdr-media://` scheme handler, a Tauri shell over the core's `media_serve.rs` (§ "Media
  rendering" in the core's docs has the token model and the `Range` rules).
- `materialize.rs` + the core's `pending_open.rs`: pulling a file the OS can't open into a bounded temp. § "Preview of a
  routed file".
- `markdown_image_fetch.rs`: `VolumeImage` and `RemoteImage`, how a rendered document's pictures are reached.
- `open_with_extract.rs`, `open_with_copies.rs`: "Open with" on a routed file. § "Open with on a routed file".
- `analytics.rs`: `viewer_opened`, ❌ never a file name or extension.
- Tests here need the app (its volume manager, routes, log markers, scheme): `snapshot_open_test.rs`,
  `smb_open_test.rs`, `log_view_test.rs`, `markdown_session_test.rs`, `media_volume_test.rs`, and the rest beside
  their files.

## Preview of a routed file

`materialize.rs` lets the viewer preview a file the OS can't open: one only a ROUTE can serve, an archive-inner
path (`/…/foo.zip/inner.txt`) or a path in a repo's virtual `.git` trees (`/…/.git/branches/main/src/lib.rs`), and a
file on a volume whose paths aren't OS-visible (§ "A file on a volume the OS can't open" below). The viewer core is
100% `std::fs::File`-based (byte-seek, line-index, encoding, and the `cmdr-media://` handler all `File::open` a real
path), and a route mints a read-only volume with no positioned reads, so `open_session` streams the file to a bounded
temp and opens THAT — the deliberately simple bridge. A volume that DOES hold a file open for positioned reads skips
the temp: `crates/cmdr-viewer/DETAILS.md` § "A file read through its volume". So does a picture or PDF on a volume that reads by position: its § "Media on a volume".

**One code path for both routes.** Both mint a read-only volume, both answer `open_read_stream`, and a second copy of
the temp lifecycle would drift. The only thing that reads the `RoutedKind` is the error mapping: the archive family
(encrypted, corrupt, unsupported codec) has its own frontend copy under `ViewerError::Archive`; its nested
`ArchiveFailureKind` preserves `NotSupported` for non-UI consumers while the viewer keeps one generic message. A portal
read that fails has no such family, so it stays a plain `ViewerError::Io`. Path-shaped errors keep their twins either
way.

Flow (in `open_session_inner`, before the media/text split):

1. `materialize_for_viewer(requested, volume_id, open)` calls `VolumeManager::resolve(volume_id, requested)` — the SAME shared
   routes + on-demand volume registration + LRUs the listing and copy paths use, so the pane label and the preview
   target can't disagree. An unrouted path returns `None` and the open flows through unchanged. `volume_id` is threaded
   from `viewer_open` (command → FE `viewerOpen` wrapper → the viewer window's `volume` URL param, sourced from the
   opening pane's DRIVE volume id), so a `.zip` on a REMOTE parent (direct SMB / MTP) is pulled through that parent's
   `read_range`, not a hardcoded `"root"`. `volume::manager::path_routes_over_its_parent` gates the resolve; the
   confirm inside `resolve` handles a mislabeled or remote-only archive and a `.git` that isn't a repository.
2. For a routed path it reads the entry's `get_metadata` (from the archive's central directory or the portal's tree
   entry, never a decompression or a blob read). A directory entry → `ViewerError::IsDirectory`. A size over the cap →
   `ViewerError::TooLargeToPreview`, refused BEFORE any temp is created — this up-front refusal is the zip-bomb guard for
   preview.
3. Otherwise it streams `open_read_stream` into `<extract_dir>/.cmdr-viewer-<uuid>/<entry-basename>`, enforcing the cap
   again on bytes written (a central directory that understates the real size can't sneak past). The basename is the
   entry's, so the viewer window shows the right title and media classification sees the right extension.
4. `open_session` then runs its normal media/text classification on the temp. The `ViewerSession` holds it as an
   `Arc<PreviewTemp>`; a media open threads the same value through `try_open_media`. Extracted sessions spawn NO
   watcher (the temp is immutable for the session's life).

**A temp lives as long as the last session holding it.** `PreviewTemp`'s `Drop` removes the subdir, and a session
drops its share in `close_session` (the single choke point both teardown paths funnel through). An open that fails
after its pull drops the temp on the spot. **A view switch shares, a fresh open re-pulls.** "View as text" (or back to
the image) reopens the file for the same window before the frontend closes the old session; `open_for_window` asks
`reusable_temp` for the window's current session's copy, and when `PreviewTemp::is_copy_of` matches the path and
volume, the new session shares it and reads nothing from the source. The reuse is scoped to one window: a second window
on the same file pulls its own copy, and closing and reopening a viewer re-pulls, which is how a user gets a newer copy.
Pinned by `materialize_test::switching_the_view_mode_reuses_the_windows_temp_and_reads_nothing_from_the_phone` (a
counting slow volume: zero chunks on the switch, one shared temp, gone after the last close) and
`another_window_on_the_same_file_pulls_its_own_copy`.

**The second caller: the agent's `inspect_file`** (`tools/read/inspect/`). Archives route first because they have
richer rows to give; every remaining route or non-OS-visible connected volume goes through
`materialize_for_inspect(path, volume_id, cancel)`, then the per-kind pipeline reads `temp_file`. Its `Drop` guard
removes `cleanup_dir`, so the temp lives for one read rather than a session. The inspector's five-second per-path
budget flips `cancel`; the pull observes it between chunks (never by dropping an in-flight MTP read), returns
`Cancelled` → `unreachable`, and removes the partial temp. A materialized row reports no `modified`: the temp was
written a moment ago, and quoting its mtime would date an old remote file as today. The contract both callers share,
and any third one inherits: the 256 MiB cap and refuse-before-extract guard stay inside the materializer, and whoever
receives a `MaterializedFile` owns removing `cleanup_dir` (the reaper only covers a crash).

**The cap is 256 MiB** (`PREVIEW_CAP_BYTES`), chosen to comfortably cover real preview content (documents, images, PDFs,
most media) while bounding the temp write, extraction time, and decompression amplification. It's independent of the FE
copy-selection ceiling (`COPY_REFUSE_BYTES`, 100 MB): that caps a *selection*, this caps a whole-entry materialization.
A viewer pull has no total time budget, only a stall rule, and its window shows progress; an `inspect_file` pull obeys
that tool's five-second per-path budget: § "Watching a pull" below.

`ViewerError::TooLargeToPreview` names no namespace, in the Rust `Display` string and in the frontend copy it maps to
(`viewer.error.tooLargeToPreview`, "too big to preview from here"): a `.zip` entry, a >256 MiB blob in a `.git`
snapshot, and a large file on a phone all reach this cap, so naming one of them would be a plain lie in the others.
Pinned on both sides (`materialize_test.rs`, `viewer-i18n-parity.test.ts`).

### A file on a volume the OS can't open

A phone over ADB or MTP and an SFTP or WebDAV server hand out paths no `std::fs` call can open (`adb://R58M1/sdcard/notes.txt`, `mtp://…`, `sftp://…`). `materialize_for_viewer` (for what `open_ranged_for_media` didn't take: `crates/cmdr-viewer/DETAILS.md` § "Media on a volume") and
`materialize_for_inspect` try the route first (so a `.zip` ON a phone still routes), then `resolve` the path and, when
the volume's `paths_are_os_visible()` is false, pull the file through that volume with the same `pull_to_temp` a routed
entry uses. Same cap, refused from the volume's stat before a temp exists; same `.cmdr-viewer-<uuid>/` subdir; same
cleanup ownership; same local media and text pipeline. Errors map like a portal read's: `NotFound` and `IsDirectory`
keep their twins, and anything else (a phone dropping mid-read) is a plain `ViewerError::Io`.

**Decision**: locality is `Volume::paths_are_os_visible()`, not `supports_local_fs_access()`. **Why**: the viewer core
opens paths with `std::fs`, which is the question `paths_are_os_visible` answers.

Which volumes this covers (verified in code, 2026-09-10): ADB, MTP, SFTP, and WebDAV answer `false`, so their files
materialize; before, each answered `notFound`. Direct SMB never gets here: it reads through smb2 (§ "A file read through
its volume"), and only pulls into a temp when that open fails AND the mount is gone. `inspect_file` still pulls a
mount-gone SMB file, under its own cap. A local disk and an OS-mounted share are unchanged.

**The temp is a snapshot, like a routed temp.** No watcher is spawned for it, so tail mode never extends it and
`viewer_reload` re-reads the temp, not the phone. The toolbar's tail toggle stays available and does nothing visible,
exactly as for a file inside a `.zip`. To see a newer copy, close and reopen the viewer.

### Open with on a routed file

`open_with_extract.rs` serves the file context menu's "Open with" (a listed app or "Other…") for a row only a route
serves: a file inside an archive, or a blob in a `.git` snapshot. It reuses this pull (`extract_routed_into`, the same
`pull_to_temp` behind a `TempSpot` naming its own dir and prefix) and owns everything after it. The module doc carries
the reasons; the shape:

- **A fresh copy per launch, `0o444`, under `<app_data_dir>/open-with-extract/.cmdr-open-with-<uuid>/`**, reaped by
  `init_open_with_extract_dir` at startup: a launched app has no close event, so the process boundary is the only sure
  signal a copy is done with. ❌ No refcount, dedup cache, shared dir with the viewer, or write-back. The reap runs on
  its own `open-with-reaper` thread (it asks the Trash, an XPC round trip per item, which startup's main thread must not
  wait on), so it races this session's launches; it takes only folders whose mtime is before the process started
  (`left_by_an_earlier_run`). A new pull is a fresh uuid folder and an app saving into an old one bumps its mtime, so
  neither is ever reaped. The stand-ins' folder follows the same rule.
- **The session sweep bounds the pile** (`sweep_launch_folders`, run by `launch_paths` before each launch's pulls, under
  `SESSION_SWEEP`): untouched copies past the newest four go once they're 12 hours old, or oldest first once the
  untouched copies pass 4 GiB together. Ordered by the record's `pulledAt`. A changed copy is never swept (an app may be
  editing it; startup trashes it), nor a folder with no record yet (a pull running beside this one). Decision/Why: the
  sweep doesn't ask whether a process holds the copy open. Preview and TextEdit read a file, close it, and keep showing
  it, so an open-file check (`proc_pidinfo`, `lsof`) answers "free" for a copy on screen; and removing a file an app
  does hold open is harmless, since its inode lives until the app closes it. Unchanged, past the newest few, and old or
  over budget is the conservative test.
- **A launch that refuses a row removes the copies it already made** for the rows before it (`launch_paths_in`): no app
  ever opens them.
- **A pull checks for room first** (`TempSpot::check_room`, open-with's spot only): the declared size against the
  volume's free space, purgeable space counted (`cmdr_local::space::get_space_info_for_path`, about 0.6 s of NSURL),
  refused as `ViewerError::NotEnoughSpace` before a byte is written. The viewer's spot skips it: a preview stays under
  256 MiB, and an open shouldn't pay the NSURL query. No answer goes ahead, and a full disk then stops the write.
- **A changed copy goes to the Trash, ❌ never away** (`open_with_copies.rs`). Gotcha/Why: `0o444` guards the file,
  not its folder, so an atomic save (vim `:w!`, BBEdit's safe save) replaces the copy and "Save a copy" lands beside it;
  the reaper used to remove those with the rest, content that existed nowhere else. Each pull writes a
  `.cmdr-open-with-pull.json` record (file name, size, mtime, inode) after `make_read_only` (a mode change moves ctime
  only). The inode catches a save that keeps the size and restores the mtime. At startup, a folder holding exactly that
  file and record, with all three unchanged, is removed; any other
  folder (an edit, a second file, no record) has every entry but the record moved to the Trash under its own name
  (`cmdr_operations::move_to_trash_sync`, unjournaled: no undo should list Cmdr's leftovers), logged at `warn`. An
  entry the Trash refuses keeps its folder for the next launch. The stand-ins' folder goes outright. Decision/Why: a
  record-less folder goes to the Trash too, which includes every copy a build without records left, once: nothing can
  tell such a copy untouched (the archive it came from isn't recorded), and a cluttered Trash beats a lost edit.
- **The launch says it's a copy.** After `open_paths_with` succeeds, `launch_with` emits `OpenWithCopyLaunched` (the
  first copy's name, the copy count, the app) and the main window shows `fileExplorer.openWith.readOnlyCopy` as an info
  toast: changes saved there won't reach the original.
- **Each family's reaper sees only its own prefix in its own dir**, so the viewer's reaper (`reap_orphan_temps`,
  unconditional, since no other app ever gets a viewer temp's path) can never take a copy an app has open, and the
  reverse.
- **The cap is `OPEN_WITH_CAP_BYTES` (2 GiB)**, refused from the declared size before a byte is written, like the
  viewer's 256 MiB preview cap: what people open in a real app (a video, a big PDF) runs bigger than what they preview.
- **The launch path.** `../menu/open_with.rs::launch_with` keeps an ordinary launch synchronous on the main thread. When
  any row routes, it pulls on the blocking pool and hops back to the main thread for `open_paths_with`. The volume
  under the path is `mount_id_for_path` (else the default volume), since the menu carries paths only and a route rides
  on its parent drive's volume.
- **A pull that can't finish launches nothing and says why.** `launch_paths` answers a `RefusedCopy` (the row, plus a
  typed `OpenWithCopyRefusal`: `tooLarge { cap }`, `notEnoughSpace { needed }`, `needsPassword`, `archiveUnreadable`, `unreadable`, mapped from the
  `ViewerError` variant, ❌ never its message), and an `OpenWithCopySource` (`archive` / `repoHistory`) taken from the
  resolved `RoutedKind`, which `extract_routed_into` hands back with its failure (`RoutedPullFailure`) so the too-big
  toast can name where the file sits. The archive reasons only ever come from an archive: a portal read that breaks
  off maps to `Io` (`map_volume_error`), so a snapshot lands on `unreadable`. `launch_with` logs it and emits `OpenWithCopyRefused`, which the main
  window's `../../../src/lib/file-explorer/open-with-copy-bridge.ts` shows as a warning toast: the click lands on the native
  menu, so the frontend hears of it no other way. A locked archive is `ArchiveFailureKind::NeedsPassword`
  (`map_volume_error`), because copying the file out (F5) asks for the password and fixes it. There's no "preparing"
  cue while a big pull runs: nothing on the main window fits one without a new progress surface. A failed LAUNCH
  (`open_paths_with`) is still log-only, ordinary rows included.
- **The app list asks about a stand-in.** `URLsForApplicationsToOpenURL:` answers no apps for a path with nothing at it
  (verified on macOS 27.0 via `osascript`, 2026-10-01), and the menu is built before anything is pulled. So
  `../menu/context_menu_facts.rs` asks about `listing_path`: for a routed row, an empty `stand-in.<ext>` in
  `.cmdr-open-with-types/`, which LaunchServices types by extension exactly as it would the real entry, with the same
  ranking and the same per-extension cache. ❌ Not `URLsForApplicationsToOpenContentType:`: it needs
  `UniformTypeIdentifiers.framework`, which dyld refuses on the 10.15 floor (the reasoning F4's editor list follows,
  `../file_system/DETAILS.md` § "Text editor").
- Out of scope: a file on a volume the OS can't open (a phone, SFTP). The menu context carries no volume id to pull
  through, so those still launch on the raw path and fail as before.

Covered by `open_with_extract_test.rs`.

### Watching a pull

The viewer window exists before `viewer_open` runs (the FE opens the window, and its page opens the session), so a
pull can report progress to that window and stop when it closes. `pending_open.rs` holds one open's shared state;
`commands/file_viewer.rs::open_viewer` wires it.

- **Every open is a pending open.** `begin_pending_open(window_label)` registers it for the whole command, pulling or
  not, and `end_pending_open` removes it. `close_session_for_window` (the `WindowEvent::Destroyed` net) abandons the
  window's pending open BEFORE it reads the window → session link. A plain open that times out at 2 s is abandoned
  too, so the session it builds late is closed.
- **Abandon and deliver share one lock.** `open_for_window` records the window → session link inside
  `PendingOpen::deliver`. An abandon that lands first makes the open close the session it just built and answer the
  abandon's error; an open that delivers first has recorded the link, so the ordinary close frees it. No interleaving
  strands a session.
- **The pull stops between chunks.** `stream_to_file` records bytes after each chunk (`record_pull`) and returns the
  abandon's error at the next boundary. Returning drops the stream, which stops a network producer
  (`ChannelReadStream`), and `pull_to_temp` removes the temp subdir. ❌ Never race `next_chunk` against a cancel:
  dropping an in-flight MTP window read wedges the phone. A close costs at most the chunk already on the wire: 64 KiB
  on ADB, 255 KiB on SFTP, 8 MiB on MTP. Pinned by
  `materialize_test::closing_the_window_mid_pull_stops_reading_and_leaves_no_temp`, whose counting slow volume proves
  the reads stop and the stream drops, not just that the temp goes.
- **No total deadline, a stall rule instead.** An open that may pull (`materialize::open_may_materialize`: a path
  that routes, or a volume whose `paths_are_os_visible()` is false; no I/O) waits through `util.rs`'s
  `blocking_typed_result_until_stalled`. Every 200 ms, `PullWatch` emits `ViewerPullProgress` when the byte count
  changed; once `idle_for()` reaches `PULL_STALL_LIMIT` it abandons the open as `StoppedResponding` and answers that.
  Giving up detaches the task like the deadline helpers do, and the pull still reaches its chunk boundary and cleanup.
  `idle_for` counts from the open's start, so a stat that hangs trips it too. Every other open keeps the strict 2 s.
- **Why 45 s.** Above each backend's own read timeout, so a backend that notices a dead connection answers first with
  its own error: MTP gives one USB transfer 30 s, WebDAV allows 10 s between body chunks. ADB's sync stream and SFTP's
  read window carry no read timeout (verified in code, 2026-09-10), so for them this rule is the only guard.
- **Progress is a window-targeted event** (`viewer-pull-progress`, `emit_to(label)`). The FE listens on its own
  `Window`, since a global listener would hear every viewer's pull. Pinned by `commands::file_viewer::tests` (a quiet
  pull answers `StoppedResponding` and removes its temp; progress reports only news, against the declared size) and
  `deadline::stall_tests` (given-up work runs to its end; work that delivered answers its result).
- **Analytics** see a stalled pull as `failure = stopped_responding` and a closed one as `cancelled`.

**Per-instance extract dir + startup reaper.** The dir is `<app_data_dir>/viewer-extract` (set by
`init_materialize_dir` from `lib.rs`), so side-by-side dev/prod/worktree instances never reap each other's live
temps. At startup the reaper removes any `.cmdr-viewer-*` subdir a crash left behind; the prefix guard means it can only
touch our own extraction subdirs. When uninitialized (unit tests), it falls back to an OS-temp subdir.

**Save-selection from an archive preview**: `viewer_write_range_to_file`'s SOURCE can be an archive temp — it reads via
the open session and writes with `std::fs`, so it just works off the temp. The DESTINATION, though, must not be
archive-inner (archives are read-only this phase): the command rejects it with `ViewerError::DestinationInsideArchive`,
matching the write-path guards. Covered by `commands/file_viewer.rs` tests; the extraction, cap, cleanup, and media
paths by `materialize_test.rs`, and the git half by
`file_viewer::session_test::opens_a_file_out_of_a_git_snapshot_and_reads_its_lines` plus
`tools::read::inspect::git_snapshot_tests`.

## Tauri commands

- `viewer_open(path)` → `ViewerOpenResult` (session ID, metadata, initial lines, backend type)
- `viewer_get_lines(session_id, target_type, target_value, count)` → `Result<LineChunk, ViewerError>`. `target_type` is
  the typed `SeekTargetKind` (`row` / `byte` / `fraction`), which pairs with the numeric `target_value`; a typed
  parameter is what keeps the backend from re-parsing a free-form string and needing an error arm for a case no caller
  can reach. Runs under the 2 s `VIEWER_TIMEOUT` with a cancel flag of its own (`get_lines_within` in
  `commands/file_viewer.rs`): the deadline flips it, and the backend's per-row check (`collect_rows`, and the
  `FullLoad` loop) stops the orphaned read at its next row. A fetch is bounded (`CHUNK_BUDGET_BYTES`), so this only
  matters on a slow or stuck network mount, where each orphan would otherwise hold a blocking thread to the end
- `viewer_read_range(session_id, read_id, anchor, focus)` → `Result<String, ViewerError>`: reads a logical
  `(row, offset)` range as one UTF-8 string. Endpoints are `RangeEnd::Row { row, offset }` (UTF-16 code unit offset)
  or `RangeEnd::Eof`, which ⌘A emits in ByteSeek-no-index mode (`makeSelectToEof` / `toRangeEnds` in the frontend's
  `routes/viewer/selection.svelte.ts`). `read_id` is FE-allocated so cancel can land without an
  extra round-trip. The function holds the SESSIONS lock only long enough to clone the backend `Arc` and register the
  cancel flag; the read itself iterates outside the lock so other commands stay responsive.
- `viewer_cancel_read(session_id, read_id)` → flips the per-read cancel flag. No-op if the read already finished.
- `viewer_write_range_to_file(session_id, read_id, anchor, focus, dest_path)` → streams a logical range to `dest_path`,
  decoded to UTF-8 and written atomically (temp+rename), and answers a `SavedAs`: `Utf8`, or
  `SourceEncoding { encoding }` when the selection held characters the decode lost and the save kept the source's own
  bytes (§ "A save keeps bytes the decode lost"). Used by "Save as file…" in the copy dialogs. Same cancellation
  plumbing as `viewer_read_range`. The temp comes from `cmdr_fs::staging::StagingTemp::mint`, so the pane hides it while
  it's written and a name at the filesystem's limit still gets one that fits. Peak memory is one
  `STREAM_CHUNK_BYTES` buffer whatever the selection's size, and a cancel or any error removes the partial temp. No
  total deadline: it runs under a stall watch (`SAVE_STALL_LIMIT`), so a save keeps going as long as it keeps writing.
- `viewer_search_start(session_id, query, mode)` → starts background search. `mode = { useRegex, caseSensitive }`. An
  invalid regex pattern (parse error, exceeds size limits) or a multiline pattern (`(?s)`, literal newline, `\n`
  escape) makes the search status flip to `InvalidQuery { message }` synchronously; the worker isn't spawned. `(?m)`
  is fine because it only affects `^` / `$` within a line slice.
- `viewer_search_poll(session_id)` → `SearchPollResult` (matches, progress, status). `status` is a tagged union
  `{ status: "running" | "done" | "cancelled" | "idle" | "invalidQuery", message?: string }`.
- `viewer_search_cancel(session_id)` → cancels running search
- `viewer_close(session_id)` → frees resources (also signals every in-flight read to cancel)
- `viewer_setup_menu(label)`: builds viewer menu with word wrap item
- `viewer_set_word_wrap(label, checked)`: syncs menu state
- `viewer_forward_edit_action_to_page(action)` (in `commands/menu.rs`): sends Edit > Copy or Select all to the first responder as the native
  `copy:` / `selectAll:`, for the PDF view, whose selection only the responder chain reaches. macOS only;
  `apps/desktop/src/routes/viewer/DETAILS.md` § "Media rendering (image / PDF)"
- `viewer_set_search_input_focused(label, focused)`: whether this viewer's search box holds keyboard focus, which is
  what greys the viewer bar's Edit > Cut / Paste (they act on that box and nothing else). macOS only;
  `../menu/DETAILS.md` § "Dialog refusals, and the one writer of an item's enabled state"
- `viewer_get_encoding_options(session_id)` → `EncodingOptions`: current selection, detected encoding, and the full list
  of selectable encodings with their labels and groups. The FE renders the dropdown straight from this; no encoding list
  lives on the FE.
- `viewer_set_encoding(session_id, encoding)`: switches the active encoding. Instant when `same_byte_layout(current,
  new)` holds (UTF-8 ↔ Windows-1252 family): the active backend's `with_encoding(new)` method returns a fresh backend
  with only the encoding field swapped, no reindex. Otherwise snaps to ByteSeek immediately and rebuilds LineIndex in
  the background. The FE polls `viewer_get_status` for `is_indexing` to track the rebuild.
- `viewer_set_tail_mode(session_id, enabled)`: flips the per-session tail-mode flag. When true, watcher `Grew` events
  trigger an `extend_to` on the active backend so the open viewport auto-follows newly appended bytes. When false, the
  FE still hears `viewer:file-changed:<sid>` events and renders a persistent reload toast. Enabling also catches the
  backend up to the current on-disk size in one step.
- `viewer_reload(session_id)`: reopens the active backend against the file on disk under the session's current
  encoding. Called by the FE's reload toast and on rotation (`Shrunk` / `Replaced`).

**`viewer_set_encoding`, `viewer_set_tail_mode`, and `viewer_reload` are `async` + `spawn_blocking` + 2 s timeout**
(via `blocking_viewer_op` in `commands/file_viewer.rs`), not synchronous. They each touch the filesystem — a reopen, an
encoding rebuild snap, or the tail catch-up `extend_to` scan — and a synchronous call would block the viewer window's
IPC handler thread, freezing concurrent scroll / search IPC behind it (the platform-constraints rule in
`docs/architecture.md`). Don't convert them back to plain `fn`. The watcher manager thread calls `reload` /
`apply_tail_extend` directly (off the IPC thread already), so those paths stay synchronous.

