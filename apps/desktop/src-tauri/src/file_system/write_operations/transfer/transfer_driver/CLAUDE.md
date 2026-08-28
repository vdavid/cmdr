# Shared transfer driver

The scaffolding all four transfer cores run through (local-FS copy plus the three volume ops), so the bulk-skip prelude,
the per-iter cancellation check, skip accounting, post-loop bookkeeping, and the paired progress + status updates exist
ONCE. Each operation supplies a `transfer_one` closure that does only the per-source work and reports a
`TransferOutcome`. `sync_driver.rs` serves local-FS, `async_driver.rs` the volume ops, `progress.rs` the per-file
callbacks. The transfers themselves: `../CLAUDE.md`.

## Must-knows

- **The whole point is that `transfer_one` is NEVER invoked** (1) for a source in the pre-known-conflicts bulk-skip set
  under `Skip`, (2) after a top-level conflict resolution returned Skip (async driver only), or (3) after cancellation
  is signaled. ❌ Never move a destructive call above those gates. The `transfer_driver_*_tests.rs` suites pin all
  three, so a violation is caught here rather than by inspecting four functions.
- **The cancellation check sits BEFORE any destructive call**, ❌ not after the closure returns.
- **Progress is HIGH-WATER per file**, because an attempt restarts at byte zero: concurrent uses
  `last_file_bytes.fetch_max`, ❌ never `swap` (a `swap` lowers the mark on a restart, then credits the re-streamed
  prefix again). Serial keeps a `leaf_high_water` reset in `on_leaf_complete`, which still adds each leaf's exact size
  once.
- **A directory expands to many leaves through ONE `on_file_progress` / `on_file_complete` pair**, and the bars are
  leaf-granular against preflight LEAF totals, so ❌ never reset the tally per inner file.
- **Every skip credits the bars AND calls `state.note_skipped`**, ❌ never one without the other. The bars must reach
  their totals; the rate must not see bytes nothing moved, or one big skipped file spikes the reported speed.
  `../DETAILS.md` § "Skipped work moves the bars, and stays out of the rate".
- **Sync and async are deliberate siblings, ❌ not one generic driver.** Boxing futures for the sync caller would cost
  an allocation per source and lose the closure's `&mut` captures.
- **Conflict resolution is closure-owned for sync, driver-owned for async.** ❌ Don't unify without moving the sync
  closure's `&mut` state too.

Progress across a retry, and the sync/async and conflict-ownership splits: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
