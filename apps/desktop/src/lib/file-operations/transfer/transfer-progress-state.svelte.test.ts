/**
 * Headless tests for `createTransferProgressState`: the progress dialog as a
 * VIEW of one operation, driven without rendering a component.
 *
 * The view holds runes (its session binding, and the effects that watch for an
 * outcome), so each case builds it inside an `$effect.root` and disposes that
 * root afterwards — standing in for the component scope it lives in.
 *
 * Mocking approach (mirrors `queue-row-session.svelte.test.ts`):
 * `$lib/tauri-commands` is fully mocked, and the window's session registry is
 * inited per test so the event fan-out subscribes through those mocks. The
 * `on<Event>` subscriber mocks capture the fan-out's callback into a
 * module-level `let`; calling it delivers an event down exactly the path a live
 * one takes — fan-out, session, view. The dispatch commands resolve with a fixed
 * `operationId`; per-test overrides cover the deferred-IPC and error paths.
 *
 * `listOperations` answers with this operation's row because that is what the
 * backend does: it registers the operation before the start command returns, so
 * a session seeding itself finds it. A mock that answered "no such operation"
 * would be telling the session the transfer was already over.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type {
  WriteProgressEvent,
  WriteCompleteEvent,
  WriteErrorEvent,
  WriteCancelledEvent,
  WriteSettledEvent,
  WriteConflictEvent,
  OperationSnapshot,
  WriteOperationStartResult,
} from '$lib/tauri-commands'
import type { WriteOperationError, WriteOperationType } from '$lib/file-explorer/types'

// Callbacks the window's fan-out registers, captured so the test can deliver
// events at a deterministic moment.
let progressCb: ((e: WriteProgressEvent) => void) | null = null
let completeCb: ((e: WriteCompleteEvent) => void) | null = null
let errorCb: ((e: WriteErrorEvent) => void) | null = null
let cancelledCb: ((e: WriteCancelledEvent) => void) | null = null
let settledCb: ((e: WriteSettledEvent) => void) | null = null
let conflictCb: ((e: WriteConflictEvent) => void) | null = null
let opsChangedCb: ((e: { operations: OperationSnapshot[] }) => void) | null = null

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
  onWriteConflictResolved: vi.fn(() => Promise.resolve(noopUnlisten)),
  onOperationsChanged: vi.fn((cb: (e: { operations: OperationSnapshot[] }) => void) => {
    opsChangedCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  listOperations: vi.fn(() => Promise.resolve<OperationSnapshot[]>([])),
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

// The real smoother, watched. The session resolves this same module, so the
// count covers the whole main window.
vi.mock('../progress-readout', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../progress-readout')>()
  return { ...actual, createEtaSmoother: vi.fn(actual.createEtaSmoother) }
})

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
import { createEtaSmoother } from '../progress-readout'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from '../operation-session/window-operation-sessions.svelte'
import {
  getForegroundOperationId,
  setForegroundOperationId,
  isForegroundClaimPending,
  endForegroundClaim,
} from '../foreground-operation.svelte'

/** Drains the machine's `await` chains and then runs whatever effects they
 *  scheduled. Fake timers don't fake microtasks, so this works with timers
 *  active. */
async function settle(): Promise<void> {
  for (let round = 0; round < 2; round++) {
    for (let i = 0; i < 25; i++) await Promise.resolve()
    flushSync()
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
  return {
    operationId: id,
    operationType: type,
    status,
    source: '/s',
    destination: '/d',
    supportsRollback: true,
    error: null,
  }
}

/** The reactive scope the view lives in. A component owns one in the app; a
 *  test owns one here, and disposing it is what releases the session. */
let disposeScope: (() => void) | null = null

function makeState(config: TransferProgressStateConfig): ReturnType<typeof createTransferProgressState> {
  let created!: ReturnType<typeof createTransferProgressState>
  disposeScope = $effect.root(() => {
    created = createTransferProgressState(config)
  })
  return created
}

/** Builds the view, runs `start()`, and drains the async startup so the
 *  operation is named and its session bound. */
async function startedState(over: Partial<TransferProgressStateConfig> = {}) {
  const config = makeConfig(over)
  const state = makeState(config)
  state.start()
  await settle()
  return { state, config }
}

beforeEach(async () => {
  progressCb = null
  completeCb = null
  errorCb = null
  cancelledCb = null
  settledCb = null
  conflictCb = null
  opsChangedCb = null
  vi.clearAllMocks()
  vi.mocked(listOperations).mockResolvedValue([snapshot('op-1', 'running')])
  vi.useFakeTimers()
  // The slot is module-scoped, so a test that leaves an owner behind would poison
  // the next one.
  setForegroundOperationId(null)
  while (isForegroundClaimPending()) endForegroundClaim()
  await initOperationSessions()
  vi.mocked(createEtaSmoother).mockClear()
})

afterEach(() => {
  disposeScope?.()
  disposeScope = null
  destroyOperationSessions()
  vi.useRealTimers()
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

  it('opens in the scan phase, before the operation has said anything', async () => {
    // A confirmed transfer starts by counting, so the dialog shows the scan
    // readout rather than a meaningless 0% while it waits for the first tick.
    const { state } = await startedState({ previewId: 'prev-1' })
    expect(state.phase).toBe('scanning')
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
    expect(state.scan.filesFound).toBe(3)
    expect(state.scan.dirsFound).toBe(2)
    expect(state.scan.currentDir).toBe('/src/sub')

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
    flushSync()
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
    flushSync()
    expect(config.onError).toHaveBeenCalledWith(error)
  })

  it('ignores events for a different operation id', async () => {
    const { state } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ operationId: 'op-other', filesDone: 99 }))
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
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ filesDone: 7 }))
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
    } satisfies WriteOperationError)
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(structured))
    const config = makeConfig()
    const state = makeState(config)
    state.start()
    await settle()
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'permission_denied' }))
  })

  it('wraps a non-structured dispatch failure as an io_error', async () => {
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(new Error('kaboom')))
    const config = makeConfig()
    const state = makeState(config)
    state.start()
    await settle()
    expect(config.onError).toHaveBeenCalledWith(expect.objectContaining({ type: 'io_error' }))
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
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
    expect(state.conflict).not.toBeNull()

    await state.handleConflictResolution('skip', true)
    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 1, 'skip', true)
    expect(state.conflict).toBeNull()
  })

  it('resolves a single conflict with overwrite (proceed)', async () => {
    const { state } = await startedState()
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
    await state.handleConflictResolution('overwrite', false)
    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 1, 'overwrite', false)
    expect(state.conflict).toBeNull()
  })

  it('clears the prompt when another surface answered the same conflict first', async () => {
    // Two surfaces can render one clash; the backend arbitrates and reports its
    // verdict. Being the second one is not a failure, so the dialog stops asking
    // rather than leaving the question on screen.
    const { state } = await startedState()
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
    vi.mocked(resolveWriteConflict).mockImplementationOnce(() => Promise.resolve('already_resolved'))
    await state.handleConflictResolution('overwrite', false)
    expect(state.conflict).toBeNull()
    expect(state.isResolvingConflict).toBe(false)
  })

  it('keeps the prompt up when the answer never lands', async () => {
    const { state } = await startedState()
    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb(conflictEvent())
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

    if (!cancelledCb || !settledCb) throw new Error('cancel/settle subscribers never registered')
    cancelledCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 4, rolledBack: false })
    flushSync()
    expect(state.operationSettled).toBe(true)
    expect(config.onCancelled).not.toHaveBeenCalled()

    settledCb({ operationId: 'op-1', operationType: 'copy' })
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
    void state // keep reference
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
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ filesDone: 3 }))
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
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())

    void state.handleCancel(true)
    await settle()
    expect(state.isRollingBack).toBe(true)
    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)

    if (!cancelledCb || !settledCb) throw new Error('cancel/settle subscribers never registered')
    cancelledCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 2, rolledBack: true })
    settledCb({ operationId: 'op-1', operationType: 'copy' })
    flushSync()
    vi.advanceTimersByTime(450)
    expect(config.onCancelled).toHaveBeenCalledWith(2)
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

  it('shows no speed but keeps the time left while paused, like every other view of the op', async () => {
    const { state } = await startedState()
    if (!progressCb || !opsChangedCb) throw new Error('progress/operations-changed subscribers never registered')

    opsChangedCb({ operations: [snapshot('op-1', 'running')] })
    progressCb(progressEvent({ bytesPerSecond: 4096, filesPerSecond: 1905, etaSeconds: 58 }))
    expect(state.bytesPerSecond).toBe(4096)
    expect(state.filesPerSecond).toBe(1905)
    expect(state.etaSecondsDisplay).toBe(58)

    // The queue row for this same operation drops the same two numbers and
    // keeps the same third: a speed over a parked transfer is invented, while
    // how much longer it has left is what the user paused to think about.
    opsChangedCb({ operations: [snapshot('op-1', 'paused')] })
    expect(state.bytesPerSecond).toBeNull()
    expect(state.filesPerSecond).toBeNull()
    expect(state.etaSecondsDisplay).toBe(58)
  })

  it('backgrounds the op via Queue without cancelling it on teardown', async () => {
    const { state, config } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())

    state.handleQueue()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(config.onQueue).toHaveBeenCalledTimes(1)

    state.destroy()
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })

  it('auto-queues when the manager admits the op behind a busy lane', async () => {
    const { state, config } = await startedState()
    if (!opsChangedCb) throw new Error('operations-changed subscriber never registered')
    opsChangedCb({ operations: [snapshot('busy', 'running'), snapshot('op-1', 'queued')] })
    flushSync()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
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
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ etaSeconds: 80 }))
    progressCb(progressEvent({ bytesDone: 200, etaSeconds: 70 }))
    progressCb(progressEvent({ bytesDone: 300, etaSeconds: 60 }))

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
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ etaSeconds: 80 }))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)
    expect(state.etaSecondsDisplay).toBe(80)
    registry.release('op-1')
  })
})

describe('createTransferProgressState: foreground-operation ownership', () => {
  // The slot tells ambient surfaces (the corner chip, the failure notice) which
  // operation the modal is already showing in full. It has to empty on EVERY
  // route out of the dialog, and it has to empty at the moment Queue hands the
  // operation over — that's precisely when the chip must start speaking.
  it('claims the slot once the operation id lands', async () => {
    await startedState()
    expect(getForegroundOperationId()).toBe('op-1')
  })

  it('never claims the slot when the dialog is torn down before the id arrives', async () => {
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const state = makeState(makeConfig())
    state.start()
    await settle()
    state.destroy()
    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await settle()
    expect(getForegroundOperationId()).toBeNull()
  })

  it('releases the slot on Queue, so the corner can pick the operation up', async () => {
    const { state } = await startedState()
    state.handleQueue()
    expect(getForegroundOperationId()).toBeNull()
  })

  it('releases the slot when the manager auto-queues the op behind a busy lane', async () => {
    await startedState()
    if (!opsChangedCb) throw new Error('operations-changed subscriber never registered')
    opsChangedCb({ operations: [snapshot('busy', 'running'), snapshot('op-1', 'queued')] })
    flushSync()
    expect(getForegroundOperationId()).toBeNull()
  })

  it('releases the slot when the dialog unmounts after completing', async () => {
    const { state } = await startedState()
    if (!completeCb) throw new Error('complete subscriber never registered')
    completeCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 1, filesSkipped: 0, bytesProcessed: 1 })
    flushSync()
    vi.advanceTimersByTime(450)
    state.destroy()
    expect(getForegroundOperationId()).toBeNull()
  })

  it('releases the slot when the dialog unmounts after a cancel', async () => {
    const { state } = await startedState()
    void state.handleCancel(false)
    await settle()
    if (!cancelledCb || !settledCb) throw new Error('cancel subscribers never registered')
    cancelledCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 0, rolledBack: false })
    settledCb({ operationId: 'op-1', operationType: 'copy' })
    state.destroy()
    expect(getForegroundOperationId()).toBeNull()
  })

  it('releases the slot when the dialog unmounts after an error', async () => {
    const { state } = await startedState()
    if (!errorCb) throw new Error('error subscriber never registered')
    errorCb({
      operationId: 'op-1',
      operationType: 'copy',
      error: { type: 'io_error', path: '/src/file.txt', message: 'boom' },
    })
    flushSync()
    state.destroy()
    expect(getForegroundOperationId()).toBeNull()
  })

  it('flags a claim while the dispatch is in flight, and settles it with the id', async () => {
    // The conflict host defers its ownership decision while this is up: a
    // `write-conflict` can beat the start command's response, and deciding
    // against an empty slot would prompt for an operation the modal owns.
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const state = makeState(makeConfig())
    state.start()
    await settle()

    expect(isForegroundClaimPending()).toBe(true)
    expect(getForegroundOperationId()).toBeNull()

    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await settle()

    expect(isForegroundClaimPending()).toBe(false)
    expect(getForegroundOperationId()).toBe('op-1')
    state.destroy()
  })

  it('settles the claim when the dispatch itself never succeeds', async () => {
    // Nothing is ever going to own this operation, so a deferred conflict must
    // stop waiting on it rather than sit there forever.
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(new Error('ipc down')))
    const state = makeState(makeConfig())
    state.start()
    await settle()

    expect(isForegroundClaimPending()).toBe(false)
    state.destroy()
  })

  it('settles the claim when the dialog is torn down before the id arrives', async () => {
    let resolveDispatch: (r: WriteOperationStartResult) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () => new Promise<WriteOperationStartResult>((res) => (resolveDispatch = res)),
    )
    const state = makeState(makeConfig())
    state.start()
    await settle()
    state.destroy()
    resolveDispatch({ operationId: 'op-1', operationType: 'copy' })
    await settle()

    expect(isForegroundClaimPending()).toBe(false)
  })

  it('a late teardown does not release the slot the next dialog claimed', async () => {
    const { state } = await startedState()
    // The next operation's dialog mounts and claims the slot before this one
    // finishes tearing down.
    setForegroundOperationId('op-2')
    state.destroy()
    expect(getForegroundOperationId()).toBe('op-2')
  })
})

describe('createTransferProgressState: adopting a running operation', () => {
  // Foreground from the queue: the view binds an operation that started
  // somewhere else and dispatches nothing. Everything on screen comes from the
  // session, which is the same session every other surface in this window reads.

  /** The adopted operation is deliberately NOT the id the dispatch mock hands
   *  back: a view that quietly dispatched would land on `op-1`, so every
   *  assertion below would pass for the wrong reason. */
  const ADOPTED = 'op-9'

  /** Builds an adopting view and drains its startup, as `startedState` does for
   *  a dispatching one. */
  async function adoptedState(id = ADOPTED) {
    vi.mocked(listOperations).mockResolvedValue([snapshot(id, 'running')])
    const config = makeConfig({ adoptOperationId: id })
    const state = makeState(config)
    state.start()
    await settle()
    return { state, config }
  }

  it('leaves the operation alone when it closes before the session takes hold', async () => {
    // The sliver between the id landing and the binder's effect flushing: no
    // click can reach it, but a teardown can. Reporting a cancel from here
    // would run the pane tail over an operation that is still copying, so the
    // detach does what its name says and stops watching. Same refusal
    // `handleCancel` makes with no session to command.
    vi.mocked(listOperations).mockResolvedValue([snapshot(ADOPTED, 'running')])
    const config = makeConfig({ adoptOperationId: ADOPTED })
    const state = makeState(config)
    // ❌ Nothing may `await` between these two lines: the whole point is the
    // frame where `operationId` is set and `bound.current` is still null.
    state.start()
    state.detach()
    vi.advanceTimersByTime(450)

    expect(config.onCancelled).not.toHaveBeenCalled()
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    await settle()
  })

  it('binds the named operation without starting a new one', async () => {
    const { state } = await adoptedState()

    expect(copyBetweenVolumes).not.toHaveBeenCalled()
    expect(state.operationId).toBe(ADOPTED)
  })

  it('shows the live progress of the operation it adopted', async () => {
    const { state } = await adoptedState()
    if (!progressCb) throw new Error('progress subscriber never registered')

    progressCb(
      progressEvent({
        operationId: ADOPTED,
        phase: 'copying',
        filesDone: 7,
        filesTotal: 10,
        bytesDone: 700,
        bytesTotal: 1000,
      }),
    )

    expect(state.phase).toBe('copying')
    expect(state.filesDone).toBe(7)
    expect(state.bytesDone).toBe(700)
  })

  it('joins the session another surface already holds rather than estimating twice', async () => {
    // The whole reason the registry exists: a smoother started twenty minutes
    // in disagrees with the queue's for as long as it takes to converge. This is
    // the ordinary case for adoption — the corner chip is already watching.
    const registry = getOperationSessions()
    if (!registry) throw new Error('the window has no session registry')
    registry.acquire(ADOPTED)
    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)

    const { state } = await adoptedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ operationId: ADOPTED, etaSeconds: 80 }))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)
    expect(state.etaSecondsDisplay).toBe(80)
    registry.release(ADOPTED)
  })

  it('claims the foreground slot, so ambient surfaces stop repeating it', async () => {
    await adoptedState()
    expect(getForegroundOperationId()).toBe(ADOPTED)
  })

  it('hands the operation back, still running, when the view closes again', async () => {
    const { state, config } = await adoptedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ operationId: ADOPTED }))

    state.detach()

    expect(config.onQueue).toHaveBeenCalledTimes(1)
    expect(getForegroundOperationId()).toBeNull()
    state.destroy()
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
  })

  it('keeps showing an operation the manager reports as queued', async () => {
    // Auto-queue is a decision a DISPATCHING view makes: don't stack a second
    // modal over the one already up. A view that was opened precisely to watch
    // this operation would instead bounce it straight back out of sight.
    vi.mocked(listOperations).mockResolvedValue([snapshot(ADOPTED, 'queued')])
    const config = makeConfig({ adoptOperationId: ADOPTED })
    const state = makeState(config)
    state.start()
    await settle()
    flushSync()

    expect(state.operationId).toBe(ADOPTED)
    expect(config.onQueue).not.toHaveBeenCalled()
  })

  it("says nothing about a phase it hasn't heard, rather than inventing the scan", async () => {
    // A dispatching view opens on `scanning`, because that is what a confirmed
    // transfer is about to do. An adopted operation could be anywhere, and a
    // window that has heard nothing (a reload, with the operation paused so no
    // tick is coming) would otherwise title a 21%-written copy "Verifying before
    // copy…" over an empty scan readout.
    const { state } = await adoptedState()

    expect(state.phase).toBeNull()

    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ operationId: ADOPTED, phase: 'copying' }))
    expect(state.phase).toBe('copying')
  })

  it('offers Rollback only where the operation says it can be reversed', async () => {
    // The snapshot is the authority: this view has no birth context to reason
    // about volumes from, and `supportsRollback` is a promise about the
    // operation itself.
    const { state } = await adoptedState()
    if (!opsChangedCb) throw new Error('operations-changed subscriber never registered')

    opsChangedCb({ operations: [{ ...snapshot(ADOPTED, 'running'), supportsRollback: false }] })
    expect(state.rollbackUnavailable).toBe(true)

    opsChangedCb({ operations: [snapshot(ADOPTED, 'running')] })
    expect(state.rollbackUnavailable).toBe(false)
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
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it('does not cancel a settled op on teardown', async () => {
    const { state } = await startedState()
    if (!completeCb) throw new Error('complete subscriber never registered')
    completeCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 1, filesSkipped: 0, bytesProcessed: 1 })
    flushSync()
    vi.advanceTimersByTime(450)
    state.destroy()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it('closing the modal hands a running operation to the queue instead of stopping it', async () => {
    const { state, config } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent())

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
    if (!completeCb) throw new Error('complete subscriber never registered')
    completeCb({ operationId: 'op-1', operationType: 'copy', filesProcessed: 3, filesSkipped: 0, bytesProcessed: 9 })

    state.dismiss()
    flushSync()
    vi.advanceTimersByTime(450)

    expect(config.onCancelled).not.toHaveBeenCalled()
    expect(config.onComplete).toHaveBeenCalledWith(3, 0, 9)
  })

  it('closing the modal while a cancel winds down just stops watching', async () => {
    const { state, config } = await startedState()
    if (!progressCb) throw new Error('progress subscriber never registered')
    progressCb(progressEvent({ filesDone: 2 }))
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
    // Pause is a separate question and stays HIDDEN during a scan (the markup
    // gates it on `!isScanning`, and the backend declines the flip anyway).
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
    if (!progressCb) throw new Error('progress subscriber never registered')

    progressCb({
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
