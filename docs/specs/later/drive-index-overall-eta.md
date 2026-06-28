# Follow-up: overall indexing ETA (with backend per-phase calibration)

Deferred from the drive-indexing progress plan. The per-volume step checklist ships per-STEP ETAs only (the active
step's own estimate, where its denominator is trustworthy). A true **overall** "~Xm left" across all remaining steps is
deliberately NOT built yet, to keep the honest-ETA spine intact.

## Why deferred, not just unfinished

An overall ETA is only honest if the not-yet-started steps have real estimates. Those need persisted **per-phase**
priors (how long this volume's last scan / save / compute / reconcile each took). The backend records per-phase
`duration_ms` today only in the in-memory `DEBUG_STATS` ring (capped at 20, reset on restart, not per-volume-persisted).
A "rough overall ETA" built without them collapses to _just the active step's ETA wearing an "overall" label_ — which
trips the plan's own honest-ETA rule. So overall ETA is deferred as one coherent unit WITH its calibration.

## What v1 ships instead (and why it's enough for now)

- The **step-of-N structure** answers "where am I, how many steps left?" directly — every step is visible with its
  state.
- The **active step shows its own ETA** where the denominator is trustworthy (calibrated scan, computing, writing,
  replay), which on a rescan is most of the wall-clock.

## What this follow-up needs

1. **Backend**: persist last scan's per-phase durations per volume (a per-volume meta-write at pipeline end), plus a
   read to seed estimates at the next scan's start.
2. **Frontend**: sum the active step's live ETA with the seeded estimates of the pending steps into one honest overall
   figure, shown once (not per step). Keep the per-step ETA too, or fold it in — a UX call at build time.
3. Only show the overall figure once the seed exists (a first-ever scan has no priors → no overall ETA, same honest
   stance as the count-first first-scan policy).

Frontend seam today: `IndexingStatusBody` derives the steps (`indexing-steps.ts`) and renders each active step's ETA;
the overall figure would layer on top of that, fed by a new per-volume calibration read.
