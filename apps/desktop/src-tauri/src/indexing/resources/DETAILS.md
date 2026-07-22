# Indexing resources details

Read this before any non-trivial work in `indexing/resources/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in [CLAUDE.md](CLAUDE.md).

These are process-wide caps, a different concern from the per-volume lifecycle in [`../lifecycle`](../lifecycle/CLAUDE.md):
they bound the WHOLE indexing pool, not one volume.

## Resource coordination: ONE global memory budget (memory_watchdog.rs)

The memory watchdog is a single PROCESS-WIDE budget, not per-volume. At 16 GB it stops EVERY registered volume's index
via `state::stop_all_indexing` (snapshot ids, then `stop_indexing` each), not just `root`. Scans run in PARALLEL — the
network/USB wire is the bottleneck, not RAM (real scan memory is the accumulator maps plus the 20K writer channel,
hundreds of MB per normal volume) — so there's no one-at-a-time serialization, just the catastrophe-stop safety net.
`start()` is idempotent (a `WATCHDOG_RUNNING` atomic) so per-volume starts don't each spawn a redundant watchdog.
Constants: `WARN_THRESHOLD = 8 GB`, `STOP_THRESHOLD = 16 GB`, `CHECK_INTERVAL_SECS = 5`. The 16 GB number is machine
protection, NOT expected usage; measuring real peak footprint is deferred to QA. No-op stub on non-macOS.

**The threshold basis is `phys_footprint`, not `resident_size` (RSS).** On macOS, RSS counts GPU/WebView graphics
mappings — the WebKit Metal compositor's `IOAccelerator` region measured ~3.8 GB resident (~79% of a 4.8 GB RSS) during
an FSEvents storm while all malloc zones held ~185 MB (verified via `vmmap`/`footprint`, 2026-07). RSS also diverged
from `phys_footprint` by ~the IOAccelerator size (4.8 GB RSS vs 1.1 GB `phys_footprint`). `phys_footprint` is the metric
macOS keys memory pressure and jetsam on, and what Activity Monitor's "Memory" column shows, so keying the stop on RSS
would let WebView graphics trip the machine-protection stop while indexing's own heap is a couple hundred MB.

The per-tick check reads `phys_footprint` cheaply (one `TASK_VM_INFO` call). When a threshold trips, the watchdog
gathers a full `MemorySnapshot` — `phys_footprint` (+ ledger peak), RSS (+ max), the resident−phys graphics delta, the
summed malloc heap across all zones (in-use + reserved, zone count, largest zone), and `live_event_count` — and logs it
as a multi-line breakdown. The malloc heap vs RSS gap is the single best discriminator: heap ~200 MB while RSS is
multi-GB immediately says "graphics, not indexing." The `index-memory-warning` event carries `resident_gb`,
`phys_footprint_gb`, and `heap_mb` so a shipped error report tells the same story. TODO (tracked in the snapshot's
`live_event_count` comment): surface writer-channel depth and reconciler `pending_events` len once they're atomics.

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
rewritten on every scan/live write), and calls the pure, filesystem-free `select_evictions(candidates, registered, cap)`.

SAFETY, enforced by the selector and unit-tested: a candidate whose volume id is in the registry snapshot
(`all_registered_volume_ids`) is dropped before any eviction decision, so a `Running`/`Initializing` volume's DB is
never evicted no matter how old its mtime; `root` is excluded too. Eviction is a plain unlink of the DB + WAL/SHM (the
volume is offline, no writer to drain), mirroring `clear_index`'s file deletion, and logs what it evicted. Deliberately
simple: not a byte budget, not an access-time LRU — `TODO(retention)` in `select_evictions` flags those if
abandoned-drive accumulation ever proves to need more.

The user-facing forget/disable/clear paths and the prune→Disabled model live in [`../lifecycle/DETAILS.md`](../lifecycle/DETAILS.md)
(`clear_index` / `forget_drive_index` / `disable_drive_index`); retention here is the automatic bounded-accumulation
backstop.
