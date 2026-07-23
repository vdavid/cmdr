/**
 * Tier 3 a11y tests for `TransferDialog.svelte`.
 *
 * Copy/move destination picker. Volume store, Tauri IPC, and settings
 * are stubbed. Tests cover the copy and move initial states; the
 * copy/move toggle is always present, so both tests exercise it. The
 * dialog mounts lots of event-listener boilerplate, so events return
 * no-op unsubscribers.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import type { VolumeConflictInfo } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Mutable per-test result for the conflict scan, so the merge-info and
// cross-type-warning a11y cases can drive specific collision shapes.
let conflictResult: VolumeConflictInfo[] = []

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() =>
    Promise.resolve({ data: { totalBytes: 1024 * 1024 * 1024, availableBytes: 1024 * 1024 * 500 } }),
  ),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: vi.fn(() => Promise.resolve(conflictResult)),
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
  it('copy dialog has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/file.txt'],
        destinationPath: '/Users/test/dest',
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

  it('dialog with a folder-merge info line has no a11y violations', async () => {
    conflictResult = [
      {
        sourcePath: 'photos',
        destPath: 'photos',
        sourceSize: 0,
        destSize: 0,
        sourceModified: null,
        destModified: null,
        sourceIsDirectory: true,
        destIsDirectory: true,
      },
    ]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/photos'],
        destinationPath: '/Users/test/dest',
        currentVolumeId: 'root',
        fileCount: 0,
        folderCount: 1,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    // Let the parallel conflict check resolve so the merge line renders.
    for (let i = 0; i < 6; i++) {
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
      await tick()
    }
    await expectNoA11yViolations(target)
    conflictResult = []
  })

  it('dialog with the cross-type Overwrite-all warning has no a11y violations', async () => {
    conflictResult = [
      {
        sourcePath: 'photos',
        destPath: 'photos',
        sourceSize: 0,
        destSize: 0,
        sourceModified: null,
        destModified: null,
        sourceIsDirectory: false,
        destIsDirectory: true,
      },
    ]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/photos'],
        destinationPath: '/Users/test/dest',
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
    for (let i = 0; i < 6; i++) {
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
      await tick()
    }
    // Select Overwrite all to surface the red warning, then assert clean a11y.
    const overwrite = target.querySelector<HTMLInputElement>('input[type="radio"][value="overwrite"]')
    overwrite?.click()
    await tick()
    await expectNoA11yViolations(target)
    conflictResult = []
  })
})
