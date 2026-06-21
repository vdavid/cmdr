/**
 * Queue controls on `TransferProgressDialog.svelte`: Pause/Resume, Queue (send to
 * background), the dialog-scoped F2 → Queue, and auto-queue surfacing.
 *
 * The dialog learns its lifecycle status (running/paused/queued) from the
 * manager's `operations-changed` snapshot, NOT from `write-progress`. We capture
 * the registered `operations-changed` callback and drive it with synthesised
 * snapshots to flip status, exactly as the backend would.
 *
 * `openQueueWindow` and the toast system are mocked so we can assert the
 * background → window handoff without a live Tauri runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import TransferProgressDialog from './TransferProgressDialog.svelte'

// Capture the operations-changed callback so the test can flip this op's status.
let operationsChangedCb: ((event: { operations: OperationSnapshot[] }) => void) | null = null
// Hoisted so the `vi.mock` factory (lifted to the top of the file) can reference
// these. Plain `const`s declared here would be in the temporal dead zone when the
// hoisted factory runs.
const { pauseOperationMock, resumeOperationMock, cancelWriteOperationMock, listOperationsMock } = vi.hoisted(() => ({
  pauseOperationMock: vi.fn(() => Promise.resolve()),
  resumeOperationMock: vi.fn(() => Promise.resolve()),
  cancelWriteOperationMock: vi.fn(() => Promise.resolve()),
  listOperationsMock: vi.fn(() => Promise.resolve<OperationSnapshot[]>([])),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: cancelWriteOperationMock,
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  pauseOperation: pauseOperationMock,
  resumeOperation: resumeOperationMock,
  onOperationsChanged: vi.fn((cb: (event: { operations: OperationSnapshot[] }) => void) => {
    operationsChangedCb = cb
    return Promise.resolve(() => {
      operationsChangedCb = null
    })
  }),
  listOperations: listOperationsMock,
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  formatDuration: vi.fn((s: number) => `${String(s)}s`),
  formatFilesPerSecond: vi.fn((r: number) => `${String(r)} files/s`),
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
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

const { openQueueWindowMock, addToastMock } = vi.hoisted(() => ({
  openQueueWindowMock: vi.fn(() => Promise.resolve()),
  addToastMock: vi.fn(() => 'toast-id'),
}))
vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: openQueueWindowMock,
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
}))

/** A snapshot for our op (`op-1`) with the given status, plus any extra rows. */
function snapshot(status: OperationSnapshot['status'], extra: OperationSnapshot[] = []): OperationSnapshot[] {
  return [
    {
      operationId: 'op-1',
      operationType: 'copy',
      status,
      source: '/Users/test/things',
      destination: '/Users/test/dest',
    },
    ...extra,
  ]
}

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 12; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

async function mountDialog(): Promise<{
  component: ReturnType<typeof mount>
  target: HTMLDivElement
  onQueue: ReturnType<typeof vi.fn>
}> {
  operationsChangedCb = null
  const onQueue = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/things'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/test/dest',
      direction: 'right',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      conflictResolution: 'stop',
      onComplete: () => {},
      onCancelled: () => {},
      onError: () => {},
      onQueue,
    },
  })
  await flushPromises()
  // Drive the dialog into the active (copying) phase so the manage controls show.
  emitSnapshot(snapshot('running'))
  await tick()
  return { component, target, onQueue }
}

/** Fires an `operations-changed` snapshot through the captured subscriber. */
function emitSnapshot(operations: OperationSnapshot[]): void {
  if (!operationsChangedCb) throw new Error('operations-changed subscriber never registered')
  operationsChangedCb({ operations })
}

function queryButton(target: HTMLElement, ariaLabel: string): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>(`button[aria-label="${ariaLabel}"]`)
}

beforeEach(() => {
  pauseOperationMock.mockClear()
  resumeOperationMock.mockClear()
  cancelWriteOperationMock.mockClear()
  listOperationsMock.mockClear()
  listOperationsMock.mockResolvedValue([])
  openQueueWindowMock.mockClear()
  addToastMock.mockClear()
})

describe('TransferProgressDialog Pause/Resume', () => {
  it('clicking Pause calls pauseOperation, then flips to Resume on the paused snapshot', async () => {
    const { component, target } = await mountDialog()

    const pauseBtn = queryButton(target, 'Pause this transfer')
    expect(pauseBtn, 'Pause button shows during the active phase').not.toBeNull()
    pauseBtn?.click()
    await tick()
    expect(pauseOperationMock).toHaveBeenCalledWith('op-1')

    // The button flips only once the backend reports the paused status (no
    // optimistic flip). Drive the snapshot.
    emitSnapshot(snapshot('paused'))
    await tick()

    expect(queryButton(target, 'Resume this transfer'), 'flips to Resume when paused').not.toBeNull()
    expect(queryButton(target, 'Pause this transfer'), 'Pause is gone while paused').toBeNull()
    // Title reflects the paused state.
    expect(target.textContent).toContain('Paused')

    // Resume calls resumeOperation.
    queryButton(target, 'Resume this transfer')?.click()
    await tick()
    expect(resumeOperationMock).toHaveBeenCalledWith('op-1')

    void unmount(component)
  })
})

describe('TransferProgressDialog Queue button', () => {
  it('backgrounds the op: opens the queue window, shows a toast, fires onQueue, and does NOT cancel', async () => {
    const { component, target, onQueue } = await mountDialog()

    const queueBtn = queryButton(target, 'Send to the transfer queue')
    expect(queueBtn, 'Queue button shows during the active phase').not.toBeNull()
    queueBtn?.click()
    await tick()

    expect(openQueueWindowMock, 'opens the queue window').toHaveBeenCalledOnce()
    expect(addToastMock, 'shows a quiet background toast').toHaveBeenCalledOnce()
    expect(onQueue, 'asks the parent to unmount the modal').toHaveBeenCalledOnce()

    // Unmounting a backgrounded dialog must NOT cancel the still-running op.
    void unmount(component)
    await flushPromises()
    expect(cancelWriteOperationMock, 'backgrounded op keeps running on unmount').not.toHaveBeenCalled()
  })

  it('closing the modal (× / Escape / focus-trap teardown) after Queue does NOT cancel the backgrounded op', async () => {
    // Regression: in the real app the backgrounding handoff tears the modal down
    // through `ModalDialog`'s `onclose` (× button / Escape / focus-trap teardown),
    // which calls `handleCancel`. The original Queue test above only exercised
    // Svelte's `unmount()` (the guarded `onDestroy` path) and missed this one, so
    // the bug shipped: clicking Queue cancelled the op (kept partial files) and
    // the queue window opened empty because the op had already settled out of the
    // manager registry.
    const { target } = await mountDialog()

    queryButton(target, 'Send to the transfer queue')?.click()
    await tick()
    expect(openQueueWindowMock, 'Queue backgrounded the op').toHaveBeenCalledOnce()
    cancelWriteOperationMock.mockClear()

    // Fire the modal's onclose path the same way a real close does.
    const closeBtn = target.querySelector<HTMLButtonElement>('.modal-close-button')
    expect(closeBtn, 'modal close (×) affordance is present').not.toBeNull()
    closeBtn?.click()
    await flushPromises()

    expect(
      cancelWriteOperationMock,
      'a backgrounded op must survive the modal close — it is managed by the queue window',
    ).not.toHaveBeenCalled()
  })
})

describe('TransferProgressDialog dialog-scoped F2', () => {
  it('F2 while the dialog is open triggers Queue (same as the button)', async () => {
    const { component, target, onQueue } = await mountDialog()

    // The overlay carries the dialog keydown handler (ModalDialog forwards it).
    const overlay = target.querySelector<HTMLElement>('.modal-overlay')
    expect(overlay, 'dialog overlay rendered').not.toBeNull()
    overlay?.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true }))
    await tick()

    expect(onQueue, 'F2 backgrounds the op').toHaveBeenCalledOnce()
    expect(openQueueWindowMock).toHaveBeenCalledOnce()

    void unmount(component)
  })

  it('NEGATIVE: F2 with the dialog closed reaches the global file.rename handler (no leaked binding)', async () => {
    const { component } = await mountDialog()
    // Close the dialog. Its keydown handler unmounts with it.
    void unmount(component)
    await flushPromises()

    // Stand-in for the app's global key handler that maps F2 → file.rename. With
    // the dialog gone, an F2 keydown must reach it — proving the dialog handler
    // didn't leave a global binding behind.
    const globalRename = vi.fn()
    const onGlobalKeydown = (e: KeyboardEvent) => {
      if (e.key === 'F2') globalRename()
    }
    window.addEventListener('keydown', onGlobalKeydown)
    try {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2' }))
      expect(globalRename, 'F2 reaches file.rename once the dialog is closed').toHaveBeenCalledOnce()
      // And the dialog's Queue path stayed silent (nothing backgrounded).
      expect(openQueueWindowMock).not.toHaveBeenCalled()
    } finally {
      window.removeEventListener('keydown', onGlobalKeydown)
    }
  })
})

describe('TransferProgressDialog auto-queue surfacing', () => {
  it('an op admitted as Queued backgrounds itself: opens the window, toasts, fires onQueue, no second modal', async () => {
    // Seed `list_operations` so the op reports `queued` right after it starts
    // (admitted behind a busy lane), with one running op ahead of it.
    listOperationsMock.mockResolvedValue(
      snapshot('queued', [
        {
          operationId: 'op-busy',
          operationType: 'copy',
          status: 'running',
          source: '/Users/test/other',
          destination: '/Users/test/dest',
        },
      ]),
    )

    operationsChangedCb = null
    const onQueue = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/things'],
        sourceFolderPath: '/Users/test',
        destinationPath: '/Users/test/dest',
        direction: 'right',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        conflictResolution: 'stop',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
        onQueue,
      },
    })
    await flushPromises()

    expect(onQueue, 'a queued op surfaces the queue window instead of a modal').toHaveBeenCalledOnce()
    expect(openQueueWindowMock).toHaveBeenCalledOnce()
    expect(addToastMock, 'a quiet queued toast').toHaveBeenCalledOnce()

    // Backgrounding must not cancel the queued op.
    void unmount(component)
    await flushPromises()
    expect(cancelWriteOperationMock).not.toHaveBeenCalled()
  })
})
