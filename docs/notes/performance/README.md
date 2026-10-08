# CPU, RAM, and idle cost: start here

The hub for everything Cmdr knows about its own resource use: idle CPU, memory, wakeups, file descriptors, and thread
churn. It holds the current measured state, the rules every measurement here has paid for, what's fixed and where, and
**the one ranked list of open follow-ups**. The notes beside it hold the evidence; this page points at them.

Pure throughput benchmarks (scan, copy, search latency) stay in `docs/notes/README.md`.

## Read order

1. This page, all of it. The methodology rules below have each cost at least one wrong answer.
2. `docs/tooling/memory-debugging.md`: how to measure memory (the `memory_diagnostics` MCP tool first, `vmmap` second),
   which allocator holds the Rust heap, the `IOAccelerator` and `IOSurface` traps, fingerprinting a block by region
   size, and attributing allocations.
3. `idle-cpu-attribution-2026-08-03.md`: how idle CPU was mis-attributed four times and what method held.
4. The dated note for the area you're in, from the index at the bottom.

## Current measured state

**The targets** every measurement here is judged against (set by David in a comment on the idle-cost issue, 2026-09-21):

- **Steady-state RAM of 200–300 MB**, never above 300 MB while nothing is indexing, searching, or transferring.
- **Idle CPU under 1%**.

All readings below are release builds of `main` on David's dev Mac, the heavy case (see § "Methodology rules"), over a
clone of prod's data unless marked otherwise.

**The global allocator is the system one on macOS** and mimalloc on Linux, a split David chose on the post-burst numbers
below (`crates/cmdr-fs/DETAILS.md` § "Which global allocator"). Readings from before that switch are mimalloc builds;
say which one a new reading is.

**Memory, idle: met** (verified on release `e9fd713ad`, a mimalloc build, `memory_diagnostics` with `rustHeapCensus`,
2026-09-27; `idle-census-2026-09-27.md` § "Idle memory"; the system allocator reads ~95 MiB lower at idle,
`allocator-comparison-2026-09-23.md`):

- Indexing on: footprint **263–273 MiB** at 10–37 min; heap ~206–211, of which ~103 live (64 of it the SQLite page slab)
  and ~105 slack.
- Indexing off: **~195 MiB** (194–196); heap ~146, ~80 live.
- Prod 0.47.0 after 2 h: footprint 295 MiB (`main-process-iosurface-2026-09-27.md`).

**Memory, after search and listing bursts: met in most runs on the system allocator** (verified on release builds,
`memory_diagnostics` plus a `proc_pid_rusage` poller; four rounds on 2026-09-28–29,
`allocator-slack-release-2026-09-27.md`, and one check round of the switch itself on 2026-09-30, § "Check round after
the switch" there):

- The footprint settles at a median **241 MiB** at +15 min [211–382], under 300 in five of seven runs, against
  mimalloc's 403 [276–456]. The check round read 308 at +15 min, with a mimalloc build of the same commit beside it at
  517, at load average 15–99.
- What it costs: a higher burst peak (median 1,976 MiB against mimalloc's 1,458; the check round 1,825 against 1,582),
  and a transient that sometimes outlasts a minute (777 MiB at +1 min in the check round). #333 is the design that would
  cut both.
- Live bytes return to their pre-burst level (~100 MiB) within 30 s of the search arena's drop. On mimalloc what stayed
  was ~240 MiB of slack, mostly from listing the two big folders (`search-arena-reload-2026-09-27.md`).

**Idle CPU** (verified on release builds, per-thread `proc_pidinfo` deltas, 2026-09-27):

- **Indexing off, network and MCP off: 0.14%** of a core for the main process, median over four 240 s windows
  (0.13–0.17%), after the space-poll fix (`space-poll-cost-2026-09-27.md`). Met.
- **Indexing on: 2.2%** main process, WebContent 0.73%, GPU helper 0.31% (`idle-census-2026-09-27.md` § "Idle CPU").
  Measured before the child-dir index, the space-poll fix, and mDNS gating, under ~160 FS events/s from sibling agents'
  builds; the indexing-driven share was ~1.1–1.4%. Prod 0.48.0 (all three fixes in) reads **0.8–1.2%** in Activity
  Monitor (David, 2026-09-29–30): at the target line, not yet attributed per thread.
- **No hotspot**: the cost is spread over many threads at a few hundredths of a percent each, so each remaining item
  below shaves a thread or two.
- **The allocator doesn't move it**: indexing off, MCP on, one 240 s window side by side, the system allocator 0.229%
  and mimalloc 0.241% (check round, 2026-09-30).

**Idle frontend**: `webcontent-idle-cost-2026-09-23.md` (before) and `webcontent-idle-fixes-2026-09-23.md` (after).

## Methodology rules

Rules canonical elsewhere are one line here plus the pointer; the rest are canonical here.

- **`sample` can't attribute diffuse idle cost.** In a 180 s sample of an idle prod app, 99.97% of leaf samples were
  parked. Use per-thread cumulative CPU (`ps -M <pid>`) and CPU-time deltas over minutes (`ps -o time`). Never rank work
  off one `sample` window, never count a syscall leaf as CPU, never infer CPU from log volume
  (`idle-cpu-attribution-2026-08-03.md` § "The rules this leaves behind").
- **For per-thread CPU with names, use `proc_pidinfo`** (`PROC_PIDLISTTHREADS` plus `PROC_PIDTHREADINFO`), diffed per
  minute; the process delta minus the live threads' deltas is what exited threads spent (`idle-census-2026-09-27.md` §
  "A better CPU instrument").
- **`top`'s `IDLEW` column is unreliable on macOS 27**: it read static across intervals. Don't use it for wakeups.
- **Know the build's allocator before reading `vmmap`.** On the system allocator (macOS by default) the Rust heap is in
  the `Malloc *` rows, shared with Objective-C and C. On a macOS mimalloc build (`--features mimalloc`, and every build
  up to 0.48.0) `IOAccelerator` is the Rust heap (tag 100) and `Malloc *` is NOT. On Linux, which always runs mimalloc,
  the heap is `[anon:mimalloc]` in `/proc/<pid>/maps` (`docs/tooling/memory-debugging.md` § "First: which allocator
  holds the Rust heap"). `memory_diagnostics` names it in `rustHeap.allocator`; say which build a reading came from.
- **`IOSurface` (tag 88) in the main process isn't its cost**: it's WebKit's layer backing, owned and paid for by
  WebContent, and outside the main footprint (`docs/tooling/memory-debugging.md` § "The second trap").
- **The same tag is spelled three ways**: `vmmap` says `Malloc Small`, older notes say `MALLOC_SMALL`, and
  `memory_diagnostics` reports `VM_MEMORY_*` names. Match on the tag NUMBER (`docs/tooling/memory-debugging.md`).
- **`vmmap`'s `SQLite Page Cache` row (32 KB) is not the page slab.** The slab is a leaked Rust allocation inside the
  heap; `memory_diagnostics` names it as `sqlitePageCache`.
- **mimalloc's own stats (`MI_STAT`, `MIMALLOC_SHOW_STATS`) are unreliable for live bytes.** Use the census in
  `rustHeap` (`docs/tooling/memory-debugging.md` § "Live bytes vs allocator slack").
- **Swapped doesn't mean "built once and left"**: on a pressured machine, heap pages swap within 2–5 minutes
  (`rust-heap-attribution-2026-09-23.md`).
- **Release builds only for memory work.** Debug builds allocate differently and their absolute numbers mean nothing off
  that machine; quote a debug-build win as a ratio.
- **David's dev machine is an outlier**: 16.2 M FS events in 25 h from worktrees and cargo builds, with the index writer
  alone at ~20% of process CPU. Its numbers are a heavy case, not a typical user's; say which one you have.
- **Identify WebContent with a busy loop** in the webview (or a burst of pane navigations) and see which pid gains the
  CPU. The GPU helper is the one spawned in the same second.
- **Interleave A/B runs and report medians with their spread.** Machine load moves every number here; a single pair
  proves nothing. Every A/B is a fresh launch (`docs/tooling/memory-debugging.md` § "Rules for A/B experiments").
- **Isolated instances only**: before launch, verify the data dir, bundle id, and ports are the instance's own (`lsof`),
  and clone prod data with `cp -c`, including every `-wal`. Exclude secrets and the crash report. Never touch the
  running prod app beyond read-only reads.
- **Snapshot a DB prod is writing by holding a read transaction, not with `sqlite3 .backup`.** `.backup` of
  `index-root.db` never finishes while prod writes heavily (it restarts on every change). Instead open it read-only,
  `BEGIN` and read one row so no checkpoint can restart the WAL, `cp -c` the DB and its `-wal`, then end the
  transaction; `PRAGMA quick_check` on the copy said `ok` (verified on prod 0.48.0, 2026-09-30).
- **Check the peer before calling a socket leaked.** The MCP "leak" was live clients with keep-alive pools
  (`mcp-connection-leak-2026-09-22.md`); the real SMB leak showed as sockets whose peer had hung up or that outlived
  their owner (`smb2-socket-lifetime-2026-09-23.md`).
- **Re-verify the PID and the recipe before trusting either.** A stale PID and a stale recipe (the old `MALLOC_SMALL`
  spelling, which reads as a clean zero) each misled this investigation once.

## What's fixed, and where it's documented

- **Post-burst slack on macOS**: the system allocator replaced mimalloc as macOS's global allocator (Linux keeps
  mimalloc), taking the settled post-burst footprint from a median 403 to 241 MiB:
  `allocator-slack-release-2026-09-27.md`, `crates/cmdr-fs/DETAILS.md` § "Which global allocator".
- **Search arena catch-up**: a search after a walk appends the walk's new rows to the warm arena, which takes ~610 MiB
  off the burst peak and cuts that search from 1.4 s to 0.1 s: `search-arena-reload-2026-09-27.md`.
- **Child-dir partial index**: the writer's child-dir lookups drop from ~60 ms to ~10 µs on a 92,000-file folder, added
  on open with no rescan: `dir-children-index-2026-09-27.md`.
- **Space poll**: `statfs` moves an occasional important-usage reading, cutting the main process from ~0.77% to ~0.14%
  of a core at idle: `space-poll-cost-2026-09-27.md`.
- **mDNS browse gating**: the browse runs only while something needs it, saving two threads and ~0.055% of a core at
  idle: `mdns-browse-gating-2026-09-27.md`.
- **Search loop allocations**: the scan no longer allocates per row, every query is 2–10× faster, and the system
  allocator's search penalty is gone: `search-loop-allocations-2026-09-27.md`.
- **Walker thread pool**: 97–99% fewer walker thread creations. A hygiene win; memory didn't move:
  `walker-thread-pool-2026-09-27.md`.
- **`IOSurface` relabel**: `memory_diagnostics` labels tag 88 as WebKit layer backing that isn't the main process's
  cost: `main-process-iosurface-2026-09-27.md`.
- **Idle frontend**: seven fixes (scanning tooltips, Size-column width hold, disk-space emits, backend routing of folder
  sizes, refresh only on a shown change, the hourglass delay, free-space precision):
  `webcontent-idle-fixes-2026-09-23.md`.
- **Hidden-entry diffs**: a change to entries a pane doesn't show (dotfiles in `~` with hidden files off) no longer
  reaches it, and diff indices are the pane's rows: `hidden-entry-diffs-2026-09-23.md`.
- **Rust heap**: the score cache, the MCP search arena's 30 s drop, the wake inbox paged to `main.db`, and the heap
  census: `rust-heap-attribution-2026-09-23.md`.
- **SMB sockets**: fixed in `smb2` 0.24.1, and pinned in Cmdr by a mount/unmount Docker cell:
  `smb2-socket-lifetime-2026-09-23.md`.
- **mDNS log storm**: fixed upstream in `mdns-sd` 0.21.5, which is Cmdr's floor:
  `apps/desktop/src-tauri/src/network/DETAILS.md`.
- **`memory_diagnostics` over MCP**, release builds included: `docs/tooling/memory-debugging.md`.
- **Each CLIP tower loads on demand**, so enrichment never pays for the text tower:
  `crates/cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the towers costs". **A rescan-anchor storm costs
  one sweep a day**: `crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/DETAILS.md` § "Anchor-cardinality
  routing".
- **Earlier**: the importance treadmill, the page slab, the per-row INSERT re-parse, and the runaway coverage walk, each
  in its dated note below.

## Open follow-ups

The single ranked list of items that plausibly move the RAM or CPU targets. #92 closed on 2026-09-30 with the targets
met or at the line and no known meaningful waste at rest; #336 (someday) holds the next actions in order, so pick up
there when someone reports Cmdr as wasteful again. Where an issue exists, it's the place to track the work. Ranked by
expected payoff against the targets over effort. Each item's **Effect** line is the expected move against a target, from
the linked note's numbers.

1. **Get idle CPU with indexing on reliably under 1%.** David's prod 0.48.0 reads 0.8–1.2% (Activity Monitor, observed
   2026-09-29–30), so it straddles the target. Next step: attribute it per thread on the running prod, read-only
   (`proc_pidinfo` deltas, the census recipe in `idle-census-2026-09-27.md`), note the FS events/s, and rank what's
   left. Status: one pass on prod 0.50.0 at load average 25–30 (2026-10-05, per-thread `proc_pidinfo` over 90 s plus the
   log): 2.3% of a core averaged over 48 h, the index writer at 0.1–0.2% in quiet hours and 1–3% from 13:00 under agent
   churn (~420–1,000 FS events/s), and the 100%+ spikes were the writer at 70–86% of a core indexing freshly cloned
   worktrees (90,000–270,000 rows each), not the rescan walks. Two fixes landed from it: removal storms anchor per
   cluster (`crates/cmdr-index/src/indexing/watch/DETAILS.md` § "Removal-storm coalescing"; one worktree's root had been
   walked 36 times that day), and the rescan lines carry each walk's CPU. **Effect**: decides the last target; the
   indexing-driven share was ~1.1–1.4% before the child-dir index, the space-poll fix, and mDNS gating.
2. **Explain the rest of the heap on a long-running prod**: run `memory_diagnostics` on a long-running prod (~360 MiB
   was unexplained at 0.46.1). 0.48.0 carries the mimalloc census (`rustHeapCensus`); a later, system-allocator release
   reports the default zone's live and reserved bytes instead, with no census. Status: not started. **Effect**: none by
   itself; what it finds is unknown.
3. **The search arena's 30 s background refresh still does a full rebuild with the old arena alive**: the same
   three-copies peak, on a timer, while someone keeps searching. Options: catch up on each refresh and rebuild whole
   only every few minutes (the rebuild is what carries deletions), or drop the old arena first
   (`search-arena-reload-2026-09-27.md` § "Still open"). Status: not started. **Effect**: lowers the peak during
   repeated searches (the catch-up took ~610 MiB off the post-walk peak, and this is the same shape); the effect on the
   settled footprint is unknown.
4. **WebContent costs 0.73% of a core with indexing on against 0.11% off**: coalesce the index-driven size updates on
   the frontend side, for rows whose readout doesn't change (`idle-census-2026-09-27.md` § "The levers", lever 5).
   Status: not started. **Effect**: up to −0.5% of a core in WebContent on a churning machine (estimate); the main
   process doesn't move.
5. **WebContent grows over days** (138 → 262 MB): take a Web Inspector heap snapshot on a long-running build before
   changing anything. Status: not started. **Effect**: unknown until the snapshot; up to the ~120 MB of growth, in
   WebContent.
6. **The root volume can skip `drive_is_listed`'s `getfsstat` on every subtree reconcile**: the boot volume can't be
   unlisted, and the call showed ~470 samples in a churn window (`idle-census-2026-09-27.md` lever 6). Small. Status:
   not started. **Effect**: small idle CPU under FS churn; unmeasured as a share of a core.

Smaller or already filed, unranked:

- **CLIP**: should an idle tower unload itself (#233), the ~400 MB non-GPU compute-unit path (#232), and an fp16 text
  tower (#234). The costs they trade: `crates/cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the towers
  costs". **Effect**: only for someone who has run a semantic search since launch; #232 measured ~410 MB against 11.8
  MB, and #234 would roughly halve the text tower's ~184–246 MB.
- **Set the rescan-storm threshold from a week of data**: #235. **Effect**: CPU during rescan storms; unknown.
- **Spotlight "last used" sampling cost**: #229. **Effect**: CPU per importance pass, probably small; unmeasured.
- **The media live tick's `load_statuses`** reads every stored status on any tick that survives the filter
  (`live-tick-cost-2026-08-21.md`). **Effect**: unknown; unmeasured.

## Tracked in their own issues

Items that don't move the targets, each tracked in its own issue:

- #335: one SMB share reached at several addresses (LAN, VPN, mDNS name) becomes several volumes; recognize it by
  ServerGuid + share name + volume serial after connecting, and adopt the existing index. Only on that setup.
- #333: give burst-transient data (listing entries, search side tables) its own allocation region, dropped wholesale, to
  stay low at the burst peak as well as at rest.
- #318: `SmbClient::close()` (LOGOFF) in `smb2`.
- #321: the CPU half of the diagnostics instrument (per-thread CPU and wakeups over MCP).
- #323: a stuck-loop watchdog at the log sink, and third-party `log::error!` reaching Flow B.
- #324: share lists prefetched for every discovered SMB host at launch.
- #325: a sync-status pool thread wedged in a File Provider call.
- #308: refresh the Finder-style free space when purgeable space changes.
- #134: load only the active language's messages.
- #231: a fresh idle baseline on a quiet machine (largely answered by `idle-census-2026-09-27.md`).

## Retired: don't reopen

- **The MCP connection "leak"**: not a leak. Every socket had a live peer, and a killed peer's socket was reaped in
  under a second (`mcp-connection-leak-2026-09-22.md`).
- **The "thread leak"**: every thread is legitimate and bounded (`thread-and-connection-inventory-2026-09-22.md`).
- **"132 connections × 16 MB page cache"**: page memory is one process-wide slab (same note).
- **"Thread churn strands mimalloc pages"**: refuted. Five paired rounds with a 3–20× churn difference left the slack
  within noise (`walker-thread-pool-2026-09-27.md`).
- **mimalloc purge tuning, `MIMALLOC_PURGE_DELAY=0` included**: no effect on the settled slack; immediate purging only
  lowers the burst peak (`rust-heap-attribution-2026-09-23.md`, `idle-census-2026-09-27.md` § "Follow-up #2").
- **Capping tokio's blocking pool**: not where the thread churn is, and measured to change nothing
  (`allocator-comparison-2026-09-23.md`).
- **mimalloc v2**: no gain over v3 (same note).
- **The main process's `IOSurface`**: WebKit's layer backing, charged to WebContent, where Cmdr already costs under half
  a bare `WKWebView` (`main-process-iosurface-2026-09-27.md`).
- **Not indexing developers' build output** (`target/`, `node_modules`, `.svelte-kit`): build output stays indexed and
  gets no special treatment, by David's decision; issue #236 is closed as not planned.
- **The GPU-compositor theory of the memory runaway**: it was the Rust heap mislabeled
  (`memory-runaway-rust-heap-2026-07-25.md`).

## Notes index

Newest first.

- `allocator-slack-release-2026-09-27.md`: neither `mi_collect(true)` nor malloc pressure relief returns the post-burst
  slack; where mimalloc keeps it (idle threads' empty pages, sparse pages, unpurged slices), park-time collects, four
  rounds of mimalloc against the system allocator's settle (median 403 against 241 MiB at +15 min), and the check round
  after macOS switched to the system allocator.
- `search-arena-reload-2026-09-27.md`: the arena reload after a walk set the burst peak (~610 MiB) and put 1.4 s in
  front of the next search, the catch-up that replaced it, and why the settled post-burst footprint comes from big
  listings instead.
- `dir-children-index-2026-09-27.md`: a partial index over directory rows cuts the writer's child-dir queries from ~60
  ms to ~10 µs on a 92,000-file folder, added on open with no rescan; per-query and whole-table numbers, and the
  one-time build cost.
- `space-poll-cost-2026-09-27.md`: the space poller's important-usage query cost ~0.65% of a core; the cheap `statfs`
  reading that replaced it, why `statfs` is a trustworthy change detector, and the interleaved before/after numbers.
- `walker-thread-pool-2026-09-27.md`: the walker and rescan threads pooled (97–99% fewer walker thread creations), and
  the allocator comparison re-run on top: the slack isn't thread churn, so pooling doesn't move the allocator tradeoff.
  Raw numbers: `walker-thread-pool-2026-09-27.csv`.
- `main-process-iosurface-2026-09-27.md`: the main process's `IOSurface` is WebKit's layer backing, paid for by
  WebContent and outside the main footprint; what a window costs there, and a bare-`WKWebView` baseline.
- `idle-census-2026-09-27.md`: `main` against both targets, the post-burst memory that turned out to be slack, a
  per-thread CPU instrument with names, and six new levers (the space poller's 6.5 ms free-space query first).
- `mdns-browse-gating-2026-09-27.md`: the mDNS browse runs only while something needs it; the two threads and ~0.055% of
  a core it saves at idle, and the Servers-view behavior it keeps.
- `search-loop-allocations-2026-09-27.md`: the search loop allocated per row (regex cache pool, exclude fold, ranking),
  the fixes, and the interleaved before/after numbers that close the system allocator's search penalty.
- `hidden-entry-diffs-2026-09-23.md`: diffs skip rows the pane doesn't show; natural and controlled-churn A/B numbers.
- `webcontent-idle-fixes-2026-09-23.md`: the seven frontend idle fixes and their interleaved before/after numbers.
- `webcontent-idle-cost-2026-09-23.md`: where WebContent and the GPU helper spend idle CPU, why window state barely
  matters, and the frontend memory picture (including the eager translation catalogs).
- `rust-heap-attribution-2026-09-23.md`: live data against slack in the Rust heap, the named live consumers, the four
  changes and their measured effect, and the ~360 MiB still unexplained at 0.46.1.
- `allocator-comparison-2026-09-23.md`: v3 against v2 and the system allocator, and the decision to keep v3 that the
  slack note later reversed for macOS (its search penalty argument is gone: `search-loop-allocations-2026-09-27.md`).
  Raw numbers: `allocator-comparison-2026-09-23.csv`.
- `smb2-socket-lifetime-2026-09-23.md`: the two `smb2` socket bugs behind the leftover SMB sockets, fixed in 0.24.1.
- `mimalloc-purge-experiment-2026-09-22.md`: what's tunable in mimalloc v3 (option names, `launchctl setenv`, why
  `MIMALLOC_SHOW_STATS` is half-useful), the 0.46.1 baseline, and a purge A/B protocol whose prior is low.
- `thread-and-connection-inventory-2026-09-22.md`: a verdict per worker thread and SQLite connection count, and the one
  real duplication (one SMB share at three addresses becomes three volumes). Read it before counting threads.
- `mcp-connection-leak-2026-09-22.md`: why the MCP server doesn't leak connections, what one costs, and how the two
  stuck SMB sockets were found.
- `live-tick-cost-2026-08-21.md`: what a media live tick costs (the coverage gate and the scoped walk), and why
  filtering the walk without fixing the gate would have left the floor in place.
- `idle-malloc-large-clip-towers-2026-08-21.md`: Core ML holding the CLIP towers costs 307–412 MB of `Malloc Large`, 80%
  of it the text tower; the region-size fingerprint method and the one `vmmap` line that confirms it.
- `importance-treadmill-2026-08-04.md`: the 60-second importance rescore treadmill, why raising `SCOPED_WALK_MAX_DIRS`
  is refuted, and the signals-not-score equality key.
- `idle-cpu-attribution-2026-08-03.md`: 110 minutes of idle CPU over 9.1 hours, four wrong answers, and the rules they
  left.
- `idle-memory-profile-2026-07-28.md`: a 2.5 GB idle footprint from SQLite page cache across many connections and the
  rescore treadmill, and the shared page slab it led to.
- `memory-runaway-rust-heap-2026-07-25.md`: the runaway up to 50 GB (a coverage walk materializing every image path),
  and the origin of the `IOAccelerator` trap.
- `idle-cpu-indexing-streamlining-2026-07.md`: issue #37's idle-CPU stack (the importance loop and the collated-key
  subtree clear) and what each fix bought.
- `high-memory-gpu-compositor-investigation-2026-07.md`: superseded (it read the Rust heap as GPU memory); kept for the
  measurement gotchas and the frontend throttles it landed.

Related elsewhere: `docs/notes/listing-row-fetch-quadratic-2026-08-22.md` (a main thread saturated by per-row IPC),
`docs/notes/search-arena-row-2026-08-06.md` (the search arena's size), and
`docs/notes/sync-status-pool-bench-2026-07-31.md` (the sync-status pool).
