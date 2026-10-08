# Debugging Cmdr's memory

How to measure Cmdr's memory. For what's already known (the current baseline, past investigations, and the open
follow-ups), start at `docs/notes/performance/README.md`. Read the allocator section below before you measure anything:
getting it wrong has cost multi-day investigations.

## First: which allocator holds the Rust heap

Where the Rust heap shows up depends on the build's global allocator, and the two are mirror images:

- **macOS release and dev builds run on the system allocator.** The Rust heap is the default malloc zone
  (`DefaultMallocZone`), shared with Objective-C and C code, and `vmmap` shows it in the `Malloc *` rows. There is no
  Rust-only number: nothing in the zone tells a Rust block from an Objective-C one.
- **macOS built with `--features mimalloc`, and every macOS build up to 0.48.0, run on mimalloc.** Then the trap below
  applies.
- **Linux builds run on mimalloc**, with no trap: mimalloc names every mapping it makes `mimalloc`
  (`prctl(PR_SET_VMA_ANON_NAME)`), so the heap is the `[anon:mimalloc]` lines of `/proc/<pid>/maps` and `smaps`. That
  needs a kernel with `CONFIG_ANON_VMA_NAME` (5.17+); on an older one the arenas are plain anonymous mappings. (Read
  from mimalloc's `src/prim/unix/prim.c` in `libmimalloc-sys` 0.1.49, v2 and v3 alike, 2026-10-01; not yet observed on a
  running Linux build.)

`memory_diagnostics` says which one it read (`rustHeap.allocator`), and so does the watchdog's memory warning
(`globalAllocator`). Why the split: `crates/cmdr-fs/DETAILS.md` § "Which global allocator". A macOS reading from 0.48.0
or earlier is always mimalloc, so say which build a reading came from.

## The trap, macOS mimalloc builds: `vmmap` reports the Rust heap as `IOAccelerator`

`vmmap` names VM regions by their VM tag. macOS defines `VM_MEMORY_IOACCELERATOR = 100`
(`$(xcrun --show-sdk-path)/usr/include/mach/vm_statistics.h`), and **mimalloc tags every arena it `mmap`s with `os_tag`
= 100 by default**. The tag is a Mach concept, so `os_tag` does nothing on Linux. In a macOS mimalloc build:

> **The `IOAccelerator` rows in Cmdr's `vmmap` / `footprint` output ARE the Rust heap** — not GPU memory, not WebKit,
> not the compositor. Arenas are reserved in 128 MB chunks, so the region COUNT grows in 128 MB steps.

The mirror-image trap: **`MALLOC_*` / `DefaultMallocZone` rows are NOT the Rust heap there.** `malloc_zone_statistics`
and `malloc_get_all_zones` only see registered system zones, and mimalloc isn't one. A snapshot reading "malloc heap 1.6
GB" while `phys_footprint` is 16.5 GB is not a contradiction — it means ~15 GB of Rust heap is invisible to that API.

Consequences worth internalising, because each one burned a day (in macOS mimalloc builds, which every macOS build was
until 0.48.0):

- A backend heap runaway **looks like a GPU/compositor leak**. If you find yourself bisecting CSS, layer promotion, DOM
  churn, or event volume because "the compositor is leaking", stop and re-read this section.
- Real WebKit compositor memory in Cmdr is small: measured at **35.6 MB in 10 allocations** during a climb where the
  process peaked at 646 MB. WebKit's helper processes (`com.apple.WebKit.GPU`, `…WebContent`) hold ~0 `IOAccelerator`;
  if the number is big and it's in the Cmdr process, it's Rust.
- "The balloon popped back on its own" is usually **mimalloc decommitting pages**, not macOS purging GPU surfaces. The
  arena regions stay mapped, so the region count doesn't drop even though dirty bytes collapse.

## The second trap: `IOSurface` in the main process isn't in its footprint

`memory_diagnostics` reports `IOSurface` (tag 88) in the Cmdr process with 30–220 MiB dirty, depending on window size
and timing, while `vmmap` and `footprint` show those same regions at 0 dirty. The regions are WebKit's layer backing
stores: WebKit's GPU process creates them, WebContent owns them and pays for them, and the app process only maps them
for WindowServer. `physFootprintBytes` doesn't move when they grow, so ❌ never subtract them from, or divide them into,
the main process's footprint. To see what the window's pixels cost, read `footprint <WebContent pid>`'s
`Owned physical footprint (unmapped) (graphics)` row. Evidence and a bare-`WKWebView` baseline:
`docs/notes/performance/main-process-iosurface-2026-09-27.md`.

## How to measure: ask the app (start here)

One call to a RUNNING instance answers "how much is it using, and what is it", and it's the only reading that spans
every allocator in the process: it reads the Rust heap from whichever allocator holds it, the malloc zones beyond it,
and the kernel's VM map, which sees them all. It also carries `sqlitePageCache`, the page slab that hides inside the
Rust heap total with nothing else naming it.

```bash
./scripts/mcp-call.sh memory_diagnostics '{}'                  # default: 8 region-size groups per tag
./scripts/mcp-call.sh memory_diagnostics '{"sizesPerTag":24}'  # the full histogram, for fingerprinting
./scripts/mcp-call.sh memory_diagnostics '{"sizesPerTag":0}'   # tag totals only

# The /Applications build: prod's data dir has no instance suffix, so name it outright.
CMDR_DATA_DIR="$HOME/Library/Application Support/com.veszelovszki.cmdr" \
  ./scripts/mcp-call.sh memory_diagnostics '{}'
```

Sort `tags` by `dirtyBytes` and start at the top. `rustHeap` is the Rust heap, tagged by `allocator`: under the system
allocator it's the default zone's `inUseBytes` and `reservedBytes` plus a malloc-wide resident/slack split, and under
mimalloc its `committedBytes` plus the page census (next section). `systemZonesInUseBytes` is every OTHER zone, never
overlapping `rustHeap`, and `physFootprintBytes` is the honest total. Per-tag `sizes` is the fingerprint field (below).
`sizesPerTag` clamps at 24.

⚠️ **The tool and `vmmap` spell the same tags differently.** Cmdr names them after the `VM_MEMORY_*` constants
(`MALLOC_SMALL`, `MALLOC_LARGE`, and in a mimalloc build `IOAccelerator (= our Rust heap: mimalloc arenas)`), while
`vmmap` prints its own display names (`Malloc Small`, `Malloc Large`). Match on the `tag` NUMBER when you compare two
readings, so a rename on either side can't silently line up the wrong rows (verified on macOS 27.0, a live dev instance,
2026-09-22).

The tool is `[AiClient]`, ungated, and ships in release builds on purpose: the interesting numbers only appear in a
shipped build under a real workload. **A release older than the tool can't answer it** — there's no way to add a tool to
a process that's already running, so a live instance predating it has to be measured with `vmmap` below and gets the
tool at its next update. Implementation: `apps/desktop/src-tauri/src/mcp/executor/memory.rs` over the
`get_memory_diagnostics` IPC command, whose module docs say how to read every field.

## Live bytes vs allocator slack

**System allocator builds**: the zones count their live bytes exactly, so there's no census to take, and none is
possible for the Rust heap alone (it shares the default zone). `rustHeap.inUseBytes` is the default zone's live bytes
against `reservedBytes` held. `mallocResidentBytes` is every `MALLOC_*` VM tag's dirty plus swapped bytes, and
`mallocSlackBytes` is that minus every zone's live bytes: what malloc holds beyond live data, in all zones together,
since the VM tags can't say which zone a page belongs to. After a burst, macOS malloc returns freed memory on its own
schedule, sometimes a minute or more later, and `malloc_zone_pressure_relief` doesn't hurry it
(`docs/notes/performance/allocator-slack-release-2026-09-27.md`).

**mimalloc builds**: `rustHeap.committedBytes` and the `IOAccelerator` dirty bytes say what the Rust heap COSTS. Neither
says how much of it the program is using, and at idle roughly half of it isn't (heap attribution, 2026-09-23: 124 MiB
live in 226 MiB of heap). `rustHeap.census` answers that from inside a running app, release builds included:

- `liveBytes`: the blocks in use across every mimalloc page, from a walk of the pages (`mi_heap_visit_blocks` over the
  main heap, which in mimalloc v3 spans every thread).
- `blockSpaceBytes`: the block space those pages have set up. `blockSpaceBytes - liveBytes` is free blocks inside pages
  that are in use.
- `residentBytes`: the heap's VM tag (100, `IOAccelerator`) dirty plus swapped. `residentBytes - blockSpaceBytes` is
  memory mimalloc retains outside any page's blocks.
- `slackBytes`: `residentBytes - liveBytes`, the whole gap in one number.
- `largestLiveBlocks`: every live block of 1 MiB or more, biggest first. The SQLite page slab is the ~64 MiB one;
  anything else in that range is worth naming.

What the census can't see, all small or off in our build: mimalloc's own metadata, pages it took straight from the OS
(only a 2 GiB+ allocation), and blocks another thread freed that the owner hasn't collected yet, which it counts as
live, so `liveBytes` leans high. It runs only when the tool is called, walks without stopping the app, and is bounded to
a million pages. Mechanism and safety argument: `crates/cmdr-fs/src/process_memory/heap_census.rs`.

**Where mimalloc's post-burst slack sits** (verified on mimalloc v3.3.2, a page walk joined to
`mach_vm_page_range_query`, 2026-09-29): inside the spans of pages that still exist, not in free arena slices. Empty
pages idle threads keep until they allocate again (30–95 MiB after a burst), sparse pages a few long-lived blocks pin
(~120 MiB), and 20–50 MiB of free slices not yet purged. `mi_collect(true)` can't reach it: it collects only the CALLING
thread's pages plus slices already scheduled for purging. Evidence and the harness:
`docs/notes/performance/allocator-slack-release-2026-09-27.md`.

❌ Don't read live bytes off mimalloc's own stats (`MIMALLOC_SHOW_STATS`, `mi_stats_print_out` with `MI_STAT`). In v3
they're per-thread counters that merge only when a thread collects or exits, and a free on another thread decrements
that thread's counter: after a 400 MiB search arena was freed they still read 242–265 MiB live (verified on
`libmimalloc-sys` 0.1.49 / mimalloc v3, release build, 2026-09-23).

## How to measure from outside the process (`vmmap`)

For an instance that can't answer the tool, or when you want the raw map:

```
PID=$(pgrep -x Cmdr | head -1)
vmmap -summary "$PID" | grep -E "Physical footprint:|^IOAccelerator |^Malloc "
```

⚠️ **The tags are `Malloc Small` / `Malloc Large`, not `MALLOC_SMALL` / `MALLOC_LARGE`** (verified on macOS 27.0 /
26A428, 2026-09-22). The underscored spelling matches nothing and reads as a clean zero, which is how a baseline once
lost 66 MB of system malloc. The tag also contains a space, so `$4` is no longer the DIRTY column on those rows: slice
the tag by column (it ends at char 32) rather than splitting on whitespace.

`phys_footprint` is the honest total (what Activity Monitor's "Memory" shows and what jetsam keys on). `ps`/RSS lies
here — it keeps counting regions long after `phys_footprint` collapses. Read the DIRTY column (col 4), not VIRTUAL or
RESIDENT.

For a series rather than one reading, `apps/desktop/scripts/mem-sample.sh <label>` runs that recipe and appends a CSV
row (footprint, peak, `IOAccelerator` dirty/swapped, region count, the malloc zones, uptime, and the mimalloc env the
process inherited). Its `rustHeap*` columns are the `IOAccelerator` rows, so they're the Rust heap only in a mimalloc
build; in a system-allocator build read `sysMallocMiB` and `mallocLargeMiB`. `--watch <label> [mins]` samples on a
timer. It targets the `/Applications` build, so a dev build running beside it can't be sampled by accident.

Per-line RAM in the app's own log: launch with `CMDR_LOG_RAM_USE=1` (see `logging.md`), which makes every log line carry
the current footprint — the cheapest way to correlate a climb with what the backend was doing.

## How to name an anonymous block (start here)

A tag total tells you the size. The **per-tag histogram of distinct region sizes** tells you the shape, and the shape is
what names things: macOS gives every allocation past its 127 KB large-zone threshold a VM region sized to the request,
so a repeated exact size is a fingerprint of whatever asked for those bytes.

Against a live app, any build, no app support needed:

```
PID=$(pgrep -x Cmdr | head -1)
vmmap "$PID" | awk '/^Malloc Large / && /\[/ { sub(/.*\[ */, ""); print $1 }' | sort | uniq -c | sort -rn | head -12
```

Same tag-spelling caveat as above, plus a column one: a detail line is
`<tag> <start>-<end> [ VSIZE RSIZE DIRTY SWAP] …`, and a tag with a space in it (`Malloc Large`) shifts every `$N` by
one. Cutting at the `[` instead of counting fields works for any tag. Swap in `IOAccelerator` to fingerprint the Rust
heap the same way.

Known fingerprints so far:

- **`96.5M`** (101,187,584 bytes) — the CLIP text tower's `49,408 × 512` fp32 token embedding. If it's there, the CLIP
  towers are loaded and cost 307–412 MB of `Malloc Large` plus 120–176 MB of `Malloc Small` for the process's whole
  life. Expect `4096K`, `3072K`, and `2304K` in the dozens beside it.
  `docs/notes/performance/idle-malloc-large-clip-towers-2026-08-21.md`.
- **`128.0M` under `IOAccelerator`** (mimalloc builds) — a mimalloc arena. Seven of them in a 25 h prod session
  (2026-09-22). The count is how many arenas the heap has grown to, and it only ever goes up: arenas stay mapped after
  mimalloc decommits the pages inside them, so a flat region count alongside collapsing dirty bytes is normal, not a
  leak.

`memory_diagnostics` returns the same histogram as structured data (`tags[].sizes`, biggest dirty total first), so
`{"sizesPerTag":24}` gives you the fingerprints for every tag at once rather than one `awk` per tag.

## How to attribute (which code allocates)

```
MallocStackLogging=1 MallocStackLoggingNoCompact=1 <launch the app>
vmmap -fullStacks "$PID"        # allocation backtrace per VM region (confirms mimalloc vs anything else)
malloc_history "$PID" -allBySize # biggest live allocations with stacks
```

`malloc_history` only sees system-zone allocations. In a system-allocator build (every macOS build by default) that
includes the Rust heap, so it names Rust call sites directly. In a mimalloc build it can't see them: rebuild without
`--features mimalloc` and the growth reappears as `MALLOC_*`.

## Rules for A/B experiments

- **Every A/B must be a fresh launch.** Startup bursts run until the backend settles; once settled the process is
  effectively immune, so toggling a lever mid-run measures nothing. (A 60 s hard-churn test on a settled instance
  produced zero growth.)
- Restart between conditions and compare peak `phys_footprint`, not a single sample.
- Run-to-run noise is real; trust large deltas and shape (climb-then-settle vs flat), not 10 % differences.

## Known-good ladder (dev, 2026-07-25)

Useful as a sanity baseline when re-testing: with the NAS index resumed, the app peaked at ~646 MB; suppressing the
media-coverage walk alone made the same build flat at ~155 MB. Details and the full investigation:
`docs/notes/performance/memory-runaway-rust-heap-2026-07-25.md`.

## Past investigations and open follow-ups

Every resource-use investigation, the current measured baseline, and the ranked list of open follow-ups live in one hub:
`docs/notes/performance/README.md`. Read it before re-deriving anything.

## Before proposing an allocator setting

Read `crates/cmdr-fs/DETAILS.md` § "Which global allocator" (the current split and its evidence),
`docs/notes/performance/allocator-slack-release-2026-09-27.md` (the post-burst settle that decided it),
`docs/notes/performance/mimalloc-purge-experiment-2026-09-22.md` (what's tunable in mimalloc **v3**, which option names
are real, and how to get options into a Finder-launched app), and
`docs/notes/performance/allocator-comparison-2026-09-23.md` (v3 against v2 and the system allocator). Purge tuning
doesn't move mimalloc's slack, and v2 buys nothing.
