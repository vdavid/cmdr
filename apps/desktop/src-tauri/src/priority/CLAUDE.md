# Priority (`src/priority/`)

The per-volume priority signals background work yields to. ONE transport-generic order: **interactive > transfers >
indexing** (drive indexing AND image enrichment). This module owns the SIGNALS and pure decisions; consumers compose
them at their own loop boundaries.

## Module map

- `foreground.rs`: last-interactive-activity timestamps, app-wide + per volume. Written by the hot listing IPC.
- `transfers.rs`: per-volume gauge of user-initiated write ops (copy/move/delete/trash/drag-out).

## Must-knows

- **Feed the transfer gauge ONLY from `write_operations::state::register_operation_status` /
  `unregister_operation_status`** (the one lifecycle choke point, shared with the eject busy set). A second feed site
  desyncs the count; a missed unregister is already covered by the manager's panic-safe cleanup.
- **A missing foreground entry means "never browsed" = idle.** ❌ Don't collapse it to a `0` timestamp — `0` is a real
  clock point, and every background user would stall for the app's first threshold window.
- **Consumers pick their own scope on purpose** (documented per consumer in `foreground.rs` + `DETAILS.md`): enrichment
  reads APP-WIDE foreground + per-volume transfers; scan pacing and transfer-yield read PER-VOLUME. Don't "unify" the
  scopes.
- **Indexing yields must keep forward progress structural**: throttle-to-one or pause-with-resume, ❌ never a gate that
  can stop work with no wake-up path (see `indexing/network_scanner/scan_pace.rs`'s never-zero budget).

Design, the full consumer wiring, and decisions: `DETAILS.md`.
