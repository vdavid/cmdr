# High resident-memory / GPU-compositor investigation (2026-07-15)

Kick-off context for any future "Cmdr is using too much RAM" investigation. Captures the mental model, the fixes we
landed, the dead ends, and — most valuable — the measurement methodology and its many gotchas, so the next effort
doesn't re-derive them from scratch.

## TL;DR

- The scary multi-GB "memory" numbers were **WebKit's GPU compositor surface cache** (the `IOAccelerator` VM region),
  **not** the Rust heap, SQLite, or a leak. It is **reclaimable** (collapses to ~15 MB when rendering goes idle) and
  **not a recent regression** (v0.8.2 from 2026-03 shows the same balloon).
- `phys_footprint` (macOS's real memory-pressure metric, what Activity Monitor's "Memory" column and jetsam use) stayed
  bounded (~400–500 MB idle) the whole time. `resident_size` / RSS is what balloons, because it counts the GPU device
  mappings that `phys_footprint` excludes.
- Root cause: **continuous re-rendering of the visible file list under an indexing/FS storm** allocates compositor
  surfaces faster than WebKit reclaims them. It scales with the number of visible rows that re-render.
- Landed fixes: the memory watchdog now thresholds on `phys_footprint` + logs a rich breakdown; the file list drops a
  permanent GPU-layer promotion; and the diff-driven refetch is throttled to ≤4/sec. Row paint-containment was tried and
  **rejected — it made things worse**.

## The symptom

The indexing memory watchdog (`indexing/memory_watchdog.rs`) fired its 8 GB WARN. `vmmap` showed the process at multi-GB
resident; the peak `phys_footprint` hit ~12.7 GB once, under an extreme storm (a full `cargo build` churning `target/`
**plus** a ~979 K-event FSEvents journal replay running for ~10 min while a pane showed a directory whose recursive size
streamed). This is a developer-only pathological case; a normal user won't build the app while it indexes.

## The mental model (what's actually going on)

Verified on macOS 26.5 / WKWebView, via `vmmap -summary` region breakdowns, 2026-07-15.

1. **The dominant consumer is `IOAccelerator` (GPU/Metal compositor surfaces), not the heap.** At an 8 GB reading, the
   Rust/C malloc heap (all `MALLOC_*` zones) was only ~185 MB; `IOAccelerator` resident was ~3.8 GB. SQLite `cache_size`
   is 16 MB/connection — negligible.
2. **`resident_size` (RSS) over-counts vs `phys_footprint`.** RSS includes the `IOAccelerator` device mappings;
   `phys_footprint` (from `TASK_VM_INFO`) largely excludes them. Empirically RSS 4.8 G vs phys 1.1 G — the ~3.7 G gap ≈
   `IOAccelerator`. So a watchdog reading RSS mis-reports GPU cache as memory pressure. **Threshold on
   `phys_footprint`.**
3. **It's reclaimable cache, not a leak.** Both a 0.33 instance and a v0.8.2 instance dropped from ~1.6–1.9 G
   `IOAccelerator` to ~13–15 MB once rendering went idle — WebKit frees the surfaces under idle/occlusion/memory
   pressure. `phys_footprint_peak` can still be genuinely large during the storm, but it comes back down.
4. **Not a recent regression.** v0.8.2 (2026-03-15) ballooned to ~1.6 GB `IOAccelerator` during its own first-run scan —
   same signature. So the fix is not "revert commit X"; it's the fundamental WKWebView-under-heavy-re-render behavior.
5. **The driver is visible-list re-rendering, not event volume.** Clean A/B (same storm): pane pointed at a quiet dir
   (`/Applications`) that receives the event flood but doesn't render the churn → GPU flat; pane pointed at the churning
   dir with many streaming rows → GPU climbs. The balloon scales with the count of continuously-re-rendering rows.

### Two independent frontend update paths (know which one you're hitting)

- **Index-size updates** (`index-dir-updated` → `refresh_listing_index_sizes`): recursive sizes filling in during a
  scan. **Already leading-throttled to 2 s/pane** (`pane/index-events.ts`). A background scan of static content updates
  the list at most ~0.5/sec — this path is NOT the frequency problem.
- **`directory-diff`** (a viewed directory's own files being created/deleted — downloads, copies, builds writing into
  the open folder): backend coalesces at 50 ms (`file_system/listing/diff_emitter.rs`, up to ~20/sec) → frontend
  `softRefreshTick` → `fetchVisibleRange`. This was the **unthrottled** path.

## What we changed (landed on `main`, 2026-07-15)

- **Watchdog on `phys_footprint` + rich breakdown** (`indexing/memory_watchdog.rs`). Query `TASK_VM_INFO` for
  `phys_footprint` and base the 8/16 GB thresholds on it (not `resident_size`). On WARN/STOP, log a full snapshot:
  phys_footprint (+ ledger peak), RSS + max, the **resident−phys delta** (labeled as the graphics/GPU hint), the summed
  malloc-zone heap (indexing's real footprint), and `live_event_count`. The delta is the single best discriminator:
  large resident−phys ⇒ WebView/GPU, not indexing.
- **Drop `will-change: transform` on `.virtual-window`** (`file-explorer/views/FullList.svelte`). It force-promoted a
  permanent GPU layer WebKit kept re-backing on every scroll/content change. Fresh-scan GPU peak ~670 → ~490 MB (~25%),
  and it restored healthy oscillation (reclaim) instead of a monotonic climb. `translateY` scroll still composites on
  demand. A guardrail comment in the CSS says not to re-add it.
- **Throttle the `directory-diff` refetch to ≤4/sec** (`file-explorer/pane/listing-diff-sync.svelte.ts`). Leading +
  trailing `createThrottle` (250 ms) behind one named knob `INDEX_LISTING_UPDATE_MIN_INTERVAL_MS`; first update instant,
  rest capped. Cursor/selection reconciliation stays UNthrottled (must be exact); nav/sort/view-mode/initial-load (the
  `cacheGeneration` cold-reset path) untouched and instant.

### Rejected: paint-containment on rows

`contain: layout paint` on `.file-entry` **backfired** — it gave each of ~138 visible rows its own retained backing
store; under continuous re-render they accumulated and GPU climbed monotonically to ~980 MB (worse than baseline). Don't
reach for per-row containment here.

## The numbers (all rough — see "measurement is noisy" below)

Isolated current-HEAD instance, `phys_footprint` idle ~400–500 MB throughout. Fresh-scan GPU (`IOAccelerator`) peak over
~90 s, pane at `/Users/<user>` (~138 rows, sizes streaming):

- Baseline (stock): ~670 MB, oscillating (one hotter run hit ~1.0 GB).
- `will-change` removed: ~490 MB, oscillating — **~25%**.
- `+ paint-contain rows`: ~980 MB, monotonic — **backfired**.

`directory-diff` throttle: not cleanly GPU-quantified (remote pane-driving of the isolated instance kept failing, and
the metric is noisy). Its rate cap is deterministic (`createThrottle` unit tests): up to ~20/sec → ≤4/sec.

## Measurement methodology & gotchas (the expensive lessons)

- **`vmmap -summary <pid>` region breakdown is the key instrument** — it's what cut through wrong theories. Watch these
  rows: `IOAccelerator` (GPU), `MALLOC_SMALL`/`MALLOC_LARGE` (Rust/C heap), and `Physical footprint` / peak. RSS alone
  (`ps -o rss`) conflates them and will mislead you. Example win: a rescan's RSS spike to 2.4 GB was `MALLOC_LARGE`
  (transient heap accumulators), **not** GPU — `IOAccelerator` stayed flat. Always confirm which region moved.
- **`resident_size` above ~1 GB rounds to 0.1 G in vmmap summary.** For finer readings keep the instance's GPU below 1 G
  (fresh process) or use `ps -o rss` (KB) as a proxy — but remember RSS conflates heap + GPU.
- **GPU surfaces are sticky. Only a full process restart resets the floor.** A webview `location.reload()` does NOT free
  them (it went UP). Injecting the CSS fix does NOT retroactively free them. `window.minimize()` is blocked by the app's
  capabilities. So per-condition clean baselines require killing + relaunching the app.
- **Hard-killing the app mid-scan corrupts the index (WAL not checkpointed) → the next launch does a full fresh scan**,
  which itself balloons GPU on startup and is a different-sized storm than a replay. To compare conditions, standardize:
  `rm` the index db before each launch to force a consistent fresh scan, or ensure a graceful quit so relaunch does a
  cheap replay. Don't mix scan-launches and replay-launches in one comparison.
- **Measurement is noisy: ±~250–300 MB run-to-run**, because WebKit constantly allocates/releases. Only trust LARGE
  deltas or qualitative behavior changes (e.g. monotonic-climb vs oscillation, which caught the containment backfire).
  Prefer measuring a _bundle_ of changes for a big clean signal over chasing per-lever slivers through the noise.
- **Reproducing the balloon needs BOTH a storm AND a pane rendering many streaming rows.** A storm with the pane on a
  quiet dir doesn't balloon. Viewing a churn dir with only a few rows barely moves. The reliable repro: pane at a dir
  with ~100+ rows whose sizes stream, plus a storm.
- **Cheap storm generator: `cp -cR <big-tree> dir/cN` (APFS clonefile)** creates thousands of FSEvents at near-zero disk
  cost; loop create/delete for sustained churn. For the `directory-diff` path specifically, create/delete files
  **directly** in the _viewed_ directory (not a subdir).
- **Isolated test instance:** `pnpm dev --worktree <slug>` (current HEAD) gives a separate data dir + ports so you can
  restart/edit/measure without touching the main dev instance. Read the port from `<data-dir>/tauri-mcp.port` after each
  relaunch (it changes). Drive/measure via the Tauri MCP bridge; the app PID changes each launch. (v0.8.2-era builds
  lack `--worktree` and the `.port` files — isolate by hand via bundle id + `CMDR_*` env.)
- **Frontend memory is opaque to JS.** `getComputedStyle`/`getAnimations`/`MutationObserver` can show the DOM churn rate
  and which elements re-render (useful — that's how we saw ~135 name-node rebuilds/sec), but NOT layer backing-store
  memory. For that it's `vmmap` regions, or Safari Web Inspector's Layers panel / Instruments (GUI, needs a live repro).

## Open item (deferred)

**Thumbnail / image-index memory** was explicitly out of scope here. Unbounded thumbnail decode + GPU textures is the
classic RAM/GPU balloon and is the untested path for the "enable image indexing" scenario. If a future high-memory
report involves image indexing, start there: bound decode to display size, hard-cap the thumbnail cache (LRU MB budget),
`decoding="async"` + lazy, release off-screen decodes.
