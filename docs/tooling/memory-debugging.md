# Debugging Cmdr's memory

Start here for any "Cmdr is using too much RAM" report. Read the trap section before you measure anything: getting it
wrong has cost multi-day investigations.

## The trap: `vmmap` reports Cmdr's Rust heap as `IOAccelerator`

`vmmap` names VM regions by their VM tag. macOS defines `VM_MEMORY_IOACCELERATOR = 100`
(`$(xcrun --show-sdk-path)/usr/include/mach/vm_statistics.h`), and **mimalloc tags every arena it `mmap`s with `os_tag`
= 100 by default**. Cmdr's Rust global allocator is mimalloc (`apps/desktop/src-tauri/src/main.rs`), so:

> **The `IOAccelerator` rows in Cmdr's `vmmap` / `footprint` output ARE the Rust heap** — not GPU memory, not WebKit,
> not the compositor. Arenas are reserved in 128 MB chunks, so the region COUNT grows in 128 MB steps.

The mirror-image trap: **`MALLOC_*` / `DefaultMallocZone` rows are NOT Cmdr's heap.** `malloc_zone_statistics` and
`malloc_get_all_zones` only see registered system zones, and mimalloc isn't one. A snapshot reading "malloc heap 1.6 GB"
while `phys_footprint` is 16.5 GB is not a contradiction — it means ~15 GB of Rust heap is invisible to that API.

Consequences worth internalising, because each one burned a day:

- A backend heap runaway **looks like a GPU/compositor leak**. If you find yourself bisecting CSS, layer promotion, DOM
  churn, or event volume because "the compositor is leaking", stop and re-read this section.
- Real WebKit compositor memory in Cmdr is small: measured at **35.6 MB in 10 allocations** during a climb where the
  process peaked at 646 MB. WebKit's helper processes (`com.apple.WebKit.GPU`, `…WebContent`) hold ~0 `IOAccelerator`;
  if the number is big and it's in the Cmdr process, it's Rust.
- "The balloon popped back on its own" is usually **mimalloc decommitting pages**, not macOS purging GPU surfaces. The
  arena regions stay mapped, so the region count doesn't drop even though dirty bytes collapse.

## How to measure

```
PID=$(pgrep -x Cmdr | head -1)
vmmap -summary "$PID" | grep -E "Physical footprint:|^IOAccelerator |^MALLOC_SMALL |^MALLOC_LARGE "
```

`phys_footprint` is the honest total (what Activity Monitor's "Memory" shows and what jetsam keys on). `ps`/RSS lies
here — it keeps counting regions long after `phys_footprint` collapses. Read the DIRTY column (col 4), not VIRTUAL or
RESIDENT.

Per-line RAM in the app's own log: launch with `CMDR_LOG_RAM_USE=1` (see `logging.md`), which makes every log line carry
the current footprint — the cheapest way to correlate a climb with what the backend was doing.

## How to name an anonymous block (start here)

A tag total tells you the size. The **per-tag histogram of distinct region sizes** tells you the shape, and the shape is
what names things: macOS gives every allocation past its 127 KB large-zone threshold a VM region sized to the request,
so a repeated exact size is a fingerprint of whatever asked for those bytes.

Against a live app, any build, no app support needed:

```
PID=$(pgrep -x Cmdr | head -1)
vmmap "$PID" | awk '$1 == "MALLOC_LARGE" { print $4 }' | sort | uniq -c | sort -rn | head -12
```

Known fingerprints so far:

- **`96.5M`** (101,187,584 bytes) — the CLIP text tower's `49,408 × 512` fp32 token embedding. If it's there, the CLIP
  towers are loaded and cost 307–412 MB of `MALLOC_LARGE` plus 120–176 MB of `MALLOC_SMALL` for the process's whole
  life. Expect `4096K`, `3072K`, and `2304K` in the dozens beside it.
  `docs/notes/idle-malloc-large-clip-towers-2026-08-21.md`.

In a dev build, `get_memory_diagnostics(sizesPerTag)` returns the same histogram as structured data plus the footprint
and BOTH allocators' own accounting in one payload — the only reading that spans mimalloc and the system zones at once.
It's an ordinary IPC command (`apps/desktop/src-tauri/src/commands/memory_diagnostics.rs`), macOS only, and its module
docs say how to read the payload.

## How to attribute (which code allocates)

```
MallocStackLogging=1 MallocStackLoggingNoCompact=1 <launch the app>
vmmap -fullStacks "$PID"        # allocation backtrace per VM region (confirms mimalloc vs anything else)
malloc_history "$PID" -allBySize # biggest live allocations with stacks
```

`malloc_history` only sees system-zone allocations, so mimalloc hides Rust allocations from it. To attribute a Rust heap
problem, temporarily comment out the `#[global_allocator]` in `main.rs` and rebuild: the growth reappears as `MALLOC_*`
and `malloc_history` can name the call sites. Revert afterwards.

## Rules for A/B experiments

- **Every A/B must be a fresh launch.** Startup bursts run until the backend settles; once settled the process is
  effectively immune, so toggling a lever mid-run measures nothing. (A 60 s hard-churn test on a settled instance
  produced zero growth.)
- Restart between conditions and compare peak `phys_footprint`, not a single sample.
- Run-to-run noise is real; trust large deltas and shape (climb-then-settle vs flat), not 10 % differences.

## Known-good ladder (dev, 2026-07-25)

Useful as a sanity baseline when re-testing: with the NAS index resumed, the app peaked at ~646 MB; suppressing the
media-coverage walk alone made the same build flat at ~155 MB. Details and the full investigation:
`docs/notes/memory-runaway-rust-heap-2026-07-25.md`.

## Past investigations

Read these before re-deriving anything; between them they cover every cause found so far.

- `docs/notes/idle-memory-profile-2026-07-28.md` — the STEADY-STATE costs (2.5 GB idle): SQLite page cache across many
  thread-local connections, and the importance rescore treadmill. Start here for "it's high but not climbing".
- `docs/notes/memory-runaway-rust-heap-2026-07-25.md` — the RUNAWAY (up to 50 GB): a walk that materialized every image
  path. Also the origin of the `IOAccelerator` trap above.
- `docs/notes/idle-malloc-large-clip-towers-2026-08-21.md` — the leading candidate for the 643 MB `MALLOC_LARGE` in that
  idle profile: Core ML holding the two CLIP towers, at a measured 307–412 MB, which nothing had been able to name
  because Core ML allocates through the SYSTEM allocator and so falls between both of Cmdr's allocator APIs. Also the
  origin of the region-histogram method above.
- `docs/notes/high-memory-gpu-compositor-investigation-2026-07.md` — superseded; its conclusion is wrong (it read the
  mislabel as GPU memory). Kept for the measurement methodology only.
