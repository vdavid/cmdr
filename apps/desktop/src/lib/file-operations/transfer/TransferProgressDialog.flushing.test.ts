/**
 * What the dialog says after the bytes are copied, which is the stretch where a
 * bar sitting at 100% would be a lie.
 *
 * Two phases live here. `Flushing` must surface the honest "Writing the last
 * piece…" label while the backend `fdatasync`s the freshly written destinations
 * on slow media (both copy and move; see `CLAUDE.md` § "Durability" and the BE
 * doc § "Flushing phase"). `Deleting` on a MOVE is the cross-disk sweep that
 * removes the originals — real, unbounded work with its own bar over the
 * top-level sources.
 *
 * The window's session registry is inited per test: it is what subscribes the
 * event fan-out (through the mocked helpers below), and the mounted dialog binds
 * to the session for its operation. The test then drives a synthesised progress
 * event through the captured callback and asserts what the dialog renders.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/tauri-commands'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import TransferProgressDialogStatic from './TransferProgressDialog.svelte'

let progressCb: ((e: WriteProgressEvent) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  copyFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
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
  // session that seeds itself finds it. An empty list would tell the session the
  // transfer was already over.
  listOperations: vi.fn(() =>
    Promise.resolve<OperationSnapshot[]>([
      {
        operationId: 'op-1',
        operationType: 'copy',
        status: 'running',
        source: '/Users/test',
        destination: '/Users/test/dest',
        supportsRollback: true,
        reverses: null,
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
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

type DialogOperationType = 'copy' | 'move' | 'delete'

function flushingEvent(operationType: DialogOperationType): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType,
    phase: 'flushing',
    currentFile: null,
    filesDone: 4,
    filesTotal: 4,
    bytesDone: 1000,
    bytesTotal: 1000,
    dirsDone: 0,
    bytesPerSecond: null,
    filesPerSecond: null,
    etaSeconds: null,
  }
}

/** A cross-disk move's source sweep, one tick in: one of three top-level
 *  sources gone, no bytes moving. */
function sourceSweepEvent(operationType: DialogOperationType, filesDone = 1): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType,
    phase: 'deleting',
    currentFile: 'holiday',
    filesDone,
    filesTotal: 3,
    bytesDone: 0,
    bytesTotal: 0,
    dirsDone: 0,
    bytesPerSecond: null,
    filesPerSecond: null,
    etaSeconds: null,
  }
}

beforeEach(async () => {
  // Reset before the fan-out subscribes: the registry is what listens now, and
  // it has to be up before a dialog binds to a session.
  progressCb = null
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

async function mountDialog(operationType: DialogOperationType): Promise<{
  component: ReturnType<typeof mount>
  target: HTMLDivElement
}> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialogStatic, {
    target,
    props: {
      operationType,
      sourcePaths: ['/Users/test/file.txt'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/test/dest',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      onComplete: vi.fn(),
      onCancelled: vi.fn(),
      onError: vi.fn(),
    },
  })
  await flushPromises()
  return { component, target }
}

describe('TransferProgressDialog flushing phase', () => {
  it('shows "Writing the last piece..." for a copy in the flushing phase', async () => {
    const { component, target } = await mountDialog('copy')
    expect(progressCb, 'onWriteProgress subscriber registered').toBeTruthy()
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(flushingEvent('copy'))
    await tick()

    expect(target.textContent).toContain('Writing the last piece...')
    void unmount(component)
  })

  it('shows "Writing the last piece..." for a move in the flushing phase', async () => {
    const { component, target } = await mountDialog('move')
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(flushingEvent('move'))
    await tick()

    expect(target.textContent).toContain('Writing the last piece...')
    void unmount(component)
  })
})

describe("TransferProgressDialog: a cross-disk move's source sweep", () => {
  it('names the stage instead of leaving "Moving..." over a bar that means something else', async () => {
    const { component, target } = await mountDialog('move')
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(sourceSweepEvent('move'))
    await tick()

    expect(target.textContent).toContain('Removing the originals...')
    expect(target.textContent).not.toContain('Moving...')
    void unmount(component)
  })

  it('counts the sources it is clearing, never claiming the operation is finished', async () => {
    // Pre-fix this phase emitted nothing at all, so the dialog kept the copy's
    // last tick — `filesDone === filesTotal`, "(100%)" — through the whole
    // sweep, and a Pause here read "Paused" over a full bar.
    const { component, target } = await mountDialog('move')
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(sourceSweepEvent('move'))
    await tick()

    const amounts = [...target.querySelectorAll('.amount')].map((el) => el.textContent.trim())
    const percents = [...target.querySelectorAll('.percent')].map((el) => el.textContent.trim())
    // No size bar: nothing is transferred here, so the count bar is the only one.
    expect(amounts).toEqual(['1 / 3'])
    expect(percents).toEqual(['(33%)'])
    void unmount(component)
  })

  it('counts ITEMS, because the sweep takes a whole folder in one step', async () => {
    const { component, target } = await mountDialog('move')
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(sourceSweepEvent('move'))
    await tick()

    const labels = [...target.querySelectorAll('.bar-label')].map((el) => el.textContent.trim())
    expect(labels).toEqual(['Items'])
    void unmount(component)
  })

  it('leaves a DELETE alone, whose deleting phase is the operation itself', async () => {
    const { component, target } = await mountDialog('delete')
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(sourceSweepEvent('delete'))
    await tick()

    expect(target.textContent).toContain('Deleting...')
    expect(target.textContent).not.toContain('Removing the originals...')
    void unmount(component)
  })
})
