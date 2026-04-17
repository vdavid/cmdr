/**
 * Tier 3 a11y tests for `TransferProgressDialog.svelte`.
 *
 * Progress dialog shown while a copy/move/delete/trash is running. Tests
 * render the default "just-mounted" state for each operation type. The
 * dialog's reactive state updates via event callbacks — our mocks return
 * no-op unsubscribers so only the initial render is audited.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferProgressDialog from './TransferProgressDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(false)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  formatDuration: vi.fn((s: number) => `${String(s)}s`),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
  ],
}))

describe('TransferProgressDialog a11y', () => {
  it('copy operation (initial "Scanning" state) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/file.txt'],
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
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('move operation has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'move',
        sourcePaths: ['/Users/test/file1.txt', '/Users/test/file2.txt'],
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
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('delete operation (no destination) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'delete',
        sourcePaths: ['/Users/test/file.txt'],
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('trash operation has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'trash',
        sourcePaths: ['/Users/test/file.txt'],
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
