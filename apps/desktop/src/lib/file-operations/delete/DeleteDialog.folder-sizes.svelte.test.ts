/**
 * `DeleteDialog.svelte`: a folder row's size says how strong the drive index's
 * number is, the way the listing does.
 *
 * A folder the index hasn't fully counted (mid-first-index, an unreadable
 * subfolder) only has a lower bound. Printing it bare makes a 40 GB tree read
 * as 12 KB right where someone decides to delete it (#528).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'
import type { DeleteSourceItem } from './delete-dialog-utils'

vi.mock('#lib/tauri-commands/index.js', () => ({
  estimateOperationCost: vi.fn(() => Promise.resolve([])),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

function folder(overrides: Partial<DeleteSourceItem>): DeleteSourceItem {
  return {
    name: 'Photos',
    isDirectory: true,
    isSymlink: false,
    recursiveSize: 12_288,
    recursiveFileCount: 3,
    ...overrides,
  }
}

async function rowSizeText(item: DeleteSourceItem): Promise<string> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DeleteDialog, {
    target,
    props: {
      sourceItems: [item],
      sourcePaths: ['/Users/test/Photos'],
      sourceFolderPath: '/Users/test',
      isPermanent: true,
      supportsTrash: false,
      isFromCursor: true,
      sourceVolumeId: 'root',
      onConfirm: () => {},
      onCancel: () => {},
    },
  })
  await tick()
  const size = target.querySelector('.item-size')
  expect(size, 'the row renders its size cell').not.toBeNull()
  return size?.textContent ?? ''
}

describe('DeleteDialog folder row sizes', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('marks a partially counted folder size as a lower bound', async () => {
    const text = await rowSizeText(folder({ recursiveSizeComplete: false }))
    expect(text.trim().startsWith('≥')).toBe(true)
  })

  it('shows an exact folder size without the lower-bound mark', async () => {
    const text = await rowSizeText(folder({ recursiveSizeComplete: true }))
    expect(text).not.toContain('≥')
    expect(text).toMatch(/\d/)
  })

  it('shows no size for a partially counted folder with nothing counted yet', async () => {
    const text = await rowSizeText(folder({ recursiveSize: 0, recursiveSizeComplete: false }))
    expect(text).not.toContain('≥')
    expect(text).not.toContain('bytes')
  })
})
