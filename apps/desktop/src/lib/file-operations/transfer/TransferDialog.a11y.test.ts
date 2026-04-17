/**
 * Tier 3 a11y tests for `TransferDialog.svelte`.
 *
 * Copy/move destination picker. Volume store, Tauri IPC, and settings
 * are stubbed. Tests cover copy, move, and drag-drop (with toggle)
 * states. The dialog mounts lots of event-listener boilerplate, so
 * events return no-op unsubscribers.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() => Promise.resolve({ data: { totalBytes: 1024 * 1024 * 1024, availableBytes: 1024 * 1024 * 500 } })),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(false)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: vi.fn(() => Promise.resolve([])),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'ext', name: 'External', path: '/Volumes/External', category: 'attached_volume', isEjectable: true },
  ],
}))

describe('TransferDialog a11y', () => {
  it('copy dialog (no toggle) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/file.txt'],
        destinationPath: '/Users/test/dest',
        direction: 'right',
        currentVolumeId: 'root',
        fileCount: 1,
        folderCount: 0,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('move dialog has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'move',
        sourcePaths: ['/Users/test/file1.txt', '/Users/test/file2.txt'],
        destinationPath: '/Users/test/dest',
        direction: 'right',
        currentVolumeId: 'root',
        fileCount: 2,
        folderCount: 0,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('drag-drop with copy/move toggle has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/file.txt'],
        destinationPath: '/Users/test/dest',
        direction: 'left',
        currentVolumeId: 'root',
        fileCount: 1,
        folderCount: 0,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        allowOperationToggle: true,
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
