/**
 * The closing `Flushing` phase must surface the honest "Writing the last
 * piece…" label so the bar doesn't sit frozen at 100% while the backend
 * `fdatasync`s the freshly written destinations on slow media. Must show for
 * both copy and move. See `lib/file-operations/transfer/CLAUDE.md`
 * § "Durability" and the BE doc § "Flushing phase".
 *
 * We mount the dialog with mocked event helpers that capture the registered
 * `onWriteProgress` callback, then drive it with a synthesised `flushing`
 * progress event and assert the rendered title.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { WriteProgressEvent } from '$lib/file-explorer/types'
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
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
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

async function mountDialog(operationType: 'copy' | 'move'): Promise<{
  component: ReturnType<typeof mount>
  target: HTMLDivElement
}> {
  progressCb = null
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
