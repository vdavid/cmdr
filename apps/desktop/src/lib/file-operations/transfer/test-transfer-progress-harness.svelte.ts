/**
 * Shared fixture for the headless `createTransferProgressState` specs
 * (`transfer-progress-state.svelte.test.ts` and
 * `transfer-progress-state.ownership.svelte.test.ts`): the progress dialog as a
 * VIEW of one operation, driven without rendering a component.
 *
 * The view holds runes (its session binding, and the effects that watch for an
 * outcome), so each case builds it inside an `$effect.root` and disposes that
 * root afterwards — standing in for the component scope it lives in. That's why
 * this module is `.svelte.ts`.
 *
 * Mocking approach (mirrors `queue-row-session.svelte.test.ts`):
 * `#lib/tauri-commands` is fully mocked, and the window's session registry is
 * inited per test so the event fan-out subscribes through those mocks. The
 * `on<Event>` subscriber mocks capture the fan-out's callback into `listeners`;
 * calling one delivers an event down exactly the path a live one takes — fan-out,
 * session, view. The dispatch commands resolve with a fixed `operationId`;
 * per-test overrides cover the deferred-IPC and error paths.
 *
 * `listOperations` answers with this operation's row because that is what the
 * backend does: it registers the operation before the start command returns, so
 * a session seeding itself finds it. A mock that answered "no such operation"
 * would be telling the session the transfer was already over.
 *
 * How a spec uses it (the factories are dynamically imported INSIDE the `vi.mock`
 * callbacks, so import order and hoisting can't bite):
 *
 * ```ts
 * vi.mock('#lib/tauri-commands/index.js', async () => (await import('./test-transfer-progress-harness.svelte')).tauriCommandsMock())
 * // …one line per mock factory below…
 * import { createTransferProgressState } from './transfer-progress-state.svelte'
 * const { makeState, startedState } = useTransferProgressState(createTransferProgressState)
 * ```
 *
 * ⚠️ **The view factory comes IN, statically imported by the spec.** This module
 * imports only types, `vitest`, and `svelte` at the top level: the mock factories
 * import it while the view's module graph is still loading, so a static import of
 * anything that reaches `#lib/tauri-commands` would read these exports from their
 * temporal dead zone. The hooks reach the session modules through `await import`,
 * which by then resolves from the cache the spec's own imports filled.
 */

import { vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type {
  WriteProgressEvent,
  WriteCompleteEvent,
  WriteErrorEvent,
  WriteCancelledEvent,
  WriteSettledEvent,
  WriteConflictEvent,
  OperationSnapshot,
} from '#lib/tauri-commands/index.js'
import type { WriteOperationType } from '#lib/file-explorer/types.js'
import type * as ProgressReadout from '../progress-readout'
import type { createTransferProgressState, TransferProgressStateConfig } from './transfer-progress-state.svelte'

/** Callbacks the window's fan-out registers, captured so a test can deliver
 *  events at a deterministic moment. Reset before every test. */
export const listeners: {
  progress: ((e: WriteProgressEvent) => void) | null
  complete: ((e: WriteCompleteEvent) => void) | null
  error: ((e: WriteErrorEvent) => void) | null
  cancelled: ((e: WriteCancelledEvent) => void) | null
  settled: ((e: WriteSettledEvent) => void) | null
  conflict: ((e: WriteConflictEvent) => void) | null
  opsChanged: ((e: { operations: OperationSnapshot[] }) => void) | null
} = {
  progress: null,
  complete: null,
  error: null,
  cancelled: null,
  settled: null,
  conflict: null,
  opsChanged: null,
}

const noopUnlisten = () => {}

export function tauriCommandsMock() {
  return {
    copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
    moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'move' })),
    compressFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
    moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'move' })),
    deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'delete' })),
    trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'trash' })),
    // A finished trash tells Edit > Undo move to trash about itself (`delete/trash-undo.ts`).
    updateUndoTrashMenu: vi.fn(() => Promise.resolve()),
    onWriteProgress: vi.fn((cb: (e: WriteProgressEvent) => void) => {
      listeners.progress = cb
      return Promise.resolve(noopUnlisten)
    }),
    onWriteComplete: vi.fn((cb: (e: WriteCompleteEvent) => void) => {
      listeners.complete = cb
      return Promise.resolve(noopUnlisten)
    }),
    onWriteError: vi.fn((cb: (e: WriteErrorEvent) => void) => {
      listeners.error = cb
      return Promise.resolve(noopUnlisten)
    }),
    onWriteCancelled: vi.fn((cb: (e: WriteCancelledEvent) => void) => {
      listeners.cancelled = cb
      return Promise.resolve(noopUnlisten)
    }),
    onWriteSettled: vi.fn((cb: (e: WriteSettledEvent) => void) => {
      listeners.settled = cb
      return Promise.resolve(noopUnlisten)
    }),
    onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
      listeners.conflict = cb
      return Promise.resolve(noopUnlisten)
    }),
    onWriteConflictResolved: vi.fn(() => Promise.resolve(noopUnlisten)),
    onOperationsChanged: vi.fn((cb: (e: { operations: OperationSnapshot[] }) => void) => {
      listeners.opsChanged = cb
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
  }
}

export function queueWindowMock() {
  return { openQueueWindow: vi.fn(() => Promise.resolve()) }
}

export function toastMock() {
  return { addToast: vi.fn() }
}

export function settingsMock() {
  return {
    // Key-aware so the archive compression level is distinguishable from the
    // progress-interval / max-conflicts settings (all others resolve to 200).
    getSetting: vi.fn((key: string) => (key === 'behavior.archiveCompressionLevel' ? 6 : 200)),
  }
}

export function messagesMock() {
  return { tString: vi.fn((key: string) => key) }
}

/** The real smoother, watched. The session resolves this same module, so the
 *  count covers the whole main window. */
export function progressReadoutMock(actual: typeof ProgressReadout) {
  return { ...actual, createEtaSmoother: vi.fn(actual.createEtaSmoother) }
}

export function loggerMock() {
  return {
    getAppLogger: () => ({
      debug: vi.fn(),
      info: vi.fn(),
      warn: vi.fn(),
      error: vi.fn(),
    }),
  }
}

/** Drains the machine's `await` chains and then runs whatever effects they
 *  scheduled. Fake timers don't fake microtasks, so this works with timers
 *  active. */
export async function settle(): Promise<void> {
  for (let round = 0; round < 2; round++) {
    for (let i = 0; i < 25; i++) await Promise.resolve()
    flushSync()
  }
}

export function makeConfig(over: Partial<TransferProgressStateConfig> = {}): TransferProgressStateConfig {
  return {
    operationType: 'copy',
    sourcePaths: ['/src/file.txt'],
    destinationPath: '/dst',
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

export function progressEvent(over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
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

export function snapshot(
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
    rollbackSupport: 'supported',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    items: null,
  }
}

type TransferProgressState = ReturnType<typeof createTransferProgressState>

/**
 * Installs the per-test lifecycle (fresh listeners and mocks, fake timers, an
 * empty foreground slot, a freshly inited session registry) and returns the two
 * ways a spec builds a view.
 */
export function useTransferProgressState(create: typeof createTransferProgressState) {
  /** The reactive scope the view lives in. A component owns one in the app; a
   *  test owns one here, and disposing it is what releases the session. */
  let disposeScope: (() => void) | null = null

  function makeState(config: TransferProgressStateConfig): TransferProgressState {
    let created!: TransferProgressState
    disposeScope = $effect.root(() => {
      created = create(config)
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
    listeners.progress = null
    listeners.complete = null
    listeners.error = null
    listeners.cancelled = null
    listeners.settled = null
    listeners.conflict = null
    listeners.opsChanged = null
    vi.clearAllMocks()
    const { listOperations } = await import('#lib/tauri-commands/index.js')
    vi.mocked(listOperations).mockResolvedValue([snapshot('op-1', 'running')])
    vi.useFakeTimers()
    // The slot is module-scoped, so a test that leaves an owner behind would poison
    // the next one.
    const { setForegroundOperationId, isForegroundClaimPending, endForegroundClaim } =
      await import('../foreground-operation.svelte')
    setForegroundOperationId(null)
    while (isForegroundClaimPending()) endForegroundClaim()
    const { initOperationSessions } = await import('../operation-session/window-operation-sessions.svelte')
    await initOperationSessions()
    const { createEtaSmoother } = await import('../progress-readout')
    vi.mocked(createEtaSmoother).mockClear()
  })

  afterEach(async () => {
    disposeScope?.()
    disposeScope = null
    const { destroyOperationSessions } = await import('../operation-session/window-operation-sessions.svelte')
    destroyOperationSessions()
    vi.useRealTimers()
  })

  return { makeState, startedState }
}
