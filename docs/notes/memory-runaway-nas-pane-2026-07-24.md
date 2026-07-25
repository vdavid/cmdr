# Memory runaway: a settled NAS pane leaks GPU compositor memory at idle (2026-07-24)

> **SUPERSEDED (2026-07-25).** Its central conclusion — that this is WebKit GPU/compositor memory — is WRONG: `vmmap`
> reports Cmdr's Rust heap under the `IOAccelerator` name (a mimalloc VM-tag collision). Read
> `docs/notes/memory-runaway-rust-heap-2026-07-25.md` instead. Kept only for the experiment log.

> **Superseded in part by `memory-runaway-gpu-slabs-2026-07-25.md` — read that first.** The 2026-07-25 session corrected
> several claims here: the slabs are a fixed 128 MB allocator granularity (not ~100 MB content-sized tiles); the balloon
> is a ~2–3 minute STARTUP-window event (not an ~11-hour idle creep); it happens while the backend is busy indexing at
> launch (not "idle after a settled index"); and the leading suspect is now the space-poller footer re-render driven by
> boot-drive index-DB writes. This note's raw measurements and the `@Recently-Snapshot` exclusion finding remain valid.

Kick-off context for a fresh agent picking up the "prod Cmdr ballooned to 40 GB" investigation. This continues, and
partially updates, `high-memory-gpu-compositor-investigation-2026-07.md` (2026-07-15). **Read that one first** — it has
the mental model, the measurement gotchas, and the fixes already landed. This note is a new prod incident of that same
GPU-compositor class, now isolated to a specific trigger: a **fresh app start with a pane on the SMB NAS and a settled
index** (crucially, NOT during an active scan). Note this _contradicts_ the 2026-07-15 note's "needs a storm" model —
see the trigger discussion below.

Investigation done from a session mistakenly rooted in another repo, so it's observation + `vmmap` measurement + code
reading only. No code changed. App version: **0.36.0**, prod build (`/Applications/Cmdr.app`), Apple Silicon, macOS
26.5, unified memory.

## TL;DR

- **What the memory is:** `IOAccelerator` — WebKit's GPU/Metal compositor surfaces — **not** the Rust heap. Confirmed by
  live `vmmap` at multiple magnitudes; at a 55.2 GB `phys_footprint` peak, `IOAccelerator` was ~54 GB (5.3 GB dirty
  resident + **49 GB swapped/compressed**) while every `MALLOC_*` zone stayed **flat at ~950 MB**. The backend heap does
  not move; the GPU compositor holds it all.
- **What triggers it (corrected — it is NOT an active scan):** a **fresh app start with a pane on the NAS, a settled /
  up-to-date NAS index, and drive indexing enabled.** It balloons at idle, no interaction. It does **NOT** balloon
  _during_ a scan — during a scan memory stays bounded (that is the backend enrichment profile). The frontend WebKit
  compositor is the holder; the leak is orphaned GPU surfaces that accumulate while _something_ re-composites the
  settled NAS pane on a periodic cadence (space-info polling and/or stale-index re-validation are the leading suspects —
  not a stream of index updates). Two levers are each necessary: **drive indexing enabled** (off → flat 124 MB) and **a
  NAS pane** (both panes local → no runaway). **Image indexing is NOT it** (an earlier draft wrongly blamed it; turning
  it off does not stop the climb, and image-scan is a separate bounded backend workload).
- **The clean isolation (owner's experiments):** drive indexing OFF for all drives → steady **124 MB, zero ballooning**.
  Drive indexing ON with both panes on the LOCAL disk → 0.8–1.8 GB, no runaway. Switch one pane to the **NAS** → 2 → 5 →
  7.5 → … → 55 GB. Turning drive indexing back off is the only thing that reliably stops it; navigating away mid-climb
  does not (the surfaces are already allocated).
- **Why "backend trigger, flat backend heap" is not a contradiction:** the frontend WebKit compositor is the holder; the
  backend is only the driver. Cutting the driver (drive indexing off) stops the frontend from re-rendering, so the GPU
  allocation stops. The `vmmap` heap-vs-GPU split is what proves this; trust the region breakdown, not log correlation.
- **Ruled out by direct measurement:** the drive **scanner** heap (a full NAS rescan held at ~700 MB), image-enrichment
  backend heap (churning images sat stable at ~2.5 GB, heap flat), and settings toggling (≤1.5 GB). Two wrong turns this
  session — first blaming the scanner, then image indexing — both from correlation; the region breakdown corrected both.
- **The memory watchdog can neither see nor stop this**, and its graphics discriminator misfires — see "Watchdog is
  broken for this case" below.

## The prod incident (what the user hit)

- App ran ~10:45 → 21:33. The watchdog's **only** threshold crossings all day were 8.76 GB WARN at 21:32:54, then 16.50
  GB STOP at 21:33:29 — **35 seconds apart**. So it sat healthy (<8 GB) for ~11 hours, then went vertical in seconds.
  User saw ~24 GB by 21:34 and ~40 GB by 21:42. This is a fast runaway, not a slow leak.
- Watchdog snapshot at the 16.5 GB stop: `resident 9.41 GB` (max 12.92), `resident−phys 0.00 GB`, summed malloc-zone
  heap `1600 MB` (largest `DefaultMallocZone` 1468 MB), `live FSEvents 1,700,896`. Note `resident−phys = 0`: the
  watchdog read this as "not graphics" and it was wrong (see below).
- `state::stop_all_indexing()` fired at 16.5 GB, then the watchdog task `return`ed. Memory kept climbing to ~40 GB
  regardless — because the memory is in the WebView, which stopping indexing does not touch, and because the watchdog
  removed itself after one shot.
- Error report auto-sent: **`ERR-QAN79`** (in the api-server error dashboard; bundles the full debug log).
- Active pane was the SMB NAS (`/Volumes/naspi`). That share reports **11,327,990 images**, hugely inflated by
  `@Recently-Snapshot` (QNAP snapshot pseudo-tree; also why the share shows ~76 TB on a ~10 TB NAS). Local `root`
  enrichment logged `0 of 231718 enriched` all day: enrichment never actually ran this morning, because it gates on a
  fresh drive index and the NAS index was stale.

## Tonight's reproduction (the decisive evidence)

Same prod build, `vmmap -summary <pid>` sampled every 5 s. `phys_footprint` is the honest metric (Activity Monitor's
"Memory"); `ps` RSS lied here (it keeps counting purged `IOAccelerator` regions long after `phys_footprint` collapses).

- **Idle floor:** phys ~1 GB, of which `IOAccelerator` dirty ~1 GB. So even at rest the WebView compositor holds ~1 GB.
- **Full NAS rescan:** held ~0.7–1.5 GB. Scanner is not the cause.
- **Enrichment running** (after the drive index went fresh, `85 of 231716 enriched`): **stable** phys ~2.5 GB =
  `IOAccelerator` ~1 GB + system malloc heap ~1.8 GB. The heap growth is the image-decode path (Vision/CoreGraphics
  decode buffers go through the _system_ allocator, so they show in `DefaultMallocZone`, not mimalloc). Stable, not a
  runaway.
- **First restart:** startup spike phys 0.28 → 4.03 GB (`IOAccelerator` dirty ~2.8 GB), then **self-released**, phys
  collapsed to 257 MB. A purge.
- **Runaway reproduced (idle, NAS pane, drive indexing on):** phys climbed **4.8 → 7.0 → 13.3 GB** over ~2 minutes.
  `IOAccelerator` **dirty**: **4.0 → 6.0 → 12.3 GB**. `MALLOC_SMALL` + `DefaultMallocZone`: **flat ~1.1–1.2 GB the whole
  time.**
- **Runaway at 55 GB (the clincher):** with a pane on the NAS it ran to **`phys_footprint` 55.2 GB**. `IOAccelerator`
  ~54 GB (5.3 GB dirty resident **+ 49 GB swapped/compressed**); `MALLOC` zones **flat at ~950 MB**. So at 55× the heap,
  the heap still hadn't moved. This also settles an earlier red herring: dirty `IOAccelerator` clearly _does_ get
  compressed/swapped, so the prod snapshot's 7 GB of compressed memory (resident 9.4 < phys 16.5) was this same GPU
  memory, not the Rust heap.

The owner's isolation experiments (the definitive trigger test):

- **Drive indexing OFF for all drives → steady 124 MB, no ballooning at all.** (Lower than the usual ~240 MB floor;
  drive indexing carries a steady tax on its own, measured below, separate from the runaway.)
- Drive indexing ON, both panes on the **local** disk → 0.8–1.8 GB, settles, no runaway.
- Drive indexing ON, switch one pane to the **NAS** → 2 → 5 → 7.5 → … → tens of GB. Reproducible.
- Turning drive indexing back off is what stops it. Navigating a ballooning pane away (even to `/.nofollow`) does not
  bring it back down — the surfaces are already allocated; only a purge or restart clears them.

Signature to recognize: `phys_footprint` and `IOAccelerator` (dirty + swapped) move together; the heap does not move.
That is the whole diagnosis. The trigger is a fresh app start with a pane on the NAS and a settled index, drive indexing
enabled — NOT an active scan (during a scan it stays bounded).

### The steady drive-indexing tax (measured, off vs. on)

Two clean `vmmap` snapshots after a fresh restart, same static local view, drive indexing off then on:

- **Off:** `phys_footprint` 127.4 MB (total DIRTY 127.6 MB).
- **On (settled):** `phys_footprint` 253.9 MB (total DIRTY 254.0 MB), but **peak 701.9 MB** — turning it on causes a
  transient scan/reconcile + render burst that then reclaims.

The +126 MB settled tax, by **DIRTY** delta (the column that feeds `phys_footprint` — see the concepts section):

- `DefaultMallocZone` 38.8 → 116.2 MB = **+77 MB → back-end.** This is the _system_ malloc zone; the big tenant is
  **SQLite** (per-store 16 MB page caches across `indexing` / `media_index` / `importance` / `operation_log` / …, plus
  framework allocations from spinning the subsystems up). Note it is NOT Rust data: Rust runs on **mimalloc**, whose
  arenas would show as anonymous `VM_ALLOCATE`, and that barely moved (160 KB). So the backend tax is mostly SQLite, not
  Rust structures.
- `IOAccelerator` dirty 73.6 → 121.6 MB = **+48 MB → front-end.** Rendering the live indexing status (the green
  indicator, recursive sizes filling in) allocates GPU surfaces.

So the steady tax is a genuine ~60/40 mix (SQLite + GPU), not one side. The "~300 MB" figure seen earlier was likely
read mid-transient (peak ~700 MB) or with a pane actively updating; steady state is ~126 MB. The front-end 48 MB here is
the _tame, bounded_ version of the exact `IOAccelerator` mechanism that runs away on the NAS pane.

### Two distinct memory profiles — do NOT conflate them

There are two different "Cmdr uses a lot of RAM" workloads with completely different fixes. Always read the DIRTY
breakdown to tell which one you are looking at:

- **Image enrichment scanning** → **back-end**, `DefaultMallocZone`-dominated, **bounded ~2.5 GB.** Measured on the dev
  build mid-scan (both drives, NAS at "9000 of 2M images"), phys 2.2 GB: `DefaultMallocZone` **1.1 GB dirty** (image
  decode — `CG image` / `CG raster data` — plus SQLite), `owned unmapped (neural)` ~94 MB (the CLIP model via CoreML),
  and `IOAccelerator` only **542 MB dirty**. The Layers panel was calm at 20 MB — the frontend barely moved. This is the
  expected cost of decoding + embedding images; it is NOT the bug. If a report is dominated by `DefaultMallocZone` +
  CoreML and stays bounded, you are looking at enrichment, not the runaway.
- **The runaway** → **front-end**, `IOAccelerator`-dominated, **unbounded** (55 GB seen). This is the bug.

The runaway's fingerprint, from the prod `vmmap` series: `IOAccelerator` **region COUNT** grew **72 → 95 → 150 → 583**
as phys went 4 → 55 GB — roughly **~100 MB per region.** So it is **accumulating discrete ~100 MB GPU surfaces** (a
~5000×5000 RGBA tile is ~100 MB), one region at a time, not inflating one big allocation. That is a **surface leak**:
something re-tiles a _large_ composited element and orphans the prior tile on each update. Orphaned surfaces are not in
the live layer tree, so **the Layers panel cannot see them** — it read a calm 20 MB even while `IOAccelerator` was
multi-GB. Watch the `IOAccelerator` COUNT column as a cheap leak signal; a growing count is orphaned surfaces piling up.

## Behavioral observations

- **Does NOT balloon during a scan — only after, at a fresh start with settled indexes.** This is the load-bearing
  timing fact (owner-confirmed). During active drive or image scanning, memory stays bounded. The balloon needs a
  _settled / up-to-date_ index (the NAS's is flagged stale-but-recent), a fresh app launch, and a pane on the NAS. So it
  is not driven by a burst of index updates; the content is static while `IOAccelerator` climbs.
- **Balloons while idle, no interaction needed** — a pane on the NAS, drive indexing enabled, index settled. Points to a
  _periodic_ re-composite of a static pane (a poll or a freshness re-check), not user scrolling and not a stream.
- **Sometimes snaps back to ~1 GB on its own.** Consistent with macOS purging purgeable `IOAccelerator` pages under
  pressure. It is not monotonic; it climbs, occasionally collapses, climbs again. This is why prod took ~11 hours to
  cross 8 GB and then went vertical: the purges kept up until they didn't.
- **Local-only vs. NAS is the clean A/B.** Both panes local → no runaway. One pane on the NAS → runaway. The NAS pane
  differs from a local pane in what renders for a _settled_ volume: `≥…` "at least" recursive totals (the SMB index
  gives lower bounds), a stale-but-recent freshness state, and SMB space polling. One of those keeps re-compositing it.
- **Only cutting a lever stops it.** Turning drive indexing off → flat 124 MB; moving the NAS pane off-screen prevents a
  fresh-start balloon. But navigating a _already-ballooning_ pane away does not reclaim (surfaces already allocated).
  Turning off _image_ indexing does nothing.

## Concepts: reading a `vmmap` for this bug

- **`IOAccelerator`** is the VM region for GPU-driver memory (textures, compositing/render surfaces, framebuffers,
  command buffers). On Apple Silicon the GPU shares system RAM (unified memory), so this _is_ RAM. In Cmdr essentially
  all of it is **WebKit's compositor backing stores** — the rasterized layers of the rendered page. "IOAccelerator huge"
  = "the WebView is holding lots of layer surfaces." (Sibling region `IOSurface` is the shareable-surface handle layer;
  it shows large VIRTUAL but ~0 dirty here, so it is not the consumer.) Read its **DIRTY**, not RESIDENT: a lot of
  `IOAccelerator` resident is clean device mapping that does not count toward `phys_footprint` (a calm reading showed
  1.8 GB resident but only 542 MB dirty). And watch its **region COUNT** — it grows as orphaned ~100 MB surfaces
  accumulate (the runaway fingerprint above), so a climbing count is a cheap leak signal.
- **Dirty vs. clean.** A resident page is _clean_ if it mirrors a file on disk (executable code, mmap'd files) —
  droppable for free, barely counts as your memory (e.g. `__TEXT` is ~650 MB resident, 0 dirty). It is _dirty_ if it's a
  private page you wrote to and no file backs it — the OS can only reclaim it by compressing or swapping. **Dirty is the
  memory you actually own and pay for.**
- **`phys_footprint` ≈ total DIRTY + compressed.** Verify it in any snapshot: phys 253.9 MB ≈ total DIRTY 254.0 MB. So
  when diffing states, read the **DIRTY column** — not VIRTUAL (address space, mostly reserved and meaningless) and not
  RESIDENT (includes droppable clean and device pages). "IOAccelerator dirty 12 GB" = 12 GB of written GPU surfaces the
  OS can reclaim only by swapping; under pressure they show up in the **SWAPPED** column (the 55 GB run had 49 GB
  swapped).
- **Why a balloon persists after the trigger stops:** GPU surfaces are _sticky_. Turning indexing off, or navigating
  away, does not free already-allocated `IOAccelerator`; only a purge (macOS under pressure) or a full process restart
  clears it. A small bump right after turning indexing off is just the last render plus a drain, not new growth.

## How to debug this (the playbook)

Platform note: Tauri on macOS uses **WKWebView (WebKit), not Chromium** — so the devtools are **Safari Web Inspector**,
not Chrome DevTools.

**Dead ends already hit this session (do not repeat):**

- **The Layers panel does NOT show this leak.** It read a calm **20 MB** (12 layers, few paints) while `IOAccelerator`
  was multi-GB, because the leaked surfaces are _orphaned_ (not attached to the live layer tree). Paint flashing showed
  no storm either. The Layers panel is still useful for compositing _reasons_, but it cannot measure orphaned surfaces.
- **The dev build (`pnpm dev`) did not reproduce the runaway — but it was not yet tried in the exact condition.** The
  attempts used a scanning index and image indexing; enabling _image_ indexing on dev only produced the bounded ~2.2 GB
  **backend** enrichment profile (above), not the runaway. The correct condition (fresh start, settled NAS index, NAS
  pane, no active scan) had not been isolated when dev was tested — try that before concluding it is prod-only.
- So for THIS leak the primary instrument is **`vmmap` `IOAccelerator` (dirty + COUNT) over time**, and — for the
  allocation site — **Instruments on the prod build** (below). Treat the Layers panel and paint flashing as secondary.

1. **Reproduce the real trigger first — and it is NOT active scanning.** The trigger is a **fresh app start with a
   settled / up-to-date NAS index, a pane showing the NAS, and drive indexing enabled.** It balloons at idle, no
   interaction. It does **NOT** balloon _during_ a scan (during a scan it stays bounded — that is the enrichment/backend
   profile above). So to reproduce: let the indexes finish, quit, relaunch with a pane restored on `/Volumes/naspi`, and
   watch `IOAccelerator` dirty + COUNT via `vmmap`. Do NOT trigger a rescan — that suppresses it. If a settled-index
   fresh start still will not balloon on dev, the runaway is prod-specific (release build and/or the real 11.3 M-image
   scale) — instrument prod instead (step 6).
2. **Safari Web Inspector → Layers panel, for compositing reasons only.** Run a dev build (`pnpm dev --worktree <slug>`)
   so the WKWebView is inspectable; right-click → Inspect Element → **Layers**. It will not show the leak's size (see
   above), but it names which elements are composited and _why_ (`will-change`, 3D `transform`, `backdrop-filter`,
   overlap, canvas/video) — use it to see whether a large scroll surface (`div.virtual-spacer`, `div.full-list`) is
   promoted and re-tiling. Paint flashing shows over-render if there is any (there was little-to-none observed).
3. **Quantify the driver — what re-renders the SETTLED NAS pane?** The content is static (indexes up to date), yet
   `IOAccelerator` climbs at idle, so something re-composites the NAS pane on a periodic cadence. Instrument that: add a
   counter in the Svelte update path / `$effect` (or a `MutationObserver` on the pane) and log **updates/sec for the NAS
   pane**, and on the backend log which per-tick events still fire for a settled network volume. Optionally sample
   `phys_footprint` (reuse the watchdog's reader) on a timer next to the update count for a "re-render → GPU grew"
   series.
4. **Bisect the periodic re-render sources for a settled network pane.** Stub or freeze ONE at a time, restart (sticky
   surfaces!), reproduce the fresh-start NAS pane, and watch `IOAccelerator` COUNT. Whichever, when frozen, flattens the
   climb is the culprit. Suspects, reframed for the settled-index case:
   - a. **The space-info poller.** SMB `get_space_info` runs on a timer (~5 s) and drives a `volume-space-changed` event
     → the drive-space bar re-renders even when nothing changed. Prime suspect because it fires on a settled NAS at
     idle.
   - b. **Stale-index re-validation / the `≥…` "at least" totals.** The NAS index is marked stale-but-recent, and its
     recursive totals render with a `≥` estimate. If a freshness re-check or an indeterminate "still working" indicator
     keeps the pane in a perpetually-refreshing state, that periodically re-composites it. Check `pane/index-events.ts`
     and the freshness/badge logic for a network volume that is settled but flagged stale.
   - c. **A large composited scroll surface that re-tiles on each periodic re-render and orphans the old tile.**
     `div.virtual-spacer` / `div.full-list` are the big surfaces. This is the _leak site_ even if (a)/(b) are the
     _trigger_: the fix may be to stop promoting/re-tiling these, or to stop the needless periodic re-render entirely.
   - d. **A re-introduced permanent GPU-layer promotion.** The 2026-07-15 fix removed `will-change: transform` from
     `.virtual-window` in `file-explorer/views/FullList.svelte` (guardrail comment present). Verify nothing re-added a
     `will-change` / `transform` / `filter` / `backdrop-filter` promotion on a row / badge / status element.
5. **Measure the effect with `vmmap` (ground truth).** Web Inspector's memory timeline tracks the JS heap but
   under-captures GPU surface memory, so `IOAccelerator` (dirty + swapped + COUNT) stays the source of truth for whether
   a fix worked. Success = it **oscillates and reclaims**, not climbs monotonically. Trust large deltas and the
   qualitative shape; run-to-run noise is ±250–300 MB; restart between conditions (sticky surfaces). One-liner (PID
   changes each launch):

   ```
   PID=$(pgrep -x Cmdr | head -1)
   vmmap -summary "$PID" | grep -iE "Physical footprint:|^IOAccelerator |^MALLOC_SMALL |DefaultMallocZone"
   ```

   Watch three numbers: `Physical footprint` (the truth), `IOAccelerator` (dirty col 4 + swapped col 5; the runaway),
   and the malloc heap (should stay flat). Phys and `IOAccelerator` up together while the heap is flat = this bug.

6. **If it only reproduces in prod, instrument the release build for the allocation site.** Either set
   `isInspectable = true` on the release WKWebView to attach Web Inspector, or run **Instruments** against the prod
   build — the **Metal System Trace** or **Allocations + VM Tracker** templates can attribute `IOAccelerator` /
   `IOSurface` allocations _and their backtraces_, which is the one thing `vmmap` and the Layers panel cannot give and
   exactly what an orphaned-surface leak needs. This is the surest path, since prod reproduces reliably and dev may not.

**First hour:** relaunch with a pane on the settled NAS index and watch `IOAccelerator` **dirty + COUNT** via `vmmap` at
idle (do not scan). Once it is climbing, freeze the space-info poller and the stale/`≥` freshness path one at a time and
see which flattens the COUNT growth. In parallel, the Layers panel names whether `div.virtual-spacer` / `div.full-list`
is a promoted surface being re-tiled. The fix is likely either "stop the needless periodic re-render of a settled
network pane" or "stop re-tiling/promoting the large scroll surface" — confirm by watching `IOAccelerator` oscillate
instead of climb.

The thumbnail / image-index angle from the 2026-07-15 "Open item" (bound decode to display size, LRU thumbnail-cache
budget, `decoding="async"` + lazy, release off-screen decodes) is still worth doing, but it is NOT this bug — image
indexing was ruled out by the isolation experiments above.

## Watchdog is broken for this case (`indexing/resources/memory_watchdog.rs`)

Two independent failures, both seen in the prod incident:

1. **It can't stop the growth.** On STOP it calls `state::stop_all_indexing()` + the subsystem-stop hooks, then
   `return`s. None of that touches WebView/GPU memory, so the climb continued to 40 GB. And the `return` makes it a
   one-shot: after firing once it removed itself, so nothing watched the 16 → 40 GB climb. It should keep looping and
   escalate if memory keeps rising after a stop.
2. **Its graphics discriminator misfires on this hardware.** The snapshot logic treats `resident − phys_footprint` as
   the "this is GPU, not indexing" signal, on the premise (from the 2026-07-15 work) that `phys_footprint` excludes
   `IOAccelerator`. That holds for _resident_ device mappings, but **dirty (and swapped) `IOAccelerator` surfaces are
   counted in `phys_footprint`**, so here resident ≈ phys and the delta is ~0 even though it _is_ graphics. The prod
   snapshot showed `resident−phys 0.00` and the watchdog concluded "not graphics" — wrong. It also reports "malloc heap
   1.6 GB" and implies the heap is small and fine, which is true but misleading, because Cmdr's real Rust heap is served
   by **mimalloc**, which does not register as a system malloc zone, so `malloc_zone_statistics` /
   `malloc_get_all_zones` (what the snapshot reads) is structurally blind to it. Not the issue here (heap really was
   flat), but it means the watchdog can never quantify the Rust heap. To make future incidents diagnosable: add an
   "untracked = phys − system-malloc − a real GPU figure" line, or query mimalloc's own `mi_process_info` committed
   bytes, and read a real `IOAccelerator` figure rather than inferring graphics from the resident−phys delta.

## Separate finding: exclude `@Recently-Snapshot` from indexing

Not the memory root cause, but a real mitigation and a correctness fix, worth doing independently.

- Today a mount-rooted SMB scan applies only the macOS/Unix "junk basename" tier (`.Spotlight-V100`, `.fseventsd`, …)
  plus the `proc`/`sys`/`dev` pseudo-fs trio (`indexing/scanner/exclusions.rs`). It recognizes **no NAS system dirs** —
  `@Recently-Snapshot`, `@Recycle`, `.restic` are all fully walked. That's an omission, not a decision.
- `@Recently-Snapshot` is a QNAP virtual snapshot tree: copy-on-write, blocks shared with live data, read-only, not
  separately reclaimable. Walking it double-counts space (76 TB on a 10 TB NAS) and inflates the image count ~10× (the
  11.3 M figure). Excluding it shrinks the "where's my space" answer _and_ the NAS pane's giant `≥…` totals and
  perpetual-stale state that keep it re-compositing, plus every scale-dependent backend structure. Synology has the same
  concept (`#snapshot`), so a snapshot-pseudo-tree exclusion is a broadly-correct default, not a per-user hack.
- Keep `@Recycle` (real reclaimable trash) and `.restic` (real deduplicated backup bytes) indexed; both are genuine
  space the owner cares about.
- Independently, the enrichment full-sweep (`media_index/scheduler/enrich.rs`, `walk_image_entries`) materializes the
  whole qualifying-image `Vec` plus the full directory map at once. It streams the _file_ rows (already optimized), but
  the output list and dir map still scale with the snapshot-inflated tree (11.3 M images → multi-GB, possibly ×2 via
  `prioritized`). Not tonight's culprit (the heap stayed flat, and NAS enrichment never ran this morning), but a latent
  backend balloon on a huge network volume. Excluding `@Recently-Snapshot` also defuses this.
