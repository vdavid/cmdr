# Archive edits: details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. The must-knows are in
`CLAUDE.md`; the surrounding managed-op machinery (lanes, admission, conflicts, the settle contract) is
`../DETAILS.md`, and the mutation mechanism itself is `crates/cmdr-archive/src/mutation/DETAILS.md`.

Editing a `.zip` (mkdir/mkfile/rename/delete inside, or copy/move INTO one) is an O(archive) temp+rename rewrite, not a
metadata syscall, so it runs as a managed op through `spawn_managed`, NOT `run_instant`. The `archive_edit/` module is the driver;
the mutation mechanism (`ArchiveMutator`, temp+rename safe-overwrite) lives in the archive backend
(`crates/cmdr-archive/src/mutation/DETAILS.md`).

## Reaching the edit driver: parent-aware write-routing

A write only reaches this driver if the routing seam DETECTS its target as archive-inner. That detection MUST be
parent-aware, not `std::fs`-only: the sync `archive::path_is_inside_archive` / `path_crosses_archive_boundary`
predicates confirm a `.zip` via `std::fs::metadata` + a local magic read, which silently returns FALSE for an
`smb://` / `mtp://` path — so a write inside a remote zip would fall through to a plain parent-volume write and error
confusingly (data-safe, but wrong). So the routing seams call the async `VolumeManager::path_is_inside_archive`
(delete `../delete/mod.rs`, rename `../rename.rs`, copy-out / move-out source `commands/file_system/volume_copy.rs::resolve_source`
and the scan-preview source) and `path_crosses_archive_boundary` (create `../create.rs`), which confirm through the
parent's OWN `get_metadata` + four-byte `read_range` for a remote parent (mirroring `VolumeManager::resolve`) and keep
the zero-network `std::fs` fast path for a local one. Copy/move INTO already routed correctly (the dest goes through
the async `resolve` → `dest_resolved.is_archive`). The `route_*` functions then re-split the confirmed path with the
pure-string `archive_boundary_candidate` (NOT `confirm_archive_boundary`, whose `std::fs` confirm would wrongly fail
for a remote zip) — confirmation already happened at the seam. Pinned by the `path_is_inside_archive_*` unit tests in
`../../volume/manager.rs` (local + remote + `read_range`-unsupported + mislabeled).

## Local vs remote: one closure, one dispatcher (`run_managed_edit`)

Every apply site in `archive_edit/` runs its plan+apply through `engine::run_managed_edit(parent_volume_id, archive_path,
state, plan_and_apply)` rather than a bare `spawn_blocking(mutator::apply(...))`. The closure is the SAME blocking
plan+apply either way — it plans against, and mutates, the path it's HANDED. The dispatcher (keyed on
`parent.supports_local_fs_access()`) decides what that path is:

- **Local parent**: byte-identical to before — the closure runs on the REAL archive file, and the mutator's own
  temp+rename commits the edit. No pull, no upload.
- **Remote parent** (direct SMB / MTP): routed through `remote::pull_apply_upload_swap`.

**One failure type for both paths.** Every stage, local planning and the remote pull, upload, and swap alike, returns
`edit_error::EditError` (`Cancelled` or `Op(WriteOperationError)`), so the driver's terminal-event handling can't tell a
local edit from a remote one. It sits in its own leaf because `engine` dispatches INTO `remote` and both name it:
defining it in either one welds the two into a module cycle, which is exactly what a twin-enum-plus-`From` pair did
before the two became one type.

Because the local mutator's `raw_copy_file` needs a `Read + Seek` source (which async ranged reads can't give), a remote
edit does NOT edit in place — it PULLS the `.zip` to a local temp, runs the ordinary local closure there, uploads the
rewritten temp under a remote temp name, then swaps. This means a remote edit needs only streaming read + write + rename
+ delete on the parent; it does NOT depend on the SMB positioned-read (`read_range`) primitive that BROWSING needs (the
CD is parsed from the pulled-local copy, not over ranged reads).

## Remote edit: the data-safety contract (`remote.rs`)

The remote ORIGINAL is byte-for-byte untouched until the very last swap:

1. **Pull** streams the remote `.zip` to a local scratch copy (`open_read_stream`, cancel-checked between chunks,
   `fsync`ed). Writes nothing remote.
2. **Apply** runs the closure on the local copy — the mutator's temp+rename commits onto the scratch file. A cancel/fault
   leaves the scratch file as the pulled original; nothing remote changed.
3. **Upload** streams the edited copy to a NEW remote name (`foo.zip.cmdr-tmp-<uuid>`) via `write_from_stream`; the
   original keeps its name and bytes. A cancel/fault deletes the partial temp best-effort. Every edit route hands
   `run_managed_edit` its `MutatorHooks::remote_progress_observer`, so the upload reports its own `Transferring` axis
   over the rewritten archive's bytes, then indeterminate `FinishingTransfer` through close and swap, under the op's
   own type (the pull stays silent).
4. **Swap** is the ONLY step that changes the original. Where the backend REJECTS a same-name collision
   (`create_directory_errors_on_existing_dir()` true — SMB, local), existing-archive mutation first asks its force-rename
   operation to replace the name; a backend may implement that as multiple protocol operations. On refusal it falls back
   to delete-then-rename. A backend that ALLOWS same-name siblings (MTP, flag false) goes STRAIGHT to
   delete-then-rename — a rename onto the live name would DUPLICATE, not replace. The
   delete-then-rename path has exactly ONE crash window (between the delete and the rename): the NEW, fully-uploaded data
   survives under the temp name — never lost, only briefly misnamed.

A cancel at ANY point before the swap completes leaves the remote original intact (the local scratch dir and any partial
remote temp are cleaned up — a RAII `ScratchDir` and the upload's on-error delete). Pinned by `remote_tests`
(round-trip, cancel-before-swap-leaves-the-original, and the sibling-allowing delete-then-rename swap), plus live-remote
integration proofs that drive `pull_apply_upload_swap` against a REAL backend: the backend-blind scenarios in
`network_archive_test_support.rs` (edit, cancel before the swap, routing detection, extract-out, copy-into, compress),
which `smb_archive_integration_test.rs`, `sftp_archive_integration_test.rs`, and `webdav_archive_integration_test.rs` drive, and `mtp_archive_test` under the
`virtual-mtp` feature (`virtual_mtp_archive_browses_and_extracts_via_read_range` +
`virtual_mtp_remote_zip_edit_deletes_an_entry_through_the_device`, exercising the MTP delete-then-rename swap). Cost: O(archive)
network per edit (the pull), documented and accepted — there is no remote random-access WRITE adapter (that's only a
future in-place-append optimization). Remote backends don't carry the archive file's mode/mtime/xattr across the rewrite
the way local `copyfile` does; the upload mints a fresh remote object.

**Stale upload-temp reaping.** A crash or kill in the swap's ONE window (between the upload finishing and the swap
committing) can leave the fully-uploaded temp on the remote under its `<archive>.cmdr-tmp-<uuid>` name. It's harmless
(the original is intact and the temp holds the NEW bytes), but untidy. `pull_apply_upload_swap` reaps it at the start of
the next edit of the SAME remote archive — the mirror of the local mutator's `reap_sibling_temps` — via a single
`list_directory` of the archive's parent, deleting siblings that match this archive's own temp shape. Best-effort and
non-blocking (a listing/delete failure is logged at debug, never fails or delays the edit); one round-trip, nothing on
the read path. Pinned by the four `remote_edit_*` reap tests in `remote_tests` (stale-same-archive reaped,
fresh spared, other-archive ignored, delete-failure doesn't fail the edit).

- **Decision — age-gate the remote reap at 24 h (`REMOTE_TEMP_REAP_MIN_AGE`); the local reap has no threshold.** The
  local reap deletes every matching sibling unconditionally because edits of one archive serialize on the parent lane, so
  a local leftover is ALWAYS an abandoned build. A remote share is multi-machine: a `<archive>.cmdr-tmp-*` sibling with
  this exact shape may be a LIVE upload from ANOTHER Cmdr instance mid-flight, so the remote reap deletes only leftovers
  whose reported mtime is older than 24 h (an entry with no mtime is treated as fresh and spared). Why 24 h: it must
  comfortably exceed the longest plausible single-archive upload (tens of GB over a slow link still finishes in well under
  a day) PLUS clock skew between this machine and the remote's mtime clock (SMB reports server mtime, MTP the device's;
  the dangerous direction is a server clock BEHIND local, which inflates the computed age). The leftover is harmless while
  it waits and gets cleaned lazily at a later edit, so erring long costs almost nothing; erring short risks deleting a
  legitimate in-flight upload. Consequence, accepted: a crash-then-immediate-retry of the same archive leaves the leftover
  in place until an edit more than 24 h after the crash — mtime alone can't tell "my own crash seconds ago" from "another
  instance uploading now."

## The driver, op by op

- **Driver shape.** `archive_edit_start(events, request, interval)` mirrors the volume-delete branch: a deferred async
  start owns the op end to end (a `WriteSettledGuard`, the `ArchiveMutator` run on the blocking pool, the terminal
  event, `on_settled`). The op takes the PARENT drive's lane (archive work shares the device's serialization lane) and
  marks the parent drive busy (eject guard). A `MutatorHooks` bridge wires the mutator's control seam to the live op:
  cancel from `OperationIntent`, pause from the `PauseGate` (a sync park on the blocking thread), throttled
  `write-progress` (two-axis: entries + bytes), and the downloads-watcher ignore registration for the temp AND final
  paths (before each syscall, via the mutator's `note_pending` hook). `Cancelled` emits `write-cancelled`, never
  `write-error`; other mutator faults map to typed `WriteOperationError`. **The terminal `files_processed` is
  `MutationProgress::entries_changed`** (entries the edit adds / deletes / renames), NOT `entries_total` (the
  retained-rewrite count) — deleting one file from a 3-entry zip reports 1, not 2.
- **Every `write-progress` is mirrored into the status cache.** `MutatorHooks::emit` is the one emit site (scan ticks,
  mutator ticks, the remote upload axis), and it pairs the event with `update_operation_status`, as the transfer
  driver's `emit_progress_and_status` does. The cache is all a query API reads (the MCP `cmdr://state` resource): an
  op that only emitted showed there as `scanning` with no bytes from start to finish, paused or not. ❌ Don't call `emit_progress_via_sink` directly from an archive route. Pinned by
  `every_emitted_phase_of_a_compress_reaches_the_status_cache`.
- **E2E pacing.** Under `set_test_throttle` / `CMDR_E2E_COPY_THROTTLE_MS`, `MutatorHooks::on_progress` sleeps once per
  finished entry for the copy throttle's value, in 10 ms slices that return the moment the op is cancelled. It sleeps
  only while an entry remains: the mutator checks cancel before every entry, so a click inside the sleep always stops
  the rewrite, while a sleep after the LAST entry would be dead time, since the commit follows with no check left.
  `archive-editing.spec.ts`'s cancel-paste test depends on it: unpaced, a local paste into a small zip lands before the
  progress dialog can offer an enabled Cancel. Production reads one atomic and never sleeps.
- **Routing seams.** The former archive rejections become routing: `create_directory_managed` / `create_file_managed`
  (a `.zip`-crossing parent), `rename_managed` (an in-archive path), `delete_files_start` (in-archive sources), and the
  `copy`/`move_between_volumes` COMMANDS (an archive-resolved destination). The instant-op forks reach a `TauriEventSink`
  via the manager's startup-wired app handle (`operations_app_handle`), so no command signature changes; a
  `create`/`rename` return is the operation id, not a path (the FE reads it as an op handle).
- **Changeset per op.** mkdir → `{ mkdir }`; mkfile → `{ add }` (empty bytes); rename inside → `{ rename }`; delete
  inside → `{ delete }` (batched across a multi-select in one zip); copy/move INTO → one `{ add + mkdir }` for the whole
  transfer (`route_archive_copy_into` walks the LOCAL sources with `walkdir`). A move INTO deletes the top-level sources
  after the commit, and only when nothing was skipped (the move invariant — never delete a source whose bytes didn't
  land). The removal takes a LEDGER, ❌ never the tree: before anything reads them, `materialize_sources` stamps each
  original through its volume (`transfer::volume::stamp_source`, local and remote alike), and after the commit
  `sweep_carried_source` removes only what's still in the ledger AND still matches its stamp. A file saved over or added
  in the source during the pull or the rewrite exists only there, so it stays, and the completion event counts it on
  `appearedDuringMove` (the toast's "changed / appeared during the move" sentences). The mechanism and its gaps:
  `../transfer/volume/DETAILS.md` § "Cross-volume move source-delete removes a LEDGER". Pinned by
  `copy_into_drift_tests.rs` (local and remote source).
- **Fresh compress is its own managed driver** (`compress.rs` → `fresh_compress.rs`), not an archive mutation. It
  reserves source and destination lanes and registers `WriteOperationType::Compress` before any write. Journal and
  analytics retain `OpKind::ArchiveEdit`, `ArchiveSubkind::Compress`, and `archive_edit_completed`; `net_new` retains
  rollback eligibility and overwrite semantics. Existing-archive copy/move/create/delete continues through
  `ArchiveMutator` unchanged.

  Preflight freezes entry names, kinds, sizes, mtimes, and Unix modes; skips symlinks and special files; and rejects
  local canonical/inode aliases plus destinations inside a source. The walk stats only the SELECTED items: every child's
  facts come from its parent's listing, the `scan_walk` shape, because a stat per child cost one network round trip per
  file on SMB and SFTP and a whole parent listing per file on MTP. A symlink is skipped wherever it sits, selected or
  nested, and never followed: a nested one reads off its listing entry's `is_symlink`, and a selected one off
  `Volume::entry_kind` (an `lstat`) before its stat, because SFTP's `get_metadata` follows the link and would pack its
  target. That kind check is the one extra round trip per SELECTED item. Special files
  (fifo, socket, device) off the file-type bits of `permissions`: local and ADB listings carry the full `st_mode`,
  SFTP carries the type bits alone (its permission bits stay unset, so copies keep carrying no SFTP mode), and SMB and
  MTP report none, which never reads as special. Opening a FIFO blocks, and on SFTP it blocks the one `sftp-server`
  every operation on the volume shares. The walk reports itself as the indeterminate
  `Scanning` phase (files, directories, and bytes found so far, plus the directory being listed), throttled to the
  progress interval, with one final unthrottled tally before `Compressing` starts. On one remote volume it applies the same
  lexical containment rule; unrelated remote volume objects are not rejected merely because they expose no inode API.
  One matching remote spelling is replaced under its stored name, several matches are ambiguous and refused, and a
  new name uses the backend's preferred spelling.

  Two selected sources with the same top-level name are a collision inside the fresh plan, not an unconditional
  preflight refusal. Skip retains the first, Rename numbers the later source, Overwrite replaces the first plan, and
  the conditional policies compare the two source metadata records. Stop uses the registered operation's conflict
  slot, announces the human wait, and applies the answer (including apply-to-all) before any destination write.

  **One producer, two destination routes.** Local POSIX, SMB, SFTP, and ADB accept unknown-length writes, so the
  producer streams directly to a tracked same-directory stage with `CreateNew`. WebDAV and MTP receive the same
  producer into a private local spool, then a known-length staged upload. This sequencing makes same-device MTP
  source→destination finish the read/spool leg before any upload begins, avoiding its serialized-session reentrancy.
  No direct route materializes source trees or an archive-sized local file. Same-server SFTP and SMB compress reads and
  writes concurrently over the one volume's connection, which both multiplex (SFTP on one channel, SMB on one session);
  pinned live by `a_compress_of_server_files_onto_the_same_server_lands_a_valid_zip` in both Docker suites.

  **A dev build compresses about nine times slower than a release one, and that is the whole gap.** Deflate runs at
  opt-level 0 under `pnpm dev` (`[profile.dev]` optimizes no dependency, `zlib-rs` included): 319 MB of mixed data
  (random, text, binary) took 34.4 s through the bare `zip` writer in dev (9.3 MB/s of source) against 4.1 s in release
  (78.5 MB/s). In the dev app the same data took 36.7 s onto the local disk and 35.6 s onto a phone over ADB, so the
  destination isn't the limit there; `adb push` of the finished 154 MB ZIP ran at 35 MB/s. ❌ Don't read a dev-build
  compress rate as the product's. (Measured on an M3 MacBook Pro, `zip` 8.6.0 + `zlib-rs` 0.6.5, level 6, Pixel 9 Pro
  XL over USB, 2026-09-30.) In a release build the phone's link is the likelier limit on incompressible data.

  `fresh_zip.rs` drives `zip` 8.6 `ZipWriter::new_stream` on one OS worker. Local files are read directly; one remote
  feeder is live at a time. Remote input and generated output cross separate four-chunk Tokio channels (the worker
  parks in `blocking_recv` / `blocking_send`), split into 128 KiB payloads, so queued bytes are bounded independently of
  archive size. The ZIP writer necessarily retains O(entries) central-directory metadata. Each remote source sends
  explicit completion, and the output sends an explicit `End` only after the central directory is flushed: feeder
  loss, source error, and an output queue that closes without `End` are typed failures, never EOF, so a destination
  never closes a failed ZIP as a finished file. A source that GREW since planning (a live log) fails as `CountMismatch` one
  byte past its planned size, never at EOF: local reads run through `take(size + 1)` and remote chunks are counted as
  they arrive, so the producer never compresses bytes the plan didn't promise or waits for an end that may not come. Coordinator shutdown closes the output endpoint before joining on
  Tokio's blocking pool; `Drop` only signals cancellation (and only for an unfinished pipeline) and never blocks an
  async thread. Thread-spawn failure is typed too.

  **One cancellation source.** `FreshZipCancellation` is a child of the op's tier-1 `backend_cancel`, so a user's
  Cancel and every internal stop (the destination dropping its stream, a producer failure, coordinator shutdown) land
  on one token, and every participant watches it. The producer checks it per chunk and while paused. The feed task
  races each source read against it; on cancel it returns and drops every feeder, and that channel close is what wakes
  a producer parked in `blocking_recv` on a source that stopped answering. The output stream answers the next pull
  with `Cancelled`, and the destination's write callback returns `Break` at its next acknowledgement, which is the only
  way SFTP, SMB, and ADB hear a cancel mid-write (they then remove their own partial). Two waits are deliberately
  left to finish: opening a source stream (one protocol round trip, as in the copy engine) and a backend write already
  in flight. Tier 2 (`backend_abort`) isn't raced here. Pinned by the `cancel_reaches_*` tests in
  `fresh_compress_tests.rs` (a hung remote source and a destination holding a write in flight).

  The producer emits empty directories/files, clamps deflate level to 1–9, and carries Unix modes and each source's
  mtime (a source with none is dated now) through `cmdr_archive::mutator::with_entry_mtime`: local DOS time plus the
  exact UTC second, the convention `crates/cmdr-archive/src/mutation/DETAILS.md` owns. It sets `large_file(true)` when zlib's conservative deflate bound for the planned size
  (`n + n/8 + n/64 + 5`) reaches `zip::ZIP64_BYTES_THR`: `zip` refuses a non-ZIP64 data descriptor once the COMPRESSED
  size passes 4 GiB, and incompressible input really grows by up to an eighth at level 1, so the input size alone
  failed near-4 GiB videos. Entries from about 3.76 GiB up pay a 20-byte ZIP64 extra. Stream local headers cannot be
  rewritten; only `finish` writes the central directory and returns the writer (verified against installed zip 8.6 and
  zlib-rs 0.6.5 source, 2026-09-29).

  **Validation precedes publication.** After producer close and destination close/sync, producer bytes, writer bytes,
  and staged stat size must agree. `ArchiveVolume` then parses the staged ZIP, which must read back as exactly the
  planned tree (`ExpectedIndex`): each planned name run through the reader's own `sanitize_entry_name`, plus the
  ancestor directories the reader synthesizes, with nothing quarantined. A raw count comparison misfired because the
  reader treats `\` as a separator, so a macOS file named `a\b.txt` reads back as `a/` + `b.txt`. A name the reader
  would quarantine (a macOS `..\notes.txt` reads as `../notes.txt`, past 256 components, or nothing at all) is refused
  from the PLAN instead, by `fresh_validate::check_entry_names` before the producer spawns, as a typed
  `ArchiveEntryNameRefused` naming the entry: otherwise the whole compress and upload ran only for the stage check to
  reject it without saying which file. The same pass refuses two entries that read back as ONE path, a file `a\b.txt`
  beside a real `a/b.txt` or a file `a` beside `a\b.txt` (which needs `a` as a folder), as `ArchiveEntryNamesCollide`:
  the stage's set comparison can't see it, because one entry shadows the other and both read back as one node. Two
  folders at one path merge and pass. The stage check stays as the backstop. SFTP validates through positioned reads; ADB uses bounded, safely quoted device-side Toybox `dd`
  windows, so neither downloads the staged archive. Cancellation or any mismatch abandons only the owned stage. Local
  POSIX publishes with its declared atomic replace rename. SMB's force rename deletes first, so despite direct generation it uses the existing tracked
  `DisplacedDestination`: set the original aside, land without force, restore on refusal, and surface
  `OriginalsKeptAside` if the shared rescue had to keep it under a stable ` (recovered)` name. Other fallback backends
  use the same aside path. Stage and aside recovery records carry the real destination volume ID and remain live until
  landing, deletion, or rescue settles. No sole good copy remains in reapable temp space. After publication, the
  driver notifies the final archive path (never the hidden stage) and journals that same target.

  Progress has two unrelated byte axes. `Compressing` counts uncompressed source bytes/entries. The last source tick
  becomes indeterminate `FinishingCompression` before ZIP close, count reconciliation, validation, and local
  publication. Fallback upload starts a fresh `Transferring` axis over completed-ZIP bytes, switches to indeterminate
  `FinishingTransfer` instead of emitting 100%, and stays there through backend close, remote validation, and
  publication. Direct remote generation moves from `Compressing` to indeterminate `FinishingCompression` for the same
  close, validation, and publication work without inventing a transfer axis. The spool route is two steps a person can
  see (zip here, then upload), so `MutatorHooks::number_steps_as_zip_then_upload` stamps its compress phases
  `step: 1 of 2` and its transfer phases `2 of 2` on `WriteProgressEvent::step`; the frontend only words it ("Step 1 of
  2: Compressing"). A direct stream is one step and carries no `step`. Pause parks source production or spool
  reads at chunk boundaries. Cancel reaches every participant through the one cancellation source above, removes the
  owned stage, and never publishes. Pinned by `fresh_zip` tests plus local/remote compress tests for backpressure, late source failure,
  publication refusal/recovery, aliases, old-target preservation, remote sources, and same-device MTP fallback.

- **The writability guard precedes registration and every write.** `compress_start` first calls
  `ensure_zip_writable`, so document containers and read-only archive formats are refused with their bytes unchanged.
- **Compression level has two owners with one setting.** Fresh creation passes
  `VolumeCopyConfig::compression_level` directly to `new_stream`; existing-archive additions carry it on the
  `Changeset`. `None` means level six, values clamp to 1–9, and internal diagnostic ZIPs keep their own fixed level.
- **Source-side pull for a REMOTE source (SMB / MTP → zip).** A copy/move INTO a zip whose SOURCE volume has no
  `local_path()` can't be walked with `std::fs`, so `archive_copy_into_start` runs a pull stage FIRST, inside the op: it
  streams each source subtree into a `ScratchDir` via the copy engine's `pull_path_to_local` seam (which reuses
  `copy_single_path` — nested-tree recursion, chunked streaming, cancel, pause), then the ordinary changeset walk + apply
  runs against the pulled bytes. This is ORTHOGONAL to the archive PARENT's local-vs-remote handling (`run_managed_edit`),
  so all four source×parent combinations work. The pull is SILENT (no progress events); the rewrite stage drives the
  progress bar, matching the remote-PARENT flow. The metadata size is never trusted — the pull streams the real bytes, so
  a source whose listed size lies still lands correct content. A cancel or fault during the pull returns before
  `run_managed_edit` opens the archive, so the zip stays byte-for-byte intact; the `ScratchDir` (shared with the
  remote-edit flow, `../scratch_dir.rs`) is cleaned on every exit. Pinned by the remote-source `copy_into_tests`.
- **Duplicate pre-check for create / rename** (`archive_inner_exists`). `route_archive_create` and
  `route_archive_rename` reject a name that already exists inside the zip UP FRONT with the same friendly "already
  exists" message the real-FS mkdir/rename paths use, so the FE shows the standard copy — the mutator otherwise only
  rejects a duplicate at write time (`zip`'s `Duplicate filename`), after building a temp. It dispatches on the parent
  like `run_managed_edit`: a LOCAL (or unregistered) parent parses the central directory straight off the real file
  (off-executor), a REMOTE parent reads it through the parent volume (a ranged tail read via `resolve`, not a full pull).
  A parse failure resolves to "not a duplicate" so the managed op still surfaces the real fault. Copy/move-INTO conflicts
  are handled by the policy layer below, not this pre-check.
- **Unrepresentable source entries are skipped, never lost (data safety).** A zip changeset can only carry real files
  and directories. When `route_archive_copy_into` walks the sources, any entry that's a symlink or special file
  (fifo/socket/device — including a broken symlink, since `symlink_metadata` classifies it as neither file nor dir) is
  counted as skipped rather than added. On a MOVE, any skip suppresses the source deletion (all-or-nothing — the whole
  transfer degrades to a copy, so a symlink is never removed from the source while absent from the archive). The skip
  count rides in `ArchiveEditRequest.skipped_count` and surfaces as `files_skipped` on the terminal event.
- **Both managed routes end through `engine.rs::emit_archive_terminal`**, which owns the one three-arm match from
  outcome to `write-complete` / `write-cancelled` / `write-error`. Each route keeps only what it must do BEFORE that
  emit (a move's source delete and the journal row, both in `copy_into.rs`); the match itself was a 26-line clone
  across the two, free to drift. An archive edit reports `top_level_skipped: None`: its `skipped_count` is
  unrepresentable entries and in-zip clashes, not items a user picked in a pane, so the FE words the summary from
  `files_skipped` alone.
- **Move OUT of a zip is a compound op** (`route_archive_move_out`), NOT a per-file `Volume::delete` (the `ArchiveVolume`
  is read-only). One managed Move op runs two phases on ONE lifecycle: (1) extract the selected entries to the
  destination through the ordinary cross-volume copy engine (`copy_volumes_with_progress`, wrapped in a
  `SuppressTerminalsSink` that withholds the copy's terminal event so the compound op emits the single Move terminal,
  reads `files_skipped`, and collects the fully-extracted sources via `note_source_landed_clean`); (2) a batch
  `{ delete }` archive rewrite via the mutator. **MOVE INVARIANT**: an entry is deleted ONLY after its destination copy
  is durably committed (the copy engine fsyncs each file) AND won't be rolled back, so a crash or cancel never loses both
  copies. **Partial-move policy: per-source convergence.** The batch drops exactly the top-level sources that extracted
  with ZERO deep skips: a source with a skipped child stays in the archive (deleting its subtree would drop the un-landed
  child — the partial-merge-skip hazard); a HARD error deletes the durable PREFIX so a retry moves only the remainder;
  CANCEL and ROLLBACK delete nothing (cancel matches the plain cross-volume move, whose source-delete never runs on
  cancel; rollback removes the dest copies, so nothing durable remains). The delete stays ONE atomic O(archive) rewrite
  over the converged subset (a dir source deletes by prefix), never n per-entry rewrites. **The deep-skip count is
  load-bearing**: a merge child resolved to Skip is invisible to the driver's top-level accounting, so the copy engine
  folds each source's `CreatedPaths::skipped_file_count` into `files_skipped`; without that fold a directory source with
  a deep skip would report zero skips and the delete would drop its whole subtree (data loss). Progress is two honest
  phases (extract bytes, then rewrite bytes). Pinned by the `move_out_*` tests (incl. the deep-skipped-child,
  partial-converge, durable-prefix-on-error, and rollback pins).
- **Conflicts.** An add whose inner path already exists is resolved against the archive index. BOTH the pre-resolved
  policies and Stop PLAN inside the managed op (`archive_copy_into_start`), against the working copy `run_managed_edit`
  hands the closure — the real archive for a LOCAL parent, the pulled-local copy for a REMOTE one. Planning up front
  against the archive path would break a REMOTE edit (`LocalFileSource::open` on a direct-SMB / MTP path fails, or opens
  the OS mount the design routes around); planning inside the op is what keeps a remote plan on the pulled bytes. A
  pre-resolved policy resolves each collision non-interactively (`build_copy_into_changeset`): Skip drops the add;
  Overwrite deletes the existing entry then adds (a clean replace); Rename picks a unique ` (n)` name;
  OverwriteSmaller/Older compare size/mtime (strict). **Crossing types takes a person's consent**: an incoming FILE
  landing on an archive DIRECTORY of that name reduces to Skip under the pre-resolved policy and under a same-kind
  latched "* all", so neither can delete the directory and everything under it. What does replace is a plain Overwrite
  answered on the prompt for that shape, and the "* all" it latched into the file-over-folder bucket — the person saw
  both kinds named. `conflicts.rs::resolve_effective` routes every arm through the shared
  `../conflict.rs::resolution_for_clash`, which the local-FS and cross-volume engines answer with too; the conditional
  variants stay refused whoever asked, having no honest question to ask here anyway (a directory node carries no size
  and no mtime). The mirror direction never consults
  the policy at all: a source DIRECTORY meeting a same-named FILE entry just skips its `mkdir` and adds its children
  under the name. Pinned by
  `copy_into_tests.rs::a_blanket_overwrite_never_replaces_an_archive_directory_with_a_file`. **The Stop policy prompts
  interactively**
  (`build_copy_into_changeset_interactive`): the op is registered so `resolve_write_conflict(op_id)` can reach the
  oneshot, and each FILE collision emits a `write-conflict` and blocks on the answer, reusing the pure `ApplyToAll` latch
  + the oneshot plumbing (store the sender BEFORE the emit). Dir-vs-dir collisions merge silently — only files prompt
  (the app-wide rule). A cancel during a pending prompt drops the sender → the planner bails → the archive is untouched.
  Every Skip (a conflict resolved to
  Skip, a conditional policy that declines to overwrite, or an unrepresentable entry) increments the plan's
  `skipped_count`, which gates the move-source deletion and surfaces as `files_skipped` on the terminal event. Pinned by
  the `interactive_*` tests.
- **Duplicating INSIDE a zip is deliberately out of scope.** The rule that turns a same-folder copy into a duplicate
  (`../transfer/DETAILS.md` § "Self-collision (duplicating in place)") governs the two transfer engines; this is a third,
  independent pipeline with its own conflict layer and no same-location guard to remove, so a source pasted into its own
  folder inside a zip behaves tolerably by accident: `Rename` numbers it, `Stop` asks a question about a file that is
  its own clash. Making that question go away here is its own effort. Related: `conflicts.rs::find_unique_inner` is a
  THIRD ` (N)` numbering implementation, kept separate on purpose (it numbers slash-joined inner-path strings against an
  `ArchiveIndex` plus a planned set, and doesn't continue a trailing sequence); its own doc comment says what to reach
  for if archive numbering ever has to match the filesystem's.
- **Mutation-test coverage (`cargo mutants` on `archive_edit/`).** Every conflict-resolution and routing/data-path
  mutant is killed (Rename numbering incl. dotfiles, OverwriteSmaller/Older strict `<` incl. the equal-size/mtime
  boundary, move-source deletion gating, per-source move-out convergence (deep-skip count, durable-prefix delete), dir-merge mkdir guard, settle payloads). The only
  deliberately-unkilled survivors are in `MutatorHooks` — progress-emit THROTTLING, pause parking, and the
  cancel-during-rewrite bridge. These are UX/timing, data-safe by construction (the mutator's own cancel-abandons-temp
  and progress semantics are pinned in `crates/cmdr-archive/src/mutation/mutator_test.rs`), and killing them would need flaky
  timing-based tests — not worth it per the mutation-score guidance.


A single Copy can supply an explicit top-level destination leaf to the copy-into changeset. The shared contract and
validation live in `../transfer/DETAILS.md` § "Named destinations"; the change applies before archive conflict resolution.
