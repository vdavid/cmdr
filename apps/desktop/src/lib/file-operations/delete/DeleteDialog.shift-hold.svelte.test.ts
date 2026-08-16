/**
 * `DeleteDialog.svelte`: holding Shift upgrades an F8 (trash) dialog to a permanent
 * delete for as long as the key is down.
 *
 * A Shift+F8 dialog is already permanent and stays that way: the user is still
 * holding Shift when it opens, so the keyup that follows must not demote it to trash.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'

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

const ITEMS = [{ name: 'notes.txt', isDirectory: false, isSymlink: false, size: 12 }]

interface MountedDialog {
  target: HTMLElement
  confirms: boolean[]
}

function mountDialog(options: { isPermanent: boolean; supportsTrash?: boolean }): MountedDialog {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const confirms: boolean[] = []
  mount(DeleteDialog, {
    target,
    props: {
      sourceItems: ITEMS,
      sourcePaths: ['/Users/test/notes.txt'],
      sourceFolderPath: '/Users/test',
      isPermanent: options.isPermanent,
      supportsTrash: options.supportsTrash ?? true,
      isFromCursor: true,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      onConfirm: (_previewId: string | null, isPermanent: boolean) => confirms.push(isPermanent),
      onCancel: () => {},
    },
  })
  return { target, confirms }
}

/** The dialog wears `alertdialog` while it would delete permanently, `dialog` while it would trash. */
function isPermanentMode(target: HTMLElement): boolean {
  return target.querySelector('[role="alertdialog"]') !== null
}

async function pressShift(): Promise<void> {
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift', shiftKey: true, bubbles: true }))
  await tick()
}

async function releaseShift(): Promise<void> {
  window.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift', shiftKey: false, bubbles: true }))
  await tick()
}

describe('DeleteDialog Shift-hold upgrade', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  afterEach(() => {
    vi.clearAllMocks()
  })

  it('flips a trash dialog to permanent while Shift is down, and back on release', async () => {
    const { target } = mountDialog({ isPermanent: false })
    await tick()
    expect(isPermanentMode(target)).toBe(false)

    await pressShift()
    expect(isPermanentMode(target)).toBe(true)

    await releaseShift()
    expect(isPermanentMode(target)).toBe(false)
  })

  it('confirms permanently while Shift is held', async () => {
    const { target, confirms } = mountDialog({ isPermanent: false })
    await tick()
    await pressShift()

    const confirmButton = [...target.querySelectorAll('button')].at(-1)
    confirmButton?.click()
    await vi.waitFor(() => {
      expect(confirms).toEqual([true])
    })
  })

  it('leaves a Shift+F8 dialog permanent when the held Shift is released', async () => {
    const { target } = mountDialog({ isPermanent: true })
    await tick()
    expect(isPermanentMode(target)).toBe(true)

    await releaseShift()
    expect(isPermanentMode(target)).toBe(true)
  })

  it('drops back to trash when the window loses focus mid-hold', async () => {
    const { target } = mountDialog({ isPermanent: false })
    await tick()
    await pressShift()
    expect(isPermanentMode(target)).toBe(true)

    window.dispatchEvent(new Event('blur'))
    await tick()
    expect(isPermanentMode(target)).toBe(false)
  })

  it('keeps the switch choice: flipping to permanent by hand survives a Shift tap', async () => {
    const { target } = mountDialog({ isPermanent: false })
    await tick()

    const trashSwitch = target.querySelector<HTMLElement>('[role="switch"]')
    trashSwitch?.click()
    await tick()
    expect(isPermanentMode(target)).toBe(true)

    await pressShift()
    await releaseShift()
    expect(isPermanentMode(target)).toBe(true)
  })
})
