# Indexing resources (process-wide caps)

Process-wide resource governance for indexing: bounded memory and bounded disk. Unlike `../lifecycle` (per-volume),
these cap the WHOLE indexing pool.

## Module map

- **memory_watchdog.rs** — the single global `phys_footprint` budget (warn 8 GB, stop ALL indexing 16 GB, then keep
  watching). Policy only; the readers live in `cmdr_fs::process_memory`, re-exported as `crate::process_memory`.
- **memory_snapshot.rs** — the breakdown a trip logs (macOS), and its derived verdict.
- **subsystem_stop.rs** — the stop-hook registry the watchdog runs beside the index stop.
- **retention.rs** — the external-index-DB count cap with LRU eviction, plus `sweep_legacy_scheme_dbs` (one shot from
  `Index::start_root_at_launch`: deletes every store's files keyed by a retired volume-ID scheme). Both remove through
  `crate::volume_files`, ❌ never an unlink of their own.

## Must-knows

- **ONE global process-wide memory watchdog stops ALL indexing.** Warn at 8 GB, stop at 16 GB via
  `state::stop_all_indexing` (snapshot ids, then stop each). Scans run in PARALLEL; this is a catastrophe-stop for
  machine protection, NOT a usage target. `start()` is idempotent (`WATCHDOG_RUNNING`), called from `start_indexing`;
  macOS-only, no-op stub elsewhere. Scans spawn via the `host::runtime` seam (`tokio::spawn` panics in `setup()`).
- **The stop is NOT the end of the watch.** The loop runs for the process lifetime and escalates when `phys_footprint`
  keeps climbing after a stop (+2 GB, then 4, 8, 16), re-arming below the warn line. Don't reintroduce a `return` in the
  stop path: that one-shot shape let a 2026-07 incident climb 16→40 GB completely unobserved.
- **Read the heap through `crate::process_memory`, never a zone total.** `query_rust_heap` reads whichever allocator is
  global; `query_system_malloc_zones` reads the zones beyond it. Under mimalloc the zone APIs are BLIND to the heap (a
  16.5 GB footprint once logged as a 1.6 GB heap), and `vmmap`'s `IOAccelerator` rows ARE the heap, not GPU memory.
- **The threshold basis is `phys_footprint`, NOT RSS.** RSS counts graphics and shared mappings that aren't real memory
  pressure, so keying the stop on RSS would let graphics trip a machine-protection stop. ❌ Don't reintroduce a
  "resident − phys means GPU" hint: in the 2026-07 runaway that delta was 0.00 GB and the memory was the Rust heap. The
  log's verdict comes from `MemoryAttribution`, computed from the same numbers it prints.
- **ONE budget covers other resident-pool subsystems** (`subsystem_stop.rs`): a subsystem (media_index image enrichment,
  which decodes HEIC/RAW) calls `register_subsystem_stop_hook` once at startup, and `stop_all_indexing` runs every hook.
  Deliberate: a second independent 16 GB ceiling over the SAME pool would let the two sum to ~2× real headroom. Hooks
  run INLINE in the stop path, so they must be cheap and non-blocking (flip an atomic cancel flag).
- **Retention cap: at most `MAX_EXTERNAL_INDEX_DBS = 32` external (non-root) index DBs.** `enforce_external_index_cap`
  runs after a successful SMB/MTP enable and LRU-evicts the least-recently-used OFFLINE DBs via the pure,
  filesystem-free `select_evictions`. SAFETY (enforced by the selector, unit-tested): never evict a registered
  (`Running`/`Initializing`) volume's DB nor `root`, no matter how old its mtime. `forget`/`disable`/`clear` are
  lifecycle's, not here.
- **The same enumeration answers what the whole index OCCUPIES** (`Index::disk_footprint`) and which volumes have a
  database. Both read the FILES: ❌ the registry can't be asked, since it can't see a database a search's walk built and
  nothing re-registered. ❌ No size cap, by decision (`docs/specs/unindexed-search-plan.md` Decision 17); coverage that
  would have to be rebuilt from scratch is EVICTED instead, and `DETAILS.md` names the three cases.

Thresholds, the memory-snapshot breakdown, the shared-ceiling rationale, and the LRU + safety logic: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
