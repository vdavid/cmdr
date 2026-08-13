/**
 * The progress dialog, as a view of one operation.
 *
 * Two things live here, and keeping them apart is the point:
 *
 * **Birth** (`beginOperation`) runs once. It claims the foreground slot,
 * dispatches through `transfer-dispatch.ts`, answers the MCP round-trip, and
 * names the operation. It ends the moment an `operationId` exists.
 *
 * A view can skip birth entirely: `adoptOperationId` names an operation that is
 * already running, and `start()` binds it instead of dispatching. That is the
 * queue window's Foreground button, and it is the reason the two halves are
 * separable at all. An adopted view is the same view; the difference lives with
 * the parent, which has no birth context to run a pane tail against.
 *
 * **The view** is everything after that, and it owns only what belongs to a
 * piece of UI rather than to the operation: the anti-flicker floor, the
 * dismissal, the "(finishing USB transfers)" label, the last-resort close
 * timer, the Queue handoff, and the four outcome callbacks that tell the pane
 * what to do next. What the operation IS — phase, counts, rates, smoothed ETA,
 * clash, outcome — and what can be DONE to it — pause, resume, cancel, roll
 * back, answer the clash — come from its session
 * (`../operation-session/CLAUDE.md`), shared with every other surface watching
 * the same transfer.
 *
 * Nothing subscribes to an event stream here. The window's fan-out subscribes
 * once at init and holds what arrives for an id nobody has claimed yet, which is
 * what makes the gap between "the backend named the operation" and "the binder
 * acquired its session" harmless.
 *
 * ## A close is a detach, never a cancel
 *
 * Unmounting stops nothing. The operation lives in the backend registry; the
 * corner chip and the queue window keep showing it; only the Cancel button asks
 * for a cancel. The one place a teardown still means "stop" is an explicit
 * Cancel pressed before the backend had named the operation, which
 * `cancelRequestedBeforeId` records and birth acts on.
 *
 * ## Why `backgrounded` and `destroyed` are plain `let`s, not `$state`
 *
 * `handleQueue` sets `backgrounded = true` and then synchronously unmounts the
 * modal (via `onQueue` → the parent flips its show flag), so `destroy()` (the
 * component's `onDestroy`) runs inside that same reactive-scope disposal. A
 * `$state` rune read during synchronous disposal returns a STALE value: that is
 * how a just-backgrounded transfer once got cancelled and the queue window
 * opened empty. Any flag a teardown path reads has the same hazard, so these
 * stay plain variables that read live. They're never read reactively, so they
 * need no reactivity. Don't convert them to `$state`.
 */

import { cancelOperation, type TransferActivity, type WriteCancelledEvent } from '$lib/tauri-commands'
import { emit } from '@tauri-apps/api/event'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import {
  beginForegroundClaim,
  clearForegroundOperation,
  endForegroundClaim,
  setForegroundOperationId,
} from '../foreground-operation.svelte'
import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
import { addToast } from '$lib/ui/toast'
import type {
  TransferOperationType,
  ConflictResolution,
  WriteOperationPhase,
  WriteOperationError,
} from '$lib/file-explorer/types'
import { pluralize } from '$lib/utils/pluralize'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import type { BytesPerSecond, Seconds } from '$lib/units'
import { bindOperationSession } from '../operation-session/bind-operation-session.svelte'
import type { OperationOutcome, ScanReadout } from '../operation-session/operation-session.svelte'
import { dispatchTransferOperation, type TransferDispatchConfig } from './transfer-dispatch'

export interface TransferProgressStateConfig extends TransferDispatchConfig {
  /** An operation already running that this view ADOPTS instead of starting one
   *  (Foreground from the queue window). When set, `start()` binds the session
   *  for that id and dispatches nothing, so every `TransferDispatchConfig` field
   *  above is inert: nobody reads them on this path.
   *
   *  The view is otherwise identical. What differs sits with the PARENT, which
   *  has no birth context for an adopted operation and must not run a pane tail
   *  against it (`../../file-explorer/pane/dialog-state.svelte.ts`). */
  adoptOperationId?: string
  onComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
  onCancelled: (filesProcessed: number) => void
  onError: (error: WriteOperationError) => void
  /** Send this operation to the background: unmount the modal but keep the op running. */
  onQueue?: () => void
  /** The MCP round-trip request id, present only for an auto-confirmed op started
   *  via the MCP `copy`/`move`/`delete`/`compress` tool. When set, this state
   *  replies `mcp-response` with the spawned `operationId` (or an error) so the
   *  waiting tool can return the id — see `mcp/executor/file_ops.rs`. */
  mcpRequestId?: string
}

/** What a view shows before its operation has said anything. A confirmed
 *  transfer starts by counting, so the dialog opens in the scan phase rather
 *  than at a meaningless 0%.
 *
 *  An ADOPTED view almost never shows it: the window's fan-out holds the latest
 *  `write-progress` for every id nobody has claimed, and flushes it inside the
 *  same synchronous block the binder acquires in, so the operation's real phase
 *  is there before the first paint. What's left is a window that has heard
 *  nothing at all (a reload mid-transfer), where "counting" is the honest thing
 *  to say for the one tick until the backend says otherwise. */
const OPENING_PHASE: WriteOperationPhase = 'scanning'

const EMPTY_SCAN: ScanReadout = {
  filesFound: 0,
  dirsFound: 0,
  bytesFound: 0,
  currentDir: null,
  filesPerSecond: null,
  bytesPerSecond: null,
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
    compress: 'Compress',
  }
  const operationLabel = operationLabelMap[config.operationType]

  /** Minimum time this VIEW stays on screen, to prevent a jarring one-frame
   *  flash. It is the view's clock, not the operation's: the floor exists
   *  because something appeared and vanished too fast to read, which is a fact
   *  about the thing on screen. They coincide for a dialog that started its own
   *  transfer, and they don't for one that adopts a transfer already running —
   *  where the operation's clock would say "twenty minutes, no flash possible"
   *  about a dialog that had been up for 50 ms. */
  const MIN_DISPLAY_MS = 400
  /** After this many ms of waiting for the backend to settle, the
   *  "Cancelling…" label gets a clarifying tail ("(finishing USB transfers)").
   *  Picked at 200 ms so a fast settle (the common case once cancel
   *  propagation lands on the backend) clears before the label ever changes. */
  const SLOW_SETTLE_LABEL_MS = 200
  /** Last-resort cap on how long we'll keep the dialog open once the operation
   *  starts winding down. The settle gate is supposed to fire `write-cancelled`
   *  + `write-settled` quickly, but if the BE op state was already gone when
   *  the cancel was issued (e.g. it was cleaned up by
   *  `cancel_all_write_operations` during a hot-reload or by a previous
   *  teardown), no events ever fire and the dialog would otherwise stay at
   *  "Cancelling…" forever.
   *
   *  Sits deliberately ABOVE the backend's `CANCEL_DRAIN_DEADLINE` (15 s, in
   *  `transfer/volume/copy.rs`), which is the point by which a cancelled
   *  transfer is guaranteed to emit its terminal event. Firing first would
   *  make the dialog report `0 files processed` moments before the backend
   *  reported the real number. The user never has to sit through this window:
   *  `dismiss()` closes the dialog on their say-so at any moment. */
  const CANCEL_SETTLE_FALLBACK_MS = 20_000

  /** When this view appeared, for {@link MIN_DISPLAY_MS}. */
  const viewOpenedAtMs = Date.now()

  /** The operation this view was opened to WATCH rather than to start, or
   *  `null` for the ordinary dispatching view. Fixed for the view's whole life:
   *  a dialog adopts once, at mount. */
  const adoptedOperationId = config.adoptOperationId ?? null

  /** This operation's backend id, `null` until the start command answers (an
   *  adopted view knows it from the first frame). */
  let operationId = $state<string | null>(null)

  /** The session for the operation this view is watching. `null` for the first
   *  frame after the id lands, which no click can reach and nothing here needs:
   *  birth commands the operation directly, because there is no session yet. */
  const bound = bindOperationSession(() => operationId)

  /** True once this view has handed the operation to the queue window (the
   *  Queue button, the dialog-scoped F2, or the auto-queue path). A plain `let`
   *  — see the module doc. */
  let backgrounded = false
  /** True once the view has been torn down. A plain `let` — see the module doc. */
  let destroyed = false
  /** True once the user pressed Cancel while the dispatch was still in flight.
   *  The one teardown-adjacent flag that still means "stop the operation": it
   *  records a COMMAND, not a detach, and birth acts on it as soon as the
   *  backend names the operation. */
  let cancelRequestedBeforeId = false
  /** True once the modal was closed while the dispatch was still in flight.
   *  Same idea one notch gentler: hand the operation over as soon as it has a
   *  name, so an Escape in that sliver isn't silently dropped. */
  let backgroundRequestedBeforeId = false
  /** True once an outcome has been reported to the parent. Single-shot: a
   *  dismissal racing a terminal event must not close the dialog twice. */
  let closed = false

  /** Flips true once the settle wait has exceeded `SLOW_SETTLE_LABEL_MS`.
   *  Drives the "(finishing USB transfers)" tail on the dialog label. */
  let settleSlow = $state(false)
  let slowSettleTimer: ReturnType<typeof setTimeout> | null = null
  /** Last-resort fallback that closes the dialog if the operation never
   *  finishes winding down. See the doc comment on `CANCEL_SETTLE_FALLBACK_MS`. */
  let cancelSettleFallbackTimer: ReturnType<typeof setTimeout> | null = null
  /** Latches the wind-down timers so the effect that arms them can re-run
   *  freely. A plain `let`: reading it reactively would re-trigger that effect. */
  let windDownTimersArmed = false

  /* ----------------------------------------------------------------------- */
  /* Reading the operation                                                    */
  /* ----------------------------------------------------------------------- */

  const session = () => bound.current
  const outcome = (): OperationOutcome | null => bound.current?.outcome ?? null
  const cancelledEvent = (): WriteCancelledEvent | null => {
    const settled = outcome()
    return settled?.kind === 'cancelled' ? settled.event : null
  }

  /** The operation's phase, or the opening one until it says. */
  const phase = (): WriteOperationPhase => bound.current?.phase ?? OPENING_PHASE

  /** A rollback is under way whichever surface asked for it: this view's own
   *  in-flight command, or the backend reporting the phase. */
  const isRollingBack = (): boolean => (bound.current?.rollingBack ?? false) || bound.current?.phase === 'rolling_back'

  /** How many files the backend has told us about, for a close-out that has to
   *  report a number before the operation finished saying. */
  function reportedFilesProcessed(): number {
    return cancelledEvent()?.filesProcessed ?? bound.current?.progress?.filesDone ?? 0
  }

  /** Whether handing this operation to the queue window still makes sense: it
   *  is running rather than winding down or over. */
  const canHandOff = (): boolean => {
    const op = bound.current
    return op !== null && !op.settled && !op.cancelling && !isRollingBack()
  }

  /** A paused op is still mid-transfer (not cancelling, not settled, no
   *  conflict prompt up), so the Pause/Resume + Queue controls show during the
   *  active copy/move/delete phases only. */
  const canPauseOrQueue = (): boolean => canHandOff() && bound.current?.conflict === null

  /* ----------------------------------------------------------------------- */
  /* Closing the view                                                         */
  /* ----------------------------------------------------------------------- */

  /** Reports an outcome to the parent exactly once, no sooner than the
   *  anti-flicker floor allows. `holdOpen: false` skips the floor for the paths
   *  that hand straight over to another dialog, where nothing could flash. */
  function close(report: () => void, holdOpen = true): void {
    if (closed) return
    closed = true
    clearWindDownTimers()
    const remainingMs = holdOpen ? MIN_DISPLAY_MS - (Date.now() - viewOpenedAtMs) : 0
    if (remainingMs > 0) setTimeout(report, remainingMs)
    else report()
  }

  function startSlowSettleTimer(): void {
    if (slowSettleTimer !== null || settleSlow) return
    slowSettleTimer = setTimeout(() => {
      settleSlow = true
      slowSettleTimer = null
    }, SLOW_SETTLE_LABEL_MS)
  }

  /** Arms both wind-down timers: the label tail and the last-resort close.
   *  Idempotent, so the effect that calls it can re-run on every tick. */
  function armWindDownTimers(): void {
    if (windDownTimersArmed) return
    windDownTimersArmed = true
    startSlowSettleTimer()
    cancelSettleFallbackTimer = setTimeout(() => {
      cancelSettleFallbackTimer = null
      // The backend went quiet: its op state is gone (or never existed), so no
      // terminal event is coming. Close on what it did tell us rather than
      // leaving the dialog at "Cancelling…" forever.
      log.warn('The wind-down for op={operationId} went quiet after {ms}ms; closing the dialog', {
        operationId,
        ms: CANCEL_SETTLE_FALLBACK_MS,
      })
      close(() => {
        config.onCancelled(reportedFilesProcessed())
      })
    }, CANCEL_SETTLE_FALLBACK_MS)
  }

  function clearWindDownTimers(): void {
    if (slowSettleTimer !== null) {
      clearTimeout(slowSettleTimer)
      slowSettleTimer = null
    }
    settleSlow = false
    if (cancelSettleFallbackTimer !== null) {
      clearTimeout(cancelSettleFallbackTimer)
      cancelSettleFallbackTimer = null
    }
    windDownTimersArmed = false
  }

  /**
   * Close the dialog now, on the user's say-so, without waiting for the
   * backend.
   *
   * The settle gate is the right DEFAULT (it buys honest file counts and keeps
   * a new op off a volume that's still tearing down), but it must never be the
   * only way out: in the 2026-07-31 incident the window wouldn't close and
   * force-quit was the only option left, which is what turned a recoverable
   * stall into data loss. The operation keeps winding down in the background
   * exactly as it would have; we stop making the user watch it.
   */
  function dismiss(): void {
    if (closed) return
    // It reports a CANCEL, so it may only fire while one is what's happening. A
    // completion or a failure is already on its way to the parent, and telling
    // it "cancelled" instead would run the wrong tail over the user's panes.
    const settled = outcome()
    if (settled !== null && settled.kind !== 'cancelled') return
    log.info('User dismissed the dialog while the backend was still settling: op={operationId}', { operationId })
    // Report what the backend has told us so far, rather than pretending zero.
    const filesProcessed = reportedFilesProcessed()
    close(() => {
      config.onCancelled(filesProcessed)
    })
  }

  /* ----------------------------------------------------------------------- */
  /* Watching the operation                                                   */
  /* ----------------------------------------------------------------------- */

  /** Reports the operation's end to the parent, once it has finished ending.
   *  A cancel waits for `write-settled` as well: the backend may still be
   *  tearing down USB / network sessions, and dispatching a new op against a
   *  volume in that state is what wedged the device in the original incident.
   *  See the "Settle contract" in
   *  `src-tauri/src/file_system/write_operations/CLAUDE.md`. */
  function reportOutcome(settled: OperationOutcome, settleEventReceived: boolean): void {
    switch (settled.kind) {
      case 'complete': {
        const event = settled.event
        log.info('{op} complete: {filesProcessed} {filesNoun}, {bytesProcessed} {bytesNoun}', {
          op: operationLabel,
          filesProcessed: event.filesProcessed,
          filesNoun: pluralize(event.filesProcessed, 'file'),
          bytesProcessed: event.bytesProcessed,
          bytesNoun: pluralize(event.bytesProcessed, 'byte'),
        })
        close(() => {
          config.onComplete(event.filesProcessed, event.filesSkipped, event.bytesProcessed)
        })
        return
      }
      case 'error': {
        const error = settled.event.error
        if (error.type === 'archive_needs_password') {
          // Expected, recoverable flow: the write-error only exists to prompt for
          // a password and retry (intercepted upstream in `handleTransferError`),
          // so log at warn to keep it out of prod error-report bundles (error+).
          log.warn('{op} operation needs an archive password: {errorType}', {
            op: operationLabel,
            errorType: error.type,
            error,
          })
        } else {
          log.error('{op} error: {errorType}', { op: operationLabel, errorType: error.type, error })
        }
        // No floor: the error dialog takes this dialog's place, so there is
        // nothing that could flash.
        close(() => {
          config.onError(error)
        }, false)
        return
      }
      case 'cancelled': {
        if (!settleEventReceived) return
        const event = settled.event
        log.info('{op} cancelled after {filesProcessed} {filesNoun}, rolledBack={rolledBack}', {
          op: operationLabel,
          filesProcessed: event.filesProcessed,
          filesNoun: pluralize(event.filesProcessed, 'file'),
          rolledBack: event.rolledBack,
        })
        close(() => {
          config.onCancelled(event.filesProcessed)
        })
        return
      }
      case 'gone':
        // The registry has no record of this operation, so it ended before this
        // view could watch it. Close honestly rather than sit empty.
        log.info('op={operationId} was already over by the time the dialog looked; closing', { operationId })
        close(() => {
          config.onCancelled(0)
        })
    }
  }

  $effect(() => {
    const op = bound.current
    if (op === null) return
    const settled = op.outcome
    if (settled === null) return
    reportOutcome(settled, op.settleEventReceived)
  })

  $effect(() => {
    const op = bound.current
    if (op === null) return
    // Winding down: a cancel is on its way, or one has landed and the backend
    // is still tearing the task down. `write-settled` is what ends the wait.
    // Disarming on the way back out matters as much as arming: a cancel the
    // backend refused lets go of `cancelling`, and a timer left running would
    // close the dialog on a transfer that is still going.
    const windingDown = op.cancelling || op.outcome?.kind === 'cancelled'
    if (windingDown && !op.settleEventReceived) armWindDownTimers()
    else clearWindDownTimers()
  })

  // Auto-queue is a decision a DISPATCHING view makes: the manager admitted the
  // operation behind a busy lane, and stacking a second modal over the one
  // already up would be worse than surfacing the queue. A view opened precisely
  // to watch this operation makes the opposite call — bouncing it back out of
  // sight is the button appearing to do nothing.
  $effect(() => {
    const op = bound.current
    if (op === null || backgrounded || adoptedOperationId !== null) return
    if (op.status === 'queued') handleAutoQueued()
  })

  /* ----------------------------------------------------------------------- */
  /* Commanding the operation                                                 */
  /* ----------------------------------------------------------------------- */

  /** Cancel (keep what's written) or Rollback (undo it), through the operation's
   *  own commands, so every other surface watching sees the press. */
  async function handleCancel(rollback: boolean): Promise<void> {
    if (operationId === null) {
      // The backend hasn't named the operation yet. Record the command; birth
      // issues it the moment the id lands.
      log.warn('Cancel requested but no operationId yet; will cancel after the start command answers')
      cancelRequestedBeforeId = true
      return
    }
    const op = bound.current
    if (op === null) {
      // The binder acquires on the first effect flush after the id lands, a
      // sub-frame sliver no click can reach. Logged rather than swallowed.
      log.warn('Cancel requested for op={operationId} before its session took hold; ignoring', { operationId })
      return
    }
    if (rollback) {
      log.info('Rolling back operation: {operationId}', { operationId })
      await op.rollback()
    } else {
      log.info('Cancelling operation (keeping partial files): {operationId}', { operationId })
      await op.cancel()
    }
  }

  /** Pauses or resumes this operation in place. Which way it goes is decided
   *  from the registry snapshot's lifecycle status, so the button flips only
   *  once the backend actually parked/resumed, never optimistically. */
  async function handlePauseResume(): Promise<void> {
    await bound.current?.togglePause()
  }

  /** Answers the clash this dialog is showing. The backend arbitrates and
   *  reports its verdict; the session lets go of the clash on any of them, so
   *  only a call that never landed leaves the prompt up. */
  async function handleConflictResolution(resolution: ConflictResolution, applyToAll: boolean): Promise<void> {
    const op = bound.current
    if (op === null || op.conflict === null) return
    log.info('Resolving conflict with {resolution}, applyToAll={applyToAll}', { resolution, applyToAll })
    await op.resolveConflict(resolution, applyToAll)
  }

  /** Sends this operation to the background: keep it running, open the queue
   *  window, and unmount this modal. The op is now managed in the queue window.
   *  Fired by the Queue button, the dialog-scoped F2, and the auto-queue path. */
  function handleQueue(): void {
    if (!operationId || backgrounded) return
    log.info('Backgrounding operation to the queue window: {operationId}', { operationId })
    handOff(operationId)
    addToast(tString('fileOperations.transferProgress.backgroundedToast'), {
      level: 'info',
      toastGroup: 'transfer-queue',
    })
    config.onQueue?.()
  }

  /** Called once the operation first reports itself as `queued`: the manager
   *  admitted it behind a busy lane rather than running it now. Don't stack a
   *  second modal on top of the foreground op — surface the queue window with a
   *  quiet toast and unmount, exactly like a manual Queue. */
  function handleAutoQueued(): void {
    if (backgrounded || !operationId) return
    // How many operations this one is waiting behind: the ones occupying lanes
    // right now, from the same live rows the Background/Queue label reads.
    const ahead = getMainWindowOperationRows().filter(
      (row) =>
        row.snapshot.operationId !== operationId &&
        (row.snapshot.status === 'running' || row.snapshot.status === 'paused'),
    ).length
    const aheadCount = Math.max(1, ahead)
    log.info('Operation queued behind {ahead} on a busy lane; surfacing the queue window', { ahead: aheadCount })
    handOff(operationId)
    const countText = tString('fileOperations.transferProgress.queuedToastCount', { count: aheadCount })
    addToast(tString('fileOperations.transferProgress.queuedToast', { countText }), {
      level: 'info',
      toastGroup: 'transfer-queue',
    })
    config.onQueue?.()
  }

  /**
   * The modal closed: its × button, Escape, or the focus trap tearing down.
   *
   * ❌ Never a cancel. Closing a looking glass says nothing about the thing it
   * was looking at, and a transfer that dies because someone pressed Escape is
   * the coupling this whole seam exists to remove. So a still-running operation
   * is handed to the queue window, exactly as the Queue button hands it over,
   * and one that is already winding down or over is simply stopped watching.
   */
  function detach(): void {
    if (operationId === null) {
      // Nothing to hand over yet. Birth does it the moment the id lands, so the
      // key press isn't silently dropped.
      backgroundRequestedBeforeId = true
      return
    }
    if (canHandOff()) handleQueue()
    else dismiss()
  }

  /** The half the manual Queue and the auto-queue share: mark the handoff and
   *  open the window that now owns it. The foreground slot is released HERE
   *  rather than in `destroy()`, because handing over is exactly when the corner
   *  chip and the failure notice must start speaking about this operation, and
   *  `onQueue` is optional so the modal may stay mounted. */
  function handOff(id: string): void {
    backgrounded = true
    clearForegroundOperation(id)
    void openQueueWindow()
  }

  /* ----------------------------------------------------------------------- */
  /* Birth                                                                    */
  /* ----------------------------------------------------------------------- */

  async function beginOperation(): Promise<void> {
    log.info('Starting {op} operation: {sourceCount} {sourcesNoun}', {
      op: config.operationType,
      sourceCount: config.sourcePaths.length,
      sourcesNoun: pluralize(config.sourcePaths.length, 'source'),
    })

    // From here until the slot is claimed (or the dispatch is abandoned), this
    // dialog owns an operation nothing can name yet. The conflict host waits out
    // that window rather than deciding ownership against an empty slot; see
    // `../foreground-operation.svelte.ts`.
    beginForegroundClaim()

    try {
      try {
        const result = await dispatchTransferOperation(config)
        const id = result.operationId
        operationId = id
        log.info('{op} operation started with operationId: {operationId}', { op: operationLabel, operationId: id })

        // Reply to the MCP round-trip (if this op was started via an auto-confirmed
        // MCP tool) with the spawned operationId, so the waiting tool can return it
        // for a follow-up `queue` / `await operation_complete`. Fire-and-forget: the
        // op is already running regardless of whether the reply lands.
        if (config.mcpRequestId) {
          void emit('mcp-response', { requestId: config.mcpRequestId, ok: true, operationId: id })
        }

        if (cancelRequestedBeforeId) {
          // An explicit Cancel that arrived before the operation had a name.
          // Through the MANAGER, because an operation admitted behind a busy
          // lane hasn't spawned a write op yet and only this path can drop it.
          // Reported to the parent too, so the modal comes down: nothing else
          // will close it, and a stuck progress dialog poisons every following
          // operation through `ensureAppReady`'s Escape.
          log.info('Cancel arrived before the id did; cancelling op={operationId}', { operationId: id })
          void cancelOperation(id)
          close(() => {
            config.onCancelled(0)
          })
          return
        }

        if (backgroundRequestedBeforeId) {
          // The modal was closed before the operation had a name. Now it has
          // one, so hand it to the queue window rather than losing the press.
          log.info('The modal closed before op={operationId} was named; backgrounding it', { operationId: id })
          handleQueue()
          return
        }

        // A view that went away without commanding anything has DETACHED. The
        // operation keeps running, and the queue window and the corner chip are
        // where it shows up now. It owns no foreground slot, though: a dialog
        // that is already gone owns nothing.
        if (destroyed) {
          log.info('The dialog was gone before op={operationId} was named; it keeps running', { operationId: id })
          return
        }

        // This dialog now owns the operation in the foreground, so ambient
        // surfaces stay quiet about it (`../foreground-operation.svelte.ts`).
        setForegroundOperationId(id)
      } finally {
        // Every route out of the dispatch settles the claim: the id landed, the
        // dialog was already gone, or the command threw. A leaked claim would
        // leave every later conflict deferred forever.
        endForegroundClaim()
      }
    } catch (err: unknown) {
      log.error('Failed to start {op} operation: {error}', { op: config.operationType, error: err })
      clearWindDownTimers()
      // Fail the MCP round-trip too (the op never spawned, so no operationId).
      if (config.mcpRequestId) {
        const message = err instanceof Error ? err.message : String(err)
        void emit('mcp-response', { requestId: config.mcpRequestId, ok: false, error: message })
      }
      // Tauri commands return structured WriteOperationError objects on validation failure
      // (e.g. destination_inside_source). Pass them through to preserve the specific error type.
      const error: WriteOperationError =
        typeof err === 'object' && err !== null && 'type' in err
          ? (err as WriteOperationError)
          : {
              type: 'io_error',
              path: config.sourcePaths[0] ?? '',
              message: `Failed to start ${config.operationType}: ${String(err)}`,
            }
      close(() => {
        config.onError(error)
      }, false)
    }
  }

  /** Adopts an operation that is already running: name it, claim the foreground
   *  slot, and let the binder do the rest. There is no dispatch, no MCP reply,
   *  and no unnamed window to defer a conflict over — the id was known before
   *  this view existed. */
  function adopt(id: string): void {
    log.info('Adopting op={operationId} into the progress dialog', { operationId: id })
    operationId = id
    // Ambient surfaces (the corner chip, the failure notice) must stop repeating
    // an operation the user is now watching in full.
    setForegroundOperationId(id)
  }

  /** Starts the dialog's work. Called from the component's `onMount`.
   *
   *  Dispatches straight away even when a `TransferDialog` preview is still
   *  walking: the backend claims that preview at registration and its own task
   *  waits for it, so the operation has an id, a queue row, and Background from
   *  the first frame. (Pause stays hidden until the write starts — a scan-wait
   *  has nothing to park, and the backend declines the flip.) */
  function start(): void {
    if (adoptedOperationId !== null) {
      adopt(adoptedOperationId)
      return
    }
    void beginOperation()
  }

  /** Tears the view down (the component's `onDestroy`).
   *
   *  ❌ It does NOT stop the operation, and it does NOT cancel the scan preview.
   *  A dialog going away is a viewer detaching; the operation owns its preview
   *  and its own life. The session is released by the binder, whose effect is
   *  torn down with this component's scope. */
  function destroy(): void {
    destroyed = true
    // The catch-all release: completion, cancel, error, and any other unmount
    // all land here. `clearForegroundOperation` no-ops if a later dialog already
    // took the slot.
    if (operationId) clearForegroundOperation(operationId)
    clearWindDownTimers()
  }

  return {
    start,
    destroy,
    detach,
    dismiss,
    handleCancel,
    handleConflictResolution,
    handlePauseResume,
    handleQueue,
    /** The counting readout, zeros until the operation says otherwise. */
    get scan(): ScanReadout {
      return session()?.scan ?? EMPTY_SCAN
    },
    get phase(): WriteOperationPhase {
      return phase()
    },
    get currentFile(): string | null {
      return session()?.progress?.currentFile ?? null
    },
    get filesDone(): number {
      return session()?.progress?.filesDone ?? 0
    },
    get filesTotal(): number {
      return session()?.progress?.filesTotal ?? 0
    },
    get bytesDone(): number {
      return session()?.progress?.bytesDone ?? 0
    },
    get bytesTotal(): number {
      return session()?.progress?.bytesTotal ?? 0
    },
    get isCancelling(): boolean {
      return session()?.cancelling ?? false
    },
    get isRollingBack(): boolean {
      return isRollingBack()
    },
    /** The operation says it can't be reversed. `supportsRollback` is a promise
     *  about the OPERATION, so the registry row is the authority wherever it has
     *  arrived; the dialog's own same-volume-move rule stands beside it, for the
     *  window before the first snapshot lands. An adopted view has nothing but
     *  this: no volume ids, no direction, no birth context to reason from. */
    get rollbackUnavailable(): boolean {
      const snapshot = session()?.snapshot ?? null
      return snapshot !== null && !snapshot.supportsRollback
    },
    /** The operation has reported a cancel, and the dialog stays in
     *  "Cancelling…" until the backend has finished tearing down too. */
    get cancelEventReceived(): boolean {
      return cancelledEvent() !== null
    },
    get settleSlow(): boolean {
      return settleSlow
    },
    get operationSettled(): boolean {
      return session()?.settled ?? false
    },
    get isPaused(): boolean {
      return session()?.status === 'paused'
    },
    get pauseInFlight(): boolean {
      return session()?.pauseInFlight ?? false
    },
    get canPauseOrQueue(): boolean {
      return canPauseOrQueue()
    },
    /** The clash this operation is parked on, if any. */
    get conflict() {
      return session()?.conflict ?? null
    },
    get isResolvingConflict(): boolean {
      return session()?.resolvingConflict ?? false
    },
    get bytesPerSecond(): BytesPerSecond | null {
      return session()?.readout?.bytesPerSecond ?? null
    },
    get filesPerSecond(): number | null {
      return session()?.progress?.filesPerSecond ?? null
    },
    get etaSecondsDisplay(): Seconds | null {
      return session()?.etaSecondsDisplay ?? null
    },
    get activity(): TransferActivity | null {
      return session()?.progress?.activity ?? null
    },
    /** This operation's backend id, `null` until the start command answers. The
     *  dialog needs it to exclude itself from the queue it's asking about. */
    get operationId(): string | null {
      return operationId
    },
  }
}
