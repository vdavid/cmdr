# Indexing read side

Serve recursive sizes and index status back to the app. Everything here reads via the per-volume `ReadPool`
(lock-free thread-local connections), NEVER the lifecycle registry lock.

## Must-knows

- **`get_read_pool_for(vid)` returning `None` IS the skip signal.** `enrich_entries_with_index_on_volume` early-returns
  before any DB work when the volume has no registered index (`None` pool). The gate is pool-presence, not
  registry-key-presence, so it can never disagree with the routing call that asks the same question. Every non-root
  listing (SMB/MTP/network mounts) skips here for free.
- **`root` ALSO keeps the `scanner::should_exclude(parent_path)` check.** A `root` listing can be navigated to a path
  root never indexes (`/Volumes/`, `/proc/`, system trees); without it, enrichment resolves against root's DB, misses,
  and logs "Parent path not found" on every ~2/s refresh.
- **Map the read path into index-relative space via `routing::index_read_path` before `resolve_path`.** A mount-absolute
  SMB/MTP path resolves to nothing otherwise (the bug that made sizes invisible). Owned by `../paths/CLAUDE.md`.
- **Derive `{complete, stale}` booleans from `min_subtree_epoch` vs `current_epoch`; never ship raw epochs.** Read the
  epoch ONCE per pass. `expected_totals` returns `None` for ANY incomplete (`min_subtree_epoch == 0`) or unindexed
  source: a lower bound would overshoot the write-op progress bar past 100%.
- **The pending-sizes hourglass is a marked-SET, cleared WHOLESALE on writer `queue_depth == 0`** (self-healing, no
  per-entry pairing to leak). Marked only at the live loop's drain points (live-only, so replay doesn't flag everything
  on startup). Rides `DirStats` only, NOT `FileEntry` enrichment (deliberate). A second held-roots tier survives the
  wholesale clear for seconds-long coalesced rescans.
- **Enrichment logs once per changed result, via `EnrichResultMemo`** (fires only when `(dir_count, enriched)` differs).
  Don't add a per-pass line; an idle pane triggers this ~2/s per pane.

## Module map

- `enrichment.rs` — the `ReadPool` type + `enrich_entries_with_index[_on_volume]` (integer-keyed fast path, per-path fallback).
- `queries.rs` — the IPC read surface (`get_status`, `get_volume_index_status*`, `get_dir_stats*`); no registry mutation.
- `expected_totals.rs` — index-derived copy/move/delete progress denominators.
- `pending_sizes.rs` — the "size updating" hourglass `PendingSizes` marked-set + its held-roots tier.

Owned elsewhere: the `dir_stats` ledger, honest sizes, and epochs live in `../writer/CLAUDE.md`; the registry,
`ReadPool`/`PendingSizes` bootstrap, phase, and freshness in `../lifecycle/CLAUDE.md`; path routing in
`../paths/CLAUDE.md`.

Enrichment, the IPC query surface, `expected_totals`, and the hourglass: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
