/**
 * Headless tests for `createTransferProgressState`: what the progress dialog's
 * view shows and does across one operation's life (progress, birth, conflicts,
 * cancel, rollback, pause and queue, the ETA smoother, disposal, and a
 * still-scanning transfer), driven without rendering a component.
 *
 * The shared mocks, fixtures, and per-test lifecycle (and why they're shaped
 * that way) live in `test-transfer-progress-harness.svelte.ts`. Who owns the
 * operation (the foreground slot, adoption, an adopted reversal) is in
 * `transfer-progress-state.ownership.svelte.test.ts`.
 */

import { describe, it, expect, vi } from 'vitest'
import { flushSync } from 'svelte'
import type { WriteConflictEvent, WriteOperationStartResult } from '$lib/tauri-commands'
import type { WriteOperationError } from '$lib/file-explorer/types'

vi.mock('$lib/tauri-commands', async () =>
  (await import('./test-transfer-progress-harness.svelte')).tauriCommandsMock(),
)
vi.mock('$lib/file-operations/queue/queue-window', async () =>
  (await import('./test-transfer-progress-harness.svelte')).queueWindowMock(),
)
vi.mock('$lib/ui/toast', async () => (await import('./test-transfer-progress-harness.svelte')).toastMock())
vi.mock('$lib/settings', async () => (await import('./test-transfer-progress-harness.svelte')).settingsMock())
vi.mock('$lib/intl/messages.svelte', async () =>
  (await import('./test-transfer-progress-harness.svelte')).messagesMock(),
)
vi.mock('../progress-readout', async (importOriginal) =>
  (await import('./test-transfer-progress-harness.svelte')).progressReadoutMock(
    await importOriginal<typeof import('../progress-readout')>(),
  ),
)
vi.mock('$lib/logging/logger', async () => (await import('./test-transfer-progress-harness.svelte')).loggerMock())

import { createTransferProgressState } from './transfer-progress-state.svelte'
import {
  copyBetweenVolumes,
  resolveWriteConflict,
  cancelOperation,
  cancelWriteOperation,
  cancelScanPreview,
  pauseOperation,
  resumeOperation,
  listOperations,
} from '$lib/tauri-commands'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import { addToast } from '$lib/ui/toast'
import type { CancelRollbackReadout } from './cancel-rollback-toast'
import { createEtaSmoother } from '../progress-readout'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from '../operation-session/window-operation-sessions.svelte'
import {
  listeners,
  makeConfig,
  progressEvent,
  settle,
  snapshot,
  useTransferProgressState,
} from './test-transfer-progress-harness.svelte'

const { makeState, startedState } = useTransferProgressState(createTransferProgressState)

describe('createTransferProgressState: progress + complete', () => {
  it('reflects a progress event in the exposed getters', async () => {
    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ filesDone: 2, filesTotal: 4, bytesDone: 200, bytesTotal: 400, etaSeconds: 12 }))
    expect(state.phase).toBe('copying')
    expect(state.filesDone).toBe(2)
    expect(state.bytesDone).toBe(200)
    expect(state.etaSecondsDisplay).toBe(12)
  })

  it('opens in the scan phase, before the operation has said anything', async () => {
    // A confirmed transfer starts by counting, so the dialog shows the scan
    // readout rather than a meaningless 0% while it waits for the first tick.
    const { state } = await startedState({ previewId: 'prev-1' })
    expect(state.phase).toBe('scanning')
  })

  it('handles a scanning → copying phase transition and smooths the displayed ETA', async () => {
    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    // Scanning phase: tallies + current dir come through the scan-meta fields.
    listeners.progress(
      progressEvent({
        phase: 'scanning',
        filesDone: 3,
        dirsDone: 2,
        bytesDone: 30,
        currentDir: '/src/sub',
        etaSeconds: null,
      }),
    )
    expect(state.phase).toBe('scanning')
    expect(state.scan.filesFound).toBe(3)
    expect(state.scan.dirsFound).toBe(2)
    expect(state.scan.currentDir).toBe('/src/sub')

    // Transition to copying: resets the smoothed ETA, then re-warms from raw.
    listeners.progress(progressEvent({ phase: 'copying', etaSeconds: 10 }))
    expect(state.etaSecondsDisplay).toBe(10)
    // A second copying tick smooths toward the new raw value (25% of the gap).
    listeners.progress(progressEvent({ phase: 'copying', etaSeconds: 20 }))
    expect(state.etaSecondsDisplay).toBeCloseTo(12.5)
  })

  it('enters rolling_back from a backend progress event', async () => {
    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ phase: 'rolling_back' }))
    expect(state.isRollingBack).toBe(true)
    expect(state.phase).toBe('rolling_back')
  })

  it('fires onComplete after the min-display window', async () => {
    const { state, config } = await startedState()
    if (!listeners.complete) throw new Error('complete subscriber never registered')
    listeners.complete({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 5,
      filesSkipped: 1,
      bytesProcessed: 999,
    })
    expect(state.operationSettled).toBe(true)
    flushSync()
    // Min-display floor: not yet called, then called after advancing past it.
    expect(config.onComplete).not.toHaveBeenCalled()
    vi.advanceTimersByTime(450)
    expect(config.onComplete).toHaveBeenCalledWith({
      filesProcessed: 5,
      filesSkipped: 1,
      bytesProcessed: 999,
      appearedDuringMove: null,
      topLevelSkipped: null,
      refused: null,
    })
  })

  it('fires onError on a write-error event', async () => {
    const { state, config } = await startedState()
    if (!listeners.error) throw new Error('error subscriber never registered')
    const error: WriteOperationError = { type: 'io_error', path: '/src/file.txt', message: 'boom' }
    listeners.error({ operationId: 'op-1', operationType: 'copy', error, progressAtStop: null })
    expect(state.operationSettled).toBe(true)
    flushSync()
    expect(config.onError).toHaveBeenCalledWith(error, null)
  })

  /**
   * ❗ A move that fails in the backend BEFORE its start command even returns (a
   * same-volume rename of a locked file refuses at once) still reaches the dialog:
   * the failure is emitted before anything here knows the id. It spun on
   * "Verifying before move… 0 files" forever (QA round 2, M5).
   */
  it('fires onError for a failure that arrived before the operation id did', async () => {
    const error = {
      type: 'permission_denied' as const,
      path: '/a/locked.txt',
      message: 'EPERM',
      errno: 1,
      refusal: 'systemProtected' as const,
      refusedFolder: null,
      side: 'source' as const,
    }
    const { copyBetweenVolumes } = await import('$lib/tauri-commands')
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => {
      listeners.error?.({ operationId: 'op-1', operationType: 'copy', error, progressAtStop: null })
      return Promise.resolve({ operationId: 'op-1', operationType: 'copy' })
    })
    // The backend retains the failure as a `failed` row, which is what the registry snapshot shows.
    vi.mocked(listOperations).mockResolvedValue([{ ...snapshot('op-1', 'failed'), error }])

    const { config } = await startedState()
    flushSync()

    expect(config.onError).toHaveBeenCalledWith(error, null)
  })

  it('ignores events for a different operation id', async () => {
    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ operationId: 'op-other', filesDone: 99 }))
    expect(state.filesDone).toBe(0)
  })
})

describe('createTransferProgressState: birth', () => {
  it('loses no event that arrives before the operation is named', async () => {
    // The window's fan-out holds events for an id nobody has claimed yet and
    // flushes them when a session claims it, so the view no longer buffers
    // anything of its own.
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const state = makeState(makeConfig())
    state.start()
    await settle()
    // Parked on the dispatch await: no session exists yet, so the fan-out holds
    // the tick.
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ filesDone: 7 }))
    expect(state.filesDone).toBe(0)

    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await settle()
    // The claim flushed it.
    expect(state.filesDone).toBe(7)
  })

  it('cancels through the manager when Cancel is pressed before the id arrives', async () => {
    // Was: "cancels and reports the op when the dialog is torn down mid-dispatch",
    // asserting `cancelWriteOperation(id, true)`. Two things changed. A TEARDOWN
    // no longer implies a cancel (see the disposal suite); only an explicit
    // Cancel does, and that is what this drives. And the cancel goes through the
    // MANAGER, because an operation admitted behind a busy lane hasn't spawned a
    // write op yet, so `cancelWriteOperation` could not drop it and the transfer
    // would have run on regardless of the press.
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const config = makeConfig()
    const state = makeState(config)
    state.start()
    await settle()
    // Cancel before the operationId arrives: records the command and defers.
    void state.handleCancel(false)
    await settle()
    expect(cancelOperation).not.toHaveBeenCalled()

    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await settle()
    expect(cancelOperation).toHaveBeenCalledWith('op-1')
    vi.advanceTimersByTime(450)
    expect(config.onCancelled).toHaveBeenCalledWith(0)
  })

  it('backgrounds instead when the modal is CLOSED before the id arrives', async () => {
    // Closing the dialog is a detach, so the press that could not be honoured
    // yet becomes a handoff, not a cancel.
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const config = makeConfig()
    const state = makeState(config)
    state.start()
    await settle()
    state.detach()

    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await settle()

    expect(cancelOperation).not.toHaveBeenCalled()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(config.onQueue).toHaveBeenCalledTimes(1)
  })

  it('routes a structured backend error through onError', async () => {
    // Tauri rejects with a structured `WriteOperationError`; model it as an
    // Error carrying the typed fields so the SUT's `'type' in err` branch hits
    // (and so we reject with an Error, per prefer-promise-reject-errors).
    const structured = Object.assign(new Error('nope'), {
      type: 'permission_denied',
      path: '/src/file.txt',
      message: 'nope',
      errno: null,
      refusal: 'unclassified',
      refusedFolder: null,
      side: null,
    } satisfies WriteOperationError)
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(structured))
    const config = makeConfig()
    const state = makeState(config)
    state.start()
    await settle()
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'permission_denied' }), null)
  })

  it('wraps a non-structured dispatch failure as an io_error', async () => {
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(new Error('kaboom')))
    const config = makeConfig()
    const state = makeState(config)
    state.start()
    await settle()
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'io_error' }), null)
  })
})

describe('createTransferProgressState: conflict resolution', () => {
  function conflictEvent(): WriteConflictEvent {
    return {
      operationId: 'op-1',
      conflictId: 1,
      sourcePath: '/src/file.txt',
      destinationPath: '/dst/file.txt',
      sourceSize: 10,
      destinationSize: 20,
      sourceModified: null,
      destinationModified: null,
      destinationIsNewer: false,
      sizeDifference: 10,
    }
  }

  it('surfaces a conflict then clears it on resolve (skip all)', async () => {
    const { state } = await startedState()
    if (!listeners.conflict) throw new Error('conflict subscriber never registered')
    listeners.conflict(conflictEvent())
    expect(state.conflict).not.toBeNull()

    await state.handleConflictResolution('skip', true)
    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 1, 'skip', true)
    expect(state.conflict).toBeNull()
  })

  it('resolves a single conflict with overwrite (proceed)', async () => {
    const { state } = await startedState()
    if (!listeners.conflict) throw new Error('conflict subscriber never registered')
    listeners.conflict(conflictEvent())
    await state.handleConflictResolution('overwrite', false)
    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 1, 'overwrite', false)
    expect(state.conflict).toBeNull()
  })

  it('clears the prompt when another surface answered the same conflict first', async () => {
    // Two surfaces can render one clash; the backend arbitrates and reports its
    // verdict. Being the second one is not a failure, so the dialog stops asking
    // rather than leaving the question on screen.
    const { state } = await startedState()
    if (!listeners.conflict) throw new Error('conflict subscriber never registered')
    listeners.conflict(conflictEvent())
    vi.mocked(resolveWriteConflict).mockImplementationOnce(() => Promise.resolve('already_resolved'))
    await state.handleConflictResolution('overwrite', false)
    expect(state.conflict).toBeNull()
    expect(state.isResolvingConflict).toBe(false)
  })

  it('keeps the prompt up when the answer never lands', async () => {
    const { state } = await startedState()
    if (!listeners.conflict) throw new Error('conflict subscriber never registered')
    listeners.conflict(conflictEvent())
    vi.mocked(resolveWriteConflict).mockImplementationOnce(() => Promise.reject(new Error('ipc down')))
    await state.handleConflictResolution('skip', false)
    // Nothing reached the backend, so the question is still open.
    expect(state.conflict).not.toBeNull()
    expect(state.isResolvingConflict).toBe(false)
  })

  it('no-ops resolution when there is no active conflict', async () => {
    const { state } = await startedState()
    await state.handleConflictResolution('skip', false)
    expect(resolveWriteConflict).not.toHaveBeenCalled()
  })
})

describe('createTransferProgressState: cancel + settle close-out', () => {
  it('closes only after both write-cancelled and write-settled arrive', async () => {
    const { state, config } = await startedState()
    void state.handleCancel(false)
    await settle()
    expect(state.isCancelling).toBe(true)
    expect(cancelOperation).toHaveBeenCalledWith('op-1')

    // Slow-settle label tail appears after 200 ms.
    vi.advanceTimersByTime(200)
    expect(state.settleSlow).toBe(true)

    if (!listeners.cancelled || !listeners.settled) throw new Error('cancel/settle subscribers never registered')
    listeners.cancelled({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 4,
      rollback: {
        outcome: 'notRolledBack',
        reversed: 0,
        skips: [],
        stagedLeftovers: null,
        originalsStillInPlace: null,
        recovered: [],
      },
    })
    flushSync()
    expect(state.operationSettled).toBe(true)
    expect(config.onCancelled).not.toHaveBeenCalled()

    listeners.settled({ operationId: 'op-1', operationType: 'copy' })
    flushSync()
    expect(state.settleSlow).toBe(false)
    vi.advanceTimersByTime(450)
    expect(config.onCancelled).toHaveBeenCalledWith(4)
  })

  it('is idempotent against a repeated cancel click', async () => {
    const { state } = await startedState()
    void state.handleCancel(false)
    await settle()
    void state.handleCancel(false)
    await settle()
    expect(cancelOperation).toHaveBeenCalledTimes(1)
  })

  it('falls back to closing if neither terminal event arrives', async () => {
    const { state, config } = await startedState()
    void state.handleCancel(false)
    await settle()
    // Last-resort fallback fires at CANCEL_SETTLE_FALLBACK_MS, which sits ABOVE
    // the backend's 15 s `CANCEL_DRAIN_DEADLINE` so it can't report `0 files`
    // moments before the backend reported the real number. The user never waits
    // this out in practice: the dialog's Close button dismisses immediately.
    vi.advanceTimersByTime(20_000)
    expect(config.onCancelled).toHaveBeenCalledWith(0)
  })

  it('stops waiting when the backend refuses the cancel', async () => {
    // The operation is still going, so the last-resort close must NOT fire and
    // shut the dialog on a live transfer. The session lets go of `cancelling`
    // for exactly that reason, and the view follows it back out.
    vi.mocked(cancelOperation).mockImplementationOnce(() => Promise.reject(new Error('ipc down')))
    const { state, config } = await startedState()
    await state.handleCancel(false)
    await settle()
    expect(state.isCancelling).toBe(false)

    vi.advanceTimersByTime(20_000)
    expect(config.onCancelled).not.toHaveBeenCalled()
    expect(state.settleSlow).toBe(false)
  })

  it('lets the user out at once while the backend is still winding down', async () => {
    const { state, config } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ filesDone: 3 }))
    void state.handleCancel(false)
    await settle()

    state.dismiss()
    vi.advanceTimersByTime(450)
    // Reports what the backend did tell us, rather than pretending zero.
    expect(config.onCancelled).toHaveBeenCalledWith(3)
  })
})

describe('createTransferProgressState: rollback', () => {
  it('starts a rollback and closes when the cancelled event lands', async () => {
    const { state, config } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent())

    void state.handleCancel(true)
    await settle()
    expect(state.isRollingBack).toBe(true)
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)

    if (!listeners.cancelled || !listeners.settled) throw new Error('cancel/settle subscribers never registered')
    listeners.cancelled({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 2,
      rollback: {
        outcome: 'rolledBack',
        reversed: 2,
        skips: [],
        stagedLeftovers: null,
        originalsStillInPlace: null,
        recovered: [],
      },
    })
    listeners.settled({ operationId: 'op-1', operationType: 'copy' })
    flushSync()
    vi.advanceTimersByTime(450)
    expect(config.onCancelled).toHaveBeenCalledWith(2)
  })

  it('says what the reversal left behind, rather than closing on a bar that drained to zero', async () => {
    // The join this covers: the reversal's report rides `write-cancelled`, and the
    // bar always lands on zero whether items came off the disk or were left alone.
    // Without a summary, "the bar hit zero" reads as "everything was removed".
    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent())
    void state.handleCancel(true)
    await settle()

    if (!listeners.cancelled || !listeners.settled) throw new Error('cancel/settle subscribers never registered')
    listeners.cancelled({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 4,
      rollback: {
        outcome: 'partiallyRolledBack',
        reversed: 3,
        skips: [{ reason: 'drift', count: 1, exampleName: 'notes.md' }],
        stagedLeftovers: null,
        originalsStillInPlace: null,
        recovered: [],
      },
    })
    listeners.settled({ operationId: 'op-1', operationType: 'copy' })
    flushSync()
    vi.advanceTimersByTime(450)

    expect(vi.mocked(addToast)).toHaveBeenCalledTimes(1)
    const [, options] = vi.mocked(addToast).mock.calls[0]
    expect(options?.level).toBe('info')
    const readout = options?.props?.readout as CancelRollbackReadout
    expect(readout.reasons).toHaveLength(1)
  })

  it('stays quiet when a plain Cancel kept what was written', async () => {
    const { state } = await startedState()
    void state.handleCancel(false)
    await settle()

    if (!listeners.cancelled || !listeners.settled) throw new Error('cancel/settle subscribers never registered')
    listeners.cancelled({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 4,
      rollback: {
        outcome: 'notRolledBack',
        reversed: 0,
        skips: [],
        stagedLeftovers: null,
        originalsStillInPlace: null,
        recovered: [],
      },
    })
    listeners.settled({ operationId: 'op-1', operationType: 'copy' })
    flushSync()
    vi.advanceTimersByTime(450)

    expect(vi.mocked(addToast)).not.toHaveBeenCalled()
  })

  it('cancels an in-progress rollback (keep remaining files)', async () => {
    const { state } = await startedState()
    void state.handleCancel(true)
    await settle()
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)

    // A plain Cancel while rolling back stops the rollback without reversing.
    void state.handleCancel(false)
    await settle()
    expect(cancelOperation).toHaveBeenCalledWith('op-1')
    expect(state.isCancelling).toBe(true)
  })
})

describe('createTransferProgressState: pause, queue, and auto-queue', () => {
  it('tracks pause status from the operations-changed snapshot and toggles it', async () => {
    const { state } = await startedState()
    if (!listeners.opsChanged) throw new Error('operations-changed subscriber never registered')

    listeners.opsChanged({ operations: [snapshot('op-1', 'running')] })
    expect(state.isPaused).toBe(false)
    expect(state.canPauseOrQueue).toBe(true)

    await state.handlePauseResume()
    expect(pauseOperation).toHaveBeenCalledWith('op-1')

    listeners.opsChanged({ operations: [snapshot('op-1', 'paused')] })
    expect(state.isPaused).toBe(true)

    await state.handlePauseResume()
    expect(resumeOperation).toHaveBeenCalledWith('op-1')
    expect(state.pauseInFlight).toBe(false)
  })

  it('shows no speed but keeps the time left while paused, like every other view of the op', async () => {
    const { state } = await startedState()
    if (!listeners.progress || !listeners.opsChanged)
      throw new Error('progress/operations-changed subscribers never registered')

    listeners.opsChanged({ operations: [snapshot('op-1', 'running')] })
    listeners.progress(progressEvent({ bytesPerSecond: 4096, filesPerSecond: 1905, etaSeconds: 58 }))
    expect(state.bytesPerSecond).toBe(4096)
    expect(state.filesPerSecond).toBe(1905)
    expect(state.etaSecondsDisplay).toBe(58)

    // The queue row for this same operation drops the same two numbers and
    // keeps the same third: a speed over a parked transfer is invented, while
    // how much longer it has left is what the user paused to think about.
    listeners.opsChanged({ operations: [snapshot('op-1', 'paused')] })
    expect(state.bytesPerSecond).toBeNull()
    expect(state.filesPerSecond).toBeNull()
    expect(state.etaSecondsDisplay).toBe(58)
  })

  it('backgrounds the op via Queue without cancelling it on teardown', async () => {
    const { state, config } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent())

    state.handleQueue()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    // Backgrounding keeps the person in the main window: the queue shows, unfocused.
    expect(openQueueWindow).toHaveBeenCalledWith({ focus: false })
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(config.onQueue).toHaveBeenCalledTimes(1)

    state.destroy()
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })

  it('auto-queues when the manager admits the op behind a busy lane', async () => {
    const { state, config } = await startedState()
    if (!listeners.opsChanged) throw new Error('operations-changed subscriber never registered')
    listeners.opsChanged({ operations: [snapshot('busy', 'running'), snapshot('op-1', 'queued')] })
    flushSync()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(openQueueWindow).toHaveBeenCalledWith({ focus: false })
    expect(config.onQueue).toHaveBeenCalledTimes(1)

    state.destroy()
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it('auto-queues an operation seeded as queued, with no live snapshot at all', async () => {
    // A cold main window learns the status from `list_operations()` rather than
    // from a tick: the manager emits `operations-changed` at registration, which
    // can fire before anything is watching for it. The window's fan-out is what
    // takes that seed, once at init, so the window is reopened on it here.
    destroyOperationSessions()
    vi.mocked(listOperations).mockResolvedValue([snapshot('op-1', 'queued')])
    await initOperationSessions()
    const { config } = await startedState()
    expect(config.onQueue).toHaveBeenCalledTimes(1)
  })
})

describe('the main window smooths an ETA exactly once per operation', () => {
  // The queue window already proves this for its rows
  // (`queue/queue-row-session.svelte.test.ts`); it could not prove it here while
  // the progress dialog still built a smoother of its own. Two smoothers fed
  // identical samples from identical starting points agree, so the hazard only
  // bites when one starts later — which is exactly what a second surface
  // attaching to a transfer already in flight would do.
  it('builds one smoother however many ticks arrive', async () => {
    await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ etaSeconds: 80 }))
    listeners.progress(progressEvent({ bytesDone: 200, etaSeconds: 70 }))
    listeners.progress(progressEvent({ bytesDone: 300, etaSeconds: 60 }))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)
  })

  it('adds none of its own when another surface is already watching', async () => {
    // The corner chip, standing in: it holds the session for this operation
    // before the dialog ever binds, and the dialog must join that one rather
    // than start a second estimate beside it.
    const registry = getOperationSessions()
    if (!registry) throw new Error('the window has no session registry')
    registry.acquire('op-1')
    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)

    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ etaSeconds: 80 }))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)
    expect(state.etaSecondsDisplay).toBe(80)
    registry.release('op-1')
  })
})

describe('createTransferProgressState: disposal', () => {
  it('an unexpected teardown leaves the operation running', async () => {
    // Replaces "fires the safety-net cancel for an unexpected teardown". A view
    // going away is a DETACH now, not a command: the operation lives in the
    // backend registry, the corner chip and the queue window keep showing it,
    // and only the Cancel button asks for a cancel. Stopping a transfer because
    // the thing rendering it unmounted is the coupling this seam removes.
    const { state } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent())
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it('does not cancel a settled op on teardown', async () => {
    const { state } = await startedState()
    if (!listeners.complete) throw new Error('complete subscriber never registered')
    listeners.complete({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 1,
      filesSkipped: 0,
      bytesProcessed: 1,
    })
    flushSync()
    vi.advanceTimersByTime(450)
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it('closing the modal hands a running operation to the queue instead of stopping it', async () => {
    const { state, config } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent())

    state.detach()

    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(config.onQueue).toHaveBeenCalledTimes(1)
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })

  it('never reports a cancel for an operation that completed', async () => {
    // `dismiss()` tells the pane "cancelled", which runs a different tail over
    // its selection than a completion does. It must stay silent once the
    // operation ended some other way.
    const { state, config } = await startedState()
    if (!listeners.complete) throw new Error('complete subscriber never registered')
    listeners.complete({
      operationId: 'op-1',
      operationType: 'copy',
      filesProcessed: 3,
      filesSkipped: 0,
      bytesProcessed: 9,
    })

    state.dismiss()
    flushSync()
    vi.advanceTimersByTime(450)

    expect(config.onCancelled).not.toHaveBeenCalled()
    expect(config.onComplete).toHaveBeenCalledWith({
      filesProcessed: 3,
      filesSkipped: 0,
      bytesProcessed: 9,
      appearedDuringMove: null,
      topLevelSkipped: null,
      refused: null,
    })
  })

  it('closing the modal while a cancel winds down just stops watching', async () => {
    const { state, config } = await startedState()
    if (!listeners.progress) throw new Error('progress subscriber never registered')
    listeners.progress(progressEvent({ filesDone: 2 }))
    void state.handleCancel(false)
    await settle()

    state.detach()
    vi.advanceTimersByTime(450)

    expect(openQueueWindow).not.toHaveBeenCalled()
    expect(config.onCancelled).toHaveBeenCalledWith(2)
  })
})

describe('createTransferProgressState: a still-scanning transfer', () => {
  // The scan-wait lives in the backend's own operation task, so the dialog
  // dispatches at once and the operation waits for the preview it claimed. An
  // operation exists from the first frame, so it can be paused, backgrounded,
  // cancelled, and counted by the quit gate while it counts.

  it('dispatches immediately even with a preview still walking, and names the operation', async () => {
    const { state } = await startedState({ previewId: 'prev-1' })

    expect(copyBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(state.operationId).toBe('op-1')
    expect(state.phase).toBe('scanning')
  })

  it('offers Background while the operation is still scanning', async () => {
    // The shipped bug: `canPauseOrQueue` used to require the scan to be over,
    // so a large transfer could not be backgrounded for as long as it counted.
    // It now covers Pause during the scan as well, since the walk parks.
    const { state } = await startedState({ previewId: 'prev-1' })

    expect(state.canPauseOrQueue).toBe(true)
  })

  it('backgrounds a scanning operation to the queue', async () => {
    const { state, config } = await startedState({ previewId: 'prev-1' })

    state.handleQueue()

    expect(config.onQueue).toHaveBeenCalledTimes(1)
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it("renders the scan-phase counts from the operation's own progress stream", async () => {
    // The backend forwards the claimed preview's counts as `write-progress` in
    // `phase: 'scanning'` under the operation's id, so one branch feeds the
    // readout for both the preview and the backend's own re-scan.
    const { state } = await startedState({ previewId: 'prev-1' })
    if (!listeners.progress) throw new Error('progress subscriber never registered')

    listeners.progress({
      ...progressEvent(),
      phase: 'scanning',
      filesDone: 5,
      dirsDone: 2,
      bytesDone: 500,
      filesTotal: 0,
      bytesTotal: 0,
      currentDir: '/src',
    })

    expect(state.scan.filesFound).toBe(5)
    expect(state.scan.dirsFound).toBe(2)
    expect(state.scan.bytesFound).toBe(500)
    expect(state.scan.currentDir).toBe('/src')
  })

  it('never cancels the preview itself: the operation owns it now', async () => {
    // A dialog going away is a viewer detaching. Stopping the walk here would
    // pull the result out from under a transfer that is still queued or
    // running, which is exactly the coupling this seam exists to remove.
    const { state } = await startedState({ previewId: 'prev-1' })

    await state.handleCancel(false)
    state.destroy()

    expect(cancelScanPreview).not.toHaveBeenCalled()
    expect(cancelOperation).toHaveBeenCalledWith('op-1')
  })
})
