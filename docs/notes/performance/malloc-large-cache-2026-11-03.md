# Turning off libmalloc's large-block cache with `MallocLargeCache=0` (2026-11-03)

**What this settles:** #864, option (a) of #742: whether the shipped app should run with `MallocLargeCache=0`.
**Verdict: ship it, for the whole app, through `LSEnvironment` in `apps/desktop/src-tauri/Info.plist`.** Search and
listing throughput hold within noise. The burst peak drops from ~1.1 GiB to ~730 MiB, a restic index load holds half the
footprint, and the blocks that used to sit parked for minutes are gone within seconds. The one measurable cost is ~4% on
a restic file read over a warm mount. Raw numbers: `malloc-large-cache-2026-11-03-memory.csv` and
`malloc-large-cache-2026-11-03-timings.csv`.

Verified on macOS 27.0.1 (26A434), M3 Max, 64 GB, release build of `9313f09438` on the system allocator, 2026-11-03,
`vmmap -summary` plus `memory_diagnostics` and timed MCP calls. The knob is undocumented, so it has a re-check per macOS
release (`docs/maintenance.md` § "Per macOS release").

## Machine state

The quietest machine any note here has had: no sibling agents, load average 3.7–5.3 across the runs. In the background,
untouched: the dev `target/debug/Cmdr`, prod 0.58.0, and a JetBrains `chatter` process at about one core. Swap in use
~26 GiB, flat, with 53% of RAM free. So these settles are a floor: prior notes' readings at load 5–99 aren't comparable.

## Method

- **One binary, two arms**: the same release build, with and without `MallocLargeCache=0` in its environment, each a
  fresh launch on a fresh APFS clone.
- **Data**: a snapshot of prod's root index (7,637,455 rows) by the hub's read-transaction recipe, `main.db`,
  `importance-root.db`, and settings with analytics, updates, reports, network, media indexing, Ask Cmdr, and the global
  shortcut off. Secrets, MCP files, the crash report, and every SMB database stayed behind. `lsof` showed no prod file
  and no port-445 socket.
- **Memory, four rounds, arms side by side, start order rotating**: idle 8 min, three bursts (MCP search for `*.pdf`,
  the 200,000-entry folder, `~`, the 100,000-entry folder, `~`), census at the end of the bursts and +1, +5, and +15
  min.
- **Throughput, eight solo passes in ABBA order**: only one instance running, 3 min settle, three repeats of each step
  below, timed from the MCP client. Side-by-side arms aren't usable for this: their timings came out identical to the
  millisecond in places, so something is shared between them.
- **Restic**: `crates/cmdr-restic/examples/open_cost.rs`, which refuses and counts writes (zero), against
  `/Volumes/testshare/backups/test-restic-repo` through the OS mount: 18 snapshots, 64 index files, 162.5 MB of
  `index/`. `lifecycle` is unlock, enter `latest`, browse three levels, close the view, sample every 30 s past the idle
  drop. Three paired runs side by side, then interleaved `load` and `full` runs one process at a time.

## Memory

MiB footprint, median [min–max] over four rounds. "Peak" is `vmmap`'s peak since launch, which prints `1.1G` for every
arm-off round.

- **Idle 8 min**: off 122 [120–122]; on 116 [116–120].
- **Peak so far, at idle**: off 170 [170–176]; on 150 [147–153].
- **Burst peak**: off ~1,126 in all four; on 731 [725–764]. The solo passes agree: off 1.1G, on 726–745.
- **End of the bursts**: off 1,024 [706–1,126]; on 571 [569–579]. Live heap is 549 MiB in both.
- **`Malloc Large (empty)` at the end of the bursts**: off 131, 462, 456, 482; on no such row, every time.
- **+1 min**: off 164 [135–452]; on 133 [131–145]. Malloc slack: off 18–380, on 17–18.
- **+5 min**: off 142 [137–186]; on 137 [131–148].
- **+15 min**: off 145 [137–191]; on 137 [133–154].

So at rest on a quiet machine the two converge, with arm off sometimes keeping ~50 MiB. What the knob removes is the
~400 MiB transient and its tail. The earlier allocator notes' larger settles came from busier machines, so expect a
bigger gap there (not measured here).

## Throughput

Solo passes, ms from the MCP client, median [min–max], off then on:

- **First search, loads the arena (4 per arm)**: 4,047 [3,943–4,232], 3,994 [3,902–4,061].
- **`*.pdf`, warm**: 85 [81–102], 82 [80–83]; again 73 [73–75], 74 [72–75].
- **No match**: 70.5 [66–77], 70.5 [64–75].
- **`report*`**: 63 [59–74], 64 [60–72].
- **`*e*`**: 255 [250–281], 254 [235–269].
- **List 200,000 entries**: 1,152 [1,007–3,235], 1,108 [1,017–1,383].
- **List 100,000 entries**: 522 [464–2,510], 515 [474–813].
- **Back to `~`**: 131 [109–161], 126 [111–149].

Every median is within 4%, in both directions. The list medians lean toward the knob, and the outliers are all arm off,
but that's within what a pass-to-pass spread shows (200,000 entries by pass: off 1,118–1,214, on 1,071–1,155).

## Restic

- **Footprint after the index load**: off 324, 374, 329; on 162, 162, 162.
- **Footprint 7 min after closing the view**, the index dropped at the 300 s sample in every run: off 325, 8, 223; on 5,
  6, 8. Left to itself, arm off held 325 MiB with a heap of zero in one run out of three, and `Malloc Large (empty)`
  read 421 MiB there.
- **Steady footprint, six `load` runs**: off 337–370; on 171–172.
- **Index load time, six interleaved runs**: off 1.13 s [1.12–3.72, the 3.72 is the cold first read]; on 1.16 s
  [1.13–1.31].
- **Reading a file, 1 MiB windows, one in flight, warm mount**: off 119.4 MB/s [111.7–122.0]; on 114.1 MB/s
  [113.9–117.1] over eight reads each (the cold 16.4 MB/s first read left out). That's ~4%, and it's the one cost that
  repeats. On a direct SMB session, at ~20 MB/s (`docs/notes/restic/open-cost.md`), a window's few fresh mappings are a
  far smaller share.

Arm off reads about half what #742 did on 2026-10-26 (614–634 MiB after the load, then against 67 index files, over a
slower 3.5–5.3 s load). This repo has grown, not shrunk. Not chased: the knob's before-and-after is what this note
needed, and tonight's load ran 3–5× faster, so a likely reason is rustic's buffers overlapping less. The sizing
consequence: with the knob a loaded index costs ~1x its `index/` folder in footprint (171 MiB for 162.5 MB), and ~1.7x
in live heap, some of it untouched (`crates/cmdr-restic/DETAILS.md` § "Why one index and eight opened repos").

## What the knob costs in a tight loop

A C loop of `malloc`, `memset` the block, `free`, 400 cycles, three runs per arm, median µs per cycle, off then on:

- 128 KiB: 1.2, 6.0
- 1 MiB: 9.0, 49.6
- 4 MiB: 36, 241
- 16 MiB: 148, 965
- 64 MiB: 596, 4,102
- A block grown by `realloc` from 1 MiB to 1 GiB: 64 ms, 72 ms

So 5–7× when nothing else happens between the allocation and the free, ~40 µs per MiB in fresh mappings and page faults.
That's why the app-level numbers barely move: its big blocks sit next to far more expensive work. A future hot loop that
recycles a large buffer through malloc should keep the buffer instead.

## How it ships, and what that leaves out

- **`LSEnvironment`**, because Finder, Dock, login items, and the "Open in Cmdr" service all launch through
  LaunchServices. Checked end to end: a real `pnpm tauri build --bundles app` bundle, under a bundle id of its own,
  signed with the hardened runtime, launched with `open -n`, showed no `Malloc Large (empty)` row after three bursts, a
  743 MiB peak, and 130 MiB a minute later. The merged `Info.plist` carries the key.
- **The hardened runtime doesn't strip it.** The signed, notarized `/Applications/Cmdr.app` binary of 0.58.0, launched
  directly with the variable set, showed no `(empty)` row and a 797 MiB peak. libmalloc honors it without a dyld
  entitlement.
- **Decision: no self re-exec in `main.rs`.** A launch that bypasses LaunchServices (the binary from a terminal, a first
  launch after an update if macOS hasn't re-read the bundle) runs one session with the cache on, as every version has so
  far. Harmless, and not worth an `execve` at the top of every launch.
- **Decision: dev scripts don't set it.** `tauri-wrapper.ts`'s environment reaches cargo, rustc, and Vite as well as the
  app, and rustc is exactly the kind of program a 5–7× large-block cycle could slow. Dev builds' memory numbers aren't
  trusted anyway. **So a release binary launched directly, as every A/B harness here does, runs WITHOUT the knob: set
  `MallocLargeCache=0` in its environment to measure what users run.**
- **Child processes inherit it** (`llama-server`, `adb`, `osascript`). Nothing to do: it only changes when they return
  freed large blocks. Left in the environment on purpose: Tauri's relaunch after an update starts the binary directly
  and inherits it.
- **Scope**: measured on 64 GB only. libmalloc picks its zones partly by RAM, so a smaller Mac may route different sizes
  to the large zone. Not measured, and `Malloc Large (empty)` would show it either way.

## Re-checking it

The recipe lives in `docs/maintenance.md` § "Per macOS release". If the `(empty)` row comes back, libmalloc has stopped
honoring the variable (it's read at process start, undocumented, and could go any release): the app still works, memory
reverts to arm off above, and the fallback is scoping the problem at its sources (#333 for search and listings).
