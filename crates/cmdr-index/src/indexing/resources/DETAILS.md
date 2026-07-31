# Indexing resources details

Read this before any non-trivial work in `indexing/resources/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

These are process-wide caps, a different concern from the per-volume lifecycle in `../lifecycle/CLAUDE.md`: they bound
the WHOLE indexing pool, not one volume.

## Resource coordination: ONE global memory budget (memory_watchdog.rs)

The memory watchdog is a single PROCESS-WIDE budget, not per-volume. At 16 GB it stops EVERY registered volume's index
via `state::stop_all_indexing` (snapshot ids, then `stop_indexing` each), not just `root`. Scans run in PARALLEL — the
network/USB wire is the bottleneck, not RAM (real scan memory is the accumulator maps plus the 20K writer channel,
hundreds of MB per normal volume) — so there's no one-at-a-time serialization, just the catastrophe-stop safety net.
`start()` is idempotent (a `WATCHDOG_RUNNING` atomic) so per-volume starts don't each spawn a redundant watchdog; the
atomic is never cleared because the loop now runs for the whole process lifetime. Constants: `WARN_THRESHOLD = 8 GB`,
`STOP_THRESHOLD = 16 GB`, `CHECK_INTERVAL_SECS = 5`, `FIRST_ESCALATION_STEP = 2 GB`. The 16 GB number is machine
protection, NOT expected usage; measuring real peak footprint is deferred to QA. No-op stub on non-macOS.

### The decision logic is pure (`WatchdogState::decide`)

One tick in, one typed `WatchdogAction` out (`Nothing` / `Warn` / `Stop` / `Escalate` / `Recovered`), with no Mach call,
no registry, and no `AppHandle` involved, so thresholds and escalation are unit-testable directly. The loop body just
dispatches to `on_warn` / `on_stop` / `on_escalate`.

**Decision (why the watchdog keeps looping after a stop).** In the 2026-07 runaway the watchdog stopped all indexing at
16 GB and then `return`ed. Nothing watched afterwards, so the climb from 16 GB to 40 GB was unobserved and the app had
to be stopped by hand. A stop is now one event in an endless loop. After a stop the watchdog holds a `PostStop` record
and escalates when `phys_footprint` climbs another 2 GB, then 4, 8, 16: the step doubles so a runaway yields a handful
of proportionate alerts instead of one per 5 s tick (a 16→40 GB climb produces three). Each escalation logs via an
`IndexEvent::Error` (so it reaches shipped error reports), reports the warning with `StillGrowingAfterStop`, and re-runs
`stop_all_indexing` in case a volume registered again. It says plainly that the stop didn't hold, so the growth is not
(only) the index scan. Dropping back under the warn line logs a recovery and clears the record, re-arming the stop.

### What the snapshot measures, and the mimalloc blindness

**The threshold basis is `phys_footprint`, not `resident_size` (RSS).** RSS counts graphics and shared mappings that
aren't real memory pressure; `phys_footprint` is what macOS keys memory pressure and jetsam on and what Activity
Monitor's "Memory" column shows. Keying the stop on RSS would let graphics trip a machine-protection stop.

**Gotcha: the macOS malloc-zone APIs cannot see the Rust heap.** Cmdr sets mimalloc as the global allocator in
`main.rs`, and mimalloc registers no malloc zone, so `malloc_zone_statistics` and `malloc_get_all_zones` report WebKit,
Objective-C, and C-library allocations only. The snapshot used to read exactly those zones and label the result "the
real Rust/C heap; indexing lives here"; in the runaway that printed "malloc heap 1.6 GB" against a 16.5 GB
`phys_footprint`. `crate::process_memory` is the canonical home for this and for all four readers (`query_task_vm_info`,
`query_basic_info`, `query_mimalloc_heap`, `query_system_malloc_zones`); the watchdog holds policy only.

`query_mimalloc_heap` calls `mi_process_info` through a direct `libmimalloc-sys` dependency (the `mimalloc` wrapper
crate doesn't re-export its `ffi` module) for `current_commit` / `peak_commit`. Committed, not in-use: mimalloc exposes
no cheap process-wide in-use total, and committed is what tracks the arenas. Note that `#[global_allocator]` lives in
`main.rs`, so the unit-test harness does NOT run on mimalloc; the blindness test allocates via `mi_malloc` directly to
stay meaningful there.

**Gotcha: `vmmap`'s `IOAccelerator` rows are the Rust heap.** mimalloc `mmap`s its arenas with `os_tag` 100, and macOS
defines `VM_MEMORY_IOACCELERATOR = 100`, so `vmmap` / `footprint` label every 128 MB mimalloc arena `IOAccelerator`
(verified with `MallocStackLogging=1` + `vmmap -fullStacks`: each region backtraces to `mmap` ← `_mi_prim_alloc` ←
`mi_arena_reserve`; commenting out the `#[global_allocator]` collapses those rows to 64 KB and the same memory reappears
as `MALLOC_*`, macOS 15, 2026-07). Reading those rows as GPU memory is what sent three investigations into the frontend.
Any older analysis that split "GPU vs heap" off a zone-only heap reading inherits this error.

When a threshold trips, the watchdog captures a `MemorySnapshot` — `phys_footprint` (+ ledger peak), RSS (+ max), the
mimalloc heap (+ peak), the system malloc zones (in use + reserved, zone count, largest zone), the `untracked`
remainder, and `live_event_count` — and logs it as a multi-line breakdown where every line states what its number MEANS.

**Decision (why the verdict is derived, not asserted).** The old report ended with "a large resident−phys_footprint
delta usually means WebView/GPU memory, not the indexing heap", printed unconditionally. In the runaway that delta was
0.00 GB and the memory was the Rust heap, so the log confidently said the opposite of the truth and cost two days. The
`verdict` line now comes from `MemoryAttribution::classify(phys_footprint, rust_heap, system_malloc)`, a pure function
over the same figures the report prints: whichever source holds a majority wins (`RustHeap` / `SystemMalloc` /
`Unattributed`), otherwise `Mixed`. Graphics is only ever named when neither allocator claims the majority. If you add a
hint here, derive it from the numbers or leave it out.

The `index-memory-warning` event carries the five figures in bytes plus a typed `MemoryWatchdogAction`; see
`../events/DETAILS.md`. TODO (tracked in the snapshot's `live_event_count` comment): surface writer-channel depth and
reconciler `pending_events` len once they're atomics.

### The shared ceiling (subsystem_stop.rs)

That one budget covers OTHER resident-pool subsystems too: a subsystem (image enrichment in `media_index/`, which
decodes HEIC/RAW and can spike RAM) calls `register_subsystem_stop_hook` once at startup, and `stop_all_indexing` runs
`run_subsystem_stop_hooks` alongside stopping indexing. This is deliberate — a second independent 16 GB ceiling over the
same pool would let the two sum to ~2× real headroom. `STOP_HOOKS` is a process-global, append-only `Vec` (a subsystem
registers once and never unregisters; it lives for the process). Hooks run inline in the stop path, so they must be
cheap and non-blocking (flip an atomic cancel flag).

## Index retention and cleanup (retention.rs)

Local disk has exactly one index DB; every SMB share and MTP storage spawns its own `index-{volume_id}.db`, so the data
dir can accumulate one DB per drive the user ever connected. `retention.rs` bounds that.

A simple COUNT cap (`MAX_EXTERNAL_INDEX_DBS = 32`) on external (non-root) index DBs, with LRU eviction of the
least-recently-used OFFLINE ones. `enforce_external_index_cap(app)` runs after a successful SMB/MTP enable (exactly when
accumulation can grow): it enumerates `index-*.db` in the data dir, pairs each with its mtime (the LRU proxy — a DB is
rewritten on every scan/live write), and calls the pure, filesystem-free
`select_evictions(candidates, registered, cap)`.

SAFETY, enforced by the selector and unit-tested: a candidate whose volume id is in the registry snapshot
(`all_registered_volume_ids`) is dropped before any eviction decision, so a `Running`/`Initializing` volume's DB is
never evicted no matter how old its mtime; `root` is excluded too. Eviction is a plain unlink of the DB + WAL/SHM (the
volume is offline, no writer to drain), mirroring `clear_index`'s file deletion, and logs what it evicted. Deliberately
simple: not a byte budget, not an access-time LRU — `TODO(retention)` in `select_evictions` flags those if
abandoned-drive accumulation ever proves to need more.

The user-facing forget/disable/clear paths and the prune→Disabled model live in `../lifecycle/DETAILS.md` (`clear_index`
/ `forget_drive_index` / `disable_drive_index`); retention here is the automatic bounded-accumulation backstop.
