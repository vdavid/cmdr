# Delete + trash details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## Volume-delete internals

`delete_volume_files_with_progress_inner` consumes the scan preview via `take_cached_scan_result(preview_id)`. On hit, top-level
files come straight from `CopyScanResult` with no `is_directory` probe; top-level dirs recurse via the oracle-aware
walker. On no-preview paths (MCP, programmatic), the top-level `is_directory(source)` probe stays unless the source's
parent is watcher-fresh in `LISTING_CACHE`, in which case the type comes from the cached entry. Both emit paths use
`with_scan_meta(current_dir, dirs_done, None)` so the scanning UI shows the dir count and the directory the walker is
currently in. The per-entry callback is throttled so the FE tally climbs mid-listing on slow MTP roundtrips.

## Key decisions

**Decision**: Volume delete reuses the scan preview and is oracle-aware on the no-preview path.
**Why**: Before this, `delete_volume_files_with_progress_inner` ignored `config.preview_id` and re-ran
`scan_volume_recursive`. On MTP that meant a second 17 s parent listing for a 135-photo `/DCIM/Camera` delete after the
user already paid that cost in the pre-flight dialog, and the second scan emitted no per-top-level-file progress so the
UI looked frozen. The fix has three parts. (1) `take_cached_scan_result(preview_id)` at the top: on hit, top-level files
are recorded from `CopyScanResult::total_bytes` with no `is_directory` probe and no `list_directory` round-trip, and
top-level dirs recurse via the oracle-aware `scan_volume_recursive` (passing `is_dir_hint = Some(true)` so the recursion
never re-probes). (2) The walker's internal `volume.list_directory(path, ...)` is preceded by
`try_get_watched_listing(volume_id, path)`; on hit, the cached entries replace the volume call at every recursion level.
(3) On the no-preview path, the top-level `volume.is_directory(source)` probe stays only when the parent oracle misses;
when a pane has the source's parent open and watcher-fresh, the type comes from the cached `FileEntry` and the probe is
skipped. The cache-hit path emits a throttled scan-progress event per `progress_interval` while building the entry list,
so the FE dialog shows movement. Pinned by `delete_volume_reuse_tests.rs`.

Data-safety contract: stale-by-one cached entries can either silently skip a now-gone file (acceptable: the user already
moved it) or attempt to delete a missing one (the volume's `delete` errors cleanly). Neither can delete the wrong file
because we feed `volume.delete(&entry.path)` exact paths the cache observed; a cached entry that races with a concurrent
rename addresses the old path the next call won't find.

**Decision**: Delete and trash don't `fsync` (or fire any global `sync(2)`) after removing files.
**Why**: A non-durable delete fails annoyance-class, never data-loss-class: if the machine crashes before the deletion is
flushed, the deleted file reappears and the user re-deletes it. Paying for a targeted `fdatasync` over every removed path
(and its parent dirs) isn't worth the cost. The old code fired a detached whole-machine `sync(2)`; that flushed every
filesystem on the box, stalling unrelated apps (against AGENTS.md principle #5, "be respectful to the user's
resources"), and as fire-and-forget it didn't even make "complete" mean "durable." Copy and move are the data-loss-class
operations (a move can leave bytes nowhere durable), so they get the real targeted flush; see `../transfer/DETAILS.md`
§ "Durability" and `../DETAILS.md` § "Key decisions (shared)". Pinned by
`tests.rs::no_global_sync_or_spawn_async_sync_in_write_operations`, which fails the suite if `spawn_async_sync` or a raw
`libc::sync()` reappears in `write_operations/`.
