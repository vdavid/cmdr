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
- **Remote parent** (direct SMB / MTP): routed through `archive_remote_edit::pull_apply_upload_swap`.

Because the local mutator's `raw_copy_file` needs a `Read + Seek` source (which async ranged reads can't give), a remote
edit does NOT edit in place — it PULLS the `.zip` to a local temp, runs the ordinary local closure there, uploads the
rewritten temp under a remote temp name, then swaps. This means a remote edit needs only streaming read + write + rename
+ delete on the parent; it does NOT depend on the SMB positioned-read (`read_range`) primitive that BROWSING needs (the
CD is parsed from the pulled-local copy, not over ranged reads).

## Remote edit: the data-safety contract (`../archive_remote_edit.rs`)

The remote ORIGINAL is byte-for-byte untouched until the very last swap:

1. **Pull** streams the remote `.zip` to a local scratch copy (`open_read_stream`, cancel-checked between chunks,
   `fsync`ed). Writes nothing remote.
2. **Apply** runs the closure on the local copy — the mutator's temp+rename commits onto the scratch file. A cancel/fault
   leaves the scratch file as the pulled original; nothing remote changed.
3. **Upload** streams the edited copy to a NEW remote name (`foo.zip.cmdr-tmp-<uuid>`) via `write_from_stream`; the
   original keeps its name and bytes. A cancel/fault deletes the partial temp best-effort.
4. **Swap** is the ONLY step that changes the original. Where the backend REJECTS a same-name collision
   (`create_directory_errors_on_existing_dir()` true — SMB, local), it tries an atomic rename-overwrite first (SMB with
   `ReplaceIfExists`); on refusal it falls back to delete-then-rename. A backend that ALLOWS same-name siblings (MTP,
   flag false) goes STRAIGHT to delete-then-rename — a rename onto the live name would DUPLICATE, not replace. The
   delete-then-rename path has exactly ONE crash window (between the delete and the rename): the NEW, fully-uploaded data
   survives under the temp name — never lost, only briefly misnamed.

A cancel at ANY point before the swap completes leaves the remote original intact (the local scratch dir and any partial
remote temp are cleaned up — a RAII `ScratchDir` and the upload's on-error delete). Pinned by `archive_remote_edit_tests`
(round-trip, cancel-before-swap-leaves-the-original, and the sibling-allowing delete-then-rename swap), plus live-remote
integration proofs that drive `pull_apply_upload_swap` against a REAL backend: `smb_integration_test`
(`smb_integration_remote_zip_edit_deletes_an_entry_through_the_share` + `..._cancel_before_swap_keeps_original`, and
routing detection + extract-out in `smb_integration_archive_routing_detection_and_extract_out`) and `mtp_archive_test` under the
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
the read path. Pinned by the four `remote_edit_*` reap tests in `archive_remote_edit_tests` (stale-same-archive reaped,
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
- **Routing seams.** The former archive rejections become routing: `create_directory_managed` / `create_file_managed`
  (a `.zip`-crossing parent), `rename_managed` (an in-archive path), `delete_files_start` (in-archive sources), and the
  `copy`/`move_between_volumes` COMMANDS (an archive-resolved destination). The instant-op forks reach a `TauriEventSink`
  via the manager's startup-wired app handle (`operations_app_handle`), so no command signature changes; a
  `create`/`rename` return is the operation id, not a path (the FE reads it as an op handle).
- **Changeset per op.** mkdir → `{ mkdir }`; mkfile → `{ add }` (empty bytes); rename inside → `{ rename }`; delete
  inside → `{ delete }` (batched across a multi-select in one zip); copy/move INTO → one `{ add + mkdir }` for the whole
  transfer (`route_archive_copy_into` walks the LOCAL sources with `walkdir`). A move INTO deletes the top-level sources
  after the commit, and only when nothing was skipped (the move invariant — never delete a source whose bytes didn't
  land): local sources go straight off the FS, remote ones through the source volume (recursive for trees).
- **Compress = seed an empty zip, then copy-into** (`compress.rs`, `compress_start`). Creating a NEW zip and packing the sources into it IS an archive edit, so compress is built ON copy-into rather than as a parallel path: `seed_empty_zip` writes a valid empty archive at the target, then `compress_start` calls `route_archive_copy_into` with `is_move = false`. The seed is the ONLY net-new backend surface — scan, plan-in-closure, progress/ETA, cancel, lane admission, and the mutator's temp+rename durability are all inherited. **The seed is LOAD-BEARING**: `route_archive_copy_into` (and the mutator) open the target with `ZipArchive::new`, which rejects a 0-byte file (`ZipError::InvalidArchive`) — so a brand-new target must already be a valid archive before the copy-into runs. `seed_empty_zip` writes the 22-byte bare end-of-central-directory record (`PK\x05\x06` + 18 zero bytes) — the minimal valid zip, a zero-entry archive that `ZipArchive::new` opens with `len() == 0` and whose first bytes pass `bytes_start_with_zip_signature`. It uses the SAME temp+rename discipline as the mutator (build a `.cmdr-tmp-<uuid>` sibling, fsync, atomic rename over the target, fsync the parent dir), so a crash mid-seed never leaves a torn file and an overwrite is atomic. **Seed matches the parent, local or remote.** `route_archive_copy_into`'s remote path PULLS the existing `.zip` before editing (see the remote-edit contract above), so a local-FS seed would be invisible to a remote parent — the seed must land wherever the copy-into will look for it. So `compress_start` branches on `parent.supports_local_fs_access()`: a LOCAL parent gets the local-FS `seed_empty_zip`; a REMOTE parent (SMB / MTP) gets `seed_empty_zip_remote`, which stages the 22 bytes in a scratch file and places them THROUGH the parent volume via `archive_remote_edit::place_local_file` (the remote edit's own upload-to-temp + atomic-swap commit, generalized to tolerate a MISSING original for a brand-new target). Then the copy-into pulls the seed, adds the sources, and swaps the full archive in. The remote path composes for both swap shapes: SMB's atomic rename-replace and MTP's delete-then-rename (same-name siblings allowed) — MTP needs no compress-specific work beyond the shared remote-edit machinery. **Remote cancel-safety** is inherited, not re-earned: the seed is placed atomically, and a cancel/fault during the copy-into leaves at worst the valid empty seed at the target (`place_local_file` reuses `pull_apply_upload_swap`'s swap, so the target keeps its bytes until the final atomic swap, and any partial upload temp is deleted). `compress_start` reuses `WriteOperationType::ArchiveEdit` (compress has no distinct backend op type — its identity is frontend-only). Pinned by `compress_tests` (local seed validity + atomic overwrite, end-to-end compress of local files and a directory subtree; the seed's load-bearing role is shown by the copy-into failing against a 0-byte target), `compress_remote_tests` (seed-through-volume onto a non-local `InMemoryVolume` for both swap shapes, plus overwrite-replaces-not-merges), and the live-Samba `smb_integration_compress_local_files_onto_the_share`.
- **Compression level threads from the op config onto the changeset.** `VolumeCopyConfig::compression_level` (frontend-owned, read from the `behavior.archiveCompressionLevel` setting at dispatch) is passed through `compress_start` / `route_archive_copy_into` as an `Option<i64>` param and stored on the `Changeset` (`archive_copy_into_start` sets `plan.changeset.compression_level` before `mutator::apply`). It governs every user-driven zip write uniformly — compress AND copy/move INTO an existing archive — because both funnel through the shared mutator. `None` (no caller opinion, or a non-archive copy) means the crate default (level 6). The level applies to NEWLY added entries only and is clamped 1..=9; the mechanism and the clamp rationale are single-sourced in `crates/cmdr-archive/src/mutation/DETAILS.md` § "Compression level applies to ADDED entries only". Internal zips (crash/error-report bundles) keep their own fixed level and never read this setting.
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
  OverwriteSmaller/Older compare size/mtime (strict). **The Stop policy prompts interactively**
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
