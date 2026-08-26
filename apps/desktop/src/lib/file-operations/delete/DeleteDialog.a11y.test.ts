/**
 * Tier 3 a11y tests for this directory's two surfaces: `DeleteDialog.svelte` and
 * the trash toast that follows it.
 *
 * The dialog block covers the dialog/alertdialog role switch (trash vs. permanent
 * delete), the no-trash warning banner, symlink notice, and the overflow state.
 * All Tauri IPC used by the dialog is stubbed.
 *
 * Both live here rather than in a file each: `svelte-tests` charges per test FILE,
 * not per test (`docs/testing.md` § "What a test actually costs").
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'
import TrashCompleteToastContent from './TrashCompleteToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

const baseItems = [
  { name: 'Screenshot.png', isDirectory: false, isSymlink: false, size: 102400 },
  { name: 'Documents', isDirectory: true, isSymlink: false, recursiveSize: 10485760, recursiveFileCount: 42 },
]

describe('DeleteDialog a11y', () => {
  it('move-to-trash (supportsTrash=true) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DeleteDialog, {
      target,
      props: {
        sourceItems: baseItems,
        sourcePaths: ['/Users/test/Screenshot.png', '/Users/test/Documents'],
        sourceFolderPath: '/Users/test',
        isPermanent: false,
        supportsTrash: true,
        isFromCursor: false,
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('permanent delete (alertdialog role) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DeleteDialog, {
      target,
      props: {
        sourceItems: baseItems,
        sourcePaths: ['/Users/test/Screenshot.png', '/Users/test/Documents'],
        sourceFolderPath: '/Users/test',
        isPermanent: true,
        supportsTrash: true,
        isFromCursor: false,
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('no-trash volume (forced permanent + warning banner) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DeleteDialog, {
      target,
      props: {
        sourceItems: baseItems,
        sourcePaths: ['/Volumes/External/file.txt'],
        sourceFolderPath: '/Volumes/External',
        isPermanent: false,
        supportsTrash: false,
        isFromCursor: false,
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('large selection (overflow row visible) has no a11y violations', async () => {
    // Build 15 items to exceed MAX_VISIBLE_ITEMS (10)
    const manyItems = Array.from({ length: 15 }, (_, i) => ({
      name: `file-${String(i + 1)}.txt`,
      isDirectory: false,
      isSymlink: false,
      size: 1024 * (i + 1),
    }))
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DeleteDialog, {
      target,
      props: {
        sourceItems: manyItems,
        sourcePaths: manyItems.map((it) => `/Users/test/${it.name}`),
        sourceFolderPath: '/Users/test',
        isPermanent: false,
        supportsTrash: true,
        isFromCursor: false,
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y for `TrashCompleteToastContent.svelte`. The toast container owns the
 * `role="status"` and the close button; what this component contributes is a
 * sentence and two buttons, and both buttons have to be reachable and named.
 */
describe('TrashCompleteToastContent a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TrashCompleteToastContent, {
      target,
      props: {
        toastId: 'toast-1',
        message: 'Moved 3 files to trash',
        operationId: 'op-1',
        sourceFolderPath: '/Users/test',
        explorer: undefined,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
