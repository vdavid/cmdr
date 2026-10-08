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

### What the snapshot measures, and which allocator it reads

**The threshold basis is `phys_footprint`, not `resident_size` (RSS).** RSS counts graphics and shared mappings that
aren't real memory pressure; `phys_footprint` is what macOS keys memory pressure and jetsam on and what Activity
Monitor's "Memory" column shows. Keying the stop on RSS would let graphics trip a machine-protection stop.

**The Rust heap comes from whichever allocator is global** (`crates/cmdr-fs/DETAILS.md` § "Which global allocator"): the
default malloc zone's reserved bytes under the system allocator (macOS by default), mimalloc's committed bytes under
mimalloc. `query_rust_heap` returns an enum naming which, and `query_system_malloc_zones` reads the zones beyond it, so
the two never overlap. `crate::process_memory` is the canonical home for the readers (`query_task_vm_info`,
`query_basic_info`, `query_rust_heap`, `query_system_malloc_zones`); the watchdog holds policy only.

**Gotcha, mimalloc builds: the macOS malloc-zone APIs cannot see the Rust heap.** mimalloc registers no malloc zone, so
`malloc_zone_statistics` and `malloc_get_all_zones` report WebKit, Objective-C, and C-library allocations only. The
snapshot used to read exactly those zones and label the result "the real Rust/C heap; indexing lives here"; in the
runaway that printed "malloc heap 1.6 GB" against a 16.5 GB `phys_footprint`.

**Gotcha, mimalloc builds: `vmmap`'s `IOAccelerator` rows are the Rust heap.** mimalloc `mmap`s its arenas with `os_tag`
100, and macOS defines `VM_MEMORY_IOACCELERATOR = 100`, so `vmmap` / `footprint` label every 128 MB mimalloc arena
`IOAccelerator` (verified with `MallocStackLogging=1` + `vmmap -fullStacks`: each region backtraces to `mmap` ←
`_mi_prim_alloc` ← `mi_arena_reserve`; on the system allocator those rows collapse to 64 KB and the same memory
reappears as `MALLOC_*`, macOS 15, 2026-07). Reading those rows as GPU memory is what sent three investigations into the
frontend. Any older analysis that split "GPU vs heap" off a zone-only heap reading inherits this error. The report's
last line tells a `vmmap` reader which rows hold the heap in the running build.

When a threshold trips, the watchdog captures a `MemorySnapshot` (`memory_snapshot.rs`) — `phys_footprint` (+ ledger
peak), RSS (+ max), the Rust heap (mimalloc's committed + peak, or the default zone's in use + reserved), the other
malloc zones (in use + reserved, zone count, largest zone), the `untracked` remainder, and `live_event_count` — and logs
it as a multi-line breakdown where every line states what its number MEANS.

**Decision (why the verdict is derived, not asserted).** The old report ended with "a large resident−phys_footprint
delta usually means WebView/GPU memory, not the indexing heap", printed unconditionally. In the runaway that delta was
0.00 GB and the memory was the Rust heap, so the log confidently said the opposite of the truth and cost two days. The
`verdict` line now comes from `MemoryAttribution::classify(phys_footprint, rust_heap, system_malloc)`, a pure function
over the same figures the report prints: whichever source holds a majority wins (`RustHeap` / `SystemMalloc` /
`Unattributed`), otherwise `Mixed`. Graphics is only ever named when neither allocator claims the majority. If you add a
hint here, derive it from the numbers or leave it out.

The `index-memory-warning` event carries the five figures in bytes, the `GlobalAllocator` they came from, and a typed
`MemoryWatchdogAction`; see `../events/DETAILS.md`. TODO (tracked in the snapshot's `live_event_count` comment): surface
writer-channel depth and reconciler `pending_events` len once they're atomics.

### The shared ceiling (subsystem_stop.rs)

That one budget covers OTHER resident-pool subsystems too: a subsystem calls `register_subsystem_stop_hook` once at
startup, and `stop_all_indexing` runs `run_subsystem_stop_hooks` alongside stopping indexing. Two register today: image
enrichment in `media_index/` (it decodes HEIC/RAW and can spike RAM), and the `importance/` scheduler (a full pass holds
a transient ~166 MB on a big volume). This is deliberate — a second independent 16 GB ceiling over the same pool would
let the two sum to ~2× real headroom. `STOP_HOOKS` is a process-global, append-only `Vec` (a subsystem registers once
and never unregisters; it lives for the process). Hooks run inline in the stop path, so they must be cheap and
non-blocking (fire a cancellation token).

**The hooks run FIRST, before any volume is stopped.** Each `stop_indexing` drains its volume for up to seconds, one
after another, and a hook only fires a signal and returns. Run last, a subsystem busy on the fifth volume would keep
allocating through four drains of an emergency stop. ❌ Don't move them back behind the drains, and don't let a hook
block: it would delay every volume's stop.

## Index retention and cleanup (retention.rs)

Local disk has exactly one index DB; every SMB share and MTP storage spawns its own `index-{volume_id}.db`, so the
drive-index dir can accumulate one DB per drive the user ever connected. `retention.rs` bounds that.

A simple COUNT cap (`MAX_EXTERNAL_INDEX_DBS = 32`) on external (non-root) index DBs, with LRU eviction of the
least-recently-used OFFLINE ones. `enforce_external_index_cap(app)` runs after a successful SMB/MTP enable (exactly when
accumulation can grow): it enumerates `index-*.db` in the drive-index dir, pairs each with its mtime (the LRU proxy — a
DB is rewritten on every scan/live write), and calls the pure, filesystem-free
`select_evictions(candidates, registered, cap)`.

SAFETY, enforced by the selector and unit-tested: a candidate whose volume id is in the registry snapshot
(`all_registered_volume_ids`) is dropped before any eviction decision, so a `Running`/`Initializing` volume's DB is
never evicted no matter how old its mtime; `root` is excluded too. An evicted volume is a FORGOTTEN one: its files go
through `volume_files::remove` with `Removal::Forgotten`, the same door `clear_index` uses, so the importance database
beside the index goes with it and the importance writer lets go first (the volume is offline, so there is no index
writer to drain). ❌ Never unlink an `index-{id}.db` here by hand. Deliberately simple: not a byte budget, not an
access-time LRU — `TODO(retention)` in `select_evictions` flags those if abandoned-drive accumulation ever proves to
need more.

`sweep_legacy_scheme_dbs` is the other automatic removal, one shot per launch: every store's files keyed by a volume ID
from the retired scheme, removed as `Removal::Unreachable`. It reads the ids from every store's files and never from the
index's alone, so a sibling whose index an earlier forget already took is swept too. Which stores each reason takes, and
why: `crates/cmdr-index/DETAILS.md` § "A volume's files, and the one door they leave by".

The user-facing forget/disable/clear paths and the prune→Disabled model live in `../lifecycle/DETAILS.md` (`clear_index`
/ `forget_drive_index` / `disable_drive_index`); retention here is the automatic bounded-accumulation backstop.

### What it all takes up, and clearing it (the settings screen)

`total_index_db_bytes` and `volume_ids_on_disk` answer over the files of every store a forgotten volume loses
(`volume_files::volume_ids_on_disk` with `Removal::Forgotten`), `root` included. They exist because the REGISTRY can't
answer either question: a database a search's walk built has no instance behind it the moment the app restarts, and
neither does the index of a drive whose indexing the user turned off. Those are exactly the bytes a person is entitled
to see and reclaim, so the settings row and its Clear button read the files (`Index::disk_footprint`,
`Index::forget_all_volumes`). Both read the SAME set, so the number shown is the number a clear takes to zero, and it
includes an importance database whose index is already gone.

There is **no size cap** on any of it, by decision (`docs/specs/unindexed-search-plan.md` Decision 17): the answer to
disk use is that the size and the Clear button work with drive indexing off, not a byte budget. If people complain about
disk use, a cap can come later — which is the `TODO(retention)` above, one policy for both concerns.

### Rebuilt-from-scratch coverage is EVICTED, not refilled (Decision 17)

Three things invalidate a whole index rather than part of one, and all three now drop the database instead of writing
over it:

- **A schema change** — `IndexStore::open` deletes and recreates the file (`../store/connection.rs`), rather than
  `DROP`-ing tables and stranding the freed pages.
- **A store that won't open or read** — the volume goes `Failed`, and the next start forgets it before rebuilding
  (`Index::start_volume`).
- **An exclusion policy the index predates** — the one this effort had to wire. The stamp says which policy the rows
  were written under, and a mismatch means NOTHING in the index may be trusted as covered
  (`../scanner/exclusions.rs::index_predates_exclusion_policy`), so every search re-walks the whole scope forever while
  the untrusted rows sit underneath, each frontier root landing on the slow non-virgin repair path. A full scan would
  fix it by truncating and re-stamping — but a walk-built index is precisely the one no scan is coming for. So a
  writer-only start (a search's walk, `Activation::WriterOnly`) evicts a database that predates the policy and lets the
  walk fill a clean one, which `prepare_database_for_a_walk` can then stamp.

The cost, stated plainly: a drive with a real scanned index that nobody is indexing right now loses it the first time
someone searches that drive after a release edits the exclusion lists. That index was already worthless to search (its
coverage claims were all refused), and folder sizes come back as the walk covers ground. The alternative was a drive
that never converges again.
