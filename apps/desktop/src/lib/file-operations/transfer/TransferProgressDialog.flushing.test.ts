/**
 * The closing `Flushing` phase must surface the honest "Writing the last
 * piece…" label so the bar doesn't sit frozen at 100% while the backend
 * `fdatasync`s the freshly written destinations on slow media. Must show for
 * both copy and move. See `lib/file-operations/transfer/CLAUDE.md`
 * § "Durability" and the BE doc § "Flushing phase".
 *
 * The window's session registry is inited per test: it is what subscribes the
 * event fan-out (through the mocked helpers below), and the mounted dialog binds
 * to the session for its operation. The test then drives a synthesised
 * `flushing` progress event through the captured callback and asserts the
 * rendered title.
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

function flushingEvent(operationType: 'copy' | 'move'): WriteProgressEvent {
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

beforeEach(async () => {
  // Reset before the fan-out subscribes: the registry is what listens now, and
  // it has to be up before a dialog binds to a session.
  progressCb = null
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

async function mountDialog(operationType: 'copy' | 'move'): Promise<{
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
