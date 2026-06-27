# Transfer queue + pause — v2 follow-ups

Created 2026-06-21. Status: deferred. Parent: the shipped v1 transfer-queue + pause work (durable home:
`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § Operation manager; design history in git).

v1 shipped a lane-based queue (serialize per shared resource, parallelize disjoint), pause/resume between files, a
standalone macOS queue window, and Pause + Queue (F2) controls on the progress dialog (cancel-only, no rollback). These
are the deliberately-deferred extensions, each independent.

## Per-lane concurrency budgets > 1

v1 fixes `LANE_BUDGET = 1` (one op per lane). The `lane_use` table is already a count map, not a set, so raising a
lane's budget is a localized change.

- Make the budget per-lane, not global: `Volume::lane_budget()` (default 1) alongside `lane_key()`.
- The motivating case: **FTP** (when added) allows N simultaneous connections (often 5); its lane budget = min(N, server
  limit). Local SSD could also benefit from a small budget > 1.
- Admission already reserves all of an op's lanes atomically; only the free-check (`lane_free`) needs to compare against
  the lane's budget instead of the constant.
- Finer local-device detection (per physical disk vs the current per-mount-root key) so two copies on genuinely
  different disks parallelize even under one mount tree.

## Mid-large-file pause

v1 parks pause at between-files boundaries. A single huge file can't be paused mid-transfer.

- Gate at a chunk boundary inside the per-file copy loop (the `SerialLeafProgress::on_chunk` /
  `make_concurrent_per_file_progress` callbacks currently break only on cancel).
- Keep the in-flight stream + `.cmdr-tmp-<uuid>` + the backend connection alive across the pause.
- Decide resume semantics for a partially-written temp (continue the same temp vs restart the file).

## Concurrent copy path pause

v1's `copy_volumes_with_progress` `FuturesUnordered` path is a documented no-op for mid-batch pause. Gate it by pausing
admission of new per-file sub-tasks (and, with mid-file pause above, the in-flight ones).

## Connection keep-alive / reconnect-on-resume

A long pause holds SMB/MTP connections idle and may hit server/USB timeouts. v1 accepts that resume may surface a normal
transient error (SMB reconnects; MTP has a one-shot stale-handle retry).

- Add keep-alive pings during a long pause, or an explicit reconnect-on-resume step before the next chunk/file.
- Bound concurrent paused-and-parked `spawn_blocking` ops if the parked-thread pressure (noted in the parent spec)
  proves real in practice.

## Queue reordering and priorities

- Drag-to-reorder the pending queue; a "run next" / priority bump.
- Admission already walks a single FIFO `order` vector, so reordering is a reorder of that vector under the manager
  lock.

## Persist the queue across app restarts

v1's registry is in-memory; a crash or quit drops queued (not-yet-started) ops. Persist pending op descriptors (with
enough to reconstruct the deferred start) and offer to resume on next launch. Interacts with mid-file resume above.

## Rollback in the new UI (maybe)

v1 is cancel-only by deliberate choice. The backend rollback machinery still exists (the legacy progress-dialog Rollback
button uses it). If users want it, expose a rollback affordance in the queue window / a rollback variant of "Cancel
selected" — but only if the demand is real; cancel-keep-partials covers most needs.
