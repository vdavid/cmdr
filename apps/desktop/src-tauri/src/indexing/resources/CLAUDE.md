# Indexing resources (process-wide caps)

Process-wide resource governance for indexing: bounded memory and bounded disk. Unlike `../lifecycle` (which is
per-volume), these cap the WHOLE indexing pool.

## Module map

- **memory_watchdog.rs** — the single global `phys_footprint` budget (warn 8 GB, stop ALL indexing 16 GB).
- **subsystem_stop.rs** — the stop-hook registry the watchdog runs alongside the index stop.
- **retention.rs** — the external-index-DB count cap with LRU eviction.

## Must-knows

- **ONE global process-wide memory watchdog stops ALL indexing.** Warn at 8 GB, stop at 16 GB via
  `state::stop_all_indexing` (snapshot ids, then stop each). Scans run in PARALLEL; this is a catastrophe-stop for
  machine protection, NOT a usage target. `start()` is idempotent (`WATCHDOG_RUNNING`), called from `start_indexing`;
  macOS-only, no-op stub elsewhere. Scans spawn via `tauri::async_runtime::spawn` (`tokio::spawn` panics in `setup()`).
- **The threshold basis is `phys_footprint`, NOT RSS.** RSS counts WebView graphics mappings (the WebKit Metal
  compositor's `IOAccelerator` region measured ~3.8 GB, ~79% of a 4.8 GB RSS, during an FSEvents storm while all malloc
  zones held ~185 MB; verified via `vmmap`/`footprint`, 2026-07). Keying the stop on RSS would let graphics trip the
  machine-protection stop while indexing's own heap is a couple hundred MB. The malloc-heap-vs-RSS gap is the single
  best discriminator (heap ~200 MB while RSS is multi-GB says "graphics, not indexing").
- **ONE budget covers other resident-pool subsystems** (`subsystem_stop.rs`): a subsystem (media_index image
  enrichment, which decodes HEIC/RAW) calls `register_subsystem_stop_hook` once at startup, and `stop_all_indexing`
  runs every hook. Deliberate: a second independent 16 GB ceiling over the SAME pool would let the two sum to ~2× real
  headroom. Hooks run INLINE in the stop path, so they must be cheap and non-blocking (flip an atomic cancel flag).
- **Retention cap: at most `MAX_EXTERNAL_INDEX_DBS = 32` external (non-root) index DBs.** `enforce_external_index_cap`
  runs after a successful SMB/MTP enable and LRU-evicts the least-recently-used OFFLINE DBs via the pure,
  filesystem-free `select_evictions`. SAFETY (enforced by the selector, unit-tested): never evict a registered
  (`Running`/`Initializing`) volume's DB nor `root`, no matter how old its mtime. `forget`/`disable`/`clear` are
  lifecycle's, not here.

Thresholds, the memory-snapshot breakdown, the shared-ceiling rationale, and the LRU + safety logic:
[DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
