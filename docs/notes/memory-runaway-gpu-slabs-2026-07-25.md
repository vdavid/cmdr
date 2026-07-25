# Memory runaway: WebKit orphans 128 MB GPU compositor slabs during the startup indexing window (2026-07-25)

> **SUPERSEDED (2026-07-25).** Its central conclusion — that this is WebKit GPU/compositor memory — is WRONG: `vmmap`
> reports Cmdr's Rust heap under the `IOAccelerator` name (a mimalloc VM-tag collision). Read
> `docs/notes/memory-runaway-rust-heap-2026-07-25.md` instead. Kept only for the experiment log.

Handoff for a fresh agent continuing the "prod Cmdr balloons to 40–55 GB" investigation. This session ran ~16 live dev
A/B experiments plus live prod repro, `vmmap`/`footprint`/`sample`/Instruments, external web research, and a frontend
re-render probe. It **corrects several claims** in the earlier notes; read this one first, then
`high-memory-gpu-compositor-investigation-2026-07.md` (mental model) and `memory-runaway-nas-pane-2026-07-24.md` (prior
incident, now partly superseded — see "Corrections" below).

No product code changed this session. All probe edits were reverted (`git checkout`); the tree is clean.

## TL;DR (what's new and load-bearing)

- **It's still `IOAccelerator` (WebKit GPU compositor backing stores), heap flat.** Re-confirmed by `vmmap` +
  `footprint`. Not the Rust heap, not SQLite resident, not a malloc leak.
- **The slabs are a FIXED 128 MB allocator granularity, NOT ~100 MB content-sized tiles.** `vmmap -v` shows uniform
  `[128.0M` regions filling progressively. This weakens the earlier "re-tiling a large element orphans a content-sized
  tile" theory and matches the external evidence (Bun/claude-code reports: fixed 128 MB IOAccelerator slabs). The leak
  signal is the region **COUNT** stepping up ~1 slab/second.
- **The balloon is a STARTUP-WINDOW phenomenon, ~2–3 minutes, right after launch — not an 11-hour idle creep.** It
  climbs ~1 GB / 10 s (often faster) while the backend does startup indexing work, then FREEZES when that work settles,
  and macOS purges it back down (David's "pops back to 0.5 GB"). The 40–55 GB peak is this same climb when the driver
  runs long enough that purging loses the race.
- **Dev only reproduces a MILD, BOUNDED version** (~1.5 GB / 22 slabs over ~12 s, then freeze + purge). Prod reproduces
  the full runaway reliably. **This gap is the crux: the dev burst is just the initial render; the prod runaway is a
  SUSTAINED driver that dev's fast-settling indexes never trigger.** My whole bisection below tested the dev burst,
  which turned out insensitive to every lever — so it ruled things out for the _burst_, but the _sustained prod driver_
  is still unidentified.
- **Mechanism (stack-confirmed + external evidence): on macOS 26, WebKit allocates a fresh IOSurface/128 MB slab per
  compositor backing-store update and ORPHANS the prior one instead of recycling.** Stacks show
  `RemoteLayerTreeDrawingAreaProxy::commitLayerTree` → `applyBackingStore` → `IOSurface::asCAIOSurfaceLayerContents` →
  `CAIOSurfaceCreate`. Orphaned surfaces aren't in the live layer tree, so the Web Inspector Layers panel and JS can't
  see them (calm ~20 MB while `IOAccelerator` is multi-GB).
- **The direct driver MUST be a frontend re-render.** A pure-backend workload cannot allocate WebKit compositor surfaces
  — those are only ever allocated by a FE render/composite (David's insight, generalized from "importance is 100% BE").
  So backend subsystems matter only two ways: (a) they emit an event the FE renders, or (b) they load the CPU and
  lengthen the FE render. This is the search compass: find the FE re-render that runs continuously during startup and is
  gated by drive-indexing-ON.
- **Leading hypothesis (revised): the index event stream → `refreshIndexSizes` → file-list re-render.** Gated by
  indexing (no index → no live-update watchers → no events → flat), trivial in dev (the local reconcile is a 1-diff
  no-op and dev's NAS is quiet → dev only shows the brief initial-render burst), sustained on prod (real local
  reconcile + the busy-NAS `CHANGE_NOTIFY` firehose). Two FE refresh paths feed it: `index-dir-updated` AND
  `index-aggregation-complete` (both call `refreshIndexSizes` on both panes). **Decisive test (on prod, where the stream
  is real): disable BOTH handlers so `refreshIndexSizes` never fires, and see if the runaway stops.** I only disabled
  `index-dir-updated` in dev, which had no sustained stream to remove.
- **H1 (space-poller footer re-render) is DEMOTED — probably falsified.** It required the poller to be gated by
  indexing, but the poller runs independent of drive indexing, and the dev logs show `volume-space-changed: root` firing
  ~every 2 s during indexing-ON runs. If it also fires with indexing OFF (likely, from normal disk activity) yet
  indexing-OFF was flat, the footer re-render can't be the driver. **Confirm/kill cheaply: re-run indexing-OFF and grep
  the log for `volume-space-changed`; if it fires and stays flat, H1 is dead.** (The original H1 also wrongly assumed
  the NAS index DB is scan-_written_ at startup — it is only _loaded_; only `CHANGE_NOTIFY` upserts write it.)

## The confirmed facts (measured this session)

### It is the GPU compositor, in fixed 128 MB slabs

- `footprint -p <pid>` mid-climb (dev): `IOAccelerator` 606 MB dirty / 24 regions, `MALLOC_SMALL` 161 MB, `Foundation`
  102 MB, everything else <10 MB. Unambiguously GPU, not heap.
- `vmmap -v <pid> | grep '^IOAccelerator'`: every region is `[128.0M` virtual, filling from 0 → 128 MB dirty. Uniform
  slabs, not variable content-sized tiles.
- The **region COUNT** is the cheap leak signal. It only ever goes up during a climb; purges drop the _dirty_ bytes but
  leave the count (purged-but-still-mapped slots).

### The staircase and the freeze

- Climb rate ≈ 1 slab (128 MB) per second while the driver runs. Dev: count 9→22 over ~12 s (webview-alive ≈ t+8 to
  t+20), then FREEZES at 22 and macOS purges (dirty collapses, count holds). Prod: count climbs into the 40s–80s+ over
  minutes.
- The freeze coincides with the backend's startup indexing work settling. When the driver stops, no new slabs; macOS
  reclaims under pressure.

### Prod repro (reliable, live tonight)

- Launched `/Applications/Cmdr.app` with a NAS pane restored. Three relaunches:
  - Run A (right pane Brief): phys 2.0→3.1 GB, `IOAccelerator` dirty 1.7→2.6 GB, count 28→36→43, then purge to ~700 MB.
  - Run B (both panes Full): phys 1.7→5.2 GB→purge, `IOAccelerator` count 27→31→40→47→63→79→81→83, dirty peaked ~6.7 GB
    (much of it swapped/compressed). **View mode is NOT the trigger — both-Full still ran away.**
  - Run C: confirmed uniform 128 MB slabs via `vmmap -v`.
- Purges mid-run don't stop the count climbing (run B stepped 79→83 straight through a purge). The driver stops on its
  own schedule, not the purge's.

### Dev repro (mild, bounded — the important limitation)

- `pnpm --filter @cmdr/desktop tauri dev -m`, NAS pane + `/Users` pane, drive indexing ON. Every launch: count 9→~22
  over ~12 s, ~1.5 GB peak dirty, then freeze + purge. **Never sustains, never approaches prod scale**, regardless of
  what I disabled.
- Dev with drive indexing OFF: flat, count ~10, ~185 MB. This is the only lever that materially changed the dev burst.

## Stack + external evidence for the mechanism

- `sample <pid>` during both dev and prod climbs shows the main thread in
  `RemoteLayerTreeDrawingAreaProxy::didReceiveMessage` → `commitLayerTree` →
  `RemoteLayerTreePropertyApplier::applyProperties` → `RemoteLayerTreeNode::applyBackingStore` →
  `WebCore::IOSurface::asCAIOSurfaceLayerContents` → `CAIOSurfaceCreate`, plus `runJavaScriptInFrameInScriptWorld`
  (Tauri pushing events/IPC into the webview). This is the UI-process side mapping a fresh backing surface per commit.
- External research (web-research subagent, full report in this session's transcript):
  - **Apple Developer Forums thread 774005** — the closest engine match: a minimal WKWebView making a tiny periodic
    change (pageZoom / SVG mutation) leaks IOSurface to 3.58 GB in ~20 s, stack rooted in
    `RemoteLayerBackingStoreProperties::layerContentsBufferFromBackendHandle`. "Small periodic change → large backing
    store re-allocated, prior orphaned." Apple said file a bug (FB16462982); no fix. (iOS 18, same engine.)
  - **macOS 26 (Tahoe) broad memory-leak regression**, Apple DTS semi-acknowledged (apps up to 5× memory vs Sequoia,
    blamed on the new "Liquid Glass" rendering stack). So part of this is an OS regression that amplifies orphaning;
    mitigations reduce but may not fully eliminate.
  - **claude-code #35804 / Bun #28234** — same fingerprint (monotonic 128 MB IOAccelerator slabs from a periodic
    re-render, RSS wildly understates footprint, orphaned per `lsof`). Our 128 MB matches their fixed slab size.
  - **The "orphaned but sometimes purged" behavior** matches WebKit's backing-store volatility: idle backing stores are
    normally marked purgeable, but if the UI process holds a "use" on a surface it can never go volatile → it stays
    dirty until a hard pressure purge. Explains the climb-freeze-purge shape.

## What I RULED OUT (each still produced the full dev ~22-slab burst)

These were bisected against the dev burst. Because the dev burst is just the initial render (see below), ruling them out
means "not the cause of the dev initial-render burst." A few (space poller, aggregation handler) were NOT tested and
remain open — see gaps.

1. **View mode (Brief vs Full).** Prod run B: both Full still ran away. Note: `BriefList.svelte` still has
   `will-change: transform` on `.virtual-window` (the July-15 fix was applied only to `FullList.svelte`; guardrail
   comment at `FullList.svelte` ~line 1207). Worth removing as hygiene, but NOT this bug.
2. **Window size** (800×500 via `.window-state.json` vs full-screen). Identical staircase.
3. **Media / Vision / CLIP scheduler.** Disabled `media_index::scheduler::start(app.handle())` in `lib.rs` → still 22.
   **Image indexing was OFF the entire session** (David confirmed) — the media stack is definitively not involved.
4. **Importance scheduler.** Disabled `importance::scheduler::start` → 22 (vs 28 with it on; importance contributes ~6
   slabs of startup rescore work but is not the core driver).
5. **Frontend `index-dir-updated` handler.** Commented out `initIndexEvents(handleIndexDirUpdated)` in
   `DualPaneExplorer.svelte` (~line 678) → still 22. So live size-refresh via that path is not the burst driver.
6. **Pulse animations.** Disabled the 2 s infinite opacity pulses on `IndexingStatusIndicator.svelte` (~line 171) and
   `DriveIndexBadge.svelte` (~line 287) → still 22. Also: the local reconcile at startup was a **1-diff no-op**
   (`Verifier: 1 diffs … 1 modified [~.claude.json]`), so the indexing indicator wasn't even actively animating. David
   was right that "we're not scanning" at these starts.
7. **Folder-size rendering.** Forced `getDirSizeDisplayState()` (`views/full-list-utils.ts` ~line 386) to always return
   `'dir'` so every folder rendered the `<dir>` placeholder → still 22. So the _text content_ of sizes isn't it (a
   re-render that renders `<dir>` still re-composites).
8. **Pane content.** Both panes on an empty dir → 17 (vs 22 for real dirs). Content adds ~5 slabs, but 17 is still a big
   burst — so most of it is content-independent.

**The only lever that changed the dev burst: the master drive-indexing toggle (ON ~22 / OFF flat ~10).**

## The reframe (ultrathink takeaway — read this before doing more work)

The dev ~22-slab climb is **insensitive to every subsystem lever**, scales only with the master indexing toggle, and
FREEZES after ~12 s. That profile says it's the **initial-render composite burst** of the webview: the page paints and
composites over ~12 s (slow because dev is a debug build and the machine is busy indexing), allocating backing store
that macOS 26 orphans instead of recycling, then it settles and purges. Drive-indexing-ON lengthens/heavies that render
(more backend load during startup → longer settle → more transient slabs), which is the 10→22 delta.

**This is NOT the prod runaway.** Prod's balloon runs for 2–3 minutes and reaches 300+ slabs. Something SUSTAINS the
re-compositing on prod that dev's fast-settling indexes never trigger. My bisection localized the _burst_, but the
_sustained driver_ is still open. The correlation to chase: **climb duration ≈ how long the backend stays busy at
startup** (dev: ~12 s tiny indexes; prod: minutes on 231k local + a huge NAS index). So the driver is very likely a
periodic backend→frontend event that fires while the backend is busy and re-composites a surface each time.

## Leading hypotheses for the SUSTAINED prod driver

**Compass (David's insight):** the direct driver MUST be a frontend re-render. A pure-backend workload cannot allocate
WebKit compositor surfaces; backend subsystems matter only by (a) emitting a FE-rendered event or (b) CPU-contending to
lengthen the FE render. So look for the FE re-render that (i) runs continuously through the startup window and (ii) is
gated by drive-indexing-ON.

### H-primary: index event stream → `refreshIndexSizes` → file-list re-render

- Gated by indexing: no index → no live-update watchers (local FSEvents reconcile + SMB `CHANGE_NOTIFY`) → no events →
  flat. This is the missing "why drive-indexing-ON matters" link that H1 lacked.
- Two FE paths fire `refreshIndexSizes()` on BOTH panes: the `index-dir-updated` handler (`DualPaneExplorer.svelte`
  ~line 678, via `index-events.ts`) AND the `index-aggregation-complete` handler (`DualPaneExplorer.svelte` ~line 681).
  `refreshIndexSizes` re-fetches the visible range and re-renders the list (a compositor commit) — even when the
  rendered text is unchanged, which is why forcing sizes to `<dir>` didn't help (it changed the text, not whether the
  re-render fired).
- Trivial in dev (local reconcile = 1-diff no-op, NAS quiet) → dev only shows the brief initial-render burst and never
  sustains. Sustained on prod (real 231k-entry local reconcile + the busy-QNAP `CHANGE_NOTIFY` firehose). This matches
  "climb duration ≈ how long the backend stays busy at startup" exactly.
- The NAS `CHANGE_NOTIFY` firehose (`smb_watcher`, starts at mount regardless of viewing — log:
  `smb_watcher(naspi): connected, starting watch`) is the prod amplifier: on the real QNAP (11.3 M files,
  `@Recently-Snapshot`, background services; the incident's "1.7 M live FSEvents") it's a continuous `index-dir-updated`
  stream. `index-events.ts:41` has a `"/"` sentinel (`if (paths.includes('/')) return true`) that refreshes EVERY pane
  on a single overflow event — so a NAS firehose re-renders both panes even with both on local dirs (explains "NAS
  enabled makes it worse without viewing the NAS"). NOTE the NAS does NO scan/reconcile (SMB has no journal); the
  firehose is live `CHANGE_NOTIFY` upserts, not indexing.
- I disabled only the `index-dir-updated` handler in dev (still climbed) — but dev had no sustained stream to remove,
  and `index-aggregation-complete` stayed live. Not a valid test of this hypothesis.
- **Decisive test (on PROD, where the stream is real): disable BOTH FE refresh paths** (`index-dir-updated` +
  `index-aggregation-complete`) so `refreshIndexSizes` never fires, relaunch, and watch whether the runaway stops. If it
  does → the fix is to stop the list from fully re-compositing on a size refresh (patch cells in place / decouple from
  the big scroll surface), which helps regardless of the macOS 26 orphaning. Alternative repro path: get a sustained
  event stream in dev (hard broad NAS `CHANGE_NOTIFY` churn with the FE handlers live) and confirm a SUSTAINED climb
  past the ~12 s freeze first.

### H1 (space-poller footer re-render) — DEMOTED, probably falsified

- Chain was: index DB writes → boot-drive free space moves ≥ 1 MB → `space_poller.rs` emits `volume-space-changed` →
  `volume-space.svelte.ts:81` reassigns `$state` unconditionally → footer re-renders → re-composite.
- **Why it probably fails (David's objection):** the space poller is independent of drive indexing. Dev logs show
  `volume-space-changed: root` firing ~every 2 s during indexing-ON runs; if it also fires with indexing OFF (likely,
  from ordinary disk activity) yet indexing-OFF was flat, the footer re-render can't be the driver — it would fire in
  both. Also, the NAS index DB is _loaded_ at startup, not scan-_written_, so it doesn't drive boot-drive free-space
  churn (only `CHANGE_NOTIFY` upserts write it) — killing the "NAS without viewing" rationale.
- **Cheap confirm/kill:** re-run indexing-OFF and grep the log for `volume-space-changed`. Fires + flat → H1 dead. Keep
  the unconditional-`$state`-reassignment (no equality guard) noted as a minor perf smell regardless.

### Ruled out as the DIRECT driver (structural)

- **Importance / interestingness scoring** and any other pure-backend startup work: 100% backend, no FE render, so it
  cannot allocate compositor surfaces directly. Disabling importance dropped 28→22 (CPU-contention shortening of the
  render), consistent with "contributes via (b), isn't the allocator." Cheap to disable as a confirmation, but
  structurally it can't be the driver — don't spend the prod cycle on it.

### Still-open sub-questions

- Confirm a `refreshIndexSizes` re-render actually re-backs a _128 MB_ (large) surface, not a small one — i.e. that the
  list's big scroll surface is the one being orphaned (consistent with the Apple-forum "small change → large backing
  store re-allocated" mechanism, but unverified here).
- `index-scan-progress` (500 ms during an actual scan) and the initial listing streaming render — lower priority (no
  scan happens at these starts), but part of the same "FE re-render during startup" family.

## The tooling playbook (exact commands — reuse these)

### Measuring GPU memory (ground truth)

```
PID=$(pgrep -x Cmdr | head -1)            # prod; for dev exclude the prod PID
vmmap -summary "$PID" | grep -iE "Physical footprint:|^IOAccelerator "
# Watch: Physical footprint (the truth), IOAccelerator dirty (col 4) + swapped (col 5),
# and the region COUNT (last col). Phys + IOAccelerator up together, heap flat = this bug.

vmmap -v "$PID" | grep '^IOAccelerator'   # per-region sizes; proves uniform 128 MB slabs
footprint -p "$PID"                        # category attribution (IOAccelerator vs MALLOC vs Foundation)
```

`ps`/RSS lies here (keeps counting purged regions after phys collapses). Use `phys_footprint`.

### Attribution (which stack allocates)

```
sample "$PID" 12 -file out.txt             # 12 s stack sample; grep commitLayerTree, CAIOSurfaceCreate, applyBackingStore
xcrun xctrace record --template 'Metal System Trace' --attach "$PID" --time-limit 30s --output climb.trace
# Then: xctrace export --input climb.trace --xpath '...'  (heavy, weak per-alloc attribution — lower ROI than sample)
```

For prod allocation backtraces (surest path if dev won't sustain): Instruments **"Game Memory"** template (VM Tracker +
Metal Resource Events) against `/Applications/Cmdr.app`, or set `isInspectable = true` on the release WKWebView (one
line + a prod build) to attach Safari Web Inspector during the real 40 GB climb. Platform note: Tauri on macOS is
**WKWebView (WebKit), devtools = Safari Web Inspector**, not Chromium.

### Frontend re-render probe (injected into `app.html` <head>, dev only)

Inject a `<script>` before `%sveltekit.head%` that installs a `MutationObserver` (counts per-second by
`m.type + ':' + className`), a `requestAnimationFrame` frame counter (compositor activity), `document.getAnimations()`,
and a `PerformanceObserver({entryTypes:['resource']})`. Read it back via the Tauri MCP bridge with `webview_execute_js`
returning `JSON.stringify(window.__probe...)`. **Finding this session:** DOM mutations burst at t≈3–5 s (initial listing
render: `col-name-text` ×210, `file-entry`, `virtual-window`), then go quiet except `disk-space-text` ~1/s — while
`IOAccelerator` keeps climbing to t≈20. **The GPU climb OUTLASTS visible DOM mutation**, so either the re-composite
isn't DOM-mutation-driven after initial paint, or it's driven by something MutationObserver doesn't see (the
`disk-space-text` footer tick is the main survivor — supports H1).

### Tauri MCP bridge (drive/inspect the dev webview)

- Find the bridge port: `lsof -p <devPID> -iTCP -sTCP:LISTEN -P` (the non-19225 port), or
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/tauri-mcp.port`.
- `driver_session` action=start with that port → `webview_execute_js` / `webview_dom_snapshot` / `read_logs`.
- **Gotcha:** the bridge returns "WebView execution failed" during heavy startup load; retry after settle, or stop+start
  the session. Two windows exist (`main`, `settings`) — pass `windowId: "main"`.

### Prod repro setup

- Restore a NAS pane: edit `~/Library/Application Support/com.veszelovszki.cmdr/app-status.json` — set `leftPath`/
  `leftVolumeId` to `/Volumes/naspi` / `smb-192-168-1-111-445-naspi` AND the matching active tab entry.
- Launch with per-line RAM: `CMDR_LOG_RAM_USE=1 /Applications/Cmdr.app/Contents/MacOS/Cmdr`.
- **Quit gracefully:** `osascript -e 'tell application "Cmdr" to quit'` then wait for exit. A hard `kill` corrupts the
  index WAL → next launch does a full fresh scan → different (heavier) memory profile. Don't mix scan-launches and
  load-launches in one comparison.

### Dev repro setup

- `pnpm --filter @cmdr/desktop tauri dev -m` (from repo root; the wrapper handles the dev data dir + ports).
- Dev data dir: `~/Library/Application Support/com.veszelovszki.cmdr-dev/`. `app-status.json` controls panes;
  `.window-state.json` controls size; `logs/cmdr.log` is the live log (NOT the stale `~/Library/Logs/…-dev/Cmdr.log`).
- **Process hygiene (bit us once):** before relaunching, `pkill -f "tauri dev"` + `kill` the Cmdr binary AND wait for
  full exit (`until [ -z "$(pgrep -x Cmdr)" ]; do sleep 1; done`). Otherwise the single-instance guard fires ("Cmdr is
  already running" dialog) and you measure a stale/failed instance. Multiple orphaned `tauri dev` trees accumulate
  across runs — kill them all.

### Logs

- Prod: `~/Library/Logs/com.veszelovszki.cmdr/cmdr.log`. Dev:
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/logs/cmdr.log`.
- Useful greps: `start_indexing`, `network_scan` ("loading as Stale"), `reconcile`/`MustScanSubDirs`,
  `volume-space-changed`, `smb_watcher.*processing`, `live tick`.

## Dev measurement table (IOAccelerator slab count at freeze, ~1.5 GB peak dirty each)

- Drive indexing OFF: flat, count ~10, ~185 MB.
- Indexing ON, default-ish: 28.
- Indexing ON, importance + media schedulers disabled: 22.
- Indexing ON, empty panes: 17.
- Indexing ON, sizes forced to `<dir>`: 22.
- Indexing ON, pulse animations off: 22.

## Prioritized next steps

1. **Test H-primary on PROD (the real repro): disable BOTH FE refresh paths** — comment out
   `initIndexEvents(handleIndexDirUpdated)` (~line 678) AND the `index-aggregation-complete` listener (~line 681) in
   `DualPaneExplorer.svelte`, prod build, relaunch with a NAS pane. If the runaway stops or shrinks sharply, the
   file-list re-render on size-refresh is the driver. **Highest-value test.**
2. **Cheap H1 confirm/kill in parallel:** re-run indexing-OFF and grep the dev log for `volume-space-changed`. Fires +
   flat ⇒ the footer re-render is out (expected).
3. **If a prod source rebuild is heavy, first get dev to SUSTAIN the climb:** enable NAS indexing, panes local, drive a
   HARD broad `CHANGE_NOTIFY` stream (churn many NAS dirs at once, not one) with the FE handlers live; confirm a climb
   that keeps going past the ~12 s freeze. That gives a fast local bisection surface for H-primary.
4. **If dev still won't sustain, instrument PROD directly** (reproduces reliably): Instruments "Game Memory" (VM Tracker
   - Metal Resource Events) for per-surface allocation backtraces, or `isInspectable=true` + Safari Web Inspector during
     the 40 GB climb (Timeline/Layers).
5. **Skip (structural):** importance / other pure-BE toggles as _the_ driver — they can't allocate compositor surfaces.
   Fine as a one-shot confirmation, not worth a prod cycle.
6. **Mitigations worth landing regardless** (reduce re-composite churn; shrink the balloon even if macOS 26 is also at
   fault): make `refreshIndexSizes` patch the visible cells in place instead of re-rendering / re-fetching the whole
   list window; narrow the `"/"` sentinel in `index-events.ts` so a NAS overflow doesn't refresh unrelated panes;
   coalesce/rate-limit the backend `index-dir-updated` emit under a `CHANGE_NOTIFY` firehose; remove
   `BriefList.svelte`'s leftover `will-change: transform`; audit for any continuously-animating or re-tiling composited
   layer.
7. **File a WebKit/Apple Feedback** with the `vmmap` (128 MB orphaned IOSurfaces) + `sample` (`commitLayerTree` →
   `CAIOSurfaceCreate`) evidence. Part of this is the macOS 26 orphaning regression; app-side mitigations reduce but may
   not fully eliminate it.

## Corrections to prior notes (supersede where they conflict)

- **Slab size:** 128 MB fixed allocator granularity, not ~100 MB content-sized. (The
  [2026-07-24 note](memory-runaway-nas-pane-2026-07-24.md) said ~100 MB "a re-tiled large element"; the uniform 128 MB
  `vmmap -v` regions say fixed-slab allocator instead.)
- **Timing:** ~2–3 minutes at startup, fast (~1 GB/10 s), not "healthy ~11 hours then vertical." The 2026-07-24 note's
  11-hour framing was that day's watchdog log; David clarified the real balloon is a fast, minutes-long startup event
  reproducible on demand.
- **Trigger phase:** the STARTUP indexing window (backend busy), not "idle after a settled index with no interaction."
  It freezes once startup work settles.
- **Not a scan:** at these starts the NAS loads its cached index (no scan/reconcile) and the local reconcile is a
  ~1-diff no-op. So the indexing _indicator_ and _scan progress_ are not the driver.
- **Image indexing:** OFF all session; the media/Vision/CLIP stack is definitively not involved (scheduler fully
  disabled → still climbs).
- **View mode:** not the trigger (both-Full still ran away in prod). BriefList's `will-change` is a separate hygiene
  fix.

```

## Addendum (2026-07-25 ~02:45, post-handoff runs 13+)

Late-night results that CORRECT parts of this doc:

- **It does NOT self-heal on David's machine.** The 50 GB run had to be killed manually; the swap-spiral exit (surfaces
  compressed/swapped, never purged) is the real-hardware default, not the benign purge-pop. Treat severity accordingly.
- **H-primary is falsified for the dev burst.** Run 13: drive indexing ON, BOTH FE refresh paths disabled
  (`initIndexEvents(handleIndexDirUpdated)` AND the `index-aggregation-complete` listener in
  `DualPaneExplorer.svelte`) → identical staircase (28 slabs, ~1.5 GB dirty, freeze ~t+20 s). The dev burst is
  indexing-gated but NOT via any FE index-refresh handler. (Still untested on prod's sustained stream, but the dev
  falsification removes its priority-1 status.)
- **The "virgin window" property (new, load-bearing):** the leak only operates from launch until the first big
  purge/settle. After that the process is immune: 60 s of hard local churn on a settled indexing-ON instance (FSEvents
  → upserts → backend emits at full cadence) produced ZERO new slabs (dirty stayed ~430 MB, count frozen). This is why
  every mid-run lever test is uninterpretable, why quit+relaunch is the only repro, and why turning things off
  mid-climb "does nothing." All future A/B tests must be fresh-launch tests.
- Implication for the sustained-prod-driver hunt: the question is not "what re-renders forever" but **"what keeps the
  startup window open"** — the driver only needs to outrun the first purge. Prod's longer window (~80 s vs dev's
  ~20 s) with a NAS index correlates with startup work depth, still unattributed.
- Remaining untested candidates for the indexing-ON gate in dev (with all schedulers/handlers ruled out): the backend
  EVENT EMISSIONS themselves (each Tauri emit = `runJavaScriptInFrameInScriptWorld`, hot in every climb sample; emits
  fire regardless of listeners), the drive-index status/freshness FE store cycle, and the listing enrichment path
  (index-joined recursive sizes in the listing payload). Next fresh-launch bisect should silence backend emits at the
  source (one lever at a time) rather than FE listeners.
```
