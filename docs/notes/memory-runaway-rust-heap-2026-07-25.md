# Memory runaway: it's the Rust heap, not GPU memory (2026-07-25)

Earlier write-ups of this incident concluded the runaway was WebKit GPU compositor memory. **That conclusion was
wrong**, for the measurement reason in the next section. Everything worth keeping from them is folded in here; the
reusable measurement recipes now live in `docs/tooling/memory-debugging.md`.

## The measurement trap (read this first)

`vmmap` labels VM regions by their **VM tag number**. macOS defines `VM_MEMORY_IOACCELERATOR = 100`
(`$(xcrun --show-sdk-path)/usr/include/mach/vm_statistics.h:642`). **mimalloc tags every arena it `mmap`s with `os_tag`
= 100 by default.** Cmdr's Rust global allocator is mimalloc (`src-tauri/src/main.rs`), so:

> **In any Cmdr `vmmap` / `footprint` output, the `IOAccelerator` rows ARE the Rust heap.** Not GPU memory, not WebKit,
> not the compositor.

That single mislabel produced two days of wrong-direction investigation across three agents.

Proof, three independent ways:

1. **Allocation backtraces.** Launch with `MallocStackLogging=1`, then `vmmap -fullStacks <pid>`. Every 128 MB
   `IOAccelerator` region's stack is
   `mmap ← unix_mmap ← _mi_prim_alloc ← mi_os_prim_alloc ← mi_reserve_os_memory_ex2 ← mi_arena_reserve ← _mi_malloc_generic ← <Rust GlobalAlloc>`.
   128 MB is mimalloc's default arena reservation size.
2. **Swap the allocator.** Comment out the `#[global_allocator]` in `main.rs` and the `IOAccelerator` rows collapse to
   64 KB / 2 regions, while the same growth reappears as `MALLOC_SMALL` + `MALLOC_LARGE`. Same memory, different label.
3. **Process attribution.** During a full climb, WebKit's helper processes hold nothing: `com.apple.WebKit.GPU` ≈ 64 KB
   `IOAccelerator`, `com.apple.WebKit.WebContent` = 0. All of it is in the Cmdr process.

Corollary: the external "128 MB IOAccelerator slab leak" reports the earlier note cites (Bun #28234, claude-code #35804)
are very probably the same mislabel — Bun also uses mimalloc. They are not evidence about Cmdr.

## What the bug actually is

**A large, fast transient allocation in the Rust backend during the startup indexing window**, gated by drive indexing
and dominated by the SMB/NAS index. Measured in dev (fresh launch each time, peak `phys_footprint`):

- Floor, SMB index suppressed: **~148 MB, completely flat** (no climb at all).
- SMB (NAS) index resumed at launch: **~560–660 MB**, climbing ~1 slab/s for ~20 s, then freeing back.
- Plus local drive indexing: **~1.0 GB**.
- Prod, real NAS index (1.88 GB DB, 11.3 M entries): **6.7 GB**, and in the worst observed run **50 GB, which never came
  back down and had to be killed**.

Behaviour that used to look mysterious, now explained:

- **"It pops back to 0.5 GB on its own"** = mimalloc decommitting pages after the burst. The arena _regions_ stay
  mapped, which is why the region COUNT never drops while dirty bytes collapse.
- **"The watchdog says malloc heap 1.6 GB while phys is 16.5 GB"** = `malloc_zone_statistics` only sees _system_ malloc
  zones. mimalloc is not a registered zone, so **the watchdog is structurally blind to ~all of Cmdr's real heap**.
- **No frontend lever ever mattered** (view mode, window size, DOM churn, CSS animations, the `index-dir-updated` and
  `index-aggregation-complete` refresh handlers, emit volume, CPU contention): the frontend was never involved.

## Ruled out this session (fresh-launch A/B, clean 148 MB floor)

Positive controls added to the flat floor — both **negative**, which is what finally broke the GPU theory:

- 10 Hz listener-less backend `emit` for 30 s → flat.
- 6 CPU-burning threads for 30 s → flat.

Subtractive tests against the SMB-index-on condition, all **no effect**:

- Both frontend index-refresh paths disabled (`initIndexEvents(handleIndexDirUpdated)` + `index-aggregation-complete` →
  `refreshIndexSizes`).
- All CSS animations and transitions killed (`* { animation: none !important; transition: none !important }`).
- Importance scheduler disabled (`importance::scheduler::start`): peak 625 MB vs 646 MB baseline — noise.

## Root cause (CONFIRMED, FIXED): `coverage::get_or_build` materialized every image path in the volume

> **Fixed.** Three changes, all in `media_index` (contract + rationale:
> `apps/desktop/src-tauri/src/media_index/DETAILS.md` § Covered-count preview): counting is now a sink over
> `enrich::for_each_qualifying_image` (`coverage::count_qualifying_images`, `O(folders)`, no per-image path `String`);
> polls and startup paths read `coverage::cached`, which never walks, so `volume_state` can't trigger a cold build and
> image indexing being off means no walk at all; and `get_or_build` deduplicates concurrent cold callers behind a
> per-volume build lock. The diagnosis below is preserved as-is.

**Single-lever proof.** With `coverage::get_or_build` short-circuited to `None` and _everything else at defaults_ (NAS
index resumed, local drive indexing on, importance on, search weights on), a fresh launch stays **flat at 154.8 MB** —
86 → 141 → 154.8 MB, then unchanged for the whole sample window. The same build with coverage active peaks at **646
MB**. That one function is essentially the entire burst.

Full-mode `malloc_history` at peak puts the two largest allocation sites (**115 MB across ~693 000 `format!()` calls**,
far ahead of everything else) on one stack:

```
media_index::commands::volume_state              (IPC command, spawn_blocking)
  └ media_index::coverage::get_or_build          (coverage.rs:45)
      └ indexing::read::enrichment::ReadPool::with_conn
          └ media_index::scheduler::enrich::walk_image_entries   (enrich.rs:97)
              └ emit_qualifying_group → join_path → format!()
```

`get_or_build` needs only per-folder **counts**, but gets them by materializing the whole qualifying set first:

```rust
let images = pool.with_conn(walk_image_entries).ok()?.ok()?;  // Vec<ImageEntry>, one String path per image
let counts = Arc::new(build_counts(&images));                 // …then reduced to counts
```

`walk_image_entries` also pulls **every directory row** into a `Vec` plus an `id → &row` `HashMap`, streams **every file
row** in the index, and allocates a fresh absolute-path `String` per qualifying image. So peak heap scales with the
volume's image count: fine for a local disk, catastrophic for the NAS index (**11.3 M images**, inflated ~10× by the
un-excluded `@Recently-Snapshot` tree) — gigabytes of `String`s to produce a handful of integers.

Why every earlier lever missed it:

- **It is not gated by the image-indexing toggle.** `volume_state` (`commands.rs:179`) calls `get_or_build`
  unconditionally; `enabled` is read but only feeds the returned struct. Image indexing was OFF all session and it ran
  anyway.
- **It is not the media scheduler.** Disabling `media_index::scheduler::start` doesn't touch this path — it's reached
  from the IPC command, which is why last night's scheduler test came back clean.
- **It needs an index to walk**, which is exactly the drive-indexing gate, and the NAS index dominates because it has
  ~50× the entries.
- **No pane on the NAS is needed** — coverage is per volume, not per pane.

Magnitude note: `get_or_build` checks the cache, drops the lock, then walks. Concurrent callers for the same volume each
run their own full walk with its own multi-GB `Vec`, so several overlapping calls multiply the peak — a plausible route
from "a few GB" to the observed 50 GB.

## Earlier attribution detail (smaller contributors)

With mimalloc disabled + `MallocStackLogging=1`, `malloc_history <pid> -allBySize` at ~20 s into a climb attributes the
Rust-side allocations to:

- **`rusqlite` `String::from_sql`, 159 157 calls / 20.2 MB** — the call count matches the log line
  `importance weights loaded for 'root': 159007 scored folders` exactly.
- **`hashbrown` `HashMap<String, f64>` reserve/rehash, 3 947 calls / 19.3 MB** — that map (path → score).
- SQLite page cache (`pcache1Alloc`), 19.3 MB.

So the prime suspects are the **path-keyed, per-volume structures that scale with entry count** — the search ranker's
importance-weight map (`search::start_importance_weight_subscriber`) and the importance walk
(`importance/scheduler/recompute.rs::walk_index_folders`, which materializes all directories). On the NAS these scale
with an 11.3 M-entry index, inflated ~10× by the `@Recently-Snapshot` QNAP pseudo-tree that indexing does not exclude.

Note `malloc_history` lite mode only accounted for ~180 MB of ~450 MB live, so this attribution is partial. Finish it
before committing to a fix.

## How to measure this correctly

```
PID=$(pgrep -x Cmdr | head -1)
vmmap -summary "$PID" | grep -E "Physical footprint:|^IOAccelerator |^MALLOC_SMALL |^MALLOC_LARGE "
#   IOAccelerator == the Rust heap (mimalloc arenas). Read its DIRTY column (col 4).
#   Region COUNT (last col) only ever grows: arenas stay mapped after decommit.

MallocStackLogging=1 <launch>          # then:
vmmap -fullStacks "$PID"               # per-region allocation backtraces
malloc_history "$PID" -allBySize       # biggest live allocations, but ONLY for system-zone allocs,
                                       # so comment out the #[global_allocator] in main.rs first
```

**Every A/B must be a fresh launch.** The burst runs from launch until the backend settles; once it has settled the
process is immune, so any lever toggled mid-run measures nothing (60 s of hard FS churn on a settled instance produced
zero growth). This invalidates most mid-run experiments in the older notes.

Dev repro: `pnpm --filter @cmdr/desktop tauri dev -m`, both panes as persisted, drive indexing on. The runner script
used for these A/Bs (kill → relaunch → sample) is worth recreating; each run costs ~2–4 min because `tauri dev`
recompiles.

## Bugs found along the way — ALL FIXED (verified against `main`, 2026-07-29)

Each was open when this note was written and has since landed. Kept because the symptom is what a future investigation
will recognize; the pointer says where the answer now lives.

1. **`indexing.enabled: false` did not stop the SMB index** — a user who turned drive indexing off still paid the NAS
   index cost at every launch. Fixed: `smb_index_was_enabled` now gates on
   `master::drive_index_should_run(master::master_enabled(), …)`, and `start_indexing_for_smb` refuses outright when the
   master switch is off (`indexing/transports/smb/index.rs`).
2. **The memory watchdog could not see the heap it polices.** Fixed: `process_memory::query_mimalloc_heap` reads
   mimalloc's own accounting, and `MemoryAttribution::classify` derives the verdict from `phys_footprint`, the Rust
   heap, and the system zones plus an `untracked` remainder.
3. **The watchdog was one-shot** (it `return`ed after the stop). Fixed: the loop runs for the process lifetime and
   escalates at +2/4/8/16 GB, re-arming below the warn line (`indexing/resources/memory_watchdog.rs`).
4. **`@Recently-Snapshot` (QNAP) and `#snapshot` (Synology) were not excluded from SMB indexing**, inflating the NAS
   index ~10×. Fixed: `indexing/network_scanner/system_dirs.rs` holds the typed skip list.

## The root-cause fix — LANDED

`coverage::get_or_build` no longer materializes every image path. Counting is a sink over
`enrich::for_each_qualifying_image` (`coverage::count_qualifying_images`, O(folders), no per-image path `String`); polls
and startup paths read `coverage::cached`, so `volume_state` can't trigger a cold build; and concurrent cold callers
dedupe behind a per-volume build lock. Contract and rationale: `apps/desktop/src-tauri/src/media_index/DETAILS.md`
§ Covered-count preview.

## Still open

- **mimalloc's `os_tag` still collides with `VM_MEMORY_IOACCELERATOR` (100)**, so `vmmap` keeps reporting the Rust heap
  under a GPU name. Setting a non-colliding tag would retire the trap at the top of this note for good. It has cost
  three investigations; the counter-argument is that `docs/tooling/memory-debugging.md` now documents it loudly, and
  changing the tag invalidates that documentation everywhere it appears.

## Later work this note seeded

A 2026-07-28 profile of prod v0.36.2 (2.5 GB idle) found two causes NOT covered here: SQLite page cache across ~156
thread-local connections, and a 60-second importance rescore treadmill. Evidence and fixes:
`docs/specs/memory-diet-plan.md`.
