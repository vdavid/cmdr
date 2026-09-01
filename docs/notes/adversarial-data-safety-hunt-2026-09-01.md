# Adversarial data-safety hunt, 2026-09-01

A read-only bug hunt over the transfer engines, run as a fan-out of one reading agent per subsystem, then verified
finding by finding against the cited code before landing here. Nothing in this note is a fix: it is a ranked triage list
for principle #1 (protect the user's data), meant to be worked through at leisure before launch.

**Scope, honestly.** The plan was 18 subsystems (both transfer engines, the write-ops umbrella, archive edits,
delete/trash/clipboard, `cmdr-fs`, SMB, SFTP, MTP, secrets and settings persistence, the file viewer, the operation log,
four slices of `cmdr-index`, and git/downloads/listing). The run was stopped for budget after **four** hunters returned;
the rest never started. What is covered:

| Hunter               | Read in full                                                                   | Raw findings |
| -------------------- | ------------------------------------------------------------------------------ | -----------: |
| transfer-core        | `write_operations/transfer/` minus `volume/` (26 files)                        |            5 |
| transfer-volume-copy | `transfer/volume/` copy side: strategy, copy, concurrent, serial, cleanup, ... |            5 |
| transfer-volume-move | `transfer/volume/` move side: move, move_same, merge, rename_merge, naming     |            4 |
| write-ops-umbrella   | `write_operations/` root: state, manager, rollback, journal, overwrite, ...    |            5 |

Three findings were reported independently by two hunters each (a good sign for those three); after collapsing them
there are **15 distinct findings**. Every one was then verified by reading the cited code paths directly, not by a
second agent wave. "Confirmed" below means the mechanism holds as described in the code as of `5fdc323`; "plausible"
means the code reads as claimed but one step of the trigger was not traced end to end. The [Not covered](#not-covered)
section lists what a second run should pick up.

## The ranked list

Severity scale: **critical** = silent permanent loss of user data in a realistic flow; **high** = loss or corruption
needing an unusual but plausible condition, or loss the user is told about but cannot recover; **medium** = wrong
result, wedged state, or recoverable loss; **low** = hygiene.

1. **high, confirmed**: Cross-FS move Phase 4 `remove_dir_all`s the live source tree, destroying anything written into
   it after the scan.
2. **high, confirmed**: Deep merge matches destination children by exact byte name, so a case- or NFC/NFD-differing dest
   file is silently replaced.
3. **high, confirmed**: Same-FS merge follows a directory symlink and moves the link target's contents out of a folder
   the user never selected.
4. **high, confirmed**: Same-volume rename-merge does the same on a dir-vs-dir hit (the listing reports a dir symlink as
   `is_directory`).
5. **high, confirmed**: Upfront "Overwrite all smaller/older" compares a file against a directory's own inode size/mtime
   and deletes the whole folder.
6. **high, confirmed**: Local copy auto-rollback on a mid-batch error deletes destination files that already replaced
   the user's originals, leaving neither copy.
7. **high, confirmed**: `reap_stale_transfer_temps` deletes the `.cmdr-tmp-*` that is the only copy of new data after a
   finalize failure.
8. **high, plausible**: Cross-FS move: a Skip while staging (two same-named sources) is not recorded, so Phase 4 deletes
   an original that never landed.
9. **high, confirmed**: Any `get_metadata` error on the destination is read as "nothing there", so Skip/Stop becomes an
   overwrite on a flaky link.
10. **medium, confirmed**: Same-volume Overwrite deletes the destination original before the rename that replaces it.
11. **medium, confirmed**: Same-FS move rollback after a merge cannot rename children back into a removed parent, yet
    reports `rolled_back: true`.
12. **medium, confirmed**: Concurrent driver: a resolver failure returns through `?` and skips the entire post-loop (no
    cleanup, no rollback, no cancelled event).
13. **medium, confirmed**: Folder-over-file Overwrite deletes the user's file as soon as an empty directory exists, and
    never records the directory for rollback.
14. **medium, plausible**: Rename-policy `O_EXCL` placeholders for deep-merge children are never recorded or cleaned,
    leaving 0-byte `name (1).ext` files.
15. **low, confirmed**: Three move/rename landings are check-then-`fs::rename` rather than the no-replace primitive the
    copy path uses.

## Findings

### 1. Cross-FS move deletes the live source tree, not the staged set

- **Where:** `apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op/cross_fs.rs`,
  `delete_sources_after_move` (around line 505).
- **Mechanism:** `move_with_staging` takes its file list from the cached scan preview (taken when the transfer dialog
  opened, possibly a queue wait ago) or from `scan_sources` at op start. Phase 2 stages exactly `scan_result.files`,
  Phase 3 renames the staged tree into place, and Phase 4 removes each top-level source by identity:
  `fs::remove_dir_all` for a directory (or `delete_dir_preserving_skipped`, which likewise deletes every child not in
  `skipped_source_paths`). Nothing compares the tree against what was staged.
- **Trigger:** move a folder to another disk, share, or device while anything writes into it: a browser finishing a
  download, a camera import, a sync client, an IDE saving, the user dropping a file in from another app. A file
  rewritten after its chunked copy finished loses the new bytes the same way.
- **Impact:** files created or modified in the source after the scan are destroyed without ever being copied. The
  invariant in `transfer/DETAILS.md` ("never delete the source if the destination isn't fully in place") only holds for
  the scanned set.
- **Suggested fix:** delete by ledger, not by tree. Remove exactly the files Phase 2 staged (plus the scanned dirs) and
  finish with `remove_dir` (not `remove_dir_all`) so a leftover surfaces as `ENOTEMPTY` and is reported as "N items
  appeared during the move and stay in the source", instead of being deleted. Reported by two hunters independently.

### 2. Deep merge misses case-folded and normalization-differing destination names

- **Where:** `transfer/volume/merge.rs`, `merge_level` (`dest_by_name` built around line 451, looked up around line
  482); the destructive half is `transfer/staged_write.rs::land` (lines 241-253).
- **Mechanism:** for a pre-existing destination level, `merge_level` builds `dest_by_name` keyed by the exact listed
  name and looks up `entry.name` exactly. A source child `Report.docx` against a dest holding `report.docx` (or NFD vs
  NFC `café.txt`) gets `dest_hit == None`: no `resolve_merge_child`, no policy, no prompt. The child streams to a
  `.cmdr-tmp-*` sibling and `StagedWrite::commit` calls `land`, which renames onto the final name. On a case-insensitive
  destination the backend answers `AlreadyExists` (LocalPosix via `renamex_np(RENAME_EXCL)` and SMB's stat-first
  `rename_impl` both do), and `land` treats that as "something is in the way": `delete(final_path)` (which resolves onto
  the user's file) then rename. For small SMB files the single-shot `FileOverwriteIf` path truncates without even that.
- **Trigger:** copy, extract, or cross-volume move one folder onto an existing same-named folder on an SMB share or an
  external APFS volume, with policy Skip or Stop, where one file inside differs only in case or normalization.
- **Impact:** the destination file is replaced silently under a policy that promised not to touch it.
- **Why the guard doesn't apply:** the fold-aware guard exists only at the top level (`DestNameIndex`, and the serial
  driver's `get_metadata` probe which the backend resolves case-insensitively; pinned by
  `copy_precheck_tests.rs::a_destination_name_differing_only_in_case_is_still_a_conflict`). The same-volume
  `rename_merge.rs` has late detection for exactly this (`late_detected_collision`, lines 184-197); the streaming
  `merge_level` has neither.
- **Suggested fix:** key `dest_by_name` by the same fold `DestNameIndex` uses (NFC + lowercase) with the exact name as a
  second key, or give `land` a caller-supplied "expected empty" flag so an unexpected `AlreadyExists` on a Skip/Stop
  child becomes a late-detected conflict rather than a delete. Reported by two hunters independently.

### 3. Same-FS merge descends into a directory symlink and empties its target

- **Where:** `transfer/move_op/mod.rs`, `merge_move_directory`, line 271 (`source_child.is_dir()`); the same check at
  `same_fs.rs:114` for a top-level source and in cross-FS Phase 3.
- **Mechanism:** `Path::is_dir` is `fs::metadata`-based and follows symlinks. A child that is a symlink to a directory
  whose dest counterpart is a real directory recurses: `fs::read_dir(symlink)` lists the target, and each entry is
  `fs::rename(&source_child, &dest_child)`, which the kernel resolves through the link, moving the target's real entries
  out. Afterwards `fs::remove_dir(symlink)` fails with `ENOTDIR` and is ignored (line 327), so the link survives
  pointing at a now-empty directory.
- **Trigger:** `~/work/app` contains `node_modules -> /Users/me/shared/node_modules` (or any symlinked data folder);
  `~/archive/app` already has a real `node_modules/`. Move `~/work/app` into `~/archive` on the same volume.
- **Impact:** a folder outside the selection is emptied into the destination. The volume engine's documented rule is
  "symlinks move as opaque entries"; this path never checks `symlink_metadata`.
- **Suggested fix:** test `symlink_metadata(...).file_type().is_symlink()` before the `is_dir` branch and rename the
  link itself as an opaque entry (or route it through `resolve_conflict` as a type mismatch). Add a symlink case to
  `move_op_tests.rs`; today no test in the move suite creates one.

### 4. Same-volume rename-merge does the same through the listing's `is_directory`

- **Where:** `transfer/volume/rename_merge.rs`, line 159
  (`entry.is_directory && dest_hit.is_some_and(|d| d.is_directory)`).
- **Mechanism:** `LocalPosixVolume::list_directory` reports a symlink whose target is a directory as
  `is_directory: true` (`listing/reading.rs:292`, `let is_dir = metadata.is_dir() || target_is_dir;`). On a dir-vs-dir
  hit the merge recurses with `child_source` = the symlink path; `volume_list` then reads through the link and each real
  child is `ctx.volume.rename(<symlink>/<child>, <dest>/<child>, false)`, moving it out of the target. The only symlink
  test (`rename_merge_moves_symlink_as_opaque_entry`) uses a file symlink with no dest clash, so this branch is
  unexercised.
- **Trigger:** an external local volume (its own volume id, so the frontend's `isVolumeMove` routes it here) or an
  SMB/SFTP share exposing directory symlinks; move a folder containing a symlink-to-dir onto a folder that already has a
  real dir of that name.
- **Suggested fix:** consult `FileEntry::is_symlink` (the listing populates it) before the recursion and treat the link
  as a leaf; same fix shape as #3.

### 5. "Overwrite all smaller/older" compares a file against a directory and deletes the directory

- **Where:** `write_operations/conflict.rs`, the upfront-policy arm at lines 242-247 and `reduce_conditional_resolution`
  (261-309).
- **Mechanism:** `copy_single_item`'s regular-file branch calls `resolve_conflict(source, dest_path)` whenever the dest
  exists, including when `dest_path` is a directory. `resolve_conflict` computes `is_file_to_folder` (line 121) but only
  feeds it to the apply-to-all latch, which deliberately refuses to carry Overwrite into a file-over-folder clash. The
  upfront-policy arm never consults it: `OverwriteSmaller` compares `dest_meta.len()` (the directory inode's own size, a
  few hundred bytes to a few KB) with the file's size, `OverwriteOlder` compares mtimes, neither checks `is_dir`, and
  both yield `Overwrite` for any ordinary file larger or newer than the directory entry.
  `stage_and_land_file(replacing = true)` then renames the directory aside and `remove_dir_all`s it (overwrite.rs step 4
  explicitly handles a directory aside).
- **Trigger:** choose "Overwrite all smaller" (or "older") in the transfer dialog and copy a selection in which a plain
  file shares a name with a destination folder (`notes` vs `notes/`, `build` vs `build/`, `README` vs `README/`).
- **Impact:** the whole destination folder and its contents are deleted under a policy documented as conservative ("a
  borderline file is never silently overwritten"). Plain upfront `Overwrite` has the same effect, which reads as
  intended, but the conditional variants do not.
- **Suggested fix:** in `reduce_conditional_resolution`, reduce to `Skip` whenever either side is a directory; better,
  route every file-over-folder clash through the same refusal the latch already implements.
  `conflict_conditional_tests.rs` covers file/file pairs only.

### 6. Local copy auto-rollback removes files that replaced originals, leaving neither

- **Where:** `transfer/copy/mod.rs`, `PostLoopIntent::Failed` arm (line 671, `transaction.rollback()`);
  `state.rs::CopyTransaction::rollback` (line 837).
- **Mechanism:** with policy Overwrite, `stage_and_land_file(replacing = true)` renames the user's original aside, lands
  the temp, and deletes the aside. `copy_single_item` then `transaction.record_file(actual_dest)` with no marker that
  this path replaced a file (the "overwrote" fact lives only in the journal row, which rollback never reads). When any
  later item in the batch returns a non-`Cancelled` error (`EACCES` on one source, `ENOSPC`, a source deleted mid-copy,
  `NameTooLong`), the driver maps it to `PostLoopIntent::Failed` and the local engine **always** rolls back, removing
  every recorded file, including the ones that replaced originals.
- **Trigger:** copy a folder onto a destination that already holds most of its files, choose Overwrite (or "Overwrite
  all"), and have one later file fail.
- **Impact:** for every overwritten file the original is gone (aside deleted) and the new copy is gone (rolled back).
  The doc comment on `CopyTransaction::rollback` accepts "rollback can't restore overwritten originals" as a decision
  made for a **user-requested** rollback; the automatic rollback on failure turns that decision into unrequested loss.
  The volume engine deliberately keeps completed files on error (`transfer/volume/copy.rs` ~1099); the local engine is
  the outlier.
- **Suggested fix:** on `Failed`, keep completed files (match the volume engine) and only clean the partial; or exclude
  paths landed with `needs_safe_overwrite` from the automatic rollback.

### 7. Stale-temp reaping deletes the only copy of new data after a finalize failure

- **Where:** `transfer/volume/cleanup.rs::reap_stale_transfer_temps` (line 286, `volume.delete(&temp_path)`), with
  `STALE_TEMP_MIN_AGE` = one hour (line 35); the producer is `transfer/volume/conflict.rs::finalize_safe_replace` (line
  638).
- **Mechanism:** a cross-volume file-over-file Overwrite streams the new bytes into `<name>.cmdr-tmp-<uuid>`, then
  `finalize_safe_replace` deletes the original and renames the temp over it. If the rename fails after the delete
  succeeded (documented as "the nastier case"), the temp is deliberately preserved as the only complete copy of the new
  data, and the user's original is already gone. `reap_stale_transfer_temps` is name-and-age based, not ledger based:
  the next copy into that directory (Phase 0.6 of `copy_volumes_with_progress`), an hour or more later, lists the dir,
  sees a `.cmdr-tmp-*` file older than an hour, and deletes it.
- **Trigger:** overwrite a file on a NAS or phone via a cross-volume copy or move; the link drops or MTP refuses the
  rename at that instant. The user sees an error naming the destination but not the temp. Later they copy anything into
  the same folder.
- **Impact:** the last surviving copy of the new data is deleted silently. The age gate is documented as protecting a
  temp another instance is actively writing, not committed data.
- **Suggested fix:** give the finalize-failure path a distinct name (a `.cmdr-keep-*` marker, or rename the temp to the
  final name with a ` (recovered)` suffix on the spot), and never reap by name alone. The same finalize failure should
  also surface the temp's path in the error the user sees.

### 8. Cross-FS move: a Skip while staging is not recorded, so Phase 4 deletes an unlanded original

- **Where:** `transfer/move_op/cross_fs.rs` Phase 2 loop (lines 161-209) and `transfer/copy/single_item.rs` Skip arms
  (lines 308-316, 376-379, 456-460).
- **Mechanism:** Phase 2 stages every leaf through `copy_single_item` into `.cmdr-staging-<op>`. If two top-level
  sources share a basename (`/a/invoices` and `/b/invoices`), both map to `<staging>/invoices/`, so the second source's
  `summary.pdf` finds a staged file already there and `resolve_conflict` runs; under Skip (or a conditional reducing to
  Skip, or a Stop prompt answered Skip) it returns `None`, `copy_single_item` records progress and returns `Ok`.
  `skipped_source_paths` is only populated in Phase 3 (`merge_move_directory`'s `staged_skips` and the top-level Skip
  arm). Phase 4 then `remove_dir_all`s `/b/invoices`.
- **Trigger:** requires two same-named sources in one move. The hunter points at the search-results pane, whose
  `buildSnapshotTransferProps` passes entries from different parents as one source list; that frontend hop is the
  unverified step, hence "plausible". A same-named-sources move would also confuse Phase 3 (the second source's
  `staged_path` was already renamed away by the first).
- **Suggested fix:** the ledger-based delete from #1 covers this too. Independently, refuse or de-duplicate
  same-basename sources at validation time.

### 9. A destination stat failure is read as "no conflict", and the landing then clobbers

- **Where:** `transfer/volume/copy_serial.rs:187`, `transfer/volume/move.rs:467`, `transfer/volume/move_same.rs:408`
  (all `get_metadata(...).await.ok().map(...)`), `copy_concurrent_source.rs:249`, consumed by
  `transfer_driver/async_driver.rs:178` ("stat failed, treated identically to no-conflict").
- **Mechanism:** `.ok()` maps `ConnectionTimeout`, `DeviceSessionReset`, `PermissionDenied`, and every other error to
  `None`. No resolver runs, the Skip/Stop policy is never consulted, and the write proceeds: staged writes hit `land`'s
  `AlreadyExists` → delete → rename; SMB small files go through single-shot `FileOverwriteIf`, which truncates the
  existing file outright.
- **Trigger:** copy or move to a NAS or phone with policy Skip or Stop while the link is flaky; the one stat for a name
  that exists at the destination fails, the backend reconnects, and the existing file is replaced with no prompt.
- **Why the guard doesn't apply:** `conflict.rs` documents its own probes as propagate-only ("a probe whose answer can
  select a destructive branch may not have a default"), but that discipline starts at the resolver; the detection step
  before it defaults to absent.
- **Suggested fix:** make the fetcher return `Result<Option<_>>` and fail the item (or retry once) on anything but
  `NotFound`. Reported by two hunters independently; rated high because the consequence is silent, but the precondition
  is a transient failure, so a maintainer may reasonably file it as medium.

### 10. Same-volume Overwrite deletes the destination before the rename that replaces it

- **Where:** `transfer/volume/move_same.rs`, conflict-resolver closure around line 490 (`volume.delete(&orig)` then,
  later, `volume.rename(&source_path, &dest_item_path, false)` at ~646); `rename_merge.rs::apply_child_decision` (lines
  406-416) has the same shape for deep children.
- **Mechanism:** documented in `transfer/volume/DETAILS.md` § "Cross-volume file→file Overwrite is a safe-replace" as a
  deliberate choice ("rename is atomic-ish and not a stream, so the safe-replace temp dance buys nothing there"). The
  premise is wrong on the failure side: the rename is a separate call that can fail after the delete succeeded (SMB
  `STATUS_SHARING_VIOLATION` because the source is open elsewhere, an MTP `MoveObject` refusal, a session blip).
- **Impact:** the operation reports a failure with the destination gone and the source not moved. Recoverable in the
  sense that the source still exists, so medium.
- **Suggested fix:** rename the destination aside first (a `.cmdr-temp-*` sibling, like `overwrite.rs`), rename the
  source in, then delete the aside; restore the aside on rename failure.

### 11. Same-FS move rollback after a merge reports success it did not achieve

- **Where:** `transfer/move_op/mod.rs`, `merge_move_directory` (removes emptied source dirs at 323-328) and
  `MoveTransaction::rollback` (64-70, warn-only); `same_fs.rs:241-257` emits `rolled_back: true` unconditionally.
- **Mechanism:** a merge records every child rename, then removes the emptied source directory. A later Cancel with
  Rollback replays `fs::rename(moved_to_dest, original_source)` per child; every child whose parent was removed fails
  with `ENOENT`, is only logged, and the cancelled event still says `rolled_back: true`.
- **Impact:** the first folder's files stay merged into the destination while the UI says the move was undone. No test
  in `move_op_tests.rs` exercises rollback after a merge.
- **Suggested fix:** record directory removals in the transaction and recreate parents on rollback; report `rolled_back`
  from an actual failure count.

### 12. Concurrent driver: a resolver failure skips the whole post-loop

- **Where:** `transfer/volume/copy.rs:942` (`.await?` on `drive_transfer_concurrent`); `copy_concurrent.rs:119-123`.
- **Mechanism:** with ≥3 sources and a remote peer, a top-level clash under policy Stop parks the driver in
  `prepare_source` → `resolve_volume_conflict` awaiting the prompt. Cancel (or Cancel with rollback) drops the sender,
  the resolver yields `Cancelled`, `ConcurrentDriver::run` propagates it with `?`, and `copy_volumes_with_progress`
  returns before `record_created_dirs_on`, the `Cancelled` reclassification, `clean_abandoned_staged_writes`, the
  rollback branch, and the write-cancelled event. In-flight task futures are dropped mid-write.
- **Impact:** partials (as `.cmdr-tmp-*`) and created directories are left behind, a requested rollback does not run,
  and the frontend never gets the cancelled event for this path. The serial driver has the guard
  (`async_driver.rs:225-231` turns the resolver error into `PostLoopIntent::Failed`).
- **Suggested fix:** route the resolver `Err` into `ConcurrentOutcome::copy_error` the way task failures already are.

### 13. Folder-over-file Overwrite deletes the file as soon as an empty directory exists

- **Where:** `transfer/copy/single_item.rs:263` (`safe_overwrite_dir(&blocking, |t| create_dir_all(t))`);
  `overwrite.rs::safe_overwrite_dir` (288-341).
- **Mechanism:** the closure only creates the directory, so `safe_overwrite_dir` deletes the aside (the user's file) the
  moment `create_dir_all` returns `Ok`, before any content lands. Control returns, `!parent.exists()` is now false, so
  the directory is never `record_dir`'d. If the first leaf then fails, is cancelled, or is rolled back, the user has an
  empty `X/`, no `X` file, and rollback cannot even remove the directory.
- **Suggested fix:** record the directory in the transaction inside the closure, and defer the aside deletion to the
  transaction commit (or keep the aside until the subtree has landed).

### 14. Deep-merge Rename placeholders are never cleaned on cancel or failure

- **Where:** `transfer/volume/naming.rs:75` (`create_new(true)` placeholder on a local-path destination);
  `merge.rs::copy_leaf` (211-257) records into `created` only after a successful commit.
- **Mechanism:** for MTP→Local, SMB→Local, and archive-extract→Local, `find_unique_volume_name` reserves `name (1).ext`
  with an `O_EXCL` 0-byte file and returns the path; nothing records it. Top-level placeholders are cleaned (serial
  driver's `last_dest_cell`, concurrent driver's `in_flight_partials`), deep children have no equivalent slot. In
  sequential-extract plan mode every clashing file's placeholder is created before any byte is written.
- **Impact:** after a cancel or read error the destination holds one 0-byte `file (1).ext` per unresolved clash,
  indistinguishable from a real file. Not verified end to end (the `copy_leaf` abandon path was not traced), so
  plausible.
- **Suggested fix:** record the placeholder in `created` at reservation time, or have `copy_leaf` remove it on abandon.

### 15. Three move/rename landings are check-then-`fs::rename`

- **Where:** `transfer/move_op/same_fs.rs:176`, `move_op/mod.rs:317`, `cross_fs.rs:361` (existence check then
  `fs::rename`), and `write_operations/rename.rs:173` (the `root` volume branch of the inline rename: `symlink_metadata`
  then `std::fs::rename`).
- **Mechanism:** POSIX `rename` replaces an existing file atomically, so a same-named file created by another process in
  the window between the check and the syscall is overwritten with no prompt. `overwrite.rs::rename_no_replace` and
  `rename_local_exclusive` (`renamex_np(RENAME_EXCL)` / `renameat2(RENAME_NOREPLACE)`) exist for exactly this and are
  used by every copy landing and by bulk rename, but not here.
- **Impact:** low; the window is one syscall wide. Listed because the fix is a one-line swap per site.

## Not covered

The run stopped before these hunters started, so this note says nothing about them. They were scoped as follows and
should be the next run, in roughly this order of data-safety weight:

1. **Archive edits**: `cmdr-archive`, `archive_edit/`, `archive_remote_edit.rs`, `backends/archive.rs` (in-place archive
   rewrites, `move_out` ordering, zip-slip on extract, the archive LRU after a rewrite).
2. **Delete, trash, clipboard**: `write_operations/delete/`, `clipboard/`, `paste_clipboard.rs`, `volume/eject.rs`
   (symlink descent in the delete walker, trash-less volumes, cut state after a failed paste).
3. **`cmdr-fs` Volume trait, `LocalPosixVolume`, `VolumeManager`** (write API atomicity, root-anchored path escapes,
   volume id reuse after retirement).
4. **SMB** (`cmdr-smb`, `network/`, `backends/smb.rs`), **SFTP** (`cmdr-sftp`), **MTP** (`mtp/`, `backends/mtp/`).
5. **Operation log** (`operation_log/`: migrations, retention vs pending rollback, capture recording the wrong paths).
6. **Secrets, settings, recents, window state, install ids, agent memory** (atomic small-file writes, the encrypted-file
   fallback, secrets in logs).
7. **File viewer** (`file_viewer/`: archive-extract temps, media temp copies, watcher reloads of half-written files).
8. **`cmdr-index`** in four slices: writer/store/handle; reconcile/scanner/watch; lifecycle/read/aggregator/host;
   `media_index`.
9. **Git browser, Downloads watcher, listing/watcher, cloud actions, tags, `open_with`.**

## Method notes, for the next run

- One hunter per subsystem with instructions to read every non-test source file in full, kill each candidate by looking
  for the guard before reporting, and return at most eight findings in a fixed schema (verbatim excerpt, call-ordered
  mechanism, user-level trigger, the guard ruled out, confidence). The schema is what made verification cheap: every
  claim came with a `file:line` and an excerpt to check.
- The original design had two adversarial verifier agents per finding (a refuter and a reproducer). That wave was cut
  for budget; verification here was done by one reader following the cited lines. For the next run, one verifier per
  finding is enough when the hunter schema is this strict; two is a luxury.
- The four hunters that ran cost roughly the same as the verification of all 19 findings by hand would have in agent
  form. Budget for about one hunter-plus-verification per subsystem-hour of agent time, and run the hunters in the
  priority order above so a stop leaves the highest-risk areas covered.
- Three findings arriving from two hunters independently (#1, #2, #9) were the strongest signal in the set. Overlapping
  scopes are worth the duplication.
