# Transfer (copy and move)

Frontend for copy (F5) and move (F6) operations: destination picker, dry-run conflict scan, progress dialog with dual
bars, and rich error rendering. Parameterized by `operationType: 'copy' | 'move'` so a single set of components serves
both. The progress dialog is also reused by delete/trash (`operationType: 'delete' | 'trash'`).

Backend counterpart:
[`apps/desktop/src-tauri/src/file_system/write_operations/transfer/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/transfer/CLAUDE.md)
for copy/move semantics, and
[`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md)
for the shared state machine, ETA/throughput, and settle contract.

## File map

| File                            | Responsibility                                                                                                                  |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `TransferDialog.svelte`         | Destination picker, segmented Copy/Move toggle, pre-flight dry-run scan, upfront conflict-policy radios                         |
| `TransferProgressDialog.svelte` | Execution: dual progress bars, cancel/rollback, conflict dialog, scan-phase body, terminal-event handling                       |
| `TransferErrorDialog.svelte`    | Modal that renders backend `FriendlyError` or FE fallback, category-colored container, optional Retry button                    |
| `FriendlyErrorContent.svelte`   | Renders `friendly.explanation` + `friendly.suggestion` markdown; click delegate for `x-apple.systempreferences:` / http(s) URLs |
| `FallbackErrorContent.svelte`   | Renders the FE-derived message when no backend `FriendlyError` is attached to the `WriteErrorEvent`                             |
| `ScanPhaseBody.svelte`          | Scan-phase tallies (files/dirs/bytes), throughput readout, current directory, spinner. Shared by both scan-phase code paths     |
| `DirectionIndicator.svelte`     | Arrow graphic for source → destination (operation-agnostic, reused by `DeleteDialog`)                                           |
| `transfer-dialog-utils.ts`      | `generateTitle()`, `toBackendIndices()` / `toBackendCursorIndex()` ".." offset helpers, `toVolumeRelativePath()`                |
| `transfer-error-messages.ts`    | Operation-specific error strings used by `FallbackErrorContent`                                                                 |
| `transfer-complete-toast.ts`    | Pure `composeTransferCompleteToast({...})`: picks the right "Copy/Move/Trash complete" wording, branches on op/skip/single case |
| `*.test.ts` / `*.a11y.test.ts`  | Vitest unit tests (utility + component) and a11y assertions                                                                     |

## How transfer flows

1. **TransferDialog** (destination picker + dry-run scan)
   - Pre-fills destination from the opposite pane.
   - The segmented Copy/Move toggle is always shown so the user can flip the operation regardless of how the dialog was
     triggered (F5/F6, command palette, drag-and-drop).
   - Validates path structure via `validateDirectoryPath()` from `$lib/utils/filename-validation` (empty, absolute, null
     bytes, length limits), then checks logical constraints (subfolder, same location).
   - Optional dry-run scan to detect conflicts upfront. Shows sampled conflicts (max 200) with streaming progress.
   - User makes conflict decisions before operation starts via a wrap-friendly flexbox of radios: "Skip all", "Overwrite
     all", "Overwrite all smaller", "Overwrite all older", "Ask for each". When `totalConflictCount === 1`, the radio
     labels drop "all" ("Skip", "Overwrite", "Overwrite if smaller", "Overwrite if older") and "Ask for each" becomes
     "Ask later" since a single conflict can't be asked "for each". The conditional policies map to the typed
     `ConflictResolution` variants `overwrite_smaller` / `overwrite_older`. See the BE doc § "Key patterns and gotchas
     (shared)" for the strict-comparison / fail-closed contract.
   - **Folders always merge; the upfront check classifies collisions.** `checkConflicts()` runs on mount **in parallel
     with the scan preview** (it's one cheap dest listing, not the recursive byte scan — `conflictCheckPromise` is
     assigned synchronously in `onMount` BEFORE the auto-confirm branch so the MCP fast path dispatches with
     `conflictNames` populated). Each collision is classified by the backend-resolved `sourceIsDirectory` /
     `destIsDirectory` flags (the BE resolves real per-item types + sizes from the source volume via one batched stat
     when `checkConflicts` passes `sourceVolumeId` + `sourcePaths`):
     - **dir + dir** → a silent merge, NOT a conflict. Surfaced as an informational line ("N folders will merge with
       existing folders"); never counted in `totalConflictCount`; never forwarded as a bulk-skip name (a merging folder
       must not be skipped wholesale).
     - **file + file / cross-type (file↔folder)** → a real conflict. Counts toward `totalConflictCount` and feeds the
       `preKnownConflicts` bulk-skip list.
     - The file-policy radios show when there's a real conflict OR a folder merge — a merge can surface file clashes
       mid-operation the upfront (top-level-only) check can't see, and the radios pre-answer them.
     - **Cross-type guardrail.** When a real conflict is a type mismatch AND the user selects "Overwrite all", a red
       warning appears (mirrors the per-file dialog's file→folder warning): overwriting replaces items of a different
       type, including folder contents.

2. **TransferProgressDialog** (operation execution)
   - If `scanInProgress`, subscribes to scan preview events (`scan-preview-progress`, `scan-preview-complete`, etc.) to
     continue observing the same scan that `TransferDialog` started. Shows scanning progress UI until scan completes,
     then dispatches the operation (guaranteed cache hit). Handles the race condition where the scan completes between
     dialogs via `checkScanPreviewStatus()`.
   - Calls `copyFiles()` or `moveFiles()` based on `operationType`.
   - Subscribes via `onWriteProgress`, `onWriteComplete`, `onWriteError`, `onWriteCancelled`, `onWriteSettled`,
     `onWriteConflict` wrappers (which internally listen to Tauri events). Uses a `BufferedEvent` discriminated union
     (`{ type: 'progress'; event: WriteProgressEvent }`, etc.) to buffer events until the `operationId` is known.
   - Dual progress bars (size + file count). Speed (both bytes/s and files/s) and ETA come pre-computed from the backend
     (`write_operations/eta.rs`) on every `WriteProgressEvent`; the dialog renders the numbers and applies a tiny
     display low-pass to the ETA to prevent flicker. No FE-side math. See BE § "ETA + throughput".
   - Dynamic stage indicator: "Scanning" → "Copying" (+ "Cleaning up" for cross-FS move).
   - **Flushing phase.** When a `write-progress` event arrives with `phase: 'flushing'`, the dialog title shows
     **"Writing the last piece..."** (exact copy). This is the backend's closing `fdatasync` over the freshly written
     destinations — on slow media (USB sticks, SD cards) it's a real multi-second pause, so the bar must not sit frozen
     at 100% pretending the work is done. The phase maps back to the active stage chip (copying/moving) in
     `getStageStatus`, since it's the tail of the copy, not a separate chip. Shown for both copy and move. Pinned by
     `TransferProgressDialog.flushing.test.ts`. See the BE doc § "Durability" for what the flush actually does.
   - **Scanning-phase UI** (both `waitingForScan` and `phase === 'scanning'` paths): rendered via `ScanPhaseBody`. Shows
     source path, running tallies (`bytesFound / filesFound / dirsFound`), FE-computed throughput from `ScanThroughput`
     (`../scan-throughput.ts`), and a spinner. Current directory (`event.currentDir`) renders above the filename so the
     user sees where in the tree the walker is. Title is reframed per operation: "Verifying before copy…", "Counting
     items to delete…", etc. The backend still emits `expectedFilesTotal` / `expectedBytesTotal` on scan events but the
     FE ignores them — the bar this used to drive was visually indistinguishable from the destructive-phase bar and read
     as "already deleting".
   - Conflict resolution inline (if using `Stop` mode instead of dry-run). The per-file dialog has a 2-column grid: left
     column is the single-file action (`Skip` / `Rename` / `Overwrite`), right column is the apply-to-all variant
     (`Skip all` / `Rename all` / `Overwrite all`). A 4th row holds the two conditional bulk actions
     (`Overwrite all smaller` / `Overwrite all older`), which are always apply-to-all by design (no single-file variant;
     the bulk semantic is the point).
   - Cancel button → rollback transaction (user chooses keep/rollback).

3. **TransferErrorDialog** (error display)
   - Renders the backend `FriendlyError` payload from `WriteErrorEvent.friendly` when present (via
     `FriendlyErrorContent`). Falls back to `FallbackErrorContent` when the event has no friendly attached.
   - Container colors and icon vary by `friendly.category`: error-bg + CircleAlert (`serious`), warning-bg +
     TriangleAlert (`transient`), neutral secondary-bg + Info (`needs_action`).
   - "Retry" button shows when `category === 'transient'` or the friendly's `retryHint` is true.
   - Same shape as the listing-error path's `ErrorPane.svelte`, just adapted to a modal dialog.

## Key decisions

### Unified components for Copy + Move

Copy and Move share 95%+ of UI/flow. Differences:

- Labels ("Copy" vs "Move")
- Backend command (`copyFiles()` vs `moveFiles()`)
- Post-completion: move refreshes both panes (source files gone)
- Cross-FS move has an extra "Cleaning up" stage

Parameterizing by `operationType` avoids duplication and guarantees UX consistency.

### Same-FS move optimization

When source and destination are on the same filesystem (checked via `metadata.dev()`), backend uses instant `rename()`.
Frontend handles this by:

- Skipping progress dialog if operation completes before render
- Showing brief success toast instead
- Still doing conflict scan upfront in dry-run mode (just `exists()` checks, ~100 ms for 10k files)

### Index conversion for ".." entry

When the directory has a parent entry shown at index 0, frontend indices are offset by +1 from backend:

- Frontend `[0, 1, 2, 3]` with `hasParent=true` → Backend `[-1, 0, 1, 2]` → filtered to `[0, 1, 2]`
- Index 0 with `hasParent=true` is always the ".." entry (backend index `-1`, invalid)
- `toBackendCursorIndex(0, true)` returns `null` to signal no-op

## Gotchas

- **Always use batch IPC for selection lookups.** `get_paths_at_indices` (paths only) and `get_files_at_indices` (full
  `FileEntry` objects) fetch all selected items in a single IPC call. Never loop over `getFileAt` per-index; with 50k
  selected files, per-file IPC takes 5-10 seconds. Batch calls take ~1 ms regardless of count.
- **MTP move is interleaved copy + delete per file.** Moves involving MTP volumes copy and then delete each file
  individually (not copy-all-then-delete-all). Minimizes duplicates on partial failure: if it fails mid-way, only the
  current file exists in both places. The progress UI shows three stages (Scanning → Copying → Removing source). If copy
  succeeds but delete fails, the user keeps files in both places (safer than losing data). Rollback is hidden during the
  delete phase since the copy is already done.
- **Dry-run conflict sampling.** If >200 conflicts, `DryRunResult.conflicts` contains a random sample. Check
  `conflictsSampled: true` and `conflictsTotal` for the exact count.
- **Progress dialog edge case.** Same-FS move completes so fast that the complete event may fire before the dialog
  mounts. Handle by checking operation status on mount and showing toast if already done.
- **Source pane refresh.** Move operations must refresh **both** panes post-completion (source files disappeared). Copy
  only refreshes destination.
- **Rollback / Cancel buttons disable during settle window.** `TransferProgressDialog` holds open for
  `MIN_DISPLAY_MS = 400 ms` after `write-complete` so the user can read the final state. During that window, both Cancel
  and Rollback buttons must be disabled (`disabled={isCancelling || operationSettled}`); a click here hits a backend
  whose operation state was already removed, so it's a no-op but briefly flashes "Rolling back..." giving false
  feedback. `operationSettled` is a `$state(false)` that flips when the operation reaches a terminal state.
- **Cancel close is two-condition: `write-cancelled` + `write-settled`.** When the user clicks Cancel (without
  rollback), `TransferProgressDialog` does NOT close immediately. It keeps the "Cancelling…" label up until both events
  have arrived for this `operationId`, then applies the existing `MIN_DISPLAY_MS` floor and closes via
  `onCancelled(filesProcessed)`. After 200 ms of waiting, the label gains a clarifying tail: "Cancelling… (finishing USB
  transfers)". The BE-side contract — settle fires after a fully-torn-down spawn task, even on panic — lives in the BE
  doc § "Settle contract". Race protection: if `write-settled` arrives before `write-cancelled` (shouldn't happen, but
  is defensive), the dialog buffers it and closes only after `write-cancelled` has been processed. Complete / error
  paths are unchanged: they still close on the existing `MIN_DISPLAY_MS` gate without waiting for settle. Why it
  matters: the original incident was an MTP delete cancel followed by an immediate second F8 — the device was still
  mid-teardown, the second op queued behind the 17 s tail, hit the 30 s op timeout, and wedged the USB session.
- **Scan preview reuse.** `TransferDialog` starts a scan preview on mount. If the user confirms before the scan
  finishes, the scan keeps running (`TransferDialog` sets `confirmed = true` and skips cancellation in `onDestroy`).
  `TransferProgressDialog` picks up listening to the same scan events via the `scanInProgress` prop.
  `waitForScanThenStart` subscribes to the scan events first, then awaits `checkScanPreviewStatus()`. Both the
  `scan-preview-complete` listener AND the status check can signal "ready to start", especially for fast scans that
  complete during the status-check `await`. Both paths converge on a local `kickOff()` helper guarded by a `started`
  flag, so `startOperation()` dispatches exactly once. The scan-error and scan-cancelled listeners also flip
  `started = true` as a terminal signal, so a late `scan-preview-complete` event can't dispatch an operation after we've
  errored or cancelled.
