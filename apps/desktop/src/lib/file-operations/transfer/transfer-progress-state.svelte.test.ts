/**
 * Headless tests for `createTransferProgressState`, the transfer execution
 * state machine extracted from `TransferProgressDialog.svelte`. The whole point
 * of the extraction is to drive the machine without rendering a component, so
 * these tests instantiate the factory directly and exercise its branches by
 * invoking the captured Tauri-event callbacks with synthesised payloads.
 *
 * Mocking approach (mirrors `TransferProgressDialog.cancel-settle.test.ts` and
 * `operations-store.svelte.test.ts`): `$lib/tauri-commands` is fully mocked. The
 * `on<Event>` subscriber mocks capture the registered callback into a
 * module-level `let`; the test then calls that callback to deliver an event at a
 * deterministic moment. The dispatch commands resolve with a fixed
 * `operationId`; per-test overrides cover the deferred-IPC and error paths.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type {
  WriteProgressEvent,
  WriteCompleteEvent,
  WriteErrorEvent,
  WriteCancelledEvent,
  WriteSettledEvent,
  WriteConflictEvent,
  OperationSnapshot,
  ScanPreviewProgressEvent,
  ScanPreviewCompleteEvent,
  ScanPreviewErrorEvent,
  ScanPreviewCancelledEvent,
  WriteOperationStartResult,
} from '$lib/tauri-commands'
import type { WriteOperationError, WriteOperationType } from '$lib/file-explorer/types'

// Callbacks the machine registers, captured so the test can deliver events.
let progressCb: ((e: WriteProgressEvent) => void) | null = null
let completeCb: ((e: WriteCompleteEvent) => void) | null = null
let errorCb: ((e: WriteErrorEvent) => void) | null = null
let cancelledCb: ((e: WriteCancelledEvent) => void) | null = null
let settledCb: ((e: WriteSettledEvent) => void) | null = null
let conflictCb: ((e: WriteConflictEvent) => void) | null = null
let opsChangedCb: ((e: { operations: OperationSnapshot[] }) => void) | null = null
let scanProgressCb: ((e: ScanPreviewProgressEvent) => void) | null = null
let scanCompleteCb: ((e: ScanPreviewCompleteEvent) => void) | null = null
let scanErrorCb: ((e: ScanPreviewErrorEvent) => void) | null = null
let scanCancelledCb: ((e: ScanPreviewCancelledEvent) => void) | null = null

const noopUnlisten = () => {}

vi.mock('$lib/tauri-commands', () => ({
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'move' })),
  compressFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'move' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'delete' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'trash' })),
  onWriteProgress: vi.fn((cb: (e: WriteProgressEvent) => void) => {
    progressCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onWriteComplete: vi.fn((cb: (e: WriteCompleteEvent) => void) => {
    completeCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onWriteError: vi.fn((cb: (e: WriteErrorEvent) => void) => {
    errorCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onWriteCancelled: vi.fn((cb: (e: WriteCancelledEvent) => void) => {
    cancelledCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onWriteSettled: vi.fn((cb: (e: WriteSettledEvent) => void) => {
    settledCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
    conflictCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onOperationsChanged: vi.fn((cb: (e: { operations: OperationSnapshot[] }) => void) => {
    opsChangedCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onScanPreviewProgress: vi.fn((cb: (e: ScanPreviewProgressEvent) => void) => {
    scanProgressCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onScanPreviewComplete: vi.fn((cb: (e: ScanPreviewCompleteEvent) => void) => {
    scanCompleteCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onScanPreviewError: vi.fn((cb: (e: ScanPreviewErrorEvent) => void) => {
    scanErrorCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  onScanPreviewCancelled: vi.fn((cb: (e: ScanPreviewCancelledEvent) => void) => {
    scanCancelledCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  listOperations: vi.fn(() => Promise.resolve([])),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  // Key-aware so the archive compression level is distinguishable from the
  // progress-interval / max-conflicts settings (all others resolve to 200).
  getSetting: vi.fn((key: string) => (key === 'behavior.archiveCompressionLevel' ? 6 : 200)),
}))

vi.mock('$lib/intl/messages.svelte', () => ({
  tString: vi.fn((key: string) => key),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}))

import { createTransferProgressState, type TransferProgressStateConfig } from './transfer-progress-state.svelte'
import {
  copyBetweenVolumes,
  moveBetweenVolumes,
  compressFiles,
  moveFiles,
  deleteFiles,
  trashFiles,
  resolveWriteConflict,
  cancelWriteOperation,
  cancelScanPreview,
  checkScanPreviewStatus,
  pauseOperation,
  resumeOperation,
} from '$lib/tauri-commands'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import { addToast } from '$lib/ui/toast'

/** Drains the microtask queue so the machine's `await` chains settle. Fake
 *  timers don't fake microtasks, so this works with timers active. */
async function flushMicro(): Promise<void> {
  for (let i = 0; i < 25; i++) {
    await Promise.resolve()
  }
}

function makeConfig(over: Partial<TransferProgressStateConfig> = {}): TransferProgressStateConfig {
  return {
    operationType: 'copy',
    sourcePaths: ['/src/file.txt'],
    destinationPath: '/dst',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    conflictResolution: 'stop',
    preKnownConflicts: [],
    itemSizes: [],
    scanInProgress: false,
    onComplete: vi.fn(),
    onCancelled: vi.fn(),
    onError: vi.fn(),
    onQueue: vi.fn(),
    ...over,
  }
}

function progressEvent(over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'file.txt',
    filesDone: 1,
    filesTotal: 4,
    bytesDone: 100,
    bytesTotal: 400,
    ...over,
  }
}

function snapshot(
  id: string,
  status: OperationSnapshot['status'],
  type: WriteOperationType = 'copy',
): OperationSnapshot {
  return { operationId: id, operationType: type, status, source: '/s', destination: '/d' }
}

/** Builds the machine, runs `start()`, and drains the async startup so the
 *  operationId is seeded and listeners are registered. */
async function startedState(over: Partial<TransferProgressStateConfig> = {}) {
  const config = makeConfig(over)
  const state = createTransferProgressState(config)
  state.start()
  await flushMicro()
  return { state, config }
}

beforeEach(() => {
  progressCb = null
  completeCb = null
  errorCb = null
  cancelledCb = null
  settledCb = null
  conflictCb = null
  opsChangedCb = null
  scanProgressCb = null
  scanCompleteCb = null
  scanErrorCb = null
  scanCancelledCb = null
  vi.clearAllMocks()
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('createTransferProgressState: dispatch routing', () => {
  it('dispatches a local copy through copyBetweenVolumes', async () => {
    await startedState({ operationType: 'copy' })
    expect(copyBetweenVolumes).toHaveBeenCalledTimes(1)
  })

  it('dispatches a local move through moveFiles', async () => {
    await startedState({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    expect(moveFiles).toHaveBeenCalledTimes(1)
    expect(moveBetweenVolumes).not.toHaveBeenCalled()
  })

  it('dispatches a cross-volume move through moveBetweenVolumes', async () => {
    await startedState({ operationType: 'move', sourceVolumeId: 'mtp-1', destVolumeId: 'root' })
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('routes a move INTO a zip through moveBetweenVolumes, not the local fast-path', async () => {
    // Source and dest share the parent drive's `root` id (the zip lives on it), so
    // the volume-id comparison alone would pick `moveFiles`. The dest PATH inside a
    // `.zip` forces the cross-volume route (backend runs the archive-edit flow).
    await startedState({
      operationType: 'move',
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      sourcePaths: ['/left/file.txt'],
      destinationPath: '/left/foo.zip/inner',
    })
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('routes a move OUT of a zip through moveBetweenVolumes, not the local fast-path', async () => {
    // Extract-out move: the SOURCE path is inside a `.zip` while both ids are `root`.
    await startedState({
      operationType: 'move',
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      sourcePaths: ['/left/foo.zip/inner.txt'],
      destinationPath: '/right',
    })
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('dispatches delete through deleteFiles', async () => {
    await startedState({ operationType: 'delete' })
    expect(deleteFiles).toHaveBeenCalledTimes(1)
  })

  it('dispatches trash through trashFiles', async () => {
    await startedState({ operationType: 'trash' })
    expect(trashFiles).toHaveBeenCalledTimes(1)
  })
})

describe('createTransferProgressState: compression-level threading', () => {
  // The FE reads `behavior.archiveCompressionLevel` once at dispatch (mocked to 6)
  // and passes it in the operation config for every zip-writing path, so the
  // backend applies the chosen deflate level. Non-archive copies simply ignore it.
  it('passes the compression level to compressFiles', async () => {
    await startedState({ operationType: 'compress' })
    expect(compressFiles).toHaveBeenCalledWith(
      'root',
      ['/src/file.txt'],
      'root',
      '/dst',
      expect.objectContaining({ compressionLevel: 6 }),
    )
  })

  it('passes the compression level to copyBetweenVolumes (copy INTO an archive uses the same level)', async () => {
    await startedState({ operationType: 'copy' })
    expect(copyBetweenVolumes).toHaveBeenCalledWith(
      'root',
      ['/src/file.txt'],
      'root',
      '/dst',
      expect.objectContaining({ compressionLevel: 6 }),
    )
  })

  it('passes the compression level to moveBetweenVolumes (move INTO an archive uses the same level)', async () => {
    await startedState({ operationType: 'move', sourceVolumeId: 'mtp-1', destVolumeId: 'root' })
    expect(moveBetweenVolumes).toHaveBeenCalledWith(
      'mtp-1',
      ['/src/file.txt'],
      'root',
      '/dst',
      expect.objectContaining({ compressionLevel: 6 }),
    )
  })
})

describe('createTransferProgressState: progress + complete', () => {
  it('reflects a progress event in the exposed getters', async () => {
    const { state } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ filesDone: 2, filesTotal: 4, bytesDone: 200, bytesTotal: 400, etaSeconds: 12 }))
    expect(state.phase).toBe('copying')
    expect(state.filesDone).toBe(2)
    expect(state.bytesDone).toBe(200)
    expect(state.etaSecondsDisplay).toBe(12)
  })

  it('handles a scanning → copying phase transition and smooths the displayed ETA', async () => {
    const { state } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    // Scanning phase: tallies + current dir come through the scan-meta fields.
    progressCb(
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
    expect(state.scanFilesFound).toBe(3)
    expect(state.scanDirsFound).toBe(2)
    expect(state.scanCurrentDir).toBe('/src/sub')

    // Transition to copying: resets the smoothed ETA, then re-warms from raw.
    progressCb(progressEvent({ phase: 'copying', etaSeconds: 10 }))
    expect(state.etaSecondsDisplay).toBe(10)
    // A second copying tick smooths toward the new raw value (25% of the gap).
    progressCb(progressEvent({ phase: 'copying', etaSeconds: 20 }))
    expect(state.etaSecondsDisplay).toBeCloseTo(12.5)
  })

  it('enters rolling_back from a backend progress event', async () => {
    const { state } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ phase: 'rolling_back' }))
    expect(state.isRollingBack).toBe(true)
    expect(state.phase).toBe('rolling_back')
  })

  it('fires onComplete after the min-display window', async () => {
    const { state, config } = await startedState()
    if (!completeCb) throw new Error('complete subscriber never registered')
    completeCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 5, filesSkipped: 1, bytesProcessed: 999 })
    expect(state.operationSettled).toBe(true)
    // Min-display floor: not yet called, then called after advancing past it.
    expect(config.onComplete).not.toHaveBeenCalled()
    vi.advanceTimersByTime(450)
    expect(config.onComplete).toHaveBeenCalledWith(5, 1, 999)
  })

  it('fires onError on a write-error event', async () => {
    const { state, config } = await startedState()
    if (!errorCb) throw new Error('error subscriber never registered')
    const error: WriteOperationError = { type: 'io_error', path: '/src/file.txt', message: 'boom' }
    errorCb({ operationId: 'op-1', operationType: 'copy', error })
    expect(state.operationSettled).toBe(true)
    expect(config.onError).toHaveBeenCalledWith(error)
  })

  it('ignores events for a different operation id', async () => {
    const { state } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ operationId: 'op-other', filesDone: 99 }))
    expect(state.filesDone).toBe(0)
  })
})

describe('createTransferProgressState: event buffering and IPC races', () => {
  it('buffers events that arrive before the operationId, then replays them', async () => {
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const { state } = { state: createTransferProgressState(makeConfig()) }
    state.start()
    await flushMicro()
    // Parked on the dispatch await: operationId is still null, so a progress
    // event is buffered rather than applied.
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ filesDone: 7 }))
    expect(state.filesDone).toBe(0)

    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await flushMicro()
    // Replay applied the buffered event.
    expect(state.filesDone).toBe(7)
  })

  it('cancels and reports the op when the dialog is torn down mid-dispatch', async () => {
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const config = makeConfig()
    const state = createTransferProgressState(config)
    state.start()
    await flushMicro()
    // Cancel before the operationId arrives: marks destroyed and defers.
    void state.handleCancel(false)
    await flushMicro()
    expect(cancelWriteOperation).not.toHaveBeenCalled()

    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await flushMicro()
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)
    expect(config.onCancelled).toHaveBeenCalledWith(0)
  })

  it('routes a structured backend error through onError', async () => {
    // Tauri rejects with a structured `WriteOperationError`; model it as an
    // Error carrying the typed fields so the SUT's `'type' in err` branch hits
    // (and so we reject with an Error, per prefer-promise-reject-errors).
    const structured = Object.assign(new Error('nope'), {
      type: 'permission_denied',
      path: '/src/file.txt',
      message: 'nope',
    } satisfies WriteOperationError)
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(structured))
    const config = makeConfig()
    const state = createTransferProgressState(config)
    state.start()
    await flushMicro()
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'permission_denied' }))
  })

  it('wraps a non-structured dispatch failure as an io_error', async () => {
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(new Error('kaboom')))
    const config = makeConfig()
    const state = createTransferProgressState(config)
    state.start()
    await flushMicro()
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'io_error' }))
  })
})

describe('createTransferProgressState: conflict resolution', () => {
  function conflictEvent(): WriteConflictEvent {
    return {
      operationId: 'op-1',
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
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
    expect(state.conflictEvent).not.toBeNull()

    await state.handleConflictResolution('skip', true)
    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 'skip', true)
    expect(state.conflictEvent).toBeNull()
  })

  it('resolves a single conflict with overwrite (proceed)', async () => {
    const { state } = await startedState()
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
    await state.handleConflictResolution('overwrite', false)
    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 'overwrite', false)
    expect(state.conflictEvent).toBeNull()
  })

  it('keeps the prompt up when resolving the conflict fails', async () => {
    const { state } = await startedState()
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
    vi.mocked(resolveWriteConflict).mockImplementationOnce(() => Promise.reject(new Error('ipc down')))
    await state.handleConflictResolution('skip', false)
    // The catch path leaves the conflict unresolved and resets the in-flight flag.
    expect(state.conflictEvent).not.toBeNull()
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
    await flushMicro()
    expect(state.isCancelling).toBe(true)
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', false)

    // Slow-settle label tail appears after 200 ms.
    vi.advanceTimersByTime(200)
    expect(state.settleSlow).toBe(true)

    if (!cancelledCb || !settledCb) throw new Error('cancel/settle subscribers never registered')
    cancelledCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 4, rolledBack: false })
    expect(state.operationSettled).toBe(true)
    expect(config.onCancelled).not.toHaveBeenCalled()

    settledCb({ operationId: 'op-1', operationType: 'copy' })
    expect(state.settleSlow).toBe(false)
    vi.advanceTimersByTime(450)
    expect(config.onCancelled).toHaveBeenCalledWith(4)
  })

  it('is idempotent against a repeated cancel click', async () => {
    const { state } = await startedState()
    void state.handleCancel(false)
    await flushMicro()
    void state.handleCancel(false)
    await flushMicro()
    expect(cancelWriteOperation).toHaveBeenCalledTimes(1)
  })

  it('falls back to closing if neither terminal event arrives', async () => {
    const { state, config } = await startedState()
    void state.handleCancel(false)
    await flushMicro()
    // Last-resort fallback fires at CANCEL_SETTLE_FALLBACK_MS (10 s).
    vi.advanceTimersByTime(10_000)
    expect(config.onCancelled).toHaveBeenCalledWith(0)
    void state // keep reference
  })
})

describe('createTransferProgressState: rollback', () => {
  it('starts a rollback and closes when the cancelled event lands', async () => {
    const { state, config } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())

    void state.handleCancel(true)
    await flushMicro()
    expect(state.isRollingBack).toBe(true)
    expect(state.operationSettled).toBe(true)
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)

    if (!cancelledCb || !settledCb) throw new Error('cancel/settle subscribers never registered')
    cancelledCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 2, rolledBack: true })
    settledCb({ operationId: 'op-1', operationType: 'copy' })
    vi.advanceTimersByTime(450)
    expect(config.onCancelled).toHaveBeenCalledWith(2)
  })

  it('cancels an in-progress rollback (keep remaining files)', async () => {
    const { state } = await startedState()
    void state.handleCancel(true)
    await flushMicro()
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)

    // A plain Cancel while rolling back stops the rollback without reversing.
    void state.handleCancel(false)
    await flushMicro()
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', false)
    expect(state.isCancelling).toBe(true)
  })
})

describe('createTransferProgressState: pause, queue, and auto-queue', () => {
  it('tracks pause status from the operations-changed snapshot and toggles it', async () => {
    const { state } = await startedState()
    if (!opsChangedCb) throw new Error('operations-changed subscriber never registered')

    opsChangedCb({ operations: [snapshot('op-1', 'running')] })
    expect(state.isPaused).toBe(false)
    expect(state.canPauseOrQueue).toBe(true)

    await state.handlePauseResume()
    expect(pauseOperation).toHaveBeenCalledWith('op-1')

    opsChangedCb({ operations: [snapshot('op-1', 'paused')] })
    expect(state.isPaused).toBe(true)

    await state.handlePauseResume()
    expect(resumeOperation).toHaveBeenCalledWith('op-1')
    expect(state.pauseInFlight).toBe(false)
  })

  it('backgrounds the op via Queue without cancelling it on teardown', async () => {
    const { state, config } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())

    state.handleQueue()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(config.onQueue).toHaveBeenCalledTimes(1)

    // A backgrounded op must survive disposal: no safety-net cancel.
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })

  it('auto-queues when the manager admits the op behind a busy lane', async () => {
    const { state, config } = await startedState()
    if (!opsChangedCb) throw new Error('operations-changed subscriber never registered')
    opsChangedCb({ operations: [snapshot('busy', 'running'), snapshot('op-1', 'queued')] })
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(config.onQueue).toHaveBeenCalledTimes(1)

    // Already backgrounded: disposal leaves the op running.
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })
})

describe('createTransferProgressState: disposal', () => {
  it('fires the safety-net cancel for an unexpected teardown', async () => {
    const { state } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())
    state.destroy()
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', false)
  })

  it('does not cancel a settled op on teardown', async () => {
    const { state } = await startedState()
    if (!completeCb) throw new Error('complete subscriber never registered')
    completeCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 1, filesSkipped: 0, bytesProcessed: 1 })
    vi.advanceTimersByTime(450)
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })
})

describe('createTransferProgressState: scan-wait path', () => {
  it('waits for the scan to complete, then dispatches the operation', async () => {
    const { state } = await startedState({ scanInProgress: true, previewId: 'prev-1' })
    expect(state.waitingForScan).toBe(true)
    expect(copyBetweenVolumes).not.toHaveBeenCalled()

    if (!scanProgressCb || !scanCompleteCb) throw new Error('scan subscribers never registered')
    vi.advanceTimersByTime(50)
    scanProgressCb({
      previewId: 'prev-1',
      filesFound: 5,
      dirsFound: 2,
      bytesFound: 500,
      currentPath: 'file',
      currentDir: '/src',
    })
    expect(state.scanFilesFound).toBe(5)
    expect(state.scanDirsFound).toBe(2)

    scanCompleteCb({ previewId: 'prev-1', filesTotal: 10, dirsTotal: 3, bytesTotal: 1000, dedupBytesTotal: 1000 })
    await flushMicro()
    expect(state.waitingForScan).toBe(false)
    expect(copyBetweenVolumes).toHaveBeenCalledTimes(1)
  })

  it('starts immediately when the scan already completed (status check wins)', async () => {
    vi.mocked(checkScanPreviewStatus).mockResolvedValueOnce({
      filesTotal: 10,
      dirsTotal: 3,
      bytesTotal: 1000,
      dedupBytesTotal: 1000,
    })
    const { state } = await startedState({ scanInProgress: true, previewId: 'prev-1' })
    expect(state.waitingForScan).toBe(false)
    expect(copyBetweenVolumes).toHaveBeenCalledTimes(1)
  })

  it('reports a scan error through onError without dispatching', async () => {
    const { config } = await startedState({ scanInProgress: true, previewId: 'prev-1' })
    if (!scanErrorCb) throw new Error('scan-error subscriber never registered')
    scanErrorCb({ previewId: 'prev-1', message: 'disk gone' })
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'io_error' }))
    expect(copyBetweenVolumes).not.toHaveBeenCalled()
  })

  it('reports a scan cancellation through onCancelled', async () => {
    const { config } = await startedState({ scanInProgress: true, previewId: 'prev-1' })
    if (!scanCancelledCb) throw new Error('scan-cancelled subscriber never registered')
    scanCancelledCb({ previewId: 'prev-1' })
    expect(config.onCancelled).toHaveBeenCalledWith(0)
  })

  it('cancels the scan preview when the user cancels during the wait', async () => {
    const { state, config } = await startedState({ scanInProgress: true, previewId: 'prev-1' })
    expect(state.waitingForScan).toBe(true)
    await state.handleCancel(false)
    expect(cancelScanPreview).toHaveBeenCalledWith('prev-1')
    expect(config.onCancelled).toHaveBeenCalledWith(0)
  })

  it('cancels the scan preview on disposal during the wait', async () => {
    const { state } = await startedState({ scanInProgress: true, previewId: 'prev-1' })
    state.destroy()
    expect(cancelScanPreview).toHaveBeenCalledWith('prev-1')
  })
})
