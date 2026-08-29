/**
 * Two-condition cancel close: the dialog must stay in "Canceling…" until
 * both `write-cancelled` AND `write-settled` have arrived. Reversing order,
 * or closing on either alone, is wrong. See `lib/file-operations/CLAUDE.md`
 * § "Cancel close" for the contract.
 *
 * The dialog is a VIEW of its operation, so each test inits the window's
 * session registry: that is what subscribes the event fan-out (through the
 * mocked helpers below) and what a mounted dialog binds to. The test then drives
 * the dialog by invoking the captured callbacks with synthesised events,
 * asserting which `onCancelled` / `onComplete` / `onError` prop callback fires
 * and when.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot, WriteCancelledEvent, WriteSettledEvent, WriteCompleteEvent } from '$lib/tauri-commands'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
// Import the SUT statically so the linter sees a real source dependency.
// The dynamic import inside `mountDialog` is kept for the per-test reset
// pattern that vitest's module mocking expects.
import TransferProgressDialogStatic from './TransferProgressDialog.svelte'

// Capture the callbacks the dialog registers so the test can dispatch events
// at deterministic moments. Only `completeCb`, `cancelledCb`, and `settledCb`
// are exercised; the other two are captured so the mocks resolve normally.
let completeCb: ((e: WriteCompleteEvent) => void) | null = null
let cancelledCb: ((e: WriteCancelledEvent) => void) | null = null
let settledCb: ((e: WriteSettledEvent) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn((cb: (e: WriteCompleteEvent) => void) => {
    completeCb = cb
    return Promise.resolve(() => {
      completeCb = null
    })
  }),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn((cb: (e: WriteCancelledEvent) => void) => {
    cancelledCb = cb
    return Promise.resolve(() => {
      cancelledCb = null
    })
  }),
  onWriteSettled: vi.fn((cb: (e: WriteSettledEvent) => void) => {
    settledCb = cb
    return Promise.resolve(() => {
      settledCb = null
    })
  }),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  // The backend registers the operation before the start command returns, so a
  // session that seeds itself finds it. Answering with an empty list would tell
  // the session the transfer was already over.
  listOperations: vi.fn(() =>
    Promise.resolve<OperationSnapshot[]>([
      {
        operationId: 'op-1',
        operationType: 'delete',
        status: 'running',
        source: '/Users/test',
        destination: null,
        supportsRollback: false,
        error: null,
      },
    ]),
  ),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'mtp-1', name: 'Pixel', path: 'mtp://pixel/', category: 'mtp', isEjectable: true },
  ],
}))

async function flushPromises(): Promise<void> {
  // Drain the microtask + setTimeout-0 queue. svelte mount triggers async
  // work in onMount; we need multiple drains because each await chains.
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

beforeEach(async () => {
  // Reset the captured callbacks BEFORE the fan-out subscribes: the registry is
  // what subscribes it, and it has to be up before the dialog mounts, because a
  // view binds to a session rather than listening for itself.
  completeCb = null
  cancelledCb = null
  settledCb = null
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

async function mountDialog(): Promise<{
  component: ReturnType<typeof mount>
  onComplete: ReturnType<typeof vi.fn>
  onCancelled: ReturnType<typeof vi.fn>
  onError: ReturnType<typeof vi.fn>
  target: HTMLDivElement
}> {
  // The static `TransferProgressDialogStatic` import is what we mount.
  const TransferProgressDialog = TransferProgressDialogStatic
  const onComplete = vi.fn()
  const onCancelled = vi.fn()
  const onError = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      operationType: 'delete',
      sourcePaths: ['/Users/test/file.txt'],
      sourceFolderPath: '/Users/test',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: 'mtp-1',
      onComplete,
      onCancelled,
      onError,
    },
  })
  // Allow onMount → startOperation → all `await onWrite*` calls to resolve.
  await flushPromises()
  return { component, onComplete, onCancelled, onError, target }
}

describe('TransferProgressDialog cancel-settle gate', () => {
  it('stays in "Canceling…" until both write-cancelled and write-settled arrive', async () => {
    const { component, onCancelled, target } = await mountDialog()
    vi.useFakeTimers({ shouldAdvanceTime: false })
    try {
      expect(cancelledCb, 'onWriteCancelled subscriber registered').toBeTruthy()
      expect(settledCb, 'onWriteSettled subscriber registered').toBeTruthy()
      if (!cancelledCb || !settledCb) throw new Error('subscribers never registered')

      // The op is now running (mock returned operationId synchronously).
      // Fire write-cancelled first. The dialog must NOT call onCancelled yet.
      cancelledCb({
        operationId: 'op-1',
        operationType: 'delete',
        filesProcessed: 3,
        rolledBack: false,
      })
      await tick()
      expect(onCancelled, 'onCancelled must not fire on write-cancelled alone').not.toHaveBeenCalled()

      // Title should now reflect canceling state.
      expect(target.textContent).toContain('Canceling')

      // Now fire write-settled. The dialog applies MIN_DISPLAY_MS (400 ms)
      // gate; advance time past it.
      settledCb({ operationId: 'op-1', operationType: 'delete', volumeId: 'mtp-1' })
      await tick()
      // Advance fake timers past MIN_DISPLAY_MS.
      vi.advanceTimersByTime(450)
      await tick()
      expect(onCancelled, 'onCancelled fires after settle + min-display gate').toHaveBeenCalledTimes(1)
      expect(onCancelled).toHaveBeenCalledWith(3)

      void unmount(component)
    } finally {
      vi.useRealTimers()
    }
  })

  it('handles write-settled arriving before write-cancelled (defensive ordering)', async () => {
    const { component, onCancelled } = await mountDialog()
    vi.useFakeTimers({ shouldAdvanceTime: false })
    try {
      if (!cancelledCb || !settledCb) throw new Error('subscribers never registered')
      // Settle arrives first (shouldn't happen in production, but be safe).
      settledCb({ operationId: 'op-1', operationType: 'delete', volumeId: 'mtp-1' })
      await tick()
      expect(onCancelled, 'onCancelled must not fire on settle alone').not.toHaveBeenCalled()

      // Cancel arrives.
      cancelledCb({
        operationId: 'op-1',
        operationType: 'delete',
        filesProcessed: 7,
        rolledBack: false,
      })
      await tick()
      vi.advanceTimersByTime(450)
      await tick()
      expect(onCancelled, 'onCancelled fires once both events have landed').toHaveBeenCalledTimes(1)
      expect(onCancelled).toHaveBeenCalledWith(7)

      void unmount(component)
    } finally {
      vi.useRealTimers()
    }
  })

  it('write-complete path closes normally without waiting for settle', async () => {
    const { component, onComplete } = await mountDialog()
    vi.useFakeTimers({ shouldAdvanceTime: false })
    try {
      if (!completeCb) throw new Error('subscribers never registered')
      completeCb({
        operationId: 'op-1',
        operationType: 'delete',
        filesProcessed: 5,
        filesSkipped: 0,
        bytesProcessed: 1234,
      })
      await tick()
      vi.advanceTimersByTime(450)
      await tick()
      expect(onComplete, 'complete path closes on min-display gate, no settle wait').toHaveBeenCalledTimes(1)
      expect(onComplete).toHaveBeenCalledWith({ filesProcessed: 5, filesSkipped: 0, bytesProcessed: 1234 })
      void unmount(component)
    } finally {
      vi.useRealTimers()
    }
  })

  it('event filterEvent rejects events for a different operation id', async () => {
    const { component, onCancelled } = await mountDialog()
    vi.useFakeTimers({ shouldAdvanceTime: false })
    try {
      if (!cancelledCb || !settledCb) throw new Error('subscribers never registered')
      // Different operation id: must be ignored.
      cancelledCb({
        operationId: 'op-other',
        operationType: 'delete',
        filesProcessed: 99,
        rolledBack: false,
      })
      settledCb({ operationId: 'op-other', operationType: 'delete', volumeId: 'mtp-1' })
      await tick()
      vi.advanceTimersByTime(450)
      await tick()
      expect(onCancelled, 'foreign op id must not close this dialog').not.toHaveBeenCalled()
      void unmount(component)
    } finally {
      vi.useRealTimers()
    }
  })
})

/**
 * M3.4: the dialog must ALWAYS be dismissable, whatever the backend is doing.
 *
 * The settle gate above is the right default — it buys honest numbers — but it
 * must never be the only way out. In the 2026-07-31 incident the window
 * wouldn't close and force-quit was the only option left, which is what turned
 * a recoverable stall into data loss.
 */
describe('TransferProgressDialog is always dismissable', () => {
  it('offers a way out while waiting for the backend to settle, and takes it immediately', async () => {
    const { component, onCancelled, target } = await mountDialog()
    vi.useFakeTimers({ shouldAdvanceTime: false })
    try {
      if (!cancelledCb) throw new Error('subscribers never registered')

      // Cancel, then have the backend go quiet: `write-settled` never arrives.
      cancelledCb({
        operationId: 'op-1',
        operationType: 'delete',
        filesProcessed: 7,
        rolledBack: false,
      })
      await tick()
      expect(onCancelled, 'still waiting on settle').not.toHaveBeenCalled()

      const closeButton = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Close'))
      if (!closeButton) throw new Error('a Close affordance must exist while the backend is silent')

      closeButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      await tick()
      vi.advanceTimersByTime(450)
      await tick()

      // Closes on the user's say-so, reporting what the backend did tell us.
      expect(onCancelled, 'the user must not have to wait for the backend').toHaveBeenCalledTimes(1)
      expect(onCancelled).toHaveBeenCalledWith(7)

      void unmount(component)
    } finally {
      vi.useRealTimers()
    }
  })
})
