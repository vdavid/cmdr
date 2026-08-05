# Indexing read side

Serve recursive sizes, index status, and coverage back to the app. Everything here reads via the per-volume `ReadPool`
(lock-free thread-local connections), NEVER the lifecycle registry lock.

## Must-knows

- **Handles arrive PUSHED, they are never pulled.** Lifecycle installs a volume's `ReadPool`/`PendingSizes` into the
  volume-keyed tables in `handles.rs` and withdraws them on teardown. ❌ Nothing here may import `lifecycle::state`: a
  registry lookup would put the hot read path behind the mutex a shutdown drain holds. The table lock is a LEAF — ❌
  never call out while holding it.
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
- **`ReadPool`'s thread-local is a 3-slot LRU (`sqlite_util::ThreadConnCache`), not one connection.** One slot made a
  thread alternating between two volumes (an ordinary two-pane setup) reopen on every alternation, losing the
  connection's `prepare_cached` statements on the hot path. ❌ Don't add a mutex here, and ❌ don't shrink it back to
  one slot. DETAILS § Enrichment.
- **The coverage frontier needs BOTH epoch fields; ❌ never `min_subtree_epoch` alone** (it 0-absorbs upward, so the cut
  is always the scope root and the answer is "walk everything"). `min > 0` covered, `min == 0 && listed > 0` descend,
  `listed == 0 && known_unreadable` skip, else frontier. Rests on `min > 0` ⇒ `listed > 0`, and on the
  `EXCLUSION_POLICY_KEY` stamp: absent or stale ⇒ no coverage claim is trusted. DETAILS § The coverage frontier.
- **Enrichment logs once per changed result, via `EnrichResultMemo`** (fires only when `(dir_count, enriched)` differs).
  Don't add a per-pass line; an idle pane triggers this ~2/s per pane.

## Module map

- `enrichment.rs` — the `ReadPool` type + `enrich_entries_with_index[_on_volume]` (integer-keyed fast path, per-path
  fallback).
- `coverage.rs` — the search frontier (`Index::coverage`), the descent rule, `CoverageToken`. Read-only; the WALK is
  `../lifecycle/cover.rs`.
- `queries.rs` — the IPC read surface (`get_status`, `get_volume_index_status*`, `get_dir_stats*`); no registry
  mutation.
- `expected_totals.rs` — index-derived copy/move/delete progress denominators.
- `pending_sizes.rs` — the "size updating" hourglass `PendingSizes` marked-set + its held-roots tier.
- `handles.rs` — the volume-keyed tables both handles live in, and their leaf-lock discipline.

Owned elsewhere: the `dir_stats` ledger, honest sizes, and epochs live in `../writer/CLAUDE.md`; the registry,
`ReadPool`/`PendingSizes` bootstrap, phase, and freshness in `../lifecycle/CLAUDE.md`; path routing in
`../paths/CLAUDE.md`.

Enrichment, the IPC query surface, `expected_totals`, and the hourglass: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
