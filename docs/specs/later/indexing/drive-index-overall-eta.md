# Follow-up: overall indexing ETA (with backend per-phase calibration)

Deferred from the drive-indexing progress plan. The per-volume step checklist ships per-STEP ETAs only (the active
step's own estimate, where its denominator is trustworthy). A true **overall** "~Xm left" across all remaining steps is
deliberately NOT built yet, to keep the honest-ETA spine intact.

## Why deferred, not just unfinished

An overall ETA is only honest if the not-yet-started steps have real estimates. Those need persisted **per-phase**
priors (how long this volume's last scan / save / compute / reconcile each took), and only the SCAN phase has one.

- **Scan: already persisted, per volume and per walk kind.** `scan_duration_ms` lands in the index DB's `meta` at
  completion, in three buckets (`scan_duration_ms_full_walk`, `scan_duration_ms_change_check`, and the unsuffixed
  last-completed-scan fallback), and is read back as a `ScanCalibrationSet` to seed the active step's ETA. The two walks
  differ by roughly 5x on the same volume, which is why they are bucketed rather than averaged.
  `crates/cmdr-index/src/indexing/store/mod.rs` owns that shape; `lifecycle/manager/start.rs` and
  `lifecycle/network_scan.rs` are the write sites.
- **Save, compute, and replay: no priors at all.** Their `duration_ms` lives only in `DEBUG_STATS.phase_history`
  (`crates/cmdr-index/src/indexing/events/mod.rs`), an app-wide ring capped at 20 entries that a reset clears and a
  restart empties. Nothing is per-volume and nothing survives a launch.

A "rough overall ETA" built without them collapses to _just the active step's ETA wearing an "overall" label_ — which
trips the plan's own honest-ETA rule. So overall ETA is deferred as one coherent unit WITH the calibration the remaining
steps still lack.

## What v1 ships instead (and why it's enough for now)

- The **step-of-N structure** answers "where am I, how many steps left?" directly — every step is visible with its
  state.
- The **active step shows its own ETA** where the denominator is trustworthy (calibrated scan, computing, writing,
  replay), which on a rescan is most of the wall-clock.

## What this follow-up needs

1. **Backend**: extend the persisted calibration from scan-only to every phase (a per-volume meta-write at pipeline end
   for save, compute, and replay), plus a read to seed their estimates at the next scan's start. The scan half of this
   is done, so the work is following its pattern outward, not inventing one: same `meta` keys, same per-walk-kind
   bucketing where the phase's cost differs by walk.
2. **Frontend**: sum the active step's live ETA with the seeded estimates of the pending steps into one honest overall
   figure, shown once (not per step). Keep the per-step ETA too, or fold it in — a UX call at build time.
3. Only show the overall figure once the seed exists (a first-ever scan has no priors → no overall ETA, same honest
   stance as the count-first first-scan policy).

Frontend seam today: `IndexingStatusBody` derives the steps (`indexing-steps.ts`) and renders each active step's ETA;
the overall figure would layer on top of that, fed by a new per-volume calibration read.
