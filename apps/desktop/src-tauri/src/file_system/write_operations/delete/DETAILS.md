# Delete + trash details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## Volume-delete internals

`delete_volume_files_with_progress_inner` consumes the scan preview via `take_cached_scan_result(preview_id, sources)`. On hit, top-level
files come straight from `CopyScanResult` with no `is_directory` probe; top-level dirs recurse via the oracle-aware
walker. On no-preview paths (MCP, programmatic), the parent oracle answers the top-level type when the source's parent
is watcher-fresh in `LISTING_CACHE`, and otherwise the walker resolves it. Both emit paths use
`with_scan_meta(current_dir, dirs_done, None)` so the scanning UI shows the dir count and the directory the walker is
currently in. The per-entry callback is throttled so the FE tally climbs mid-listing on slow MTP roundtrips.

## What each branch does with a missing or wrong fact

Delete is the operation with no rollback, so every branch here has to have an answer for "what if this fact is absent or
wrong". The audit, top to bottom:

- **Which selection the cached preview describes.** Bound at the cache, not here: `take_cached_scan_result` compares the
  entry's `sources` against the operation's and treats a mismatch as a miss (`../DETAILS.md` § "The cache is bound to
  its request"). Both walkers then fall through to a fresh scan. Before that binding, the LOCAL walker was the sharpest
  edge in the app: it takes the cached result wholesale and iterates `scan_result.files` without ever re-reading its own
  `sources`, so a `preview_id` pointing at another tree deleted that tree. Pinned by `preview_binding_tests.rs`.
- **A source the cached `per_path` doesn't cover** (the volume walker's `None` arm). Forwards `is_dir_hint: None`, and
  `scan_volume_recursive` PROPAGATES a failed probe rather than defaulting. It looks like the bug class and is its
  opposite; a `Some(false)` there would be the guess. The whole-map-empty case (`file_count > 0`, `per_path` empty) is
  the exact production shape the original copy bug rode in on, so it's pinned end to end in
  `delete_volume_reuse_tests.rs`.
- **A source whose stat can't be answered** (the no-preview path). Also forwards the oracle's `Option` straight to
  `scan_volume_recursive`, so an unanswerable stat fails that item. The walker used to resolve the top level itself with
  `.unwrap_or(false)`, which made an unanswered stat a confident "file": the entry went in as `is_dir: false` with zero
  bytes, so progress described one file and no bytes for what might be a whole tree, and the `delete` that followed
  either took the tree (on a backend that recursed) or died on a confusing `ENOTEMPTY`. **What the user sees now**:
  deleting a folder whose stat failed reports a per-item failure with the folder still standing, instead of appearing to
  delete "one file". That's the honest outcome, it fires only on a probe error, and a retry after a transient MTP stat
  failure is cheap.
- **A cached listing that's stale by one.** Covered by the data-safety contract below: exact observed paths only.

What the `Volume::delete` non-recursion contract does and doesn't buy this walker: it bounds the BLAST RADIUS of a wrong
`is_dir` on a top-level source (a `delete` that would have taken a tree refuses instead), but it never bought a truthful
progress count, and it's a property of the conformance assertion every backend runs, not of a doc comment. The contract
itself is single-sourced at `crates/cmdr-fs/src/volume/mod.rs`.

## Key decisions

**Decision**: Volume delete reuses the scan preview and is oracle-aware on the no-preview path.
**Why**: Before this, `delete_volume_files_with_progress_inner` ignored `config.preview_id` and re-ran
`scan_volume_recursive`. On MTP that meant a second 17 s parent listing for a 135-photo `/DCIM/Camera` delete after the
user already paid that cost in the pre-flight dialog, and the second scan emitted no per-top-level-file progress so the
UI looked frozen. The fix has three parts. (1) `take_cached_scan_result(preview_id, sources)` at the top: on hit, top-level files
are recorded from `CopyScanResult::total_bytes` with no `is_directory` probe and no `list_directory` round-trip, and
top-level dirs recurse via the oracle-aware `scan_volume_recursive` (passing `is_dir_hint = Some(true)` so the recursion
never re-probes). (2) The walker's internal `volume.list_directory(path, ...)` is preceded by
`try_get_watched_listing(volume_id, path)`; on hit, the cached entries replace the volume call at every recursion level.
(3) On the no-preview path, the parent oracle supplies the top-level type when a pane has the source's parent open and
watcher-fresh, skipping the probe; on a miss the hint stays `None` and the walker resolves it (see the branch audit
above). The cache-hit path emits a throttled scan-progress event per `progress_interval` while building the entry list,
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
