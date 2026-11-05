/**
 * `DeleteDialog.svelte`: the person's own home folder, or a folder holding it.
 *
 * The backend refuses the delete preview up front (`home_folder.rs`), and the
 * dialog then says why in place of the confirm UI: no Delete, no trash switch,
 * only Close. Regression anchor: deleting `~` used to open the normal dialog and
 * size up the whole disk.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'
import * as commands from '#lib/tauri-commands/index.js'

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

let mounted: ReturnType<typeof mount> | null = null

async function mountDialog(path: string, onConfirm = vi.fn(), onCancel = vi.fn()): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(DeleteDialog, {
    target,
    props: {
      sourceItems: [{ name: path.split('/').pop() ?? path, isDirectory: true, isSymlink: false }],
      sourcePaths: [path],
      sourceFolderPath: '/',
      isPermanent: false,
      supportsTrash: true,
      isFromCursor: true,
      sourceVolumeId: 'root',
      onConfirm,
      onCancel,
    },
  })
  // Let the scan start's awaits (four listeners, then the start) settle.
  for (let i = 0; i < 10; i++) await tick()
  return target
}

function buttonLabels(target: HTMLElement): string[] {
  return [...target.querySelectorAll('button')].map((b) => b.textContent.trim()).filter((label) => label !== '')
}

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
  vi.mocked(commands.startScanPreview).mockResolvedValue({ previewId: 'preview-1' })
})

describe('DeleteDialog over the home folder', () => {
  it('says the home folder stays, and offers only Close', async () => {
    vi.mocked(commands.startScanPreview).mockResolvedValue({
      refusal: { type: 'home_folder', path: '/Users/me', relation: 'isHome' },
    })
    const onConfirm = vi.fn()
    const target = await mountDialog('/Users/me', onConfirm)

    const refusal = target.querySelector('[data-test="delete-home-folder-refusal"]')
    expect(refusal?.textContent).toContain('Hey, you can’t delete your own home folder.')
    expect(refusal?.textContent).toContain('it still sounds like a very bad idea')
    // The header's × close stays; the footer holds only Close.
    expect(buttonLabels(target).filter((label) => label !== '×')).toEqual(['Close'])
    expect(target.querySelector('.file-list')).toBeNull()
    expect(target.querySelector('[role="switch"]')).toBeNull()

    // Enter is a confirm everywhere else; here there's nothing to confirm.
    target
      .querySelector('[data-dialog-id="delete-confirmation"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    for (let i = 0; i < 5; i++) await tick()
    expect(onConfirm).not.toHaveBeenCalled()
  })

  it('names a folder holding the home folder, as code', async () => {
    vi.mocked(commands.startScanPreview).mockResolvedValue({
      refusal: { type: 'home_folder', path: '/Users', relation: 'holdsHome' },
    })
    const target = await mountDialog('/Users')

    const refusal = target.querySelector('[data-test="delete-home-folder-refusal"]')
    expect(refusal?.querySelector('code')?.textContent).toBe('/Users')
    expect(refusal?.textContent).toContain('because it contains your home folder')
    expect(refusal?.textContent).toContain('do an OS reset')
  })

  it('Close cancels the dialog', async () => {
    vi.mocked(commands.startScanPreview).mockResolvedValue({
      refusal: { type: 'home_folder', path: '/Users/me', relation: 'isHome' },
    })
    const onCancel = vi.fn()
    const target = await mountDialog('/Users/me', vi.fn(), onCancel)

    const close = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Close')
    close?.click()
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('keeps the confirm UI for anything else', async () => {
    const target = await mountDialog('/Users/other')
    expect(target.querySelector('[data-test="delete-home-folder-refusal"]')).toBeNull()
    expect(buttonLabels(target)).toContain('Move to trash')
  })
})
