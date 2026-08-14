# Indexing read side

Serve recursive sizes, index status, and coverage back to the app. Everything here reads via the per-volume `ReadPool`
(lock-free thread-local connections), NEVER the lifecycle registry lock.

`enrichment.rs` the `ReadPool` type + `enrich_entries_with_index[_on_volume]`; `coverage.rs` the search frontier and
`CoverageToken` (read-only — the WALK is `../lifecycle/cover.rs`); `queries.rs` the IPC read surface, no registry
mutation; `expected_totals.rs` progress denominators; `pending_sizes.rs` the "size updating" hourglass; `handles.rs` the
volume-keyed handle tables.

## Must-knows

- **Handles arrive PUSHED, they are never pulled.** Lifecycle installs and withdraws them. ❌ Nothing here may import
  `lifecycle::state`: a registry lookup would put the hot read path behind the mutex a shutdown drain holds. The table
  lock is a LEAF — ❌ never call out under it.
- **`get_read_pool_for(vid)` returning `None` IS the skip signal.** The gate is pool-presence, not registry-key
  presence, so it can't disagree with the routing call that asks the same question.
- **`root` ALSO keeps the `scanner::should_exclude(parent_path)` check.** A `root` listing can navigate to a path root
  never indexes; without it, enrichment misses and logs on every ~2/s refresh.
- **Map the read path into index-relative space via `routing::index_read_path` before `resolve_path`** — a
  mount-absolute SMB/MTP path resolves to nothing otherwise. Owned by `../paths/CLAUDE.md`.
- **Derive `{complete, stale}` from `min_subtree_epoch` vs `current_epoch`; ❌ never ship raw epochs.** Read the epoch
  ONCE per pass. `expected_totals` returns `None` for ANY incomplete or unindexed source: a lower bound would overshoot
  the progress bar past 100%.
- **The pending-sizes hourglass is a marked-SET, cleared WHOLESALE on writer `queue_depth == 0`** (self-healing, no
  per-entry pairing to leak). Marked only at the live loop's drain points, so replay doesn't flag everything on startup.
  Rides `DirStats` only, ❌ NOT `FileEntry` enrichment.
- **`ReadPool`'s thread-local is a 3-slot LRU, not one connection.** One slot made a thread alternating between two
  volumes reopen every time, losing its `prepare_cached` statements on the hot path. ❌ No mutex here, ❌ don't shrink
  it, ❌ don't reset a new pool's starting generation to a constant.
- **The coverage frontier needs BOTH epoch fields; ❌ never `min_subtree_epoch` alone** (it 0-absorbs upward, so the
  answer becomes "walk everything"). `min > 0` covered, `min == 0 && listed > 0` descend,
  `listed == 0 && unreadable_cause` skip (into `permission_denied`, `declined`, or `abandoned`, ❌ never merged — and
  `abandoned` is a HOLE nothing else in the answer hints at, so a caller reporting completeness must read it), else
  frontier. Rests on the `EXCLUSION_POLICY_KEY` stamp: stale or absent ⇒ nothing is trusted.
- **Enrichment logs once per changed result, via `EnrichResultMemo`.** ❌ No per-pass line: an idle pane triggers this
  ~2/s.
- ⚠️ **An UNLISTED directory's rows aren't its contents**, so `list_dir_children` answers `None` there.

Owned elsewhere: the `dir_stats` ledger, honest sizes, and epochs in `../writer/CLAUDE.md`; the registry, handle
bootstrap, phase, and freshness in `../lifecycle/CLAUDE.md`; path routing in `../paths/CLAUDE.md`.

Enrichment, the IPC query surface, `expected_totals`, and the hourglass: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
