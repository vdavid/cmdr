/**
 * Reactive execution state machine lifted out of `TransferProgressDialog.svelte`.
 *
 * Owns everything the progress dialog coordinates while an operation runs: the
 * write-event subscriptions (progress / complete / error / cancelled / settled /
 * conflict) plus the `operations-changed` lifecycle stream, the
 * `operationId`-scoped event buffering and replay, the phase machine
 * (scanning → active → flushing, plus rolling_back), the cancel/settle close-out
 * with its slow-label and last-resort fallback timers, the pause/resume and
 * background-to-queue flow (including auto-queue behind a busy lane), the
 * conflict prompt, and the scan-wait path that defers the write until a
 * `TransferDialog` preview finishes.
 *
 * The factory takes its static per-operation inputs and the dialog's outcome
 * callbacks as a plain config object (these never change for a given dialog
 * instance — a new operation mounts a fresh dialog), matching the codebase's
 * factory pattern (`createTransferScanState`, `createTransferConflictCheck`).
 * State is exposed through getters; the component aliases them via `$derived`
 * and reads them in its markup. The component drives the lifecycle: `start()`
 * from `onMount`, `destroy()` from `onDestroy`.
 *
 * ## Why `backgrounded` is a plain `let`, not `$state`
 *
 * `handleQueue` sets `backgrounded = true` and then synchronously unmounts the
 * modal (via `onQueue` → the parent flips its show flag). `destroy()` (the
 * component's `onDestroy`) reads `backgrounded` to decide whether to fire the
 * safety-net cancel — but a `$state` rune read during that synchronous
 * reactive-scope disposal returns a STALE `false`, so the guard would wrongly
 * pass and cancel the just-backgrounded op (the transfer dies, the queue window
 * opens empty). A plain variable reads its live value in `destroy()` regardless
 * of disposal. It's never read reactively, so it needs no reactivity. Don't
 * convert it to `$state`. The same live-read reasoning applies to `destroyed`.
 */

import {
  copyBetweenVolumes,
  moveBetweenVolumes,
  moveFiles,
  deleteFiles,
  trashFiles,
  onWriteProgress,
  onWriteComplete,
  onWriteError,
  onWriteCancelled,
  onWriteSettled,
  onWriteConflict,
  resolveWriteConflict,
  cancelWriteOperation,
  cancelScanPreview,
  checkScanPreviewStatus,
  onScanPreviewProgress,
  onScanPreviewComplete,
  onScanPreviewError,
  onScanPreviewCancelled,
  pauseOperation,
  resumeOperation,
  onOperationsChanged,
  listOperations,
  DEFAULT_VOLUME_ID,
  type WriteProgressEvent,
  type WriteCompleteEvent,
  type WriteErrorEvent,
  type WriteCancelledEvent,
  type WriteSettledEvent,
  type WriteConflictEvent,
  type OperationSnapshot,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import { addToast } from '$lib/ui/toast'
import type {
  TransferOperationType,
  WriteOperationPhase,
  WriteOperationError,
  SortColumn,
  SortOrder,
  ConflictResolution,
} from '$lib/file-explorer/types'
import { pluralize } from '$lib/utils/pluralize'
import { getSetting } from '$lib/settings'
import { getAppLogger } from '$lib/logging/logger'
import { ScanThroughput } from '../scan-throughput'
import { tString } from '$lib/intl/messages.svelte'
import { pathInsideArchive } from '$lib/file-explorer/pane/volume-capabilities'

export interface TransferProgressStateConfig {
  operationType: TransferOperationType
  sourcePaths: string[]
  /** Destination path (not applicable for delete/trash). */
  destinationPath?: string
  /** Current sort column on the source pane (files processed in this order). */
  sortColumn: SortColumn
  /** Current sort order on the source pane. */
  sortOrder: SortOrder
  /** Preview scan ID from `TransferDialog` (for reusing scan results), or null. */
  previewId: string | null
  /** Source volume ID (like "root", "mtp-336592896:65537"). */
  sourceVolumeId: string
  /** Destination volume ID (not applicable for delete/trash). */
  destVolumeId?: string
  /** Conflict resolution policy from `TransferDialog` (not applicable for delete/trash). */
  conflictResolution?: ConflictResolution
  /** Source filenames known to conflict at dest (forwarded so the BE bulk-skips them under `Skip all`). */
  preKnownConflicts?: string[]
  /** Per-item sizes for trash progress (from scan or drive index). */
  itemSizes?: number[]
  /** Whether the scan preview is still running (this dialog subscribes to scan events). */
  scanInProgress: boolean
  onComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
  onCancelled: (filesProcessed: number) => void
  onError: (error: WriteOperationError) => void
  /** Send this operation to the background: unmount the modal but keep the op running. */
  onQueue?: () => void
}

export function createTransferProgressState(config: TransferProgressStateConfig) {
  const log = getAppLogger('transferProgress')

  // English operation word for LOG lines only (not user-facing copy; user copy
  // resolves through the i18n catalog via `t()` in the component markup).
  const operationLabelMap: Record<TransferOperationType, string> = {
    copy: 'Copy',
    move: 'Move',
    delete: 'Delete',
    trash: 'Trash',
    archive_edit: 'Archive edit',
  }
  const operationLabel = operationLabelMap[config.operationType]

  // A move whose source OR destination is inside a zip must NOT take the local
  // `moveFiles` fast-path: an archive-inner path isn't a real folder, and the
  // backend fast-path rejects it. Route it through `moveBetweenVolumes`, which
  // resolves the archive boundary and runs the managed archive-edit flow (move
  // into = `{ add }`, move out = extract + `{ delete }`). Source and dest can
  // share the parent drive's `volumeId` (a zip lives on the same drive), so the
  // volume-id comparison alone misses this — the path check is what catches it.
  const touchesArchive =
    pathInsideArchive(config.destinationPath ?? '') || config.sourcePaths.some((p) => pathInsideArchive(p))

  /** Whether this move involves a non-local volume (MTP, an archive, etc.); backend handles all strategy. */
  const isVolumeMove =
    config.operationType === 'move' &&
    (config.sourceVolumeId !== DEFAULT_VOLUME_ID ||
      (config.destVolumeId ?? DEFAULT_VOLUME_ID) !== DEFAULT_VOLUME_ID ||
      touchesArchive)

  /** Minimum display time (ms) to prevent jarring one-frame flash. */
  const MIN_DISPLAY_MS = 400
  /** After this many ms of waiting for the backend to settle, the
   *  "Cancelling…" label gets a clarifying tail ("(finishing USB transfers)").
   *  Picked at 200 ms so a fast settle (the common case once cancel
   *  propagation lands on the backend) clears before the label ever changes. */
  const SLOW_SETTLE_LABEL_MS = 200
  /** Last-resort cap on how long we'll keep the dialog open after the user
   *  clicks Cancel. The settle gate is supposed to fire `write-cancelled`
   *  + `write-settled` quickly, but if the BE op state was already gone when
   *  we issued the cancel (e.g. it was cleaned up by `cancel_all_write_operations`
   *  during a hot-reload or by a previous teardown), no events ever fire and
   *  the dialog would otherwise stay at "Cancelling…" forever. Ten seconds is
   *  well above the legitimate settle window (USB tear-down is typically < 2 s
   *  even on bad devices) and well below "the user thinks the app is wedged."
   *  See the comment on `handleCancel` for the cases this catches. */
  const CANCEL_SETTLE_FALLBACK_MS = 10_000

  // Scan waiting state (when scan preview is still running from TransferDialog)
  let waitingForScan = $state(false)
  let scanFilesFound = $state(0)
  let scanDirsFound = $state(0)
  let scanBytesFound = $state(0)
  let scanCurrentDir = $state<string | null>(null)
  let scanUnlisteners: UnlistenFn[] = []
  const scanThroughput = new ScanThroughput()
  let scanFilesPerSec = $state<number | null>(null)
  let scanBytesPerSec = $state<number | null>(null)

  // Operation state
  let operationId = $state<string | null>(null)
  let phase = $state<WriteOperationPhase>('scanning')
  let currentFile = $state<string | null>(null)
  let filesDone = $state(0)
  let filesTotal = $state(0)
  let bytesDone = $state(0)
  let bytesTotal = $state(0)
  let startTime = $state(0)
  let isCancelling = $state(false)
  let isRollingBack = $state(false)
  let destroyed = false
  /** Set when `write-cancelled` arrives for this op. The dialog stays in
   *  "Cancelling…" until BOTH this AND `settleEventReceived` are true,
   *  giving the BE time to tear down in-flight USB / network ops. */
  let cancelEventReceived = $state(false)
  /** Set when `write-settled` arrives for this op. See § "Settle contract"
   *  in `src-tauri/src/file_system/write_operations/CLAUDE.md`. */
  let settleEventReceived = $state(false)
  /** Cached `WriteCancelledEvent` payload — held until settle arrives so we
   *  can pass `filesProcessed` to `onCancelled` at close time. */
  let cancelEventPayload: WriteCancelledEvent | null = null
  /** Flips true once the settle wait has exceeded `SLOW_SETTLE_LABEL_MS`.
   *  Drives the "(finishing USB transfers)" tail on the dialog label. */
  let settleSlow = $state(false)
  let slowSettleTimer: ReturnType<typeof setTimeout> | null = null
  /** Last-resort fallback that closes the dialog if neither `write-cancelled`
   *  nor `write-settled` arrives after the user clicks Cancel. See the
   *  doc comment on `CANCEL_SETTLE_FALLBACK_MS`. */
  let cancelSettleFallbackTimer: ReturnType<typeof setTimeout> | null = null
  /** Set when the operation reaches a terminal state (complete, error, cancel, rollback).
   *  Prevents destroy()'s safety-net cancel from interfering with an already-handled outcome.
   *  Reactive ($state) so the Cancel/Rollback buttons can disable themselves during the
   *  MIN_DISPLAY_MS hold-open window after write-complete; clicking them then would be a
   *  no-op since the backend state is already gone. */
  let operationSettled = $state(false)

  // Pause + background (Queue) state. The lifecycle status (running/paused/
  // queued/...) comes from the manager's `operations-changed` snapshot, NOT
  // from `write-progress` (a paused op still reports `is_running: true`; the
  // bar-is-moving truth is the snapshot status). See queue/CLAUDE.md.
  let opStatus = $state<OperationSnapshot['status'] | null>(null)
  let opsUnlisten: UnlistenFn | null = null
  /** True once this op is being managed in the queue window instead of the
   *  modal. Suppresses destroy()'s safety-net cancel. MUST be a plain `let`,
   *  NOT `$state` — see the module-level doc comment for why. */
  let backgrounded = false
  const isPaused = () => opStatus === 'paused'
  let pauseInFlight = $state(false)

  // Events that arrived before we know our operationId (from the command response).
  // Without buffering, a stale event from a previous operation could claim the ID slot first.
  type BufferedEvent =
    | { type: 'progress'; event: WriteProgressEvent }
    | { type: 'complete'; event: WriteCompleteEvent }
    | { type: 'error'; event: WriteErrorEvent }
    | { type: 'cancelled'; event: WriteCancelledEvent }
    | { type: 'settled'; event: WriteSettledEvent }
    | { type: 'conflict'; event: WriteConflictEvent }
  let pendingEvents: BufferedEvent[] = []

  /** Returns true if the event belongs to this operation and should be processed. */
  function filterEvent(entry: BufferedEvent): boolean {
    if (operationId === null) {
      pendingEvents.push(entry)
      return false
    }
    return entry.event.operationId === operationId
  }

  function replayBufferedEvents() {
    const events = pendingEvents
    pendingEvents = []
    for (const entry of events) {
      if (entry.event.operationId !== operationId) continue
      switch (entry.type) {
        case 'progress':
          handleProgress(entry.event)
          break
        case 'complete':
          handleComplete(entry.event)
          break
        case 'error':
          handleError(entry.event)
          break
        case 'cancelled':
          handleCancelled(entry.event)
          break
        case 'settled':
          handleSettled(entry.event)
          break
        case 'conflict':
          handleConflict(entry.event)
          break
      }
    }
  }

  // Conflict state
  let conflictEvent = $state<WriteConflictEvent | null>(null)
  let isResolvingConflict = $state(false)

  /** A paused op is still mid-transfer (not scanning, not cancelling, not
   *  settled, no conflict prompt up), so the Pause/Resume + Queue controls show
   *  during the active copy/move/delete phases only. */
  const canPauseOrQueue = () =>
    !waitingForScan &&
    !isCancelling &&
    !cancelEventReceived &&
    !isRollingBack &&
    !operationSettled &&
    !conflictEvent &&
    operationId !== null

  // Rates + ETA come from the backend (`EtaEstimator` in
  // `write_operations/eta.rs`). The FE just renders them. Null until the
  // backend's warm-up window is over (≈800 ms after a phase change).
  let bytesPerSecond = $state<number | null>(null)
  let filesPerSecond = $state<number | null>(null)
  /** Raw ETA from the backend (`max(ETA_bytes, ETA_files)`). */
  let etaSecondsRaw = $state<number | null>(null)
  /** Display-smoothed ETA: slow EWMA over the raw value to kill flicker on
   *  the "Ns remaining" readout. The estimator itself stays responsive. */
  let etaSecondsDisplay = $state<number | null>(null)

  /** Smooth the displayed ETA toward the latest backend value. Display-only;
   *  the underlying estimator is unsmoothed and reacts to real changes. */
  function updateDisplayEta(raw: number | null) {
    if (raw === null) {
      etaSecondsDisplay = null
      return
    }
    if (etaSecondsDisplay === null) {
      etaSecondsDisplay = raw
      return
    }
    // Cap the change per tick at 25% of the gap. Real changes still
    // propagate quickly (4 ticks ≈ 80 ms × 4 = under a second), while
    // single-tick jitter is dampened.
    etaSecondsDisplay = etaSecondsDisplay + 0.25 * (raw - etaSecondsDisplay)
  }

  function handleProgress(event: WriteProgressEvent) {
    if (!filterEvent({ type: 'progress', event })) return

    log.debug('Progress event: {phase} {filesDone}/{filesTotal} {filesNoun}, {bytesDone}/{bytesTotal} {bytesNoun}', {
      phase: event.phase,
      filesDone: event.filesDone,
      filesTotal: event.filesTotal,
      filesNoun: pluralize(event.filesTotal, 'file'),
      bytesDone: event.bytesDone,
      bytesTotal: event.bytesTotal,
      bytesNoun: pluralize(event.bytesTotal, 'byte'),
    })

    // Drop the smoothed ETA on phase transitions; the backend estimator
    // resets, so the FE display number should re-warm with it.
    if (event.phase !== phase) {
      etaSecondsDisplay = null
      // Drop the scan throughput history when leaving the scanning phase
      // so a stale sample can't leak into the active phase readout.
      if (phase === 'scanning' && event.phase !== 'scanning') {
        scanThroughput.reset()
        scanFilesPerSec = null
        scanBytesPerSec = null
      }
    }

    // When entering rolling_back phase, set isRollingBack from the backend event
    if (event.phase === 'rolling_back' && !isRollingBack) {
      isRollingBack = true
    }

    phase = event.phase
    currentFile = event.currentFile
    filesDone = event.filesDone
    filesTotal = event.filesTotal
    bytesDone = event.bytesDone
    bytesTotal = event.bytesTotal
    bytesPerSecond = event.bytesPerSecond ?? null
    filesPerSecond = event.filesPerSecond ?? null
    etaSecondsRaw = event.etaSeconds ?? null
    updateDisplayEta(etaSecondsRaw)

    // Scanning-phase metadata (current dir, dirs tally, index-derived
    // expected totals, FE-computed throughput). Mirrors the waitingForScan
    // path so the same scan-phase UI surfaces during the backend's
    // foolproof re-scan.
    if (event.phase === 'scanning') {
      scanFilesFound = event.filesDone
      scanDirsFound = event.dirsDone ?? 0
      scanBytesFound = event.bytesDone
      scanCurrentDir = event.currentDir ?? null
      const r = scanThroughput.push({
        timestampMs: Date.now(),
        files: event.filesDone,
        bytes: event.bytesDone,
      })
      scanFilesPerSec = r.filesPerSecond
      scanBytesPerSec = r.bytesPerSecond
    }
  }

  function handleComplete(event: WriteCompleteEvent) {
    if (!filterEvent({ type: 'complete', event })) return

    log.info('{op} complete: {filesProcessed} {filesNoun}, {bytesProcessed} {bytesNoun}', {
      op: operationLabel,
      filesProcessed: event.filesProcessed,
      filesNoun: pluralize(event.filesProcessed, 'file'),
      bytesProcessed: event.bytesProcessed,
      bytesNoun: pluralize(event.bytesProcessed, 'byte'),
    })

    operationSettled = true
    cleanup()

    const totalFiles = event.filesProcessed
    const totalSkipped = event.filesSkipped
    const totalBytes = event.bytesProcessed

    // Enforce minimum display time to prevent jarring one-frame flash
    const elapsed = Date.now() - startTime
    const delay = Math.max(0, MIN_DISPLAY_MS - elapsed)
    if (delay > 0) {
      setTimeout(() => {
        config.onComplete(totalFiles, totalSkipped, totalBytes)
      }, delay)
    } else {
      config.onComplete(totalFiles, totalSkipped, totalBytes)
    }
  }

  function handleError(event: WriteErrorEvent) {
    if (!filterEvent({ type: 'error', event })) return

    if (event.error.type === 'archive_needs_password') {
      // Expected, recoverable flow: the write-error only exists to prompt for a
      // password and retry (intercepted upstream in `handleTransferError`), so
      // log at warn to keep it out of prod error-report bundles (error+ only).
      log.warn('{op} needs a password: {errorType}', { op: operationLabel, errorType: event.error.type, error: event.error })
    } else {
      log.error('{op} error: {errorType}', { op: operationLabel, errorType: event.error.type, error: event.error })
    }

    operationSettled = true
    cleanup()
    config.onError(event.error)
  }

  function handleCancelled(event: WriteCancelledEvent) {
    if (!filterEvent({ type: 'cancelled', event })) return

    log.info('{op} cancelled after {filesProcessed} {filesNoun}, rolledBack={rolledBack}', {
      op: operationLabel,
      filesProcessed: event.filesProcessed,
      filesNoun: pluralize(event.filesProcessed, 'file'),
      rolledBack: event.rolledBack,
    })

    cancelEventReceived = true
    cancelEventPayload = event
    // Mark the operation as settled at the dialog state-machine level so
    // Cancel/Rollback buttons disable (the BE state is already gone or
    // about to be). The "Cancelling…" indicator stays on the FE side
    // until `write-settled` lands.
    operationSettled = true

    // Rollback path: the backend already finished tearing down by the
    // time it emits a non-rolled-back cancel (the user clicked Cancel
    // during Rollback) or a rolled-back cancel (rollback completed
    // normally). In both cases, settling has effectively happened. We
    // still wait for `write-settled` for consistency, but start the slow
    // label timer regardless.
    startSlowSettleTimer()

    maybeFinishCancelClose()
  }

  /** Called once the BE has signalled `write-settled` for the operation
   *  the dialog is bound to. Combines with `write-cancelled` (already
   *  received) to drive the close-out. Defensive about ordering: if
   *  settle somehow arrives before cancelled, we wait. */
  function handleSettled(event: WriteSettledEvent) {
    if (!filterEvent({ type: 'settled', event })) return

    log.debug('Settle event arrived for op={operationId}', { operationId: event.operationId })

    settleEventReceived = true
    clearSlowSettleTimer()
    clearCancelSettleFallbackTimer()
    maybeFinishCancelClose()
  }

  function startSlowSettleTimer() {
    if (slowSettleTimer !== null) return
    slowSettleTimer = setTimeout(() => {
      settleSlow = true
      slowSettleTimer = null
    }, SLOW_SETTLE_LABEL_MS)
  }

  function clearSlowSettleTimer() {
    if (slowSettleTimer !== null) {
      clearTimeout(slowSettleTimer)
      slowSettleTimer = null
    }
    settleSlow = false
  }

  function startCancelSettleFallbackTimer() {
    if (cancelSettleFallbackTimer !== null) return
    cancelSettleFallbackTimer = setTimeout(() => {
      cancelSettleFallbackTimer = null
      // If we still haven't received both terminal events, the BE op state
      // is gone (or never existed) — synthesise a clean close so the dialog
      // doesn't stay at "Cancelling…" forever. Hand the FE a Cancelled
      // outcome with `filesProcessed: 0` so the caller's cleanup runs.
      if (!cancelEventReceived || !settleEventReceived) {
        log.warn(
          'Cancel settle fallback fired for op={operationId} after {ms}ms (cancelled={c}, settled={s}); closing dialog',
          {
            operationId,
            ms: CANCEL_SETTLE_FALLBACK_MS,
            c: cancelEventReceived,
            s: settleEventReceived,
          },
        )
        cleanup()
        config.onCancelled(0)
      }
    }, CANCEL_SETTLE_FALLBACK_MS)
  }

  function clearCancelSettleFallbackTimer() {
    if (cancelSettleFallbackTimer !== null) {
      clearTimeout(cancelSettleFallbackTimer)
      cancelSettleFallbackTimer = null
    }
  }

  /** Closes the dialog if both `write-cancelled` and `write-settled` have
   *  arrived. Applies the existing `MIN_DISPLAY_MS` floor so a sub-frame
   *  cancel doesn't flash. Idempotent: safe to call from both handlers. */
  function maybeFinishCancelClose() {
    if (!cancelEventReceived || !settleEventReceived) return
    if (cancelEventPayload === null) return

    const payload = cancelEventPayload
    cancelEventPayload = null // single-shot
    cleanup()

    const elapsed = Date.now() - startTime
    const delay = Math.max(0, MIN_DISPLAY_MS - elapsed)
    if (delay > 0) {
      setTimeout(() => {
        config.onCancelled(payload.filesProcessed)
      }, delay)
    } else {
      config.onCancelled(payload.filesProcessed)
    }
  }

  function handleConflict(event: WriteConflictEvent) {
    if (!filterEvent({ type: 'conflict', event })) return

    log.info('Conflict detected: {sourcePath} -> {destinationPath}', {
      sourcePath: event.sourcePath,
      destinationPath: event.destinationPath,
    })

    conflictEvent = event
  }

  async function handleConflictResolution(resolution: ConflictResolution, applyToAll: boolean) {
    if (!operationId || !conflictEvent) return

    log.info('Resolving conflict with {resolution}, applyToAll={applyToAll}', { resolution, applyToAll })

    isResolvingConflict = true
    try {
      await resolveWriteConflict(operationId, resolution, applyToAll)
      conflictEvent = null
    } catch (err) {
      log.error('Failed to resolve conflict: {error}', { error: err })
    } finally {
      isResolvingConflict = false
    }
  }

  // Store multiple unlisteners
  let unlisteners: UnlistenFn[] = []

  function cleanup() {
    log.debug('Cleaning up {count} {listenersNoun}', {
      count: unlisteners.length,
      listenersNoun: pluralize(unlisteners.length, 'event listener'),
    })
    for (const unlisten of unlisteners) {
      unlisten()
    }
    unlisteners = []
    opsUnlisten?.()
    opsUnlisten = null
    clearSlowSettleTimer()
    clearCancelSettleFallbackTimer()
  }

  /** Dispatches the backend command based on operation type. */
  async function dispatchOperation(): Promise<{ operationId: string }> {
    const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
    const maxConflictsToShow = getSetting('fileOperations.maxConflictsToShow')

    if (config.operationType === 'trash') {
      return trashFiles(config.sourcePaths, config.itemSizes, { progressIntervalMs, previewId: config.previewId })
    }
    if (config.operationType === 'delete') {
      return deleteFiles(
        config.sourcePaths,
        { progressIntervalMs, sortColumn: config.sortColumn, sortOrder: config.sortOrder, previewId: config.previewId },
        config.sourceVolumeId,
      )
    }
    if (config.operationType === 'move') {
      // Volume move (MTP or other non-local); backend handles same-volume, cross-volume, etc.
      if (isVolumeMove) {
        return moveBetweenVolumes(
          config.sourceVolumeId,
          config.sourcePaths,
          config.destVolumeId ?? DEFAULT_VOLUME_ID,
          config.destinationPath ?? '',
          {
            conflictResolution: config.conflictResolution ?? 'stop',
            progressIntervalMs,
            maxConflictsToShow,
            previewId: config.previewId,
            preKnownConflicts: config.preKnownConflicts ?? [],
          },
        )
      }
      // Local-to-local move
      return moveFiles(config.sourcePaths, config.destinationPath ?? '', {
        conflictResolution: config.conflictResolution,
        progressIntervalMs,
        maxConflictsToShow,
        sortColumn: config.sortColumn,
        sortOrder: config.sortOrder,
        previewId: config.previewId,
        preKnownConflicts: config.preKnownConflicts ?? [],
      })
    }
    // Copy: always use copyBetweenVolumes; the backend handles local-to-local optimization
    return copyBetweenVolumes(
      config.sourceVolumeId,
      config.sourcePaths,
      config.destVolumeId ?? DEFAULT_VOLUME_ID,
      config.destinationPath ?? '',
      {
        conflictResolution: config.conflictResolution ?? 'stop',
        progressIntervalMs,
        maxConflictsToShow,
        previewId: config.previewId,
        preKnownConflicts: config.preKnownConflicts ?? [],
      },
    )
  }

  async function startOperation() {
    log.info('Starting {op} operation: {sourceCount} {sourcesNoun}', {
      op: config.operationType,
      sourceCount: config.sourcePaths.length,
      sourcesNoun: pluralize(config.sourcePaths.length, 'source'),
    })

    startTime = Date.now()

    // CRITICAL: Subscribe to events BEFORE starting the operation to avoid race condition
    // Events are emitted immediately when the operation starts - if we subscribe after,
    // we might miss progress/complete events for fast operations (like single large files)
    log.debug('Subscribing to write events BEFORE starting operation')

    unlisteners.push(await onWriteProgress(handleProgress))
    unlisteners.push(await onWriteComplete(handleComplete))
    unlisteners.push(await onWriteError(handleError))
    unlisteners.push(await onWriteCancelled(handleCancelled))
    unlisteners.push(await onWriteSettled(handleSettled))
    unlisteners.push(await onWriteConflict(handleConflict))
    // Track this op's lifecycle status (running vs paused, and the queued
    // case the auto-queue path needs). Held separately so cleanup() can drop
    // it without churning the write listeners.
    opsUnlisten = await onOperationsChanged((event) => {
      handleOperationsChanged(event.operations)
    })

    log.debug('Event subscriptions ready, starting {op}', { op: config.operationType })

    try {
      const result = await dispatchOperation()

      operationId = result.operationId
      log.info('{op} operation started with operationId: {operationId}', {
        op: operationLabel,
        operationId,
      })

      // If the dialog was destroyed/cancelled while waiting for the IPC response,
      // cancel the operation immediately and bail out. Crucially, we must call
      // `onCancelled` so the parent removes the dialog from state — otherwise
      // the listeners are torn down by `cleanup()` but the `<TransferProgressDialog>`
      // stays mounted forever (the BE eventually emits `write-cancelled` +
      // `write-settled`, but no one's listening anymore). That stuck dialog
      // poisons every following operation through ensureAppReady's Escape.
      if (destroyed) {
        log.info('Dialog destroyed before operationId arrived; cancelling op={operationId}', {
          operationId,
        })
        void cancelWriteOperation(operationId, true)
        cleanup()
        config.onCancelled(0)
        return
      }

      replayBufferedEvents()

      // Seed this op's lifecycle status once. The manager emits
      // `operations-changed` on registration, which may have fired before we
      // knew our `operationId` (so our subscriber dropped it). A one-shot
      // `list_operations` catches the current status — crucially the
      // "admitted as Queued behind a busy lane" case the auto-queue path
      // surfaces. After this, live snapshot ticks keep `opStatus` current.
      try {
        const snapshot = await listOperations()
        // `handleOperationsChanged` is idempotent and self-guards on
        // `backgrounded`, so a live tick that already backgrounded us
        // between subscribe and seed is a no-op here. The `destroyed`
        // re-check matters because the component can unmount during this
        // `await` (eslint can't see that async-gap mutation, hence the
        // disable).
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- destroyed can flip during the await above
        if (!destroyed) handleOperationsChanged(snapshot)
      } catch (err) {
        log.warn('Failed to seed operation status: {error}', { error: err })
      }
    } catch (err: unknown) {
      log.error('Failed to start {op} operation: {error}', { op: config.operationType, error: err })
      cleanup()
      // Tauri commands return structured WriteOperationError objects on validation failure
      // (e.g. destination_inside_source). Pass them through to preserve the specific error type.
      if (typeof err === 'object' && err !== null && 'type' in err) {
        config.onError(err as WriteOperationError)
      } else {
        config.onError({
          type: 'io_error',
          path: config.sourcePaths[0] ?? '',
          message: `Failed to start ${config.operationType}: ${String(err)}`,
        })
      }
    }
  }

  async function handleCancel(rollback: boolean) {
    // A backgrounded op was deliberately handed off to the queue window, so NO
    // teardown path may cancel it. The modal's `onclose` (× button, Escape, or
    // focus-trap teardown) fires during the backgrounding handoff and routes
    // here; without this guard it would cancel the op (keeping only partial
    // files) and the queue window would open empty. destroy() makes the same
    // exception; the explicit Cancel/Rollback buttons always run with
    // `backgrounded` false (the modal is gone by the time it's set).
    if (backgrounded) return

    // If still waiting for scan preview, cancel the scan and close
    if (waitingForScan && config.previewId) {
      log.info('Cancelling scan preview during wait: previewId={previewId}', { previewId: config.previewId })
      void cancelScanPreview(config.previewId)
      waitingForScan = false
      cleanupScanListeners()
      config.onCancelled(0)
      return
    }

    if (!operationId) {
      log.warn('Cancel requested but no operationId yet; will cancel after IPC resolves')
      destroyed = true
      return
    }
    if (isCancelling) {
      log.debug('Cancel already in progress')
      return
    }

    if (isRollingBack && !rollback) {
      // Cancel during rollback: stop deleting, keep remaining files
      log.info('Cancelling rollback for operation: {operationId}', { operationId })
      isCancelling = true
      try {
        await cancelWriteOperation(operationId, false)
        log.debug('Rollback cancel request sent successfully')
        // Dialog will close when write-cancelled event is received
      } catch (err) {
        log.error('Failed to cancel rollback: {error}', { error: err })
        isCancelling = false
      }
      return
    }

    if (isRollingBack) {
      log.debug('Rollback already in progress')
      return
    }

    if (rollback) {
      // Rollback: keep dialog open, backend will enter rolling_back phase with progress events
      log.info('Rolling back operation: {operationId}', { operationId })
      operationSettled = true
      isRollingBack = true
      try {
        await cancelWriteOperation(operationId, true)
        log.debug('Rollback request sent successfully')
        // Dialog stays open; progress events with phase=rolling_back will update the UI.
        // Dialog closes when write-cancelled event is received.
      } catch (err) {
        log.error('Failed to rollback operation: {error}', { error: err })
        isRollingBack = false
      }
    } else {
      // Cancel: keep partial files. Stay in "Cancelling…" until both
      // `write-cancelled` and `write-settled` have landed: the BE may
      // still be tearing down USB / network sessions, and dispatching a
      // new op against a volume in that state is what wedged the device
      // in the original incident. See the "Settle contract" in
      // `src-tauri/src/file_system/write_operations/CLAUDE.md`.
      log.info('Cancelling operation (keeping partial files): {operationId}', { operationId })
      isCancelling = true
      startSlowSettleTimer()
      startCancelSettleFallbackTimer()
      try {
        await cancelWriteOperation(operationId, false)
        log.debug('Cancel request sent; waiting for write-cancelled + write-settled')
        // Don't close here. The dialog closes from
        // `maybeFinishCancelClose` once both events arrive.
      } catch (err) {
        log.error('Failed to cancel operation: {error}', { error: err })
        isCancelling = false
        clearSlowSettleTimer()
        clearCancelSettleFallbackTimer()
      }
    }
  }

  /** Pauses or resumes this operation in place. The button label/icon and the
   *  dialog title follow `opStatus`, which the `operations-changed` snapshot
   *  drives — so the UI flips only once the backend actually parked/resumed,
   *  never optimistically. */
  async function handlePauseResume() {
    if (!operationId || pauseInFlight) return
    pauseInFlight = true
    try {
      if (isPaused()) {
        log.info('Resuming operation: {operationId}', { operationId })
        await resumeOperation(operationId)
      } else {
        log.info('Pausing operation: {operationId}', { operationId })
        await pauseOperation(operationId)
      }
    } catch (err) {
      log.error('Failed to pause/resume operation: {error}', { error: err })
    } finally {
      pauseInFlight = false
    }
  }

  /** Sends this operation to the background: keep it running, open the queue
   *  window, and unmount this modal. The op is now managed in the queue window.
   *  Fired by the Queue button, the dialog-scoped F2, and the auto-queue path.
   *  Sets `backgrounded` BEFORE handing off so destroy()'s safety-net cancel
   *  won't stop the op as the modal tears down. */
  function handleQueue() {
    if (!operationId || backgrounded) return
    log.info('Backgrounding operation to the queue window: {operationId}', { operationId })
    backgrounded = true
    void openQueueWindow()
    addToast(tString('fileOperations.transferProgress.backgroundedToast'), {
      level: 'info',
      toastGroup: 'transfer-queue',
    })
    // Unmount this modal. The op keeps running; `onQueue` clears the dialog
    // state in the parent (it does NOT call `cancelWriteOperation`).
    config.onQueue?.()
  }

  /** Called once `operations-changed` first reports this op as `queued`: the
   *  manager admitted it behind a busy lane rather than running it now. Don't
   *  stack a second modal on top of the foreground op — surface the queue
   *  window with a quiet toast and unmount, exactly like a manual Queue. */
  function handleAutoQueued(aheadCount: number) {
    if (backgrounded || !operationId) return
    log.info('Operation queued behind {ahead} on a busy lane; surfacing the queue window', {
      ahead: aheadCount,
    })
    backgrounded = true
    void openQueueWindow()
    const countText = tString('fileOperations.transferProgress.queuedToastCount', { count: aheadCount })
    addToast(tString('fileOperations.transferProgress.queuedToast', { countText }), {
      level: 'info',
      toastGroup: 'transfer-queue',
    })
    config.onQueue?.()
  }

  /** Reduces an `operations-changed` snapshot for THIS op: tracks its lifecycle
   *  status and, the first time it lands as `queued`, auto-backgrounds it. */
  function handleOperationsChanged(operations: OperationSnapshot[]) {
    if (!operationId) return
    const mine = operations.find((op) => op.operationId === operationId)
    if (!mine) return
    opStatus = mine.status
    if (mine.status === 'queued' && !backgrounded) {
      // Count the ops ahead of this one that are occupying lanes (running or
      // paused). That's how many transfers it's waiting behind.
      const ahead = operations.filter(
        (op) => op.operationId !== operationId && (op.status === 'running' || op.status === 'paused'),
      ).length
      handleAutoQueued(Math.max(1, ahead))
    }
  }

  /** Cleans up scan preview event listeners. */
  function cleanupScanListeners() {
    for (const unlisten of scanUnlisteners) {
      unlisten()
    }
    scanUnlisteners = []
  }

  /** Returns true if the event belongs to our scan preview. */
  function isOurScanEvent(eventPreviewId: string): boolean {
    return eventPreviewId === config.previewId
  }

  /**
   * Waits for the scan preview to complete, then starts the write operation.
   *
   * Two independent signals can say "scan done": the `scan-preview-complete`
   * event firing, or the post-subscription `checkScanPreviewStatus` IPC
   * returning true. Either can win the race. Both converge on `kickOff()`,
   * which is idempotent via the `started` flag; the operation dispatches
   * exactly once, even if both signals arrive during the `await`.
   *
   * We subscribe to events BEFORE the status check so a fast completion
   * between subscription and check isn't missed.
   *
   * Precondition: previewId must be non-null (guaranteed by TransferDialog,
   * which awaits startScanPreview IPC before calling onConfirm).
   */
  async function waitForScanThenStart() {
    if (!config.previewId) {
      log.error('waitForScanThenStart called with null previewId; TransferDialog invariant violated')
      void startOperation()
      return
    }

    let started = false
    const kickOff = () => {
      if (started) return
      started = true
      cleanupScanListeners()
      void startOperation()
    }

    // Subscribe to events FIRST to avoid missing fast completions.
    // Same pattern as TransferDialog.startScan().
    scanUnlisteners.push(
      await onScanPreviewProgress((event) => {
        if (!isOurScanEvent(event.previewId)) return
        scanFilesFound = event.filesFound
        scanDirsFound = event.dirsFound
        scanBytesFound = event.bytesFound
        scanCurrentDir = event.currentDir ?? null
        const r = scanThroughput.push({
          timestampMs: Date.now(),
          files: event.filesFound,
          bytes: event.bytesFound,
        })
        scanFilesPerSec = r.filesPerSecond
        scanBytesPerSec = r.bytesPerSecond
      }),
    )

    scanUnlisteners.push(
      await onScanPreviewComplete((event) => {
        if (!isOurScanEvent(event.previewId)) return
        log.info('Scan preview complete: {filesTotal} {filesNoun}, {bytesTotal} {bytesNoun}', {
          filesTotal: event.filesTotal,
          filesNoun: pluralize(event.filesTotal, 'file'),
          bytesTotal: event.bytesTotal,
          bytesNoun: pluralize(event.bytesTotal, 'byte'),
        })
        scanFilesFound = event.filesTotal
        scanDirsFound = event.dirsTotal
        scanBytesFound = event.bytesTotal
        waitingForScan = false
        kickOff()
      }),
    )

    scanUnlisteners.push(
      await onScanPreviewError((event) => {
        if (!isOurScanEvent(event.previewId)) return
        if (started) return // already dispatched or terminated; ignore late errors
        started = true // terminal; don't let a late scan-complete dispatch an operation
        log.error('Scan preview error: {message}', { message: event.message })
        waitingForScan = false
        cleanupScanListeners()
        config.onError({
          type: 'io_error',
          path: config.sourcePaths[0] ?? '',
          message: `Scan failed: ${event.message}`,
        })
      }),
    )

    scanUnlisteners.push(
      await onScanPreviewCancelled((event) => {
        if (!isOurScanEvent(event.previewId)) return
        if (started) return // already dispatched or terminated; ignore late cancellations
        started = true // terminal; don't let a late scan-complete dispatch an operation
        log.info('Scan preview cancelled')
        waitingForScan = false
        cleanupScanListeners()
        config.onCancelled(0)
      }),
    )

    // NOW check if already complete (covers race where scan finished during subscription setup)
    const alreadyComplete = await checkScanPreviewStatus(config.previewId)
    if (alreadyComplete) {
      log.info('Scan preview already complete for previewId={previewId}, starting operation immediately', {
        previewId: config.previewId,
      })
      kickOff()
      return
    }

    log.info('Scan preview still running for previewId={previewId}, subscribing to events', {
      previewId: config.previewId,
    })
    waitingForScan = true
  }

  /** Starts the dialog's work: defers to the scan-wait path when a preview is
   *  still running, otherwise dispatches the operation immediately. Called from
   *  the component's `onMount`. */
  function start() {
    if (config.scanInProgress) {
      void waitForScanThenStart()
    } else {
      void startOperation()
    }
  }

  /** Tears the dialog down (the component's `onDestroy`). Cancels an in-flight
   *  scan preview, drops scan listeners, and fires the safety-net cancel for an
   *  unexpected teardown — UNLESS the op was deliberately backgrounded (read
   *  live off the plain `backgrounded` / `destroyed` lets; see the module doc). */
  function destroy() {
    destroyed = true
    // Cancel scan preview if still waiting for it
    if (waitingForScan && config.previewId) {
      void cancelScanPreview(config.previewId)
    }
    cleanupScanListeners()
    if (operationId && !operationSettled && !backgrounded) {
      // Unexpected teardown (hot-reload, navigation, window close): stop the operation
      // but don't roll back; never do silent background work without visual feedback.
      // A `backgrounded` op is the deliberate exception — the user sent it to the queue
      // window, so it MUST keep running; only its modal unmounts.
      void cancelWriteOperation(operationId, false)
    }
    cleanup()
  }

  return {
    start,
    destroy,
    handleCancel,
    handleConflictResolution,
    handlePauseResume,
    handleQueue,
    get waitingForScan() {
      return waitingForScan
    },
    get scanFilesFound() {
      return scanFilesFound
    },
    get scanDirsFound() {
      return scanDirsFound
    },
    get scanBytesFound() {
      return scanBytesFound
    },
    get scanCurrentDir() {
      return scanCurrentDir
    },
    get scanFilesPerSec() {
      return scanFilesPerSec
    },
    get scanBytesPerSec() {
      return scanBytesPerSec
    },
    get phase() {
      return phase
    },
    get currentFile() {
      return currentFile
    },
    get filesDone() {
      return filesDone
    },
    get filesTotal() {
      return filesTotal
    },
    get bytesDone() {
      return bytesDone
    },
    get bytesTotal() {
      return bytesTotal
    },
    get isCancelling() {
      return isCancelling
    },
    get isRollingBack() {
      return isRollingBack
    },
    get cancelEventReceived() {
      return cancelEventReceived
    },
    get settleSlow() {
      return settleSlow
    },
    get operationSettled() {
      return operationSettled
    },
    get isPaused() {
      return isPaused()
    },
    get pauseInFlight() {
      return pauseInFlight
    },
    get canPauseOrQueue() {
      return canPauseOrQueue()
    },
    get conflictEvent() {
      return conflictEvent
    },
    get isResolvingConflict() {
      return isResolvingConflict
    },
    get bytesPerSecond() {
      return bytesPerSecond
    },
    get filesPerSecond() {
      return filesPerSecond
    },
    get etaSecondsDisplay() {
      return etaSecondsDisplay
    },
  }
}
