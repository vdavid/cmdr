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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot } from '#lib/ipc/bindings.js'
import TransferProgressDialog from './TransferProgressDialog.svelte'
import {
  initMainWindowOperations,
  destroyMainWindowOperations,
} from '#lib/file-operations/queue/main-window-operations.svelte.js'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'

// Hoisted so the `vi.mock` factory (lifted to the top of the file) can reference
// these. Plain `const`s declared here would be in the temporal dead zone when the
// hoisted factory runs.
const {
  pauseOperationMock,
  resumeOperationMock,
  cancelOperationMock,
  cancelWriteOperationMock,
  listOperationsMock,
  operationsChangedCbs,
  writeProgressCbs,
} = vi.hoisted(() => ({
  pauseOperationMock: vi.fn(() => Promise.resolve()),
  resumeOperationMock: vi.fn(() => Promise.resolve()),
  cancelOperationMock: vi.fn(() => Promise.resolve()),
  cancelWriteOperationMock: vi.fn(() => Promise.resolve()),
  listOperationsMock: vi.fn(() => Promise.resolve<OperationSnapshot[]>([])),
  // EVERY `operations-changed` subscriber, not just the dialog's: the main
  // window's operations store subscribes to the same stream, and the button's
  // label reads that store. One emit has to reach both, as it does in the app.
  operationsChangedCbs: [] as ((event: { operations: OperationSnapshot[] }) => void)[],
  // The dialog's phase comes from `write-progress`, and it starts in
  // `scanning` (the operation is registered before its preview finishes). The
  // manage controls that only make sense once bytes move need a real copying
  // tick, so the harness drives one.
  writeProgressCbs: [] as ((event: Record<string, unknown>) => void)[],
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn((cb: (event: Record<string, unknown>) => void) => {
    writeProgressCbs.push(cb)
    return Promise.resolve(() => {})
  }),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: cancelOperationMock,
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
    operationsChangedCbs.push(cb)
    return Promise.resolve(() => {
      const at = operationsChangedCbs.indexOf(cb)
      if (at >= 0) operationsChangedCbs.splice(at, 1)
    })
  }),
  listOperations: listOperationsMock,
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('#lib/stores/volume-store.svelte.js', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

const { openQueueWindowMock, addToastMock } = vi.hoisted(() => ({
  openQueueWindowMock: vi.fn(() => Promise.resolve()),
  addToastMock: vi.fn(() => 'toast-id'),
}))
vi.mock('#lib/file-operations/queue/queue-window.js', () => ({
  openQueueWindow: openQueueWindowMock,
}))
vi.mock('#lib/ui/toast/index.js', () => ({
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
      rollbackSupport: 'supported',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      startedAt: null,
      items: null,
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

/** Every dialog this file mounts, torn down between tests. A dialog is a view
 *  now, so one left mounted keeps holding the session for `op-1` and the next
 *  test's dialog would find that session already seeded. */
const mounted: ReturnType<typeof mount>[] = []

function track(instance: ReturnType<typeof mount>): ReturnType<typeof mount> {
  mounted.push(instance)
  return instance
}

/** Unmounts a dialog and forgets it, so the between-tests sweep doesn't try
 *  again. Several cases unmount mid-test, because what happens on teardown is
 *  the thing they're asserting. */
function dropView(instance: ReturnType<typeof mount>): void {
  const at = mounted.indexOf(instance)
  if (at >= 0) mounted.splice(at, 1)
  void unmount(instance)
}

async function mountDialog(): Promise<{
  component: ReturnType<typeof mount>
  target: HTMLDivElement
  onQueue: ReturnType<typeof vi.fn>
}> {
  const onQueue = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = track(
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/things'],
        sourceFolderPath: '/Users/test',
        destinationPath: '/Users/test/dest',
        direction: 'right',
        previewId: null,
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        conflictResolution: 'stop',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
        onQueue,
      },
    }),
  )
  await flushPromises()
  // Drive the dialog into the active (copying) phase so the manage controls
  // show. The status snapshot alone isn't enough: an operation is `running`
  // from the moment it's registered, and the PHASE is what says whether it is
  // still counting or actually writing.
  emitCopyingProgress()
  emitSnapshot(snapshot('running'))
  await tick()
  return { component, target, onQueue }
}

/** Mounts the dialog and leaves it in the scanning phase: registered, named,
 *  and counting, which is what a confirmed transfer looks like before its
 *  preview lands. */
async function mountScanningDialog(): Promise<{ target: HTMLDivElement; onQueue: ReturnType<typeof vi.fn> }> {
  const onQueue = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  track(
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/things'],
        sourceFolderPath: '/Users/test',
        destinationPath: '/Users/test/dest',
        direction: 'right',
        previewId: 'prev-1',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        conflictResolution: 'stop',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
        onQueue,
      },
    }),
  )
  await flushPromises()
  emitSnapshot(snapshot('running'))
  await tick()
  return { target, onQueue }
}

/** One active-phase progress tick, so the dialog leaves the scanning phase. */
function emitCopyingProgress(): void {
  for (const cb of [...writeProgressCbs]) {
    cb({
      operationId: 'op-1',
      operationType: 'copy',
      phase: 'copying',
      currentFile: 'a.bin',
      filesDone: 1,
      filesTotal: 4,
      bytesDone: 25,
      bytesTotal: 100,
    })
  }
}

/** Fires an `operations-changed` snapshot through every captured subscriber. */
function emitSnapshot(operations: OperationSnapshot[]): void {
  if (operationsChangedCbs.length === 0) throw new Error('operations-changed subscriber never registered')
  for (const cb of [...operationsChangedCbs]) cb({ operations })
}

function queryButton(target: HTMLElement, ariaLabel: string): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>(`button[aria-label="${ariaLabel}"]`)
}

/** The Background / Queue button, found by the key it carries, so the lookup
 *  holds for both of its words. Which word shows is the point of the last
 *  describe block; every other test here runs with an empty queue. */
function backgroundButton(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>('button[aria-keyshortcuts="F2"]')
}

/** The one visible word the button shows right now. */
function backgroundButtonLabel(target: HTMLElement): string | null {
  const button = backgroundButton(target)
  if (!button) return null
  // The F2 chip rides inside the button; the word is everything else.
  const word = button.cloneNode(true) as HTMLElement
  word.querySelectorAll('kbd').forEach((chip) => {
    chip.remove()
  })
  return word.textContent.trim()
}

/** The key chip inside the button, which teaches F2 to anyone who clicks instead. */
function backgroundButtonChip(target: HTMLElement): string | null {
  const button = backgroundButton(target)
  return button?.querySelector('kbd')?.textContent.trim() ?? null
}

/** Another operation in the queue, one the dialog doesn't own. */
function otherOp(
  operationId: string,
  status: OperationSnapshot['status'],
  operationType: OperationSnapshot['operationType'] = 'copy',
): OperationSnapshot {
  return {
    operationId,
    operationType,
    status,
    source: '/Users/test/other',
    destination: '/Users/test/dest',
    rollbackSupport: 'supported',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    startedAt: null,
    items: null,
  }
}

beforeEach(async () => {
  while (mounted.length > 0) {
    const instance = mounted.pop()
    if (instance) void unmount(instance)
  }
  destroyMainWindowOperations()
  destroyOperationSessions()
  operationsChangedCbs.length = 0
  writeProgressCbs.length = 0
  pauseOperationMock.mockClear()
  resumeOperationMock.mockClear()
  cancelOperationMock.mockClear()
  cancelWriteOperationMock.mockClear()
  listOperationsMock.mockClear()
  // The backend registers the operation before the start command returns, so a
  // session that seeds itself finds it. An empty list would tell the session the
  // transfer was already over.
  listOperationsMock.mockResolvedValue(snapshot('running'))
  openQueueWindowMock.mockClear()
  addToastMock.mockClear()
  // The registry subscribes the event fan-out, so it has to be up before a
  // dialog mounts: a view binds to a session rather than listening for itself.
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
  destroyMainWindowOperations()
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

    dropView(component)
  })
})

describe('TransferProgressDialog keyboard: Escape and the opening focus', () => {
  it('Escape cancels the transfer, keeping what it wrote, and backgrounds nothing', async () => {
    const { target, onQueue } = await mountDialog()

    target
      .querySelector('.modal-overlay')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    await flushPromises()

    expect(cancelOperationMock, 'Escape is the Cancel button').toHaveBeenCalledExactlyOnceWith('op-1')
    expect(cancelWriteOperationMock, 'never a rollback').not.toHaveBeenCalled()
    expect(onQueue, 'Escape no longer backgrounds').not.toHaveBeenCalled()
  })

  it('× still backgrounds: closing the window is not a cancel', async () => {
    const { target, onQueue } = await mountDialog()

    target.querySelector<HTMLButtonElement>('.modal-close-button')?.click()
    await flushPromises()

    expect(onQueue).toHaveBeenCalledOnce()
    expect(cancelOperationMock).not.toHaveBeenCalled()
  })

  it('opens with focus on Pause, so Space pauses and resumes', async () => {
    const { target } = await mountDialog()
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => {
        resolve()
      }),
    )

    expect(document.activeElement).toBe(queryButton(target, 'Pause this transfer'))
  })
})

describe('TransferProgressDialog Queue button', () => {
  it('backgrounds the op: shows a toast, fires onQueue, opens no window, and does NOT cancel', async () => {
    const { component, target, onQueue } = await mountDialog()

    const queueBtn = backgroundButton(target)
    expect(queueBtn, 'Queue button shows during the active phase').not.toBeNull()
    queueBtn?.click()
    await tick()

    expect(openQueueWindowMock, 'leaves the queue window shut: the corner chip carries the op').not.toHaveBeenCalled()
    expect(addToastMock, 'shows a quiet background toast').toHaveBeenCalledOnce()
    expect(onQueue, 'asks the parent to unmount the modal').toHaveBeenCalledOnce()

    // Unmounting a backgrounded dialog must NOT cancel the still-running op.
    dropView(component)
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

    backgroundButton(target)?.click()
    await tick()
    expect(addToastMock, 'Queue backgrounded the op').toHaveBeenCalledOnce()
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
    expect(openQueueWindowMock).not.toHaveBeenCalled()

    dropView(component)
  })

  it('NEGATIVE: F2 with the dialog closed reaches the global file.rename handler (no leaked binding)', async () => {
    const { component } = await mountDialog()
    // Close the dialog. Its keydown handler unmounts with it.
    dropView(component)
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
      expect(addToastMock).not.toHaveBeenCalled()
    } finally {
      window.removeEventListener('keydown', onGlobalKeydown)
    }
  })
})

describe('TransferProgressDialog auto-queue surfacing', () => {
  it('an op admitted as Queued backgrounds itself: toasts, fires onQueue, no window, no second modal', async () => {
    // Seed `list_operations` so the op reports `queued` right after it starts
    // (admitted behind a busy lane), with one running op ahead of it. The seed
    // is the fan-out's, taken once at init, so the window reopens on it.
    destroyOperationSessions()
    listOperationsMock.mockResolvedValue(
      snapshot('queued', [
        {
          operationId: 'op-busy',
          operationType: 'copy',
          status: 'running',
          source: '/Users/test/other',
          destination: '/Users/test/dest',
          rollbackSupport: 'supported',
          reverses: null,
          error: null,
          canRetry: false,
          conflictPolicy: null,
          startedAt: null,
          items: null,
        },
      ]),
    )
    await initOperationSessions()

    const onQueue = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = track(
      mount(TransferProgressDialog, {
        target,
        props: {
          operationType: 'copy',
          sourcePaths: ['/Users/test/things'],
          sourceFolderPath: '/Users/test',
          destinationPath: '/Users/test/dest',
          direction: 'right',
          previewId: null,
          sourceVolumeId: 'root',
          destVolumeId: 'root',
          conflictResolution: 'stop',
          onComplete: () => {},
          onCancelled: () => {},
          onError: () => {},
          onQueue,
        },
      }),
    )
    await flushPromises()

    expect(onQueue, 'a queued op backgrounds itself instead of stacking a modal').toHaveBeenCalledOnce()
    expect(openQueueWindowMock, 'and leaves the queue window shut').not.toHaveBeenCalled()
    expect(addToastMock, 'a quiet queued toast').toHaveBeenCalledOnce()

    // Backgrounding must not cancel the queued op.
    dropView(component)
    await flushPromises()
    expect(cancelWriteOperationMock).not.toHaveBeenCalled()
  })
})

describe('TransferProgressDialog backgrounding under the real synchronous teardown', () => {
  it('hands the operation over intact when onQueue synchronously unmounts the modal', async () => {
    // The real parent (dialog-state) reacts to `onQueue` by synchronously
    // unmounting the modal (`showTransferProgressDialog = false`), so `onDestroy`
    // runs in the SAME turn `handleQueue` set `backgrounded = true`. That timing
    // is what once cost a transfer: `backgrounded` was a `$state` rune, the read
    // during reactive-scope disposal came back STALE, and the op was cancelled —
    // the transfer died and the queue window opened empty. The other Queue tests
    // use a no-op `onQueue` plus a separate `unmount()`, so their teardown lands
    // in a LATER turn and they can't see this; here `onQueue` unmounts inline.
    const target = document.createElement('div')
    document.body.appendChild(target)
    let comp: ReturnType<typeof mount> | null = null
    const onQueue = (): void => {
      if (comp) dropView(comp)
    }
    comp = track(
      mount(TransferProgressDialog, {
        target,
        props: {
          operationType: 'copy',
          sourcePaths: ['/Users/test/things'],
          sourceFolderPath: '/Users/test',
          destinationPath: '/Users/test/dest',
          direction: 'right',
          previewId: null,
          sourceVolumeId: 'root',
          destVolumeId: 'root',
          conflictResolution: 'stop',
          onComplete: () => {},
          onCancelled: () => {},
          onError: () => {},
          onQueue,
        },
      }),
    )
    await flushPromises()
    emitSnapshot(snapshot('running'))
    await tick()
    cancelWriteOperationMock.mockClear()
    cancelOperationMock.mockClear()
    addToastMock.mockClear()

    backgroundButton(target)?.click()
    await flushPromises()

    expect(addToastMock, 'the handoff completes: the toast says where the op went').toHaveBeenCalledOnce()
    expect(
      cancelWriteOperationMock,
      'a backgrounded op must survive the synchronous modal teardown',
    ).not.toHaveBeenCalled()
    expect(cancelOperationMock, 'and nothing cancels it through the manager either').not.toHaveBeenCalled()
  })
})

describe('TransferProgressDialog background/queue button label', () => {
  /** The real main-window store, subscribed to the same mocked stream the dialog
   *  reads, so `emitSnapshot` moves the label exactly as the backend would. */
  async function mountWithLiveQueue(): Promise<{ target: HTMLDivElement; component: ReturnType<typeof mount> }> {
    await initMainWindowOperations()
    const { component, target } = await mountDialog()
    return { component, target }
  }

  it('reads "Background" with nothing else in the queue: there is nothing to queue behind', async () => {
    const { component, target } = await mountWithLiveQueue()
    // The snapshot holds this dialog's OWN operation and nothing else.
    emitSnapshot(snapshot('running'))
    await tick()

    expect(backgroundButtonLabel(target)).toBe('Background')
    expect(backgroundButtonChip(target), 'the button shows its shortcut').toBe('F2')
    expect(backgroundButton(target)?.getAttribute('aria-keyshortcuts'), 'and says it').toBe('F2')
    // WCAG 2.5.3 (label in name): no aria-label, so the name IS the visible word
    // (the chip is aria-hidden), and a speech-control user can say what they see.
    expect(backgroundButton(target)?.getAttribute('aria-label'), 'named by the word itself').toBeNull()

    dropView(component)
  })

  it('reads "Queue" once another operation is in flight', async () => {
    const { component, target } = await mountWithLiveQueue()
    emitSnapshot(snapshot('running', [otherOp('op-other', 'running')]))
    await tick()

    expect(backgroundButtonLabel(target)).toBe('Queue')
    expect(backgroundButtonChip(target)).toBe('F2')
    expect(backgroundButton(target)?.getAttribute('aria-label'), 'named by the word itself').toBeNull()

    dropView(component)
  })

  it('tracks the queue LIVE: the word flips when another operation joins and leaves', async () => {
    const { component, target } = await mountWithLiveQueue()
    emitSnapshot(snapshot('running'))
    await tick()
    expect(backgroundButtonLabel(target)).toBe('Background')

    emitSnapshot(snapshot('running', [otherOp('op-other', 'queued')]))
    await tick()
    expect(backgroundButtonLabel(target), 'a newcomer makes this a queue').toBe('Queue')

    emitSnapshot(snapshot('running'))
    await tick()
    expect(backgroundButtonLabel(target), 'and it goes back when the newcomer finishes').toBe('Background')

    dropView(component)
  })

  it('an instant operation never flips the word: a rename is gone before the eye lands on it', async () => {
    const { component, target } = await mountWithLiveQueue()
    emitSnapshot(snapshot('running', [otherOp('op-rename', 'running', 'rename')]))
    await tick()

    expect(backgroundButtonLabel(target)).toBe('Background')

    dropView(component)
  })

  it('a retained failure never flips the word: it is a notice, not work to wait behind', async () => {
    const { component, target } = await mountWithLiveQueue()
    emitSnapshot(snapshot('running', [otherOp('op-dead', 'failed')]))
    await tick()

    expect(backgroundButtonLabel(target)).toBe('Background')

    dropView(component)
  })
})

describe('TransferProgressDialog while the operation is still counting', () => {
  it('offers Background and Pause', async () => {
    // The shipped bug, from the other side: a confirmed transfer had no
    // `operationId` while its preview walked, so neither control rendered and a
    // large copy could not be sent to the background. Both are here now, and
    // Pause among them: the walk parks on the same gate the drivers do, and the
    // scan is when somebody notices they picked the wrong destination.
    const { target } = await mountScanningDialog()

    expect(queryButton(target, 'Pause this transfer'), 'the scan parks, so the button does something').not.toBeNull()
    expect(backgroundButton(target), 'backgrounding a scanning transfer is the whole point').not.toBeNull()
  })

  it('titles a PAUSED scan "Paused", and offers Resume', async () => {
    // The title check runs before the scanning arm on purpose: a paused walk
    // has stopped, so "Scanning to copy…" over it would describe work that
    // isn't happening. And without Resume here, a scan paused from "Pause all"
    // had no way back but Cancel.
    const { target } = await mountScanningDialog()

    emitSnapshot(snapshot('paused'))
    await tick()

    expect(target.textContent).toContain('Paused')
    expect(target.textContent, 'the title must not still claim it is counting').not.toContain('Scanning to copy')
    expect(queryButton(target, 'Resume this transfer')).not.toBeNull()
  })

  it('backgrounds a scanning operation without cancelling it', async () => {
    const { target, onQueue } = await mountScanningDialog()

    backgroundButton(target)?.click()
    await tick()

    expect(onQueue).toHaveBeenCalledTimes(1)
    expect(openQueueWindowMock).not.toHaveBeenCalled()
    expect(cancelWriteOperationMock, 'a handoff is not a cancel').not.toHaveBeenCalled()
  })

  it('disables Rollback: nothing has been written to reverse', async () => {
    const { target } = await mountScanningDialog()

    const rollback = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Rollback'))
    expect(rollback?.disabled).toBe(true)
  })
})
