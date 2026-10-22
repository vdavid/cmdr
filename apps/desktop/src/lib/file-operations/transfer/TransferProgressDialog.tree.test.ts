/**
 * The live transfer tree in the progress dialog: which operations show it and
 * when, that the dialog turns fully resizable only while it shows, that the tree
 * takes the current-file line's place, and that it pulls a fresh window on each
 * progress tick. The tree's own behavior is `../transfer-tree/`'s to test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type {
  OperationSnapshot,
  TransferTreeAnswer,
  TransferTreeViewRequest,
  WriteConflictEvent,
  WriteProgressEvent,
} from '#lib/tauri-commands/index.js'
import type { TransferOperationType } from '#lib/file-explorer/types.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import { oneCopyingFile } from '../transfer-tree/fixtures/one-copying-file'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'
import TransferProgressDialog from './TransferProgressDialog.svelte'
import { buttonLabel } from '../test-button-label'

let progressCb: ((e: WriteProgressEvent) => void) | null = null
let conflictCb: ((e: WriteConflictEvent) => void) | null = null
let operationType: TransferOperationType = 'copy'

const getTransferTreeView = vi.hoisted(() =>
  vi.fn<(operationId: string, request: TransferTreeViewRequest) => Promise<TransferTreeAnswer>>(),
)

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  getTransferTreeView,
  onWriteProgress: vi.fn((cb: (e: WriteProgressEvent) => void) => {
    progressCb = cb
    return Promise.resolve(() => {
      progressCb = null
    })
  }),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
    conflictCb = cb
    return Promise.resolve(() => {
      conflictCb = null
    })
  }),
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
  listOperations: vi.fn(() =>
    Promise.resolve<OperationSnapshot[]>([
      {
        operationId: 'op-1',
        operationType,
        status: 'running',
        source: '/Users/test',
        destination: '/Users/test/dest',
        rollbackSupport: 'supported',
        reverses: null,
        error: null,
        canRetry: false,
        conflictPolicy: null,
        items: null,
      },
    ]),
  ),
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

vi.mock('#lib/error-reporter/breadcrumbs.js', () => ({ recordBreadcrumb: vi.fn() }))

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

function event(phase: WriteProgressEvent['phase'], filesDone = 1): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType,
    phase,
    currentFile: 'holiday.mov',
    filesDone,
    filesTotal: 4,
    bytesDone: 1_000,
    bytesTotal: 16_000,
    dirsDone: 0,
    bytesPerSecond: null,
    filesPerSecond: null,
    etaSeconds: null,
    activity: null,
  }
}

beforeEach(async () => {
  progressCb = null
  conflictCb = null
  operationType = 'copy'
  getTransferTreeView.mockReset()
  getTransferTreeView.mockResolvedValue(oneCopyingFile)
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
  document.body.innerHTML = ''
})

async function mountDialog(type: TransferOperationType) {
  operationType = type
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      operationType: type,
      sourcePaths: ['/Users/test/holiday.mov'],
      sourceFolderPath: '/Users/test',
      destinationPath: type === 'delete' || type === 'trash' ? undefined : '/Users/test/dest',
      previewId: null,
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      onComplete: vi.fn(),
      onCancelled: vi.fn(),
      onError: vi.fn(),
    },
  })
  await flushPromises()
  const send = async (e: WriteProgressEvent) => {
    if (!progressCb) throw new Error('subscriber never registered')
    progressCb(e)
    await flushPromises()
  }
  return { component, target, send }
}

function bands(target: HTMLElement): string[] {
  return [...target.querySelectorAll('.resize-band')].map((band) => band.getAttribute('data-direction') ?? '')
}

describe('TransferProgressDialog’s transfer tree', () => {
  it('shows the tree for a copy once it’s past the scan, in place of the current-file line', async () => {
    const { component, target, send } = await mountDialog('copy')
    await send(event('scanning', 0))
    expect(target.querySelector('.transfer-tree')).toBeNull()
    expect(bands(target)).toEqual(['w', 'e'])

    await send(event('copying'))
    expect(target.querySelector('[role="tree"] [role="treeitem"]')?.getAttribute('aria-label')).toMatch(
      /^holiday\.mov, .+, Copying$/,
    )
    expect(target.querySelector('.current-file')).toBeNull()
    // The tree is what absorbs extra height, so the dialog grows both ways now.
    expect(bands(target)).toContain('s')
    expect(getTransferTreeView.mock.calls[0]?.[0]).toBe('op-1')
    void unmount(component)
  })

  // Regression: F did nothing in the real app. A paused tree is one somebody scrolled away
  // from, usually with the wheel, which leaves focus wherever it was (a dialog button), and F
  // listened on the tree alone. This presses F the way WebKit delivers a real keystroke.
  it('resumes following on F from anywhere in the dialog, not only from inside the tree', async () => {
    const { component, target, send } = await mountDialog('copy')
    await send(event('copying'))
    const press = (on: Element, init: KeyboardEventInit) => {
      on.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
    }
    const row = target.querySelector<HTMLElement>('[role="treeitem"]')
    if (!row) throw new Error('no tree row')
    row.focus()
    press(row, { key: 'ArrowDown', code: 'ArrowDown' })
    await flushPromises()
    expect(target.querySelector('.follow button')).not.toBeNull()

    const cancel = [...target.querySelectorAll<HTMLButtonElement>('.btn')].find((b) => buttonLabel(b) === 'Cancel')
    if (!cancel) throw new Error('no Cancel button')
    cancel.focus()
    press(cancel, { key: 'f', code: 'KeyF' })
    await flushPromises()
    expect(target.querySelector('.follow button')).toBeNull()
    void unmount(component)
  })

  it('pulls a fresh window on each progress tick', async () => {
    const { component, send } = await mountDialog('move')
    await send(event('copying', 1))
    const before = getTransferTreeView.mock.calls.length
    await send(event('copying', 2))
    // A tick this close to the last one pulls once the rate limit allows.
    await vi.waitFor(() => {
      expect(getTransferTreeView.mock.calls.length).toBeGreaterThan(before)
    })
    void unmount(component)
  })

  it('keeps today’s layout for a delete: no tree, the current file, width-only resizing', async () => {
    const { component, target, send } = await mountDialog('delete')
    await send(event('deleting'))
    expect(target.querySelector('.transfer-tree')).toBeNull()
    expect(getTransferTreeView).not.toHaveBeenCalled()
    expect(target.querySelector('.current-file')?.textContent).toContain('holiday.mov')
    expect(bands(target)).toEqual(['w', 'e'])
    void unmount(component)
  })

  // The conflict prompt takes the body's place, so the tree goes and comes back: it must
  // come back as the user left it, not following from the top with their folders shut.
  it('brings the tree back as the user left it after a conflict prompt', async () => {
    const { component, target, send } = await mountDialog('copy')
    await send(event('copying'))
    const row = target.querySelector<HTMLElement>('[role="treeitem"]')
    if (!row) throw new Error('no tree row')
    row.focus()
    row.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await flushPromises()

    if (!conflictCb) throw new Error('conflict subscriber never registered')
    conflictCb({
      operationId: 'op-1',
      conflictId: 1,
      sourcePath: '/Users/test/holiday.mov',
      destinationPath: '/Users/test/dest/holiday.mov',
      sourceSize: { kind: 'exact', bytes: 2048 },
      destinationSize: { kind: 'exact', bytes: 1024 },
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: -1024,
      sourceIsDirectory: false,
      destinationIsDirectory: false,
    })
    await flushPromises()
    expect(target.querySelector('.transfer-tree')).toBeNull()
    const before = getTransferTreeView.mock.calls.length
    const skip = [...target.querySelectorAll<HTMLButtonElement>('button')].find((b) => buttonLabel(b) === 'Skip')
    skip?.click()
    await flushPromises()
    await send(event('copying', 2))
    expect(target.querySelector('.transfer-tree')).not.toBeNull()
    expect(getTransferTreeView.mock.calls.length).toBeGreaterThan(before)
    expect(getTransferTreeView.mock.calls.at(-1)?.[1].window.kind).toBe('at')
    void unmount(component)
  })

  it('passes axe with the tree showing', async () => {
    const { component, target, send } = await mountDialog('copy')
    await send(event('copying'))
    await expectNoA11yViolations(target)
    void unmount(component)
  })
})
