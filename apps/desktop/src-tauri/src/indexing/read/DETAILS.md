# Indexing read side details

Read this before any non-trivial work in `indexing/read/`: editing, planning, reorganizing, or advising. Must-know
invariants are in `CLAUDE.md`.

This area serves recursive sizes and index status back to the app. Four concerns: enrichment (the hot path), the IPC
query surface, write-op expected totals, and the "size updating" hourglass. All read via the per-volume `ReadPool`;
none take the lifecycle registry lock.

## Enrichment (`enrichment.rs`)

`ReadPool` is defined here: lock-free thread-local read connections for enrichment and verification. `with_conn`'s
signature (`fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T)`) ensures the `&Connection` can't escape the
closure, so async task migration can't break thread affinity — enforced by the type, not convention. The root pool
lives in the `READ_POOL` module global; non-root pools live in their registry instance. `get_read_pool_for(vid)` routes
root → `READ_POOL`, non-root → `state::get_instance_read_pool`. (The globals and instance storage are owned by
`../lifecycle/DETAILS.md`; this area holds the `ReadPool` type and the readers.)

`enrich_entries_with_index(entries)` is the root-defaulting wrapper; `enrich_entries_with_index_on_volume(volume_id,
entries)` is the volume-routed form. Called when entries land in the listing cache (streaming, watcher update, re-sort),
NOT on `get_file_range`; live freshness flows separately via `index-dir-updated` → `refreshIndexSizes` →
`getDirStatsBatch`. A live pane triggers a pass about twice a second whether or not anything changed.

**The skip-vs-route gate.** `get_read_pool_for(volume_id)` returning `None` IS the "no index registered for this
volume" signal — enrichment early-returns before any DB work. The gate is pool-presence rather than registry-key
presence so it can never disagree with the routing call (`get_read_pool_for`): the gate and the route ask the exact
same question. This replaces the old `should_exclude`-only gate. For the `root` volume specifically, the
`scanner::should_exclude(parent_path)` check is ALSO kept: a `root`-volume listing navigated to `/Volumes/`, `/mnt/`,
`/proc/`, or a system path isn't in root's index, so it would still miss every lookup and log "Parent path not found"
on every refresh.

**Integer-keyed fast path.** Resolve the parent dir once (`listing_parent_path`, a pure helper) →
`list_child_dir_ids_and_names(parent_id)` → `get_dir_stats_batch_by_ids` → match by normalized name. Two indexed
queries instead of N `resolve_path` calls. Falls back to individual path resolution for the mixed-parent edge case.

**Read-side path mapping.** Both `enrich_via_parent_id_on` (fast path) and `enrich_via_individual_paths_on` (fallback)
map their mount-absolute paths into the volume's index path space via `routing::index_read_path` before
`resolve_path` — a pass-through for `root`, a mount-relative strip for SMB, a scheme/storage strip for MTP. Without it
an indexed SMB folder enriches to nothing. Owned by `../paths/DETAILS.md`.

**Deriving the honest-size booleans.** `apply_dir_stats` sets `recursive_size_complete = min_subtree_epoch > 0` and
`recursive_size_stale = complete && min_subtree_epoch < current_epoch`. `current_epoch` is read ONCE per
`enrich_entries_with_index_on_volume` pass, on the same `ReadPool` conn that fetches the stats, and threaded into both
enrichment forms. The frontend renders from `{recursive_size, complete, stale}` only; it never learns the epoch scheme.
The epoch model itself is owned by `../writer/DETAILS.md`.

**Log memo.** A per-pass line is ~14,000 lines an hour from two idle panes, and the varying counts and path defeat the
log writer's coalescer. So the pass keeps ONE line, `enrich: 12/14 dirs got sizes under <parent>`, gated on
`EnrichResultMemo`: it fires only when `(dir_count, enriched)` differs from the last logged pass for that `(volume_id,
parent_path)`. An idle pane is silent. The memo is bounded (256 listings, cleared wholesale when full).

## The IPC query surface (`queries.rs`)

The read-only index queries the IPC commands call (status + dir-stats), distinct from the lifecycle/registry core; none
mutate registry state.

- `get_status(vid)` / `get_debug_status(vid)` — read a volume's phase (plus the `Initializing` temp store) under the
  registry lock.
- `get_volume_index_status(path)` / `get_volume_index_status_by_id(volume_id)` — build the per-drive badge shape
  (`VolumeIndexStatus { volume_id, enabled, freshness, scan_completed_at, scan_duration_ms,
  coalesced_signals_since_sweep, next_sweep_due_at }`). The path form resolves the volume from a listing path (the
  always-visible active-drive badge); the id form is keyed by `volume.id` (the per-drive dropdown rows). Both return the
  same shape. `next_sweep_due_at` is computed here so the sweep-window length stays in the policy module (owned by
  `../reconcile/DETAILS.md`), not duplicated in the frontend.
- `get_dir_stats(path)` / `get_dir_stats_batch(paths)` — resolve the volume via `routing::volume_id_for_local_path`,
  delegate to `*_on_volume`, and read dir aggregates off the volume's `ReadPool` (mapping the path via
  `routing::index_read_path`). `dir_stats_from` derives the same `{complete, stale}` booleans as enrichment;
  `get_dir_stats_on_volume` reads `current_epoch` inside its `with_conn`, `get_dir_stats_batch_on_volume` once per call.
  The FE copies the booleans onto the `FileEntry` (including the `..` parent row, which renders from the current dir's
  own stats, so a partially-scanned dir shows `..` as `≥`/`—`).

The IPC boundary stays path-based; the volume is resolved internally. The path-based commands map an SMB-mounted path to
its `smb_volume_id`, an `mtp://` path to its `{device}:{storage}` id, a registered local external mount to its own id,
and the boot disk (plus cloud-drive folders) to `root` — routing owned by `../paths/DETAILS.md`. The routed
reads skip cleanly (`get_read_pool_for` → `None`) when the resolved volume has no registered index, so an unindexed SMB
share or a mounted-but-unindexed external drive costs zero DB work — which is also why such a drive reports `off` rather
than inheriting `root`'s freshness. The MCP server consumes these read APIs too (`cmdr://indexing`, the `await
index_status` condition), never re-deriving freshness.

## Expected totals (`expected_totals.rs`)

`expected_totals_for_sources()` returns the index-derived `(file count, byte total)` for a set of source paths so a
write operation (copy/move/delete) can render a real scan-phase progress bar before the foolproof re-scan completes.
Per source: `resolve_path` → `get_entry_by_id` → if a dir use `dir_stats`, if a file use the entry's `logical_size`.
Uses the same `ReadPool` as enrichment for lock-free reads. Used by `scan_preview.rs` and `scan.rs` in
`write_operations/`.

**It returns `None` if ANY source isn't covered by the index** — no pool, no entry, no `dir_stats`, no `logical_size`,
OR (via `per_source_contribution`) a directory whose subtree is incomplete (`min_subtree_epoch == 0`). A partial or
lower-bound total would let the progress bar overshoot 100%. Destructive ops re-stat live in
`write_operations/conflict.rs`; the index is never load-bearing there — it's consulted only for non-load-bearing size
estimates with an explicit "unknown" fallback.

## The pending-sizes hourglass (`pending_sizes.rs`)

`PendingSizes`: an in-memory `Mutex<HashSet<String>>` of directory paths with unprocessed writes in flight, so the UI
can show a per-directory "size updating" hourglass during big deletes/copies. Two signals, cleanly split: the global
`indexing` flag means every size is in flux during a full scan; per-dir `recursive_size_pending` means live writes are
in flight for that dir even when no scan runs.

- `mark(path)` inserts the normalized path plus every ancestor; `is_pending(path)` is the membership test; `clear()`
  wipes the transient set.
- The root tracker is a module-global (`PENDING_SIZES`) installed in lockstep with `READ_POOL`; non-root trackers live
  in their registry instance (`get_pending_sizes_for(vid)` routes root → `PENDING_SIZES`, non-root →
  `state::get_instance_pending_sizes`).
- **Marked** at the live event loop's `pending_paths` drain points (`watch/event_loop`'s `mark_pending_and_drain`,
  live-only — NOT the shared `process_fs_event`, so replay doesn't flag everything during startup).
- **Cleared wholesale** by the writer thread once `queue_depth` hits 0. This is self-healing: an empty queue means no
  unprocessed work, so the set is correct to empty, and there's no per-entry increment/decrement to leak (no "stuck
  hourglass forever" class). Chosen over counters precisely for that.
- **Read** when building `DirStats` (`queries.rs`), surfaced via `DirStats.recursive_size_pending`. It rides `DirStats`
  only, NOT the Rust `FileEntry`/`get_file_range` enrichment path — that path isn't where live size refreshes flow, and
  adding a field to `FileEntry` (no `Default`, ~30 literal sites) buys only a sub-2s hourglass on a folder navigated
  into mid-storm. This half is deliberately not "fixed".

**The held-roots tier (for coalesced rescans).** A detached `reconcile_subtree` runs for seconds while the writer queue
oscillates empty, so the wholesale queue-drain `clear()` would wipe the mark long before the reconcile finishes, and
nothing marked its scope at queue time. So `PendingSizes` has a SECOND held-roots tier (rescan root paths only):
`queue_must_scan_sub_dirs` holds the root; `is_pending(path)` is true for any transient mark OR any path related to a
held root in EITHER direction (an ancestor-or-equal, whose aggregate includes the rewriting subtree, OR a descendant,
whose own rows are being rewritten); and the writer-drain `clear()` wipes only the TRANSIENT set — holds survive.
Holding roots (not expanded ancestors) with a query-time prefix test keeps release exact under overlapping rescans
(`/a/b` and `/a/c` share `/a`; expanding would strip it while one is still in flight). On completion the sequence is
`release(root)` FIRST, then emit `index-dir-updated` for the root + ancestors via `WriteMessage::EmitDirUpdated`:
release before emit, else the triggered refetch re-reads `pending = true`. The mark/clear mechanics that feed this from
the writer side (the `dir_stats` ledger, the drain point) are owned by `../writer/DETAILS.md`.
