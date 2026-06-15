# File operations details

The must-knows are in [CLAUDE.md](CLAUDE.md); per-dialog depth lives in each subdir's docs. This file holds only the
umbrella-level detail.

## `scan-throughput.ts`

`ScanThroughput` turns scan-event tally deltas into a calm `filesPerSecond` / `bytesPerSecond` readout over a rolling
window (default 2 s, constructor-overridable). It exists because the backend `EtaEstimator` only covers write phases,
not the scan-preview pipeline, so the frontend computes its own scan-phase rate. The algorithm is deliberately tiny: a
number for the user to read, not a forecast. It returns nulls until two samples have arrived, drops samples older than
the window (always keeping the most recent so a long pause still has a baseline), clamps negative rates to zero, and
resets cleanly between scans.
