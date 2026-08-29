# Transfer (copy + move) details

Pull-tier docs for `src-tauri/src/file_system/write_operations/transfer/`: architecture, flows, and decision rationale.
Must-know invariants and gotchas live in `CLAUDE.md`.

All transfer flows go through the shared driver in `transfer_driver/` and emit progress via `OperationEventSink`.

See `../CLAUDE.md` for the shared `WriteOperationState`, `OperationIntent` state machine, cancel/rollback contract, ETA
estimator, and settle contract. `../delete/CLAUDE.md` is the parallel doc for delete + trash.

Frontend counterpart: `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md` for the dialog flow, progress UI,
conflict-policy radios, and the cancel/settle close contract.

## Files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md`. What
the mechanisms DO is in the sections below (copy strategy in § "Copy + move semantics" and § "Key decisions",
per-file flushing in § Durability, the two driver entry points and leaf-granular progress in § "Key decisions", the
same-volume rename-merge in § "Key decisions"). The cross-volume engine documents itself in `volume/DETAILS.md`
(the archive route included, § "One-pass sequential extract"). Only the layout
facts that none of those carry live here:

- **`copy/mod.rs`'s post-loop dispatch keeps a three-arm `PostLoopIntent` shape** (Completed / Cancelled / Failed)
  including a post-completion `RollingBack` recheck, for the rollback-clicked-in-the-last-millisecond race (commit
  `1de4255d`). Pre-flight scan, dry-run, disk-space, and bulk-skip filtering stay OUTSIDE the driver, in `copy/mod.rs`.
- **`volume/` holds the whole cross-volume engine and is a facade** (`volume/mod.rs`): copy phases (`copy.rs` + the two
  drivers, serial in `copy_serial.rs` and concurrent across `copy_concurrent{,_source,_task}.rs`), move (`move.rs`, declared `mod r#move;` because `move` is a
  keyword, plus `move_same.rs` and `rename_merge.rs` for the same-volume rename path), the merge/staging engine
  (`strategy.rs`, `sequential_extract.rs`), and the supporting `conflict.rs`, `cleanup.rs`, `preflight.rs`,
  `transfer_error.rs`. Every one of those is PRIVATE to `volume`; `mod.rs` re-exports the nine items outside code calls,
  so a caller writes `transfer::volume::<item>`. Adding a caller means adding a re-export, not widening a submodule —
  that boundary is what lets the engine's internals move without a repo-wide rename, and it is why the `r#move` escape
  never appears outside this directory.
- **`transfer_driver_*_tests.rs` sit at the `transfer/` level, wired in as submodules via `#[path = "../…"]`.** That is
  incidental, not load-bearing: `file-length` allowlist keys are plain paths and get re-pathed with the file (the
  `volume/` move did exactly that for eight entries), so it is no reason to keep or to move them.
- **`transfer_probe_tests.rs` is a `#[path]` sibling, not an inline `mod tests`**, for the same reason every other big
  module here splits: the probe plus its watchdog cases is 1.3k lines in one file. `retry.rs`'s policy tests stay inline
  (the module is small and the tests read as its specification).

## Copy + move semantics

**`CopyTransaction` rollback: sync with progress.** `rollback()` (synchronous, for error paths) and tracked `rollback_with_progress()` in `copy.rs` (for user-initiated rollback, emits `write-progress` events with `phase: RollingBack`, checks for `Stopped` between file deletions so the user can cancel the rollback). Auto-rollback via `Drop` remains as a panic safety net.

**Move strategy.** Same filesystem detected via device ID comparison (`MetadataExt::dev`). Cross-filesystem move uses a `.cmdr-staging-<uuid>` dir at the destination root, then atomic `rename` into place, then source deletion.

**Cross-FS move source-delete preserves Skipped sources.** `move_with_staging`'s Phase 3 (staging → final rename) resolves conflicts; a Skip discards the staged copy so the file never lands at the destination. Phase 4 (`delete_sources_after_move`) must therefore NOT delete that source — the user clicked Skip to keep both copies, and deleting the only original would be silent data loss. Phase 3 records every Skipped original in a `skipped_source_paths: HashSet<PathBuf>` (whole top-level source for a single-file / type-mismatch Skip; per-child paths remapped from the staging prefix back to the source prefix for a directory merge). Phase 4 skips whole sources in the set, removes a clean source dir wholesale (`remove_dir_all`), and for a source dir that holds a Skipped descendant walks it via `delete_dir_preserving_skipped` (deletes non-skipped children, removes a dir only once empty), so the Skipped child's original survives inside a surviving source directory. The same-FS path (`move_with_rename`) is inherently correct: it renames originals directly, and a Skipped child just leaves the source dir non-empty. Pinned by `move_op_tests.rs::{cross_fs_move_skip_preserves_source_and_dest, cross_fs_move_dir_merge_skip_child_preserves_source_child}`.

**Empty directories land via the scanned-dirs pass (`copy/scanned_dirs.rs::create_scanned_dirs_at_destination`).** The per-file loop creates directories only as FILE parents, so an empty directory — or a branch holding nothing but empty directories — has no file to hang its creation on and used to complete "successfully" while never arriving (and on a cross-FS move, Phase 4 then deleted the source: the empty dir was destroyed without ever landing). The pass runs over `ScanResult.dirs` on the local copy's Completed arm and after the move's staging loop (destination = the staging dir, so empty dirs ride the normal Phase-3 rename + cleanup). Mapping mirrors `FileInfo::dest_path`; created dirs are recorded for rollback. Data-safety: a dest path that already holds anything (dir = merge, file = type clash) is left untouched — an empty source dir never replaces user data. Pinned by `copy_tests.rs::{copy_creates_empty_directory_at_destination, copy_creates_nested_empty_directories, copy_empty_directory_does_not_clobber_same_named_dest_file}` and `move_op_tests.rs::cross_fs_move_preserves_empty_directories`. The volume (MTP/SMB) pipeline doesn't share the hole — `copy_directory_streaming` creates each dir before walking its children.

**Move rollback (same-FS).** `MoveTransaction` in `move_op.rs` tracks `(source, dest)` pairs for each rename. On cancellation, renames are reversed in reverse order. Same-FS rename rollback is instant (just another rename), so it runs synchronously. Cross-FS move rollback is handled by `CopyTransaction` (deletes the staging directory).

**Intentional duplication: `merge_move_directory` vs `copy_single_item`.** Both implement recursive merge with conflict resolution, but differ in every detail: copy has progress tracking, symlink handling, byte counting, strategy selection, and `CopyTransaction` recording. Move uses simple `fs::rename`. A shared abstraction would be forced and fragile. Cross-references are in the doc comments of both functions.

**Cross-type Rename (file↔folder).** Rename on a type-mismatch clash follows the same cmdr Rename semantics as file→file: the **existing** dest item stays untouched at its name; the **incoming** item lands under a fresh `name (1)` with its full content. `conflict::apply_resolution`'s Rename arm calls `find_unique_name`, which atomically reserves the chosen name by creating a 0-byte placeholder file (TOCTOU guard), then returns `needs_safe_overwrite: true` so the caller's copy/rename lands *on* that placeholder rather than failing against it. (Pre-fix it returned `false`, and the same-APFS-volume copy path — `copyfile(3)` with `COPYFILE_EXCL` — refused to overwrite the placeholder and lost the incoming bytes.)
- **Local copy `copy_single_item`:** the regular-file / symlink branches consume the placeholder via the overwrite path (`safe_overwrite_file` / `remove_file` + recreate). The folder→file case surfaces at the parent-creation site, where the incoming folder must land at the renamed root: the branch removes the reserved placeholder, `create_dir_all`s the renamed root, and records the redirect in a `dir_remap: HashMap<PathBuf, PathBuf>` so every subsequent child of that subtree (`copy_single_item` applies `apply_dir_remap` at the top) follows it. The Overwrite-vs-Rename split at that site is decided by `resolved.path == blocking` (Overwrite replaces in place) vs `!=` (Rename lands aside) — NOT by `needs_safe_overwrite`, which is now `true` for both.
- **Local move `move_resolved_into_place` (shared by `move_with_rename`, `merge_move_directory`, and the cross-FS staging Phase-3 loop):** same path-based Overwrite-vs-Rename split. Rename `fs::rename`s the source onto the reserved name (removing the placeholder first for a directory source, which can't rename over a file). Overwrite keeps the existing `safe_overwrite_dir` behavior for type mismatches.
- Pinned by `type_mismatch_rename_tests.rs` (copy + move, both directions, plus uniqueness-escalation cases that prove the TOCTOU reservation still escalates `name (1)` → `name (2)`).

**Volume-side Rename reserves the name on local-FS dests (TOCTOU guard).** `volume/naming.rs::find_unique_volume_name` is the volume sibling of `conflict::find_unique_name`. The candidates it walks come from `conflict::NameCandidates` (see `../DETAILS.md`), the same sequence the local-FS namer walks, so a volume dest numbers identically and `photo (1).jpg` continues to `photo (2).jpg`; this function owns only the reservation. When the destination volume is local-FS-backed (`dest_volume.local_path().is_some()`), it reserves the chosen `name (N)` with an `O_CREAT|O_EXCL` placeholder at the resolved local path (via `resolve_local_path`, which mirrors `LocalPosixVolume::resolve`) and escalates on `AlreadyExists`. The streaming writer (`LocalPosixVolume::write_from_stream` → `std::fs::File::create`) truncates the placeholder, so the new bytes land on it cleanly — there's no `COPYFILE_EXCL` in the volume copy path to refuse the overwrite. For backends without exclusive-create semantics (MTP / SMB / InMemory, `local_path()` is `None`) it falls back to the non-atomic `exists()` probe, with a documented narrow residual window. Pinned by `volume/conflict.rs::{local_fs_rename_reserves_the_chosen_name_on_disk, local_fs_rename_keeps_extension_in_the_right_place, non_local_dest_does_not_reserve_a_placeholder}`.

**Cross-type overwrites (file↔folder).** Both copy and move route Overwrite-with-type-mismatch through `overwrite::safe_overwrite_dir`:
- Local copy `copy_single_item`'s parent-creation site: when the source tree wants a directory at a path holding a file (folder→file overwrite), the closure does `create_dir_all` while the helper sets the file aside as `<name>.cmdr-temp-<uuid>`. The symlink branch's file→folder overwrite goes through the same helper.
- Local move `move_with_rename` / `merge_move_directory`: when `resolve_conflict` returns Overwrite for a type-mismatched pair, the closure does `fs::rename(source, target)`.
- Volume copy/move via `volume::conflict::apply_volume_conflict_resolution`: a type swap can't temp-rename across backends, so **cross-type** Overwrite deletes the dest first (`remove_tree(…, UserChoseOverwriteAcrossTypes)` for folder dests, `Volume::delete` for file dests) before the streaming writer / recursive copy lands the source. **File→file** Overwrite does NOT delete first — it uses the safe-replace temp+finalize path (see "Cross-volume file→file Overwrite is a safe-replace" below). Same-type dir-vs-dir still skips the delete to honor the merge-not-replace guarantee. Pinned by `volume/copy_tests.rs::test_volume_overwrite_{file_over_existing_folder,folder_over_existing_file}`.

**Copy strategy selection** (`copy_strategy.rs`). `select_local_copy_strategy` names the mechanism, `copy_file_using` runs it:
- macOS, same APFS volume → `LocalCopyStrategy::AppleClone`, `copyfile(3)` with `COPYFILE_CLONE` for instant clonefile
- macOS, everything else → `Chunked` (`chunked_copy_with_metadata`, 1 MB chunks, cancellation between chunks)
- Linux, network → `Chunked`
- Linux, local → `KernelCopyRange` (`copy_single_file_linux`, `copy_file_range(2)`, reflink on btrfs/XFS)
- Other platforms → `StdCopy`, the `std::fs::copy` fallback

The split exists so the landing discipline is written once instead of once per branch, and so a test can drive a mechanism the machine's own filesystems would never select — on a Mac every tempdir pair is one APFS volume, so without `copy_file_using` nothing would ever exercise the chunked branch that runs for an external drive or a Finder-mounted NAS.

### Self-collision (duplicating in place)

**Decision.** A top-level source that would land on ITSELF is a request to duplicate, never a conflict. Copy redirects it to a free ` (N)` name and proceeds; move writes nothing and counts the item done. Neither consults the user's conflict policy, and neither prompts. Why the suffix is ` (N)` rather than Finder's localized `copy` word: `../DETAILS.md`, the `unique_name.rs::numbered_name` bullets.

**A duplicate is same-volume by definition**, so on APFS it takes `LocalCopyStrategy::AppleClone` (§ "Copy strategy selection") and a 10 GB video duplicates instantly, at no extra space. Worth not losing: anything that routes a duplicate through a rewrite rather than the ordinary copy path pays full bytes for what the filesystem gives away.

**Why identity rather than paths.** `validation.rs::is_same_file` compares `dev+ino`, which settles a symlinked parent, a case-differing path on APFS, and NFC/NFD normalization in one comparison. It uses `fs::metadata`, which FOLLOWS symlinks — that is what makes the symlinked-parent case work, so ❌ don't "fix" it to `symlink_metadata`. (`rename/` answers a narrower question with `symlink_metadata` because it must treat a symlink as its own entry; the two are deliberately different.)

**The guard and the identity rule are one change, and a self-collision never reaches the conflict machinery.** An operation-level lexical verdict (`source.parent() == destination`) answers the wrong question at the wrong altitude: it refuses a whole batch over one item, and it misses every non-lexical route to the same file. Every answer the conflict machinery can give for a self-collision is wrong, so a guard removed without the identity rule in the same change is a data-loss change: `Overwrite` (reachable from a configured default or an apply-to-all latch on an earlier real clash) sends the item through `stage_and_land_file`, whose replace path sets the destination aside and deletes it — when destination IS the source, that leaves same name, same bytes, a NEW inode, broken hard links, and a reset birth time; `Overwrite` on a directory is worse, since `safe_overwrite_dir` recreates the source empty and the copy then walks a file list pointing at the vanished original; `Stop` shows "this file conflicts with itself" with identical size and mtime on both sides; `Skip` silently does nothing.

**Copy asks per TOP-LEVEL source, before the loop.** `copy_files_with_progress_inner` walks `sources` right after the transaction is opened and seeds `dir_remap` with `<dest>/<name>` → the free name. The loop below it iterates the scan's LEAF files, and in a same-folder folder copy every leaf is its own self-collision, so asking per leaf would scatter `a (1).txt` through the original `docs/` instead of producing `docs (1)/`. Nothing else has to change: `apply_dir_remap` already runs at every per-file destination (`single_item.rs`, `scanned_dirs.rs`), and a plain file collapses into the same rule because `starts_with` matches a path against itself. The scanned-dirs pass reading the same map is what lands an EMPTY folder's duplicate, which has no `FileInfo` to hang on.

**How the name gets claimed.** A **file** uses `conflict::next_available_name` and reserves nothing on disk: the picked name is free, and the non-overwrite landing refuses an occupied destination via `RENAME_EXCL`, so a racing writer produces a loud failure rather than a clobber. Deliberately not `find_unique_name`: its `O_CREAT|O_EXCL` 0-byte placeholder would sit at the destination, `path_exists_or_is_symlink` would call it occupied, and `copy_single_item` would raise a conflict against it: the exact prompt this rule removes. A **directory** uses `conflict::create_unique_dir`, whose `create_dir` loop IS the reservation (the directory analogue of the file placeholder), and the claimed path is recorded on the transaction and in `created_dirs`.

**The batch remembers what it handed out** (`state.claimed_names`, a `unique_name::ClaimedNames`; see `../../DETAILS.md`). Both pickers consult it and record their pick, because a probe alone can't see a name whose bytes haven't landed: selecting `photo.jpg` and `photo (1).jpg` and duplicating both sends the first past the taken `(1)` to `photo (2).jpg` and starts the second's sequence at that very name. Without the ledger both land in `dir_remap` pointing at one path, the first copy creates it, and the second finds its destination occupied and drops into `copy_single_item`'s ordinary conflict path: a prompt naming a file Cmdr just made, or, under `Overwrite` or an apply-to-all latch, two requested copies silently becoming one. The volume namer reads the same ledger for the same reason, and needs it harder: its concurrent driver resolves several top-level sources at once, and a directory there never reserves at all (`volume/DETAILS.md`). Pinned by `self_collision_tests.rs` on both engines (`two_sources_in_one_series_duplicated_together_get_a_name_each` and the reverse-order three-source case; `volume/self_collision_tests.rs::duplicating_a_whole_series_at_once_gives_every_source_its_own_name` uses three sources precisely to reach the concurrent driver).

**A pre-known conflict never bulk-skips a duplicate.** The pre-flight matches by NAME, so a same-folder paste lists every one of its own sources as conflicting; under `Skip` the bulk-skip prelude would drop them all and the duplicate would silently do nothing. The prelude excludes the sources the redirect covers.

**Move drops the item above the same-FS / cross-FS split**, in `move_files_with_progress_inner`, so neither engine can ever see a self-collision. `move_with_rename` would otherwise hand a directory to `merge_move_directory`, which threads `dest_dir` down through recursion and would self-merge the tree (every leaf renaming onto itself or getting shuffled aside to `name (1)`, depending on policy). `move_with_staging` is reachable for a self-colliding source only in a mixed batch where another source sits on a different filesystem, and there it is destructive: Phase 3 lands the staged copy over the original and Phase 4 then deletes that source, which is now the landed file. Dropped items still emit `write-source-item-done` as `Done` with `source_removed: false`, and both engines add them to `files_processed` so the completion tally matches what the user asked for.

**The pre-flight scan has to give the same answer, or the feature is unreachable.** `TransferDialog` runs a top-level conflict check before confirm (`scan_volume_for_conflicts`), a single destination listing matched by NAME, so a same-folder copy comes back with every source listed as its own conflict: the dialog announces a conflict count, shows the overwrite/skip/rename radios, and hands the backend a pre-known-conflict list naming every source. `scan_volume_for_conflicts_within` (`commands/file_system/volume_copy.rs`) drops those collisions with `routing::transfer_would_land_on_its_source`. It lives one level ABOVE the per-backend `scan_for_conflicts`, which is handed `SourceItemInfo` (a name and a size, no source path) and so cannot answer at all; widening that DTO would mean touching three backends and every test double for a question one place can answer. That one place has to fork the way `copy_between_volumes` forks, because the two engines answer identity differently: both-local goes to the local engine and `is_same_file`'s `dev+ino`, everything else to the cross-volume engine and `is_the_same_item`'s one volume, same parent, folded leaf. The filter is inert when the caller sends no `source_paths`, which is the pre-`sourceVolumeId` name-only shape.

**A dry run answers it too**, per top-level source in `scan.rs::dry_run_scan_internal`: a self-colliding subtree lands under a name nobody holds, so nothing in it can clash and the walk carries a `lands_on_itself` flag that suppresses both conflict checks. Its files and bytes still count, the same way both engines count a self-colliding item in `files_processed`. No production frontend caller sets `dryRun` today; it's reachable over IPC and MCP.

**Rollback needs nothing new, and is pinned anyway.** A duplicate is entirely the operation's own work: `created_files` / `created_dirs` hold only paths it created, so rolling back removes `photo (1).jpg` and can never reach the original. Note the shape of the IN-FLIGHT rollback: `rollback_with_progress` deletes `created_files` only, so a rolled-back FOLDER copy leaves its (now empty) directory skeleton behind, duplicates included. The journal-driven reversal doesn't: `created_dirs` is journaled as `dir` rows on every terminal path, and it removes them empty-only after the files.

### Who may commit a `CopyTransaction` bare, and why exactly one caller may

`commit_journaling_created_dirs` (`copy/mod.rs`) is `record_created_dirs(op_id, &transaction.created_dirs)` followed by `transaction.commit()`, journaling the recorded paths VERBATIM. That is right for every `copy/` arm, whose created dirs already sit where they will live.

`move_with_staging` (`move_op.rs`) is the one caller that commits bare, and it has to. Phase 3 renames the whole tree out of `.cmdr-staging-<op>/` into the destination, so by commit time every path on the transaction names a directory that no longer exists. It therefore maps `created_dirs` through the same staging→final `remap` its leaf rows got, journals THOSE, and only then commits. Routing it through the helper would journal `.cmdr-staging-<op>/…` paths instead, and a reversal reading them would find nothing to remove and leave the moved folder's empty skeleton standing at the destination. That is the bug this effort fixed; the `CLAUDE.md` rule names the exception so it doesn't get "cleaned up" back into existence.

**The end state that would delete the rule** (not built, and deliberately out of scope for a change freeze): give the transaction ONE commit entry that takes the rebase, `commit_journaling_created_dirs(op_id, rebase)`, with the copy arms passing identity and `move_with_staging` passing its `remap`, and restrict `CopyTransaction::commit` so nothing else can reach it. The staging rebase stops being an exception and becomes an argument, no arm can commit without journaling, and the `❌` line goes away. A weaker "proof token" variant (a value `record_created_dirs` returns and `commit` demands) only proves SOMETHING was journaled, not that the right paths were, so it wouldn't have caught this bug.

**Two sources sharing a basename stay consistent with the ordinary merge.** `dir_remap` is keyed by destination path, so pasting `/a/docs` (a self-collision) and `/b/docs` into `/a` lands both in `/a/docs (1)/`, merged. That's the same answer the engine already gives for `/a/docs` + `/b/docs` into `/c`, and `/a/docs` itself is untouched either way.

Pinned by `self_collision_tests.rs` (both engines, plus the `Overwrite` and apply-to-all-latch data-safety cases), by `commands/file_system/volume_copy.rs`'s own tests for the pre-flight filter, and by `scan_tests.rs` for the dry run.

### Local copies stage

**Decision**: every local file copy writes to a `.cmdr-tmp-<uuid>` sibling of its destination and takes the real name by one same-directory `rename(2)`, whether or not a conflict made it an overwrite. `overwrite::stage_and_land_file` owns it and every `LocalCopyStrategy` arm goes through it, handing it a closure that puts bytes at a path.

**Why**: only the overwrite case staged before. A fresh copy wrote straight to the final name, so copying a 4 GB file to an external drive and force-quitting halfway left a truncated file wearing the user's real filename, indistinguishable from a complete one — the same failure the cross-volume staging work eliminated, still live on the most ordinary path in the app, and reachable by a crash or a power loss too. It is also what makes ABANDONING a worker safe: a thread wedged in a `read`/`write` we can't interrupt is writing to a temp nobody will rename, so the quit deadline can walk away from it.

Two latent overwrite holes closed with it: the macOS non-APFS branch and the Linux network branch both ignored `needs_safe_overwrite` entirely and truncated the destination in place.

**The landing refuses to clobber when it isn't an overwrite.** Moving the create off the destination name lost something: a plain POSIX `rename` replaces silently, where the direct create's `COPYFILE_EXCL` / `O_EXCL` refused. `overwrite::rename_no_replace` restores it with `renamex_np(RENAME_EXCL)` on macOS and `renameat2(RENAME_NOREPLACE)` on Linux, degrading to a check-then-rename on a filesystem that supports neither (racy, but strictly better than an unconditional clobber). The case it defends is a file appearing between conflict resolution and the landing.

**On error the temp goes.** A failed write, a failed aside, or a failed landing removes the temp and deregisters it: the bytes there are a partial, or at most a complete copy whose source is still on disk (the item reports failed, so no source is ever deleted). This differs from the async cross-volume `StagedWrite::commit`, which deregisters BEFORE landing because a landing that fails there can leave the only complete copy of the new bytes. Locally the landing is one synchronous syscall, so that window doesn't exist.

**Cost**: one extra rename per file, and it doesn't show. 2 000 × 4 KB local files measured a 394-400 ms median against a 372-449 ms baseline (`transfer/copy/copy_tests.rs::local_copy_bench_many_small_files`). Pinned by `volume/copy_crashsafe_tests.rs` § `local_staging`.

**Background cleanup is best-effort.** `remove_file_in_background` and `remove_dir_all_in_background` run on detached threads (used for temp/backup file cleanup, not for user-visible rollback). If the network mount disconnects or the app exits, partial files or staging directories may remain on disk. These use the `.cmdr-` prefix, so they're recognizable.

## What a transfer says about each top-level source

Both drivers emit `write-source-item-done` per top-level source, carrying an outcome as well as `source_removed`
(`../DETAILS.md` § "Per-source outcomes"). Three of the four emit points are non-obvious:

- **A copy bulk-skipped by a pre-known conflict** emits `Skipped` from the bulk-skip prelude, before `SourceItemTracker`
  is built. Those sources are removed from `scan_result.files` up front, so nothing later in the run would ever speak
  for them and a caller tracking per-source outcomes would wait on a verdict that isn't coming.
- **A cross-filesystem move emits TWICE for one source**: `Done` with `source_removed: false` when it finishes staging,
  then either `Done` with `true` from the source-delete phase or `Skipped` when the rename phase left the source
  standing. Staging succeeding says nothing about where the item ended up, which is why the LAST event is the verdict.
- **A same-filesystem move's `source_removed` is an `lstat`, not an inference.** A directory MERGE whose children were
  partly skipped leaves the source directory on disk, so the flag is read rather than derived from the operation type.
  Pinned by `move_op_tests.rs`'s `a_same_fs_merge_that_skipped_a_child_reports_the_source_still_there`.

## Durability (flush before reporting complete)

Copy and move don't report `write-complete` until the freshly written destinations are durable on disk — "complete" means "you can eject now," not "buffered in the page cache." Two layers:

1. **Per-file, as it completes.** `chunked_copy.rs` calls `dst_file.sync_data()` (fdatasync) on each file before returning. On a long transfer, a crash mid-batch leaves every already-completed file safe.
2. **End-of-op pass.** Before `write-complete`, the copy/move handlers call `durability::flush_created_destinations`, which emits a `Flushing`-phase `write-progress` event (the FE renders **"Writing the last piece..."** instead of a bar frozen at 100% — see the FE doc), then `fdatasync`s every recorded destination, plus a best-effort `fsync` of each distinct parent directory so the rename-into-place is durable. It reuses `CopyTransaction.created_files` and skips an `already_synced` set (chunked-synced files + clonefile/reflink dests, which are CoW-shared and moot to flush).

**Per-path specifics:**
- **Local copy** (`copy_files_with_progress_inner`): `copy_single_item` populates `already_synced` from `StrategyCopyOutcome::already_durable`; the pass flushes only the strategies that don't flush themselves (Linux `copy_file_range`, the std fallback). On macOS the pass does no extra `fdatasync` (clonefile moot, chunked already synced) — it exists for the `Flushing` UI state.
- **Cross-FS move** (`move_with_staging`): Phase 2 copies to staging (records staging dests), Phase 3 renames staging → final. By flush time the staging paths are gone, so both `created_files` and `already_synced` are remapped from the `staging_dir` prefix to the final `destination` prefix and the FINAL per-file dests are flushed. This is also what makes the Phase-3 renames-into-place durable, including the `throwaway_tx` rename that isn't in the real transaction.

**Decision**: `flush_created_destinations` runs BEFORE Phase 4's `delete_sources_after_move`, not after.
**Why**: The source originals are the only other copy of the data through Phase 3. If the source delete ran first (the historic order: Phase 4 → Phase 5 → flush), a power loss in the gap between the delete and the final dir-entry fsync could leave the file absent from its final path (the Phase-3 rename-into-place not yet durable) AND the source already gone — recoverable only as orphaned blocks or a stray `.cmdr-staging-*` entry, at neither expected name. Flushing first upholds the move invariant "never delete the source if the destination isn't fully in place," matching the cross-volume move (`volume/move.rs`, which finalizes before deleting the source). Zero happy-path cost: Phase 2 already `sync_data`d every file's bytes, so this only reorders the cheap dir-entry fsync ahead of the delete. Cancellation handling is unchanged — Phase 4 still owns the in-loop cancel check and the `write-cancelled` emit; staging cleanup (Phase 5) still runs after flush so it never races the final-path reads. Pinned by `move_op_tests.rs::cross_fs_local_move_flushes_final_dests_before_deleting_sources` (a custom sink snapshots that the source still exists at the instant the `Flushing` event fires).
- **Same-FS move** (`move_with_rename`): a rename moves no data, so the pass `fdatasync`s the moved files (cheap — already durable) and their parent dirs to make the new directory entries durable. Emits the `Flushing` event too, so the UI is consistent across both move kinds.

The flush is best-effort on error (logged under `target: "write_durability"`, not propagated): the bytes are already written, and failing the whole op at the final flush is worse UX. Delete and trash don't flush at all — see `../delete/CLAUDE.md`.

## File writes are staged (no byte-incomplete file at its final name)

**Decision**: a cross-volume file write streams into a `.cmdr-tmp-<uuid>` sibling and is renamed onto its final name
only after its last byte, whether or not a conflict made it a safe-replace, unless the write is structurally incapable
of leaving a partial there (`volume/DETAILS.md` § "The single-shot exemption"). `staged_write.rs` owns it;
`stream_pipe_file` and the
sequential extractor are the two write sites.

**Why**: the conflict layer only staged a file→file Overwrite. A NEW file has no conflict, so it streamed straight to the
destination path — and a transfer killed mid-stream left a truncated file wearing the user's real filename, which is
exactly what the 2026-07-31 wedge did to two phone backups (one at 0 bytes, one truncated at 4 MiB;
`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Backends do delete their own partial on an error return,
but a force-quit, a panic, or a dropped future runs no error path at all. Staging makes the guarantee structural rather
than dependent on cleanup running.

**Who stages.** `WriteStaging::AlreadyStaged` means the caller already minted the temp (the conflict layer's
safe-replace, which additionally keeps the ORIGINAL in place until the temp is complete) and lands it itself; staging it
again would just yield a `foo.cmdr-tmp-A.cmdr-tmp-B`. Every other write is `WriteStaging::Stage`. Each call site derives
it identically via `volume::strategy::staging_for(&replace_after_write)`, so there is one rule, not four. Both write
sites then run that choice through `resolve_staging`, the single place a `Stage` can become `SingleShot`.

**Landing** (`staged_write::land`) renames FIRST and only clears the final name if that rename said something is in the
way. `finalize_safe_replace` is the other way round because there the original is known to be in the way; here it
usually isn't, and a speculative delete would burn one extra round trip per file. The name can still be taken (a
`Rename` resolution's `O_EXCL` placeholder, a cross-type Overwrite whose dest delete failed, a racing writer), which the
second attempt covers.

❗ **Only `VolumeError::AlreadyExists` earns the delete.** A rename over a network backend fails for plenty of reasons
that say nothing about the destination: a session that dropped, a server that refused, SFTP v3 folding an errno into its
one catch-all code. Clearing on every `Err` deleted a file the user still had and reported the blip that "justified" it,
and it also let a backend that can delete but not rename destroy the destination and then answer `NotSupported`. Pinned
by `staged_write.rs::{a_rename_that_failed_for_another_reason_leaves_the_destination_alone,
a_backend_without_rename_does_not_delete_what_it_cannot_replace}`, which inject a failing rename through
`InMemoryVolume::with_rename_failing`.

**Finding the litter.** A staged temp is listed in `state.in_flight_temps` for exactly as long as it is a PARTIAL:
`commit` removes it before landing, so a temp that holds committed data after a failed landing is never in the set and
can never be swept (the contract `finalize_safe_replace`'s caller comment describes, now enforced by construction
rather than by a `cleanup_temp` flag). Whatever is still listed when the driver's loop ends belongs to a task that was
DROPPED mid-write — the concurrent driver drops the rest of its window on cancel and on the first failure — so
`volume::cleanup::clean_abandoned_staged_writes` removes those, and the deep-merge children that were never tracked at
all are now covered too.

Registration goes through `write_operations::in_flight_temps`, which keeps the operation's in-memory list AND a
process-wide log in the app data dir. Local copies register there too (`overwrite::stage_and_land_file`), so
`in_flight_temps` is no longer cross-volume-only — though only the cross-volume drivers run
`clean_abandoned_staged_writes`, since a local op's temps are registered and deregistered synchronously and anything
left is a thread that never came back.

**Crash recovery, two sweeps.**

- **The recorded ones, at startup, with NO age gate.** `in_flight_temps::init_and_sweep` (called from `lib.rs` before
  any copy can start) replays the persisted log and removes what an earlier run left in flight. The age gate below
  exists to protect a temp a CONCURRENT Cmdr is streaming into; a path this app recorded when it minted the UUID in it
  needs no such protection, and the instance lock already keeps two processes off one data dir. It only ever removes
  paths carrying the scratch marker, so a corrupted log can't become a delete-anything primitive. The deletes run on
  their own thread, ❌ never inline in `setup`: a recorded partial can sit on a Finder-mounted NAS that stopped
  answering, and an `unlink` there blocks for a minute or two, which on the startup thread reads as an app that won't
  launch. Granularity and why there is no fsync: `in_flight_temps.rs` module docs.
- **The rest, on the next transfer into that directory.** `volume::cleanup::reap_stale_transfer_temps` runs once at the
  start of each cross-volume copy, over the destination directory only: one `list_directory`, then a `delete` for each
  `.cmdr-tmp-*` FILE whose mtime is at least `STALE_TEMP_MIN_AGE` (1 hour) old. The age gate is what makes it safe
  against a concurrent instance — a live staged write touches its temp every chunk, and even a destination-side
  foreground park is capped at a second — and an entry with no reported mtime is spared. It mirrors
  `archive_remote_edit::reap_remote_temps`. This is the backstop for anything the log missed (a power loss, a
  non-UTF-8 path); a leftover deeper inside a copied subtree waits for a transfer into that directory, and there is no
  global filesystem sweep and there shouldn't be.

**Cost**: one extra rename per staged file. On SMB that is one round trip, which roughly doubles the wire cost of a file
that would otherwise take the compound CREATE+WRITE+FLUSH+CLOSE fast path — the exemption below is what keeps a
10k-tiny-file copy to a NAS from paying it. A destination that can't rename can't stage; `stream_pipe_file` then re-runs
the file unstaged (a `NotSupported` landing), which no production backend triggers — Local, SMB, and MTP all rename.

Pinned by `volume/copy_staged_write_tests.rs` (abandon the copy future mid-stream — the in-process equivalent of the
force-quit — and assert nothing sits at a final name, for a fresh copy, an overwrite, and a merge child) and
`staged_write::tests`.

## Pause reaches between chunks (cross-volume streaming path)

**A paused cross-volume copy stops MID-FILE, between chunks — not only between files.** The per-source loop top in the serial drivers parks between files (after the `is_cancelled` check). But a single large file (e.g. an MTP→local import) is one source: gating only at the loop top would let it stream to completion while the UI shows "Paused" (the confirmed bug). The fix is `transfer/checkpoint_stream.rs`'s `CheckpointStream`, a `VolumeReadStream` decorator `volume::strategy`'s `stream_pipe_file` wraps the source stream in. Its `next_chunk()` runs a between-chunk checkpoint once per chunk before delegating: (1) `pause_gate.wait_while_paused_async(&intent)` parks while paused (returns the instant cancel is observed — cancellation wins), then (2) `tokio::task::yield_now()` so a long transfer doesn't starve foreground tasks (listings, navigation, progress) on the runtime.

**Why a stream decorator, not a new trait param.** The per-chunk progress callback (`on_progress`) is sync (`ControlFlow`), so it can't `.await` to park or yield. The chunk loop lives inside each backend's `write_from_stream`. Wrapping the SOURCE stream injects the async checkpoint into that loop at the single production wiring point (`stream_pipe_file`) without touching the `Volume` trait or any backend — the loop already awaits `next_chunk()` once per chunk.

**Data safety.** The checkpoint sits at a chunk boundary: the previous chunk is fully written and the next isn't yet read, so a paused op holds only its in-flight `.cmdr-tmp-<uuid>`, never a torn target. The wrapper only gates progress — it forwards each inner chunk untouched (no drop, double-write, or reorder) and forwards `total_size()` unchanged so the destination still sees the real size. Cancellation is NOT enforced in the wrapper: the backend's existing `on_progress` `is_cancelled` check after each write owns the cancel-then-cleanup ordering (drop the handle, remove the partial). Pinned by `volume::strategy::tests::streaming_copy_parks_mid_file_while_paused_then_resumes` (paused multi-chunk copy freezes, resume completes with correct bytes) and `streaming_copy_cancel_while_paused_mid_file_unblocks` (cancel-while-paused unblocks, ends `Cancelled`, leaves no partial).

**Pause for an MTP source (navigate the phone while a transfer is paused).** Park-in-place is correct for EVERY backend now, MTP included. An MTP read is a sequence of bounded ~8 MiB windows (`mtp/connection` § "Bounded-window reads"); between windows nothing is in flight and the one-per-device PTP session is free, so a paused MTP copy that simply stops starting the next window leaves the phone listable/navigable without releasing anything. `CheckpointStream` parks on `pause_gate.wait_while_paused_async` and, on resume, the next `next_chunk` reads the next window from the current offset — no `cancel_and_release`, no reopen-at-offset. The wrapper tracks `bytes_yielded` (== the destination temp's length); since parking leaves the offset alone, the next window reads `[bytes_yielded, …)` with no gap or overlap, and the destination's `write_from_stream` (and its safe-replace temp+rename) sees one continuous chunk stream. A cancel observed while parked lets the next chunk flow through to `on_progress`, so the cancel/cleanup contract is the park-in-place contract. Pinned by `volume::strategy::tests::paused_mtp_copy_parks_in_place_then_resumes_byte_exact` (pause freezes mid-file, never releases the source, resumes byte-exact with a single open at offset 0), `paused_mtp_copy_cancel_while_paused_keeps_no_partial`, and `unpaused_mtp_copy_streams_straight_through`.

**No release/reopen machinery.** This bounded-window model deliberately has NO `cancel_and_release`/reopen path — that obsolete "release the held session, reopen at the offset" machinery (the old `CheckpointReopen`/`ensure_open`) is gone, because there is no held session to release between windows. `Volume::pause_releases_read_stream()` is now `false` for every backend (kept only as a trait extension point), and `Volume::open_read_stream_at_offset` keeps its offset parameter (MTP implements it correctly) but the copy path only ever calls it with `offset == 0` via `open_read_stream`. The bounded-window read itself lives in `mtp/connection` (it needs `mtp-rs`'s `Storage::download_partial_64`, in `mtp-rs` 0.21.0 — the minimum `apps/desktop/src-tauri/Cargo.toml` pins).

### Foreground auto-yield (navigate the phone DURING a transfer, no pause)

The bounded-window mechanism and the why behind debounce/floor/Running-not-Paused are captured in this section and `mtp/connection/DETAILS.md` § "Bounded-window reads". Design history is in git (former `docs/specs/2026-06-25-bounded-window-mtp-reads-plan.md` and `2026-06-22-navigate-during-transfers-plan.md`).

The same park-in-place behavior is also driven by a SECOND trigger: foreground device work pending while the copy is RUNNING. A long MTP→local copy could otherwise keep re-grabbing the device lock window after window and starve a foreground listing/nav. The fix makes the per-window checkpoint a `background_yield_point`, exactly like the index scan does at its unit boundary: a transfer becomes a yielding background user of the per-device `DevicePriorityGate` (`mtp/connection/scheduler.rs`). Because the read is bounded windows that hold nothing between them, "yield" means simply **don't start the next window** until foreground drains — no session release, no reopen — gated on `foreground_pending` instead of the pause flag.

**The arm** (`CheckpointStream::auto_yield_to_foreground`, after the pause handling in `checkpoint`). It fires when ALL hold: the source opts in (`Volume::supports_foreground_yield()`, MTP and SMB — this is the enable-switch, NOT a release/reopen proxy), not cancelled, `bytes_yielded < total_size`, the min-progress floor is satisfied, and `Volume::foreground_pending().await` is true (an atomic load behind the device gate). Then it parks (debounces, below) and returns; the next `next_chunk` reads the next window from the current offset. The op stays **`Running`** the whole time — this is a transient DEVICE yield, not a user pause, so it must NOT touch `OperationIntent` or the manager's `LifecycleStatus` (the queue window's Pause/Resume button keys on those; flipping to Paused would misreport user intent). Byte exactness and the cancel-wins contract are the same park-in-place contract as pause; the arm only adds WHEN to park.

## Skipped work moves the bars, and stays out of the rate

A skip is progress: the child is decided, it will never be copied, and both bars have to reach their totals. So every
skip credits the same two counters a completed leaf does, and every skip also goes through
`WriteOperationState::note_skipped(files, bytes)`.

The two halves that keep it honest:

- **The event stays GROSS.** `state.rs::enrich_progress` samples the ETA estimator with `bytes_done` / `bytes_total` /
  `files_done` / `files_total` NET of `skipped_totals()`, while the emitted `WriteProgressEvent` keeps the gross
  numbers the two bars render. A skipped 4 GB file therefore advances the bar without reading as 4 GB/s of throughput.
  Remaining work is identical either way (`done` and `total` lose the same term), so this changes the RATE only, never
  the ETA's numerator. ❌ Don't reintroduce a "reseed the estimator baseline" call for this: the netting covers the
  bulk-skip preludes and the per-item skips with one rule, and a baseline reset can't express a skip that lands
  mid-window.
- **Every skip site calls it.** The bulk-skip preludes (`transfer_driver/sync_driver.rs`, `async_driver.rs`,
  `volume/copy.rs`'s concurrent one), the per-iteration skips (`TransferOutcome::Skipped`, `ConflictDecision::Skip`),
  and the deep-merge decline (`volume/merge.rs`'s `MergeChildDecision::Skip`, via `MergeCtx.on_file_skipped`). Miss one
  and it spikes the reported speed instead.

**`SerialLeafProgress::on_leaf_skipped` is THROTTLED, where `on_leaf_complete` is not.** The completion milestone
bypasses the throttle on purpose, so a big leaf's `N/N` always lands. A skip can't have that: a merge declines tens of
thousands of children back to back, and an unthrottled emit apiece is an IPC flood. The end-of-operation emit carries
the final numbers, so nothing is lost by letting the throttle eat the middle.

Why the deep-merge site is the one to guard hardest: a merge whose children all clash moves no bytes at all, so if its
skips don't credit the bars the dialog sits at `0 of N` for the entire run and reads as frozen. A user cancelled a
healthy transfer over exactly that (`ERR-AYVM4`).

## The stall signal

`TransferActivity` (`types.rs`) rides on every `write-progress` event: `in_flight`, `still_for_seconds`, and
`waiting_on`. It is attached in `state.rs::enrich_progress`, the one place every emit site already routes through, so
no signature grows and no caller has to remember.

**Why the backend classifies rather than the frontend.** The probe knows the distinction that matters — parked ON
PURPOSE (a user pause, a foreground yield that keeps the app responsive) versus genuinely stuck — because it holds each
task's phase. A frontend timer can only see "no events lately", which would call every deliberate yield a stall and
train people to ignore the warning. `OperationProbe::wait_reason` ranks them: a conflict prompt (`You`) outranks
everything because a person is being asked a question; a pause is authoritative from the pause gate; a device wait is
only claimed when EVERY in-flight task agrees, since one task still streaming means something else is holding things
up; otherwise `Unknown`, which is the shape the 2026-07-31 wedge took.

**A conflict prompt is read from the responder slot, NOT from task phases.** `wait_reason` and `watchdog_step` both
check `state.conflict_slot.is_awaiting()` first. `TaskPhase::ResolvingConflict` only ever covers a deep-merge
child, because TOP-LEVEL conflict resolution runs on the DRIVER, between tasks — so a scan of task phases misses the
common case entirely. Without that check, a transfer sitting on an unanswered overwrite prompt accrues stall time,
starts heartbeating after 3 s, and after `STALL_NOTICE_SECONDS` tells the user their transfer has stopped moving while
it is asking them a question (it reports `Unknown`, so the frontend's `you` suppression can't save it either). Pinned
by `a_transfer_waiting_on_a_conflict_answer_is_not_stalled` and
`the_watchdog_does_not_accrue_stall_time_behind_a_conflict_prompt`. The slot is authoritative: it is stored before the
`write-conflict` emit and taken when the answer lands, and it covers deep-merge prompts too.

**Why the watchdog emits.** Progress events are driven by chunk callbacks, so a wedged transfer emits nothing at all —
the UI keeps rendering the last event it received, confident ETA and all, for as long as the wedge lasts. That is
exactly what the incident's dialog did. Once the byte counter has been still for `HEARTBEAT_AFTER_SECS` (3 s), the
watchdog re-sends the last recorded event each tick with a fresh activity snapshot. The counters are deliberately
unchanged (nothing moved); only the activity is new. It goes through `emit_progress_via_sink`, so the ETA estimator
also sees the stillness and decays its own estimate to `None` rather than the FE having to special-case it.

**Movement is EITHER axis, and both counters are the ones on screen.** `watchdog_step` resets the stillness clock when `bytes_done` OR `files_done` changes. A merge declining thousands of children advances the file count with the byte total flat, and that is a transfer doing its job: judging on bytes alone called one healthy run stuck for a minute and the user cancelled it (`ERR-AYVM4`). `OperationProbe::bytes_done` / `files_done` read `state.last_progress_bytes()` / `last_progress_files()`: the aggregate totals of the newest `write-progress` event the operation published. The probe keeps NO counter of its own, and ❌ must never be given one again. It had one — an `Arc<AtomicU64>` handed in at registration — which only `volume/copy_concurrent.rs` ever fed. The serial path (fewer than three top-level sources, or any backend reporting `max_concurrent_ops() == 1`, so every MTP transfer) keeps its running totals in `SerialLeafProgress` and left that atomic at zero, so a perfectly healthy copy read as still and the dialog said "the transfer has stopped moving" from the 10 s mark to the end of the transfer, over a bar that was visibly climbing. Reading the published event instead makes "the watchdog measures the same number the user sees" true by construction rather than by review: every emit site already has to route through `state.rs::enrich_progress`, which is what stores it. Pinned by `a_transfer_publishing_progress_across_file_boundaries_is_never_called_still`, which drives three leaf boundaries through a real `SerialLeafProgress`.

Two consequences worth knowing. Movement resolution is the progress throttle (`progress_interval_ms`, 200 ms by default), two orders of magnitude finer than the 10 s notice and the 20 s log line. And the reading is `0` before the first progress event, which no registered operation can observe: `copy_volumes_with_progress` emits its opening `Copying` tick before it registers the probe.

**Thresholds, and why they differ.** `STALL_TICK` is 1 s (it sets the granularity of `still_for_seconds`, which the UI
reads). `STALL_AFTER` is 20 s for the LOG: a log line wants to stay rare. The UI speaks sooner
(`STALL_NOTICE_SECONDS` in the frontend), because a frozen bar with a confident ETA is a lie the moment it stops being
true. Both read the same `still_for_seconds`, so the dialog and the log can't disagree.

**Which transfers register a probe, and why the list is what it is.** All three STREAMING cross-volume paths:
`volume/copy.rs` for both of its drivers (concurrent and serial), and `volume/move.rs`. Each also binds its own
`CURRENT_TASK_PROBE` around `copy_single_path`, which is the half that matters most — a path that registers but doesn't
bind looks wired and isn't: its rows sit at `spawned` forever, `wait_reason` can never answer `Source` or
`Destination`, and `stream_pipe_file` can't arm a stall-abort. Without registration at all, `state.rs::enrich_progress`
misses the lookup and every event goes out with `activity: None`, so the dialog shows a frozen bar with a confident ETA
and says nothing. That is what a cross-volume MOVE did until it was registered, and a silent failure on the operation
holding the user's only copy of the data is worse than a noisy one. The move also sets `TaskPhase::Finalizing` on its
handle directly once the copy phase ends, because its safe-replace finalize and source sweep run after the task-local's
scope closes and a dump taken during them must not still claim it is streaming.

**`TaskPhase::Walking`** (label `"walking"`) covers the same gap for a DIRECTORY source: `merge_level` sets it at every
level while it lists the source and resolves what is already at the destination, before any of that level's files reach
the window. Without it a folder copy's row read `spawned` for the whole walk, so a dump of a healthy transfer looked
like a task that had never started. It is a WORKING phase, so `wait_reason()` is `None` (the level waits on source and
destination in turn, and naming either would be a guess) and `is_abortable_on_stall()` is false.

NOT registered, on purpose: the same-volume move (`volume/move_same.rs` + `rename_merge.rs`) is a rename, so it streams
no bytes and has no wedge of this shape; and the local-FS copy, delete, and trash keep no in-flight table at all, which
`WriteOperationState::activity` handles by falling back to its own pause gate and conflict slot. Pinned by
`volume/move_progress_tests.rs::{cross_volume_move_tells_the_dialog_what_it_is_doing,
a_cross_volume_move_in_flight_shows_its_source_and_phase}`, which sample the live table from inside the destination's
write — the one window where a row exists.

**The probe surface.** Two backends opt in; `LocalPosixVolume`, `InMemoryVolume`, and `ArchiveVolume` use the trait defaults (`false` / no-op) and never auto-yield.

- **`MtpVolume`** (which holds `device_id` and reaches the global `connection_manager()`): `supports_foreground_yield() → true`, `foreground_pending()` → `MtpConnectionManager::foreground_pending(device_id)` (the per-device gate's `foreground_pending()`, `false` if the device is absent), and `wait_until_foreground_idle()` → `MtpConnectionManager::background_yield_point(device_id)` (parks until the gate's pending count hits zero).
- **`SmbVolume`**: same three methods, delegating to `crates/cmdr-smb/src/volume/foreground_yield.rs`. **Decision/Why a timestamp, not a gate:** MTP can answer "a foreground op is in flight RIGHT NOW" because a PTP session is a single scarce resource with an explicit holder. SMB has no holder — every `SmbVolume` clone multiplexes frames over one connection — so there's nothing to count. The signal is instead "was there a navigation ON THIS SHARE in the last `TRANSFER_FOREGROUND_IDLE_THRESHOLD` (500 ms)", read off `priority::foreground`'s per-volume timestamp. The window is deliberately far shorter than the index scan's 2 s: a scan yield merely drops the listing budget, while a transfer yield PARKS, and `FOREGROUND_YIELD_DEBOUNCE` stacks another 400 ms on top. **Decision/Why per-volume scope:** a transfer is work the user asked for and is watching a progress bar for, so it must only stand aside for the share it actually contends with; the app-wide signal would park a NAS copy because the user clicked around a local folder. Starvation is already handled by `MIN_PROGRESS_FLOOR_BYTES`, so this layer needs no floor of its own. Pinned by `smb::foreground_yield::tests::*` and `smb_test::{supports_foreground_yield_is_on, foreground_pending_tracks_navigation_on_this_share_only}`.

**The DESTINATION-side yield (uploads: local → SMB).** The source arm above probes the SOURCE volume, so an upload's source (a local disk) never opts in and that arm is inert. A SECOND arm (`CheckpointStream::bounded_yield_to_dest_foreground`, right after the source arm in `checkpoint`) stands aside for the DESTINATION share instead, so an upload to a share the user is browsing doesn't make the pane sluggish. It fires when ALL hold: the destination opts in (`Volume::supports_foreground_yield_as_destination()`), not cancelled, `bytes_yielded < total_size`, the min-progress floor is satisfied (shared with the source arm; in practice only one arm is active per transfer since source XOR destination is the SMB side), and `dest_volume.foreground_pending().await` is true (the same per-share timestamp the source arm reads).

**Decision/Why a SEPARATE opt-in, not `supports_foreground_yield()`.** The read flag can't be reused for writes: an MTP upload streams chunks inside ONE `SendObject` PTP transaction, so parking mid-write would PIN the device session (the opposite of the read side, where a bounded window holds nothing between chunks). So MTP must NEVER opt into the destination flag, and it doesn't (default `false`). SMB writes are discrete SMB2 WRITE chunks with NO oplock or lease requested (`create_file_writer` → `OplockLevel::None`, no durable context; `crates/cmdr-smb/src/volume/streams.rs`), so a brief park between them is safe. Only `SmbVolume` overrides `supports_foreground_yield_as_destination() → true`.

**Decision/Why the destination park is HARD-CAPPED (data-safety bound, load-bearing).** Unlike the source arm's `wait_until_foreground_idle` (unbounded: a read holds nothing scarce between windows), an upload holds an OPEN SMB write handle across the park (the wrapped source read sits between two `writer.write_chunk` calls inside the destination's `write_from_stream`). An unbounded park under continuous browsing would let that handle sit idle long enough for the server/OS to reap it (`smb2` logs "idle teardown" when a quiet session is reaped), breaking the transfer. So `bounded_yield_to_dest_foreground` parks in short slices (`DEST_PARK_POLL_SLICE`, 50 ms) but never past `DEST_FOREGROUND_YIELD_HARD_CAP` (`volume/strategy.rs`, 1 s): at the cap it resumes and writes the next chunk, keeping the handle warm, then re-parks if the share is still busy. The share's own SESSION stays warm regardless (the user's navigation rides it), so the cap protects only the write handle. Resuming leaves the source offset untouched, so no desync; bytes reassemble exactly. The pure decision is `checkpoint_stream.rs::dest_park_continues(foreground_pending, parked_for, hard_cap)`, unit-tested against a fake clock like `priority::foreground::is_idle`. ❌ Don't convert this park to the unbounded source path, and ❌ don't raise the cap toward any server idle-timeout. Cancel-awareness is the same as the source arm (a cancel breaks the park loop promptly, and the next chunk flows to `on_progress` cleanup). Pinned by `volume::strategy::dest_yield_tests::{dest_yield_parks_before_next_write_then_resumes_byte_exact, dest_yield_hard_cap_bounds_the_park_under_continuous_browsing, dest_yield_cancel_while_parked_returns_cancelled_promptly, non_opting_dest_never_dest_yields}`, `checkpoint_stream::tests::*`, and `smb_test::supports_foreground_yield_as_destination_is_on`.

**Debounce + min-progress floor (load-bearing, named constants in `volume/strategy.rs`).** Each park suspends the copy, so naive per-window yielding thrashes under rapid nav. Two guards: (1) **debounce** (`FOREGROUND_YIELD_DEBOUNCE`, ~400 ms) — after foreground drains, stay parked until the device is quiet for the window; if a new foreground op arrives during it, re-park. A burst of listings is served as ONE suspension, not one park per window. (2) **min-progress floor** (`MIN_PROGRESS_FLOOR_BYTES`, ~4 MiB) — after a resume, the copy must move at least the floor before honoring the next yield, so continuous foreground nav can't starve the copy to zero throughput. The floor is currently SMALLER than one read window (`MTP_READ_WINDOW`, 8 MiB), so it's effectively "one window"; re-tune both together on real hardware. The floor baseline (`last_resume_offset`) resets at the end of the arm on every resume. Both durations/sizes are injectable fields on `CheckpointStream` (defaulting to the constants) so tests set debounce ≈ 0 and a tiny floor for determinism; the production constants are tuned against a real device.

**Cancel-awareness.** A cancel during an auto-yield must not be slept through and must not hang. The debounce wait (`sleep_cancel_aware`) slices its sleep and re-checks `is_cancelled` between slices; the `wait_until_foreground_idle` park is RACED against cancellation via `select!` + `poll_until_cancelled` (the gate only wakes when foreground drains, and a cancel doesn't clear the foreground signal, so it needs a separate waker). On cancel the arm bails out and lets the next chunk flow to the backend's `on_progress` `is_cancelled` cleanup — identical to cancel-while-paused. Pinned by `volume::strategy::tests::auto_yield_parks_before_next_window_then_resumes_byte_exact` (parks without releasing + byte-exact assembly + single open at offset 0 + op stays Running), `auto_yield_debounces_a_burst_into_one_park` (the copy stays parked across both listings in the burst), `auto_yield_min_progress_floor_prevents_starvation`, `auto_yield_cancel_while_yielding_keeps_no_partial`, the regression guard `non_mtp_source_never_auto_yields_for_foreground`, and `yield_capable_source_with_no_foreground_pending_never_self_yields` (no self-yield livelock — a yield-capable source with nothing pending must never park itself).

**Composition with the scan.** A transfer and the index scan both yield to foreground, so foreground always preempts both; the two background users don't priority-invert (lane budget 1 on the MTP device means the only foreground contender is a listing/nav/metadata op, never a second transfer). On MTP they share one signal (the device gate); on SMB they read the same per-volume timestamp through different thresholds and different responses — the scan throttles its listing budget (`indexing/network_scanner/scan_pace.rs`, which owns the reasoning for the whole yield-to-navigation design), the transfer parks. The runtime-level `tokio::task::yield_now()` (worker fairness) is a different layer and stays alongside this session-level yield.

**Scoped out: the local-FS sync chunk loop.** `chunked_copy.rs::copy_data_chunked` (and the macOS `copyfile` / Linux `copy_file_range` strategies) receive only the cancel `intent` atom, not the `PauseGate` (which lives on `WriteOperationState`). So a local→local copy of one huge file pauses only at the next file boundary, not mid-file. Threading the gate through `copy_strategy.rs` + the native paths is the v2 follow-up; the user-reported case is MTP→local (the volume streaming path), which is fully covered.

## Retrying one FILE, and the watchdog that ends a wait nothing else will

**Decision**: a transport blip re-runs the FILE from its first byte, up to three attempts with a short cancel-aware
backoff, and the retry lives in `stream_pipe_file` — the single place a file's bytes are streamed — not in any driver
above it. Policy in `retry.rs`.

**Why the file, not the operation**: a single failed write used to end the whole transfer. Twelve files into a 764-file
copy, one write that never came back took the other 752 with it
(`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). `smb2` surfaces a dead session as a typed error rather
than a hang — a send deadline, a response deadline, and `Error::ServerUnresponsive` for a link that answered nothing at
all — so the file that hit the blip can simply be run again and the batch carries on.

**Why THAT layer, and nowhere higher.** Everything a retry must not redo has already happened above `stream_pipe_file`,
which is what makes the retry safe by construction rather than by care:

- **Conflict resolution** ran on the driver (top-level) or in `copy_directory_streaming`'s merge walk (deep children).
  A retried file re-prompts nobody and re-decides nothing; it re-runs the write the user already approved. The other
  half of that decision — that the answer is not *stale* either — is deliberate: it was given for this file, in this
  operation, seconds ago, so re-asking would be the surprising behavior, not the safe one. Pinned by
  `volume/copy_retry_tests.rs::a_retried_file_never_re_asks_the_conflict_the_user_already_answered`.
- **The rollback ledger** (`CreatedPaths::record_file`, which stores the destination path AND the byte count it was
  written with — that size is the snapshot the journal row carries, and what makes a leaf inside a copied folder
  reversible), **the journal** (`journal::record_volume_transfer_source`), and **the per-file progress milestone** are
  all driven from the CALLER's `Ok` arm, so a file is recorded exactly once whatever it took. Pinned by `a_retried_child_is_recorded_in_the_rollback_ledger_exactly_once`.
- **The merge invariant** is a property of the level walk, and a retry re-walks nothing: it re-runs one child's write
  into a destination the walk already resolved. A retry can never turn a merge into a replace.
- **Overwrite** is untouched, and a retry can only improve it. A safe-replace's `finalize_safe_replace` runs after
  `copy_single_path` returns, so the ORIGINAL is intact through every attempt — including a file that fails all three.
  A cross-type Overwrite's delete-first already happened in `apply_volume_conflict_resolution`; the retry re-runs only
  the write, which is the half that can still rescue the data.

**Staging across an attempt boundary.** Each attempt re-derives its staging and mints its own `.cmdr-tmp-<uuid>`, so
nothing partial survives an attempt and no byte is written twice. `StagedWrite::abandon_attempt` clears the previous
attempt's target first — one case WIDER than the terminal `abandon`, because between two attempts the next writer is us
and not the caller: it also removes an `AlreadyStaged` caller temp, whose contents are only the partial we just gave up
on. Leaving it would make the next attempt depend on how each backend treats a write onto an existing path
(`LocalPosixVolume` truncates, `InMemoryVolume` refuses with `AlreadyExists`, MTP can make a duplicate object). A
`SingleShot` write is still left entirely to its backend.

**What is retryable, and why by type.** An exhaustive `VolumeError` match (`retry::is_retryable`), so a new variant
can't inherit a retry by falling into a wildcard — adding one forces the decision. Retryable: `ConnectionTimeout`
(where `smb2`'s `SendTimeout`, response `Timeout`, and `CreditStarvation` all land, via `ErrorKind::TimedOut`),
`DeviceDisconnected` (SMB reconnects on its own, so the next attempt runs on a new session; on MTP the device really is
gone and each attempt fails immediately, costing a bounded 1.25 s), `DeviceSessionReset` (its own doc says retrying
works), `StaleDestinationHandle`, and an `IoError` carrying one of the errnos `error_classification.rs` already maps to
`ConnectionInterrupted` — the case where a write onto an OS-mounted share reports the blip as an errno rather than a
typed backend error. ❌ Everything else is a decision or a fact about the data: re-running the write fails the same way
and only delays the report.

**❌ A cancel is never retryable, and outranks a retryable error.** `should_retry` checks `is_cancelled` first, the
backoff is a `select!` against `state.backend_cancel` (not a `sleep`), and a cancel that lands while an attempt is
failing on something we WOULD have retried is reported AS a cancel — otherwise the post-loop, which keys
`write-cancelled` off a `Cancelled`-shaped error, would log the user's own click as a failed transfer.

**Bounded, deliberately.** `MAX_ATTEMPTS = 3` and a 250 ms → 1 s backoff, so a file can add at most 1.25 s of waiting.
Three is the smallest number that survives the observed shape (a blip takes out the attempt in flight, the next runs on
a session the backend has since rebuilt, the third finds a healthy connection); past two failures the problem isn't a
blip. The bug this whole effort exists to kill is an infinite hang, so a retry loop that can spin forever would
reintroduce it in a new costume — the cap is pinned by call count, not by hope
(`a_destination_that_never_recovers_gives_up_at_the_attempt_cap`).

**Scope: streaming writes only.** `volume/sequential_extract.rs` (a compressed tar / solid 7z source) has its own write
site and deliberately gets NO retry. Its source is a one-pass decoder sitting at a fixed position, so a per-file
restart would mean re-decoding the whole archive prefix — the very O(n²) the sequential path exists to avoid. The
local-FS engine (`chunked_copy.rs`) is likewise untouched: a local write that fails with a transport errno is either a
dying disk or a network mount the volume layer should be handling, and neither is improved by retrying inside the sync
chunk loop.

### The watchdog ACTS

**Decision**: the mechanism to end a wedged wait is built and wired, and its trigger is **gated on positive evidence
that the connection is dead** — `Volume::connection_liveness() == Some(Dead)`, AND `STALL_ABORT_AFTER` (180 s) of zero
byte movement inside a backend call. On both, the watchdog trips the task's `stall_abort` token, the streaming write
races it, and the park becomes a typed `ConnectionTimeout` the retry above treats as a blip.

**Status today: the teeth are INERT, deliberately.** No backend in this workspace answers `connection_liveness` with
anything but `None`, so in production the watchdog does exactly what it did before — dumps the in-flight table,
heartbeats the UI's stall signal — and acts on nothing.

**Why gated, and why elapsed time is not allowed to be the evidence.** Telling "slow but alive" from "dead" needs a
keepalive: an ECHO the server either answers inside a window or does not. A silence deadline ALONE cannot do it,
because a large write to a loaded spinning-disk NAS is legitimately slow, and killing it trades a rare wedge for
frequent spurious failures — the worse bargain. Inventing a verdict out of elapsed time would reintroduce, one layer
up, the failure mode the keepalive exists to prevent.

**Why `smb2`'s keepalive still doesn't open this gate** (checked against 0.16.0's public API on 2026-08-02, and
re-checked against the pinned 0.18.1 on 2026-08-21 — `unresponsive_for` is still private to `Inner`, so nothing below
has changed; the decision to leave `SmbVolume::connection_liveness()` unimplemented is recorded here so nobody
re-derives it):

- **The keepalive deliberately produces no death verdict.** A missed probe means "no deadline extension" and nothing
  more, because a real NAS drops ECHO probes precisely when it is busy writing. `MetricsSnapshot::keepalive_failures`
  is therefore a count of non-events, and ❌ mapping it (or `keepalive_probes_skipped`, or a rising `sent_age`) to
  `Dead` is exactly the false positive this gate exists to avoid.
- **The one sound verdict is an error, not a state.** `Error::ServerUnresponsive { silent_for }` fires only when a
  request burned its whole deadline AND the connection put nothing on the wire meanwhile — sound, but it is handed to
  the caller and it tears the connection down. By the time a consumer could observe it, every waiter on that
  connection has already been failed, including the parked task this watchdog would have unstuck. The retry above has
  it.
- **What that leaves publicly readable is `Connection::is_disconnected()`** (a torn-down connection), which is a hard
  fact but the same consequence: true only after the write has already errored. Wiring it would add a `Dead` answer
  that arrives strictly later than the error the task is already getting.
- **The counters can't be reassembled into the verdict either.** They are monotonic per-connection totals with no
  timestamps, so reconstructing "the wire has been quiet for ≥ 3 probe intervals with work outstanding" from polled
  snapshots means re-deriving the crate's own internal `unresponsive_for()` from the outside — more machinery, still
  strictly later than the crate's own verdict, and no new coverage.

**To turn the teeth on**, `smb2` has to expose the conjunction it already computes internally as something
**pollable**: `Connection::unresponsive_for() -> Option<Duration>`, `Some(quiet)` only when the keepalive is armed AND
the wire has been silent for ≥ `LIVENESS_WINDOW_PROBES × keepalive_after` with a request outstanding — readable
WITHOUT a request having burned its deadline first and WITHOUT the connection being torn down, since that window is
the only place a Cmdr-side watchdog has anything to add. Then override `connection_liveness` on **`SmbVolume` alone**,
mapping that to `Dead`, its absence to `Alive`, and "no keepalive armed / nothing outstanding" to `None`. Nothing else
moves: the mechanism, the stillness window, the per-attempt re-arm, the guards, and the tests are all already here and
gated only on that answer. ❌ Do NOT then drop the stillness window and trust the verdict, and do NOT assume the 180 s
comes down at the same time — with a fallible verdict the debounce is doing real work rather than just waiting, so it
is a tuning call to make against the keepalive's measured false-positive behavior.

**Why the two conditions are ANDed, and why that is load-bearing rather than belt-and-braces.** The liveness verdict
this gate reads is a keepalive result, and a keepalive false-positives under exactly the load a transfer creates.
Measured against David's QNAP TS-464 (2026-08-02, `smb2`'s live-hardware suite): under heavy write load an ECHO probe
reported **`2 answered, 1 unanswered`** — a `Dead` verdict on a NAS that was demonstrably fine — while **five
consecutive runs on the same idle box reported `0 unanswered`**. So the signal is least trustworthy precisely when it
matters, and acting on it alone would kill healthy transfers to busy servers: the failure mode this whole gate exists
to prevent, one layer up.

The stillness window is what makes the pair sound. A NAS that drops a probe because it is busy writing has not ALSO
moved zero bytes for `STALL_ABORT_AFTER`; a genuinely dead session has done both. Each condition covers the other's
false positive. ❌ **Never collapse this to "trust the `Dead` verdict"** — the comment at the conjunction in
`transfer_probe.rs` carries the same warning, and
`transfer_probe::tests::a_task_that_keeps_moving_is_never_aborted` pins it (its probe reports `Dead` on every tick and
the moving task is still never touched; removing the movement check turns that test red).

**Why 180 s for the second condition.** It is a LAST resort even once armed: a backend that can bound its own waits
gets there first, so a dead SMB session errors on its own and the file's retry picks it up without the watchdog being
involved. Which `smb2` deadline actually does that, and why `SEND_TIMEOUT` is not the one: the doc comment on
`STALL_ABORT_AFTER` in `transfer_probe.rs`. What is left is the
case with no deadline anywhere: an OS-mounted share, a USB stack, a future backend that forgot. The number clears the
slowest HEALTHY gap between two byte reports, which is one chunk: a 1 MiB SMB read window needs a link under 6 KB/s to
take this long, an 8 MiB MTP window needs USB at 45 KB/s.

**The guards, each load-bearing** (the liveness verdict above is the first and most important; these are the rest):

- **Per-task, not per-operation.** A batch where any task still moves bytes leaves every other task's clock running on
  its own merits.
- **Only `OpeningSource` / `Streaming`** (`TaskPhase::is_abortable_on_stall`). Every park is deliberate and
  self-limiting — a pause ends when the user resumes, a source yield when foreground drains, a destination yield at its
  hard cap, a conflict when the human answers, a backoff on its timer — so aborting one would break something working
  as designed. A deliberate park also RESTARTS the clock, so parked seconds are never charged to the budget.
- **Never while cancelling.** Cancel and rollback own their teardown via the driver's drain deadline; a second
  abort path racing them would only make the wind-down harder to reason about.
- **Never for a `SingleShot` write** (the arm isn't even armed in `stream_pipe_file`). Those land in one indivisible
  frame at the file's FINAL name, and only the backend can tell "the server created the file and then refused the
  bytes" from "the file was already there". Aborting one from outside would add a client-initiated instance of the
  transport hazard the single-shot exemption already documents as unfixable.
- **Re-armed per attempt** (`TaskProbe::arm_stall_abort`, which also restarts the stillness clock). One task copies one
  top-level source, which for a directory is many files with their own attempts; a token that stayed tripped would
  abort every remaining child instantly and turn the retry budget into three no-ops.

**Residual risk, accepted (an abandoned write handle).** Ending the wait drops the destination's `write_from_stream`
future mid-write, so an SMB `FileWriter` goes away without `finish()` or `abort()` and its handle leaks until the server
reaps it on idle timeout. The `abandon_attempt` delete of the staged temp may then hit a sharing violation and leave a
`.cmdr-tmp-*` behind. This is the same trade the driver's drain-deadline abandon already takes, and it is
narrower here: the abort only fires on a path that has been silent for three minutes, so the session holding that handle
was not working anyway. What it cannot cost is data at a real name — the write was staged, so the user's filename was
never involved.

**The worst case, stated (once the gate is open).** A file on a proven-dead path would be aborted, retried, aborted,
retried, aborted: three `STALL_ABORT_AFTER` windows plus the backoff, so **about nine minutes** before the operation
reports the failure. Bounded where the incident was not (it needed a force-quit), and the UI heartbeats "stalled"
throughout, but it is the obvious knob to tune when the keepalive lands: cap the stall-aborts per file at one, or
shorten the window against the keepalive's own. Today it is unreachable — the gate is shut.

**A cancel does not reach a wedged write itself; a second tier does.** The backend learns about a cancel through its
`on_progress` callback, which a wedged write never calls, so on the SERIAL path a Cancel is only observed once the write
returns. That is what § "Two tiers of cancel" below exists for. The concurrent path bounds it independently too, by
dropping its in-flight futures at the drain deadline.

The gate itself is pinned by `transfer_probe::tests::a_connection_with_no_liveness_verdict_is_never_aborted` — a
volume answering `None` is never acted on however long it stays still, while the watchdog keeps reporting. That test is
the one guarding against a future change quietly re-arming the teeth on a timer; ❌ don't delete it when the keepalive
lands, re-point it. The tests that DO exercise the abort supply a `Dead` verdict through
`liveness_test_support::dead_connection_volume()`, because without one they would be asserting on a path production
cannot reach.

Also pinned by `transfer_probe::tests::{the_watchdog_ends_the_wait_on_a_task_that_stopped_moving,
a_task_that_keeps_moving_is_never_aborted, a_deliberately_parked_task_is_never_aborted,
time_spent_paused_does_not_count_toward_the_abort, the_watchdog_stands_down_once_the_operation_is_cancelling,
a_new_attempt_gets_a_fresh_signal_and_a_fresh_budget}` and, end to end,
`volume/strategy_retry_tests.rs::the_watchdog_ends_a_wedged_write_and_the_file_runs_again` (a write that never returns,
never errors, and never reports a byte; the watchdog ends it and the retry lands the file).

How progress stays honest across a retry, on both drivers: `transfer_driver/DETAILS.md`.

### Seeing it in a log

A silent retry would make "this file took three tries" and "this file never happened" the same log line, so every
attempt is visible: a `warn` per retry with the attempt number, the error, and the wait; an `info` when a file lands
past attempt 1; an error-level line with the full task row when the watchdog ends a wait; `TaskPhase::WaitingToRetry`
naming the backoff; and `retries=N` (plus `stall-aborts=N`) on every task row in `render_dump`.

### Not done here: continuing PAST a file that fails every attempt

A file that exhausts its attempts still ends the operation, exactly as before. Skipping it and carrying on would need a
terminal event shape that can say "finished, with N files missing", a frontend that shows which ones, and journal
semantics for a partially-successful op — and, more importantly, a product decision about whether a user wants 700
files copied with three quietly absent. That is a bigger change than M4.1 asked for and a worse default to guess at, so
it is deliberately left for David to call.

## Two tiers of cancel

**Decision**: stopping a transfer has two tiers, and they are not the same signal with different urgency. Tier 1 asks
the backend to stop; tier 2 stops waiting for it. Tier 1 is the default for every user-initiated cancel and tier 2 is
❌ unreachable from anything a person clicks.

**Tier 1, cooperative (`state.backend_cancel`).** The cancel travels to the backend through the per-chunk
`on_progress` callback, and the backend answers by dropping its own handle and deleting its own partial
(`volume/backends/local_posix.rs`, `crates/cmdr-smb/src/volume/streams.rs` — the `writer.abort()` + `delete_partial()` sequence). Writes are
❌ deliberately NOT raced against this token: dropping the write future would skip exactly that cleanup, on the healthy
path, for every cancel. Every Cancel button, the queue window's stop, a Rollback, and the frontend teardown walk are
tier 1 and stay tier 1.

**Tier 2, hard abort (`state.backend_abort`).** What tier 1 cannot do is bound the WAIT. A write that never calls
`on_progress` never sees the cancel, and `smb2`'s own deadline for a chunk is 30 s of server silence, stretching to
180 s while an ECHO keeps proving the connection alive (see the `STALL_ABORT_AFTER` doc comment in
`transfer_probe.rs`), so one chunk can hold a quit for minutes. Tier 2 is a deadline holder's answer: `strategy.rs::stream_pipe_file` races the source
open and the destination write against it, so the wait ends on our clock whether or not the backend comes back. Fired
only by `state::abort_write_operation` / `abort_all_write_operations`, which today means the quit deadline and nothing
else. Both fire tier 1 first — an abort is a cancel that ran out of patience, never a cancel with a shorter fuse — and
both work on an already-`Stopped` operation, which is the real sequence (cancel, wait a beat, abort what's left).

**What tier 2 costs, and where the cleanup goes instead.** The backend runs none of its own cleanup, and ❌ nothing may
go back through that connection to tidy up: the delete would hold the very deadline the tier exists to keep, a second
time. So the staged `.cmdr-tmp-*` stays registered in both `in_flight_temps` ledgers and
`in_flight_temps::init_and_sweep` removes it at the next launch, with no age gate, off the launch thread.
`volume::cleanup::clean_abandoned_staged_writes` stands down for the same reason. Nothing is ever left at a real name,
because the write was staged — which is what Q1 bought.

**Why the two phases it races are the source open and the write, and no others.** They are `TaskPhase::OpeningSource`
and `Streaming`, the same two the stall watchdog considers abortable, and for the same reason: every other await on the
path is either a deliberate, self-limiting park (a pause, a foreground yield, a conflict prompt, a retry backoff) or
short enough that the drain deadline covers it. The LANDING rename is deliberately left un-raced, so a task wedged
there is ended by the driver's drain deadline rather than by an abort mid-rename.

**Armed for a single-shot write too**, unlike the stall watchdog. The watchdog's exemption exists because an abort there
would be a *client-initiated* instance of the transport hazard, on a path that is otherwise fine. Tier 2 only ever fires
when the process is seconds from exiting, so dropping one indivisible compound frame produces exactly what the process
dying produces — the case `volume/DETAILS.md` § "The single-shot exemption" already accounts for.

**Cost on the happy path**: two already-live atomics polled per wakeup of the write future. No allocation, no timer, no
syscall, no backend change, and ❌ nothing added per chunk (the `select!` wraps the whole `write_from_stream`, which the
chunk loop lives inside).

Pinned by `volume/strategy_abort_tests.rs`: an abort ends a wedged write and a wedged source open promptly, never
retries the file, and leaves the partial registered for the sweep — plus
`an_ordinary_cancel_still_routes_through_tier_one_and_the_backend_deletes_its_own_partial`, which is the guard for the
invariant above and asserts all three of "the backend removed its own partial", "`backend_abort` never fired", and "no
temp left behind". `state_tests.rs` carries the negatives at the state layer
(`an_ordinary_cancel_never_fires_the_hard_abort`, `cancel_all_never_fires_the_hard_abort`).

## Cancel and rollback reach a parked driver

**Decision**: the concurrent copy driver observes `OperationIntent` on the await it actually sits on, and bounds how
long a cancelled operation waits for its tasks.

**Why**: `is_cancelled(&state.intent)` was consulted only in the spawn loop (`while in_flight.len() < concurrency`). A
driver whose window is full — or whose sources have run out — parks on `in_flight.next().await` and never returns to
that line while its tasks are parked. On 2026-07-31 that made Rollback a no-op: the intent was set, `write-cancelled`
never came, the dialog would not close, and the app had to be force-quit. An escape hatch that only works while the
thing it is escaping is healthy is not an escape hatch.

**How.** The `in_flight.next()` await is a `biased` `select!` against `state.backend_cancel.cancelled()`.
`backend_cancel` is the right signal because `cancel_write_operation` fires it on EVERY transition out of `Running`, so
one arm covers both Cancel and Rollback; `biased` makes the cancel win deterministically, and a task result that was
also ready is simply re-polled on the next pass. Observing the cancel arms `drain_deadline`; from then on the same await
is a `timeout_at`, so healthy tasks still get to finish (their results keep flowing through the normal handler, which is
what keeps the rollback ledger complete), while the drain deadline caps the wait.

**The deadline belongs to whoever asked for the wind-down** (`copy.rs::drain_deadline(aborting)`): 15 s for a
cooperative cancel, because a task that hasn't answered yet might still come back with its own cleanup done; 1 s once
the hard-abort tier is live, because that caller has already given up on exactly that and is holding a promise to a
person. It stays re-negotiable WHILE it runs: the quit sequence cancels first and aborts a beat later, by which point
the driver is already sitting on the cooperative deadline, so tier 2 has its own `select!` arm that cuts it short. That
arm is latched (`drain_shortened`) because a fired token is ready forever and an unguarded arm would spin.

**When the deadline fires** the driver logs at error level with the full `transfer_probe` dump — naming every task and
what it is awaiting — and breaks. Dropping `in_flight` aborts the parked futures, which is what makes an in-flight task
abortable at all: these are futures in a `FuturesUnordered`, not spawned tasks, so a drop is an abort. Their staged
partials are removed by `clean_abandoned_staged_writes` in the post-loop.

**Decision/Why abandon rather than hold**: an abandoned task can leave an open SMB write handle behind. Servers reap
idle handles on their own, and the alternative is the incident's outcome — the user's only way out being a force-quit,
which is what cost them two files. Getting the user unstuck outweighs some server-side handle churn. Nothing healthy
reaches the deadline (a task observes the cancel within one chunk), so hitting it is genuinely news and is logged that
way.

**Not covered**: the SERIAL path has no equivalent gap — it awaits each per-file transfer directly, so there is no
"driver parked while tasks run" state — but a serial transfer wedged inside a backend call still has no escape. Bounding
that would mean racing every per-file transfer against a timeout, which risks killing a healthy slow transfer, so it is
deliberately not done here.

Pinned by `volume/copy_cancel_tests.rs`: cancel and rollback each reach a driver parked on genuinely wedged tasks and
return, rollback undoes the file that already landed, and a task that never winds down is abandoned at the deadline
with nothing left at a real name.

## Key decisions

**Decision**: `copy_volumes_with_progress` scan phase calls `scan_for_copy_batch` once instead of `scan_for_copy` per source
**Why**: Network-backed volumes (SMB) pay 1 RTT per top-level source in the scan phase. A per-source loop serializes those: for 100 tiny files at ~60 ms RTT, ~5 s of pure stat latency before the copy phase can start. `scan_for_copy_batch` surfaces both the aggregate (file/dir counts, total bytes) and a per-path vec (is_directory, size) in a single trait call; the copy engine folds the per-path vec into its `source_hints` map and skips a per-source re-stat. `SmbVolume` overrides `scan_for_copy_batch` to pipeline N stats over one SMB session; measured 6.5× wall-clock win at 100 files (6.11 s → 947 ms) on a Tailscale link. `LocalPosixVolume` / `InMemoryVolume` inherit the default serial per-path loop; it's cheap for them. See `docs/notes/phase4-rtt-investigation.md`. Scoped to the scan phase, which wants the tree: ❌ the conflict pre-check deliberately does NOT use this method, because a batch of exactly one path takes a fast path into `scan_recursive` on SMB and SFTP and walks a single directory source's whole subtree. It stats top-level paths instead (`commands/file_system/volume_copy.rs`'s `stat_source_paths`).

**Decision**: a LOCAL volume's `max_concurrent_ops()` doesn't bound a REMOTE peer (`transfer_concurrency` in `volume/copy.rs`); the 32 ceiling stays.
**Why**: measured, not argued (`volume/copy_concurrency_bench.rs` against a real QNAP over direct smb2 at ~3.7 ms RTT, plus Docker Samba for corroboration; tables in `docs/notes/transfer-concurrency-window-bench-2026-08-02.md`). The two sides report that number for unrelated reasons: `LocalPosixVolume`'s `clamp(logical_cpus / 2, 4, 16)` is a guard-rail against spawning hundreds of tasks and says nothing about a peer, while `SmbVolume`'s IS the `network.smbConcurrency` setting and `MtpVolume`'s 1 is a single USB bulk transport. A plain `min()` let the guard-rail win on every Mac Cmdr ships to (8 on a 16-core M3 Max, 4 on an 8-core Air), so a setting advertised as 1-32 did nothing above 8. Worth 25% on an 8-core Mac (window 4 → 10, 4.700 → 3.522 s, spreads disjoint) and nothing measurable on a 16-core one — it is a defect fix, not a tuning change, and anyone measuring it on a high-core Mac will correctly see no difference and be wrong about the machines most users have. `Volume::operations_are_local()` (default `false`) carries the distinction; a remote cap always binds, in both directions, which is what keeps MTP's 1 routing a phone to the serial driver. ❌ Don't collapse this back to a `min()`, and ❌ don't "simplify" it by raising `LocalPosixVolume::max_concurrent_ops()` instead — that number also governs local→local copies, which nothing has measured. The 32 ceiling stays because the NAS plateaus at 12 on both corpus shapes, so it is nowhere near binding on the deciding target; only Docker keeps climbing past it, and that is the loopback artifact below.

**Decision**: nothing sizes the window off `smb2`'s credit budget, and there is no credit-based cap to add.
**Why**: the two count different things. Credits are connection-wide flow control over write FRAMES, granted and spent per request across everything sharing the session, while `transfer_concurrency` counts concurrent FILES for one `source → dest` pair. So there is no file-level number a credit budget could produce, and the choice between the two was never a choice. Credit starvation is already answered where it lands: it surfaces as `ErrorKind::TimedOut`, which `retry.rs::is_retryable` treats as a retryable `ConnectionTimeout` alongside the two smb2 deadlines. Credits stay a diagnostic: `SmbVolume::session_credits` for the soak suite (a leak bleeds it down between iterations), and `CreditInfo` in the SMB diagnostics dashboard.

**Decision**: the window was NOT the throughput story; the per-file destination pre-check was, and it is now skipped for a destination directory the operation created and answered from one listing for a merge (see "Answering the pre-check from one listing" above).
**Why**: on the NAS the curve is flat from window 12 up (12/16/24/32 medians 3.302 / 3.245 / 3.278 / 3.224 s for 500 × 16 KiB, spreads overlapping) and 8 → 16 buys only 1.16×; the few-large shape is link-bound (~97 MB/s, saturated gigabit) and shows no measurable difference at any width from 4 up. What the flat part IS: the concurrent spawn loop's `dest_volume.get_metadata(&dest_item_path)` runs once per top-level source ON THE DRIVER before the task is spawned, so a batch carries a hard floor of `N × RTT` no window width can overlap. Measured directly (`serial_precheck_floor`, outside the driver): **2.378 s of a 3.224 s best run, 74%**. ❌ Don't reach for the window formula to make transfers faster; the ceiling was never there.

**Decision**: the window's WIDTH formula is unchanged, but the window now reaches inside a directory source — one `strategy.rs::FileWindow` per operation, shared by every merge walker and by the concurrent driver's top-level file tasks.
**Why**: the driver only ever fanned out across TOP-LEVEL sources, and selecting one folder is one source, so a folder copy streamed strictly one file at a time however wide the setting was. It is a reach problem, not a formula problem — which is why the width still comes from `transfer_concurrency` and there is no second number. ❌ Never a window per level or per source (`W` sources × `W` leaves is `W²` files on one connection). Full account, including what deliberately did NOT change and the progress-bar bound it accepts: `volume/DETAILS.md` § "One window for the whole operation".

**Gotcha**: a Docker SMB number is a correctness and regression signal, NOT a latency signal.
**Why**: on loopback the per-file probe costs 492 µs instead of 4.76 ms, so it never becomes the ceiling and the extra window keeps buying parallel work — Docker many-small improves all the way to 32 where the NAS plateaus at 12. A Docker-only sweep therefore recommends "widen the window, it's still climbing", for a reason that doesn't exist on a real network. Same code, same harness, opposite conclusions. Anything whose cost is "one round trip per item" needs a real network before it means anything.

**Decision**: `drive_transfer_serial_async` bounds its closures as `for<'a> FnMut(...) -> Pin<Box<dyn Future<...> + Send + 'a>>`, not `AsyncFnMut(...) -> T`.
**Why**: Production callers live inside `tokio::spawn(async move { ... })` (see `volume::copy::copy_between_volumes`), so the driver's returned future must be `Send`. `AsyncFnMut`'s HRTB-bound `CallRefFuture<'a>` is not provably `Send` for all `'a` when the closure body captures `&Arc<...>` or similar refs — the compiler emits "implementation of Send is not general enough" because it can't discharge `for<'a> CallRefFuture<'a>: Send` (rust-lang/rust#100013-class). The explicit boxed-future shape moves the Send obligation inside the per-call return type, where it's discharged at each call site, and `+ Send` on the trait object is what makes the driver's awaiting-this-future state Send. An `async ||` + `AsyncFnMut` shape passes the driver's own `#[tokio::test]`s but breaks at the spawn boundary on real callers, so don't reach for it. `transfer_driver_async_tests.rs::driver_future_is_send_across_spawn` pins the contract by routing the driver call through an explicit `tokio::spawn` boundary.

**Decision**: `transfer_driver.rs` ships as two sibling entry points (sync + async), not one generic-over-AsyncFnMut-or-FnMut driver. Conflict resolution lives in the driver for the async path, in the closure for the sync path.
**Why**: `copy_files_with_progress_inner` is sync inside `spawn_blocking`; the three volume ops are async. A single generic driver would either force the sync path through a `Pin<Box<dyn Future>>` per source (allocation per call, no real benefit since the I/O is sync) or use a trait so gnarly that the closures stop reading as straight-line transfer code. Two siblings share `TransferContext`, `TransferOutcome`, `TransferLoopOutcome`, and `build_pre_skip_set` / `emit_progress_and_status` helpers — the duplication is small. For conflict resolution: local-FS conflicts surface mid-flight at parent directories inside `copy_single_item` (a file blocking `create_dir_all`), which the driver can't pre-detect via top-level `dest.get_metadata`; so the sync driver delegates conflict resolution to the closure entirely. Volume ops have only top-level conflicts that always reduce to `resolve_volume_conflict`, so the async driver owns that dispatch (uniform shape across all 3 volume ops, exactly what we want to deduplicate). The data-safety contract (closure never invoked for pre-skipped / resolved-as-Skip / post-cancel) is enforced in both shapes by the driver's loop structure and pinned by the `transfer_driver_*_tests.rs` suite. Volume copy's two paths are sibling modules over that shared driver: `volume/copy_serial.rs` supplies the copy-specific closures, and `volume/copy_concurrent.rs` runs its own `FuturesUnordered` window rather than becoming a third driver mode (one-of-four abstraction not worth its weight).

**Decision**: `copy_files_with_progress_inner` aligns `scan_result.files` to the driver's `&[PathBuf]` API via a paired `Vec<&FileInfo>` and a closure-captured `slice::Iter` advanced in lock-step with the driver iteration.
**Why**: The sync driver iterates a generic `&[PathBuf]`, but the local-FS copy loop needs the full `FileInfo` (for `dest_path`, `is_symlink`, `size`, and the `SourceItemTracker` key). Three alternatives were rejected: (a) indexing into `scan_result.files` by `ctx.files_done_so_far` — wrong, the cumulative counter is bytes-affecting and includes bulk-skipped files, so the index would shift; (b) extending `TransferContext` with a generic associated payload — couples the driver to local-FS specifics; (c) cloning the `FileInfo` slice for `sources` — copies on the hot path. The iterator approach is O(0) memory beyond the path vec and matches the driver's iteration order exactly (`pre_skip_paths` is empty because we pre-filter `scan_result.files` ourselves, so the driver invokes the closure once per surviving file). The `.expect()` is justified inline; if the driver ever stopped calling the closure once per source the test suite would break.

**Decision**: Cross-FS local move reuses the scan-preview cache via `config.preview_id`.
**Why**: `move_with_staging` used to ignore `preview_id` and always re-run `scan_sources`. The FE had just paid the cost of a full scan in `TransferDialog` (which emits cumulative `scan-preview-progress` events), so the second BE-side scan starting from `filesDone=0` made the count visibly reset in `TransferProgressDialog` ("scan again from 0, climb to N, then phase flips to Copying and the bar jumps to total"). Now the function checks `config.preview_id` first: on cache hit the `ScanResult` is consumed directly (same shape `copy.rs` uses), skipping the redundant scan and going straight to the active phase. On miss (no preview at all) the original `scan_sources` path stays — so MCP triggers and programmatic moves still work.

**Decision**: Volume-path progress is LEAF-granular and the closure owns it. `copy_single_path`'s `on_progress` / `on_file_complete` callbacks come from `transfer_driver.rs` (`SerialLeafProgress` for the one-source-in-flight serial paths; `make_concurrent_per_file_progress` for the `FuturesUnordered` path), all delegating to a private `try_emit_throttled_progress` core. A single top-level source can expand to many leaf files (a directory copies its whole subtree through ONE `copy_single_path` call reusing ONE callback pair), and `bytes_total` / `files_total` are the preflight LEAF totals, so the emitted `bytes_done` / `files_done` must climb across leaves: `SerialLeafProgress` seeds `byte_base` from the driver's `bytes_done_so_far` and adds each finished leaf's exact byte count in `on_leaf_complete`, while an operation-wide `Arc<AtomicUsize>` counts completed leaves (shared across every source).
**Why**: The serial path originally captured FROZEN snapshots (`bytes_done_so_far`, `files_done_so_far`) per top-level source. For a directory source every inner file emitted against the same `(0, 0)` snapshot, so the Size bar reset to 0 at each inner file and the File bar sat at 0 for the whole folder (observed moving a 9-file folder, ~10.6 GB, USB → SMB NAS). The frozen snapshot also can't survive multiple directory sources. The byte axis predates this: the move site once shipped a no-op `Continue(())` callback because the old move code sent `bytes_total = 0`; once `volume/preflight.rs` populated `bytes_total`, the Size bar pinned at 0 through a multi-minute upload (SMB dest, 3.7 GB file, "Moving... 0 bytes" the whole time) — DON'T reintroduce a no-op progress callback. The concurrent variant carries a per-task `last_file_bytes: Arc<AtomicU64>` so the orchestrating task can detect "volume never invoked on_progress" and credit the file's bytes as a compensation; its byte aggregation still loses one chunk per leaf across a DIRECTORY source boundary (a latent under-count; the concurrent path needs ≥3 directory top-level sources to hit it). Pinned by `test_cross_volume_copy_directory_source_progress_is_leaf_granular`, `cross_volume_move_directory_source_progress_is_leaf_granular`, `cross_volume_move_emits_intra_file_progress`, `test_cross_volume_copy_serial_emits_intra_file_progress`, and `test_cross_volume_copy_concurrent_emits_intra_file_progress`.

**Decision**: The per-file milestone (the bypass-throttle emit that guarantees the axes cross `N/N` even when chunked emits ate the throttle window) lives at the unit-of-work layer, not the driver. Serial volume paths: `SerialLeafProgress::on_leaf_complete` fires it per leaf. Concurrent path: the task-complete branch (`copy_volumes_with_progress::Some(Ok(...))`). Sync local-FS path: `copy_single_item` via `copy::record_file_done`. The async driver's `Transferred` arm emits a per-source milestone ONLY when `DriverConfig::emit_per_source_milestone` is `true` — set for the same-volume rename-merge, whose `transfer_one` does a bare `rename` with no streaming and thus no closure emit, so the driver milestone is its only Copying event. The streaming paths set it `false`: a top-level-granular driver milestone after a directory source would regress the File bar (9/9 → 1/9). `drive_transfer_serial_sync`'s `Transferred` arm never emits (the closure owns it). Mirrors the sync/async conflict-resolution split (sync driver delegates to closure, async driver dispatches itself).
**Why**: Chunked `on_progress` emits inside `transfer_one` (async) or `copy_single_item` (sync) carry `files_done_so_far` (the driver's iteration snapshot taken before this file started), so for single-file ops the chunked path never crosses `N/N` — the user would see "Copying... 99% / 0 of 1 files" jump straight to the complete toast, never observing the final "1 of 1" milestone. Putting the milestone in `copy_single_item` (a function called by both `copy_files_with_progress_inner` *and* `move_with_staging`'s direct copy loop) means a cross-FS local move sees the same milestone shape as a regular local copy — without `move_with_staging` needing its own duplicate emit. The throttle bypass is deliberate: per-file milestones are bounded by file count (not noisy), and throttle suppression of this event is exactly the bug being fixed. The emit fires at every `Ok`-return site in `copy_single_item` (regular copy, symlink copy, per-file Skip, type-mismatch parent Skip, same-file no-op) via a `PerFileCtx` struct that bundles the six operation-wide values so the six call sites stay one-liners. `cross_volume_move_cancel_mid_batch_preserves_completed` still passes because the async driver's milestone observes `files_done >= N` between sources before the next-iter cancellation check fires. Pinned by `cross_volume_move_emits_intra_file_progress`, `test_cross_volume_copy_serial_reaches_files_done_n`, `test_cross_volume_copy_concurrent_reaches_files_done_n`, `local_copy_single_file_reaches_files_done_n`, and `cross_fs_local_move_single_file_reaches_files_done_n`.

**Decision**: `scan_volume_sources` emits climbing `Scanning`-phase tallies via `scan_for_copy_batch_with_progress`.
**Why**: Without a cached `preview_id` (programmatic moves, MCP-triggered ops), the operation's scan phase used to emit a single `0/0/0` event up front, then sit silent through the entire `scan_for_copy_batch` call before flipping to `Copying`. On slow sources (cold MTP listing, large SMB tree) the FE shows "Scanning... 0 bytes / 0 files / 0 dirs" the whole duration. The scan-preview pipeline emits its own climbing tallies into the preview's event channel, not the operation's, so the operation needs to wire its own. The fix: pass a throttled `Fn(ListingProgress)` callback through `scan_for_copy_batch_with_progress` (the existing `_with_progress` trait variant) that emits a `Scanning`-phase event per tick, plus a final throttle-bypassed emit with the aggregate totals so a fast scan whose per-listing emits all got throttled still lands on the right number. The kickoff + final emits frame the per-tick stream. Pinned by `cross_volume_move_emits_scan_phase_tallies_during_walk`.

**Decision**: Cross-volume move runs the same preflight scan as volume copy (`scan_volume_sources` in `volume/preflight.rs`); the SAME-volume move does NOT — it's a rename-merge with top-level hints only and `bytes_total = 0`.
**Why**: The two move paths have different cost models. **Cross-volume move** (`move_volumes_with_progress`, copy+delete) genuinely transfers bytes, so it shares `scan_volume_sources` with copy: a cached `TransferDialog` preview when available, else `volume.scan_for_copy_batch_with_progress`, giving the real `(total_files, total_bytes, source_hints)` triple so the FE's Size bar tracks. **Same-volume move** (`move_within_same_volume_with_progress`) is a server-side rename — it moves ZERO bytes — so a deep recursive pre-flight scan there was pure waste that cost 30–40 s of "Verifying before move…" on a NAS (a real field incident: a same-NAS folder move blocked for 30–40 s before the 100 ms rename). It now calls `top_level_move_hints` instead: top-level `is_directory`/size hints only (one `list_directory` per distinct parent, O(distinct parents), never a subtree walk), `files_total` = number of selected top-level items, `bytes_total = 0`. The FE hides the Size bar on `bytes_total == 0`, which is honest — a rename moves no bytes, so a Size bar would be a lie. `known_directory_paths` (keeps bulk-skip file-only) comes from the same top-level hints. The perf contract is pinned by `volume/rename_merge_tests.rs::non_conflicting_move_does_no_subtree_walk` (a counting volume asserts no interior listing + O(top-level) stat count) and the SMB integration pin `smb_integration_same_share_nonconflicting_move_no_subtree_walk`.

**Decision**: Same-volume folder collisions merge via a recursive rename-merge (`volume/rename_merge.rs`), entered directly — no resolver round-trip for the folder.
**Why**: A flat `rename(source_dir, dest_dir, force=false)` onto an existing same-named dest dir fails `AlreadyExists`, so the move used to ERROR on a top-level folder collision instead of merging. The rename-merge walks the source folder level by level: it lists the source level + (pre-existing) dest level once each, then per child — no dest map hit → one `rename(child_src, child_dest, force=false)` (a whole subtree rides along, never descended); dir-vs-dir hit → recurse; file / cross-type hit → route through `resolve_volume_conflict` (folders never prompt; only files do). A file Overwrite collapses to the legacy delete-then-rename shape (the safe-replace temp dance buys nothing for a non-streaming rename). All renames are `force=false`. **Late-detected collisions** (case-insensitive SMB/APFS, TOCTOU): an unexpected `AlreadyExists` from a child rename is treated as a conflict routed through the resolver, NEVER a hard error — scoped to children with no exact-match hit, branching on a per-level `name → resolution` map so an already-resolved child finalizes its stored decision instead of re-prompting (this per-level map is orthogonal to the op-wide apply-to-all latch). **Source-dir cleanup is inside-out, empty-only**: after a level completes, `volume.delete(source_dir)` is attempted; `Volume::delete`'s "file or EMPTY directory only" contract means a non-empty source (skipped/errored/unmoved child) fails benignly and the dir — plus all its ancestors — survives. An all-moved spine deletes deepest-first via the recursion unwind. Symlinks move as opaque entries (one rename, never descended). The downloads-watcher hook fires on BOTH halves of every child rename. Cancel mid-merge keeps already-renamed children (existing contract) and never deletes a dir still holding unmoved content. Pinned by `volume/rename_merge_tests.rs` (zero-folder-prompt, skip/overwrite/rename/stop policies, cancel, source-dir cleanup matrix, dest-inside-source, case-fold prompts-once / no-double-prompt, symlink) and the SMB integration pin `smb_integration_same_share_move_merges_with_no_folder_prompt`.

**Decision**: Keep `exacl` crate for ACL copy in chunked copies (not custom FFI bindings).
**Why**: `exacl` adds zero new transitive dependencies (all of its deps, `bitflags`, `log`, `scopeguard`, `uuid`, are already in our tree). It provides cross-platform ACL support (macOS, Linux, FreeBSD) and full ACL parsing/manipulation for potential future UI features. The crate appears unmaintained (last release Feb 2024) but ACL APIs are stable and don't change. Our usage is best-effort with graceful fallback: if `exacl` ever breaks, files still copy, they just lose ACLs. MIT licensed (compatible with BSL).

**Decision**: On macOS, use `copyfile(3)` only for same-APFS-volume copies; use chunked copy for everything else.
**Why**: The only practical benefit of `copyfile(3)` is APFS clonefile (instant copy-on-write, zero extra disk usage), which only works on the same APFS volume. We evaluated `copyfile` on other filesystems:
- **HFS+**: No clonefile. Marginal metadata edge (birthtime, file flags), but HFS+ is rare since Apple converted all Macs to APFS in 2017.
- **exFAT / FAT32**: No clonefile, no xattrs, no ACLs, no file flags; the metadata `copyfile` would preserve doesn't exist on these filesystems. No practical benefit.
- **NTFS-3G**: FUSE-based, so `copyfile` goes through userspace with the same I/O buffering issues as network mounts. `COPYFILE_QUIT` is unreliable. No benefit.
- **Network mounts (SMB, NFS, AFP, WebDAV)**: `copyfile` ignores `COPYFILE_QUIT` while draining buffered I/O, causing cancellation to take 30+ seconds or complete the copy entirely. This applies when *either* the source or destination is on a network mount (for example, NAS-to-local copies).
- **USB / external drives**: Typically exFAT or HFS+, no clonefile. Different volume from the internal drive, so no same-volume benefits.

Our chunked copy (1 MB read/write chunks) provides: identical speed for non-clonefile copies, reliable cancellation between chunks, and granular progress callbacks. It preserves xattrs (including resource forks), ACLs, timestamps, and permissions. The only metadata it doesn't preserve is birthtime (creation date) and file flags (`chflags`), which matter only on same-volume copies where we use `copyfile` anyway. Detection uses `st_dev` (device ID) for same-volume and `statfs.f_fstypename` for APFS. See `copy_strategy.rs` for the implementation.

## Gotchas

**Gotcha**: In `copy_volumes_with_progress`, a per-task `VolumeError::Cancelled` populates `copy_error` and would otherwise skip the `write-cancelled` emit.
**Why**: Both the concurrent path (`Some(Err((failed_dest, e)))` arm) and the serial path (`PostLoopIntent::Failed`) feed any per-task error into `copy_error` via `map_volume_error`. `VolumeError::Cancelled` → `WriteOperationError::Cancelled`, so a mid-flight cancel that surfaces through a streaming reader lands in `copy_error` as a Cancelled-shaped `WriteFailure`. The post-loop gate `if copy_error.is_none() { emit_cancelled }` then silently swallows it, the outer `copy_between_volumes` wrapper's `matches!(Cancelled)` arm assumes the inner already emitted, and the FE never sees a terminal event — Copy dialog hangs until restart. The post-loop reclassifies a Cancelled-shaped `copy_error` as cancellation (`copy_error = None`) whenever `is_cancelled(&state.intent)` is true, so the emit gate fires; the synthetic `Err(Cancelled)` at the bottom of the function still propagates so the outer wrapper continues to skip `write-error`. Repro: copy 13 SMB files concurrent path, cancel mid-stream → no `write-cancelled` on the wire pre-fix.

**Gotcha**: Cross-type Overwrite (file↔folder) is delete-first, NOT a merge or safe-replace.
**Why**: A type swap can't temp-rename across backends, so `apply_volume_conflict_resolution` deletes the dest first (`remove_tree(…, UserChoseOverwriteAcrossTypes)` for a folder dest, `Volume::delete` for a file dest) before the source materializes. These are rare and lower-stakes (a type mismatch already means wholesale content replacement). Same-type dir-vs-dir never reaches `apply_volume_conflict_resolution` for the folder — it short-circuits to merge in `resolve_volume_conflict` before any policy dispatch.

**Test harness: the conflict responder is an event sink, prompt counts come from the sink.** The folder-merge suites (`volume/merge_tests.rs`, `volume/rename_merge_tests.rs`) drive Stop-mode prompts with `ConflictResponderSink` (`conflict_responder_test_support.rs`): it wraps a `CollectorEventSink`, forwards every event, and the instant it observes a `write-conflict` it answers `state.conflict_slot` with the scripted response. This works because the Stop branch arms the slot BEFORE emitting the event (`volume/conflict.rs`), so the answer can't miss. Assertions derive the prompt count from the recorded conflicts via the shared counters in `conflict_responder_test_support.rs` — `file_conflict_count`, plus `folder_conflict_count_both_dirs` (source AND dest are dirs; pins the copy-side "dirs never prompt" contract) and `folder_conflict_count_any_dir` (source OR dest is a dir; pins the rename-merge contract) — race-free once the op future returns, never from a side-channel counter. The pattern is order-independent by design, so there's no polling loop and no answer-accounting race to defend.

## Gotcha: a skip-guard `return` above a macOS-only block breaks the Linux lane

`copy_integration_test.rs`'s macOS tests are shaped as setup followed by one
`#[cfg(target_os = "macos")] { … }` block holding every assertion. A `return` placed
ABOVE that block (the usual "skip if the tool isn't available" guard) is a normal early
exit on macOS, but on Linux the block vanishes and the `return` becomes the function's
last statement, so `clippy::needless_return` fails the `Desktop (Rust)` CI job. **A
local macOS run cannot see it**, which is what makes it worth writing down.

Keep the guard INSIDE the `cfg` block, where it disappears along with the code it
guards (`test_copy_preserves_xattrs`). Verified by reproduction rather than by reading
the lint docs: compiling the old shape with the macOS block stripped reproduces the
exact CI error, and the new shape is clean.

This is the same class as `3d4c9816f`, where two `use` statements sat above their
macOS-only use sites. Any code whose only consumer is behind `cfg(target_os = "macos")`
needs the same `cfg` on the thing above it.
