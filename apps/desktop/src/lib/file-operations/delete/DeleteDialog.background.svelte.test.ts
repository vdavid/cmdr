/**
 * `DeleteDialog.svelte`: F2 and the Background / Queue button trash in the
 * background (#381), and ONLY trash.
 *
 * A permanent delete is the one operation nothing can undo, so it never starts
 * out of sight. The button and F2 live exactly as long as the dialog's FINAL
 * mode is a trash: the switch, a held Shift, online-only cloud content (known
 * up front or found by the walk), an archive, and a volume with no trash all
 * take them away. The background confirm carries no mode at all
 * (`onConfirmInBackground(previewId)`), so the parent can't start a delete from
 * it either.
 *
 * Keys are dispatched from the focused element and left to bubble, as in
 * `DeleteDialog.shift-hold.svelte.test.ts`: `ModalDialog`'s overlay stops
 * keydown, and a test that skipped it would pass over a dialog that never sees
 * the key.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import type { DeleteConfirmer } from '$lib/file-explorer/pane/dialog-props'
import DeleteDialog from './DeleteDialog.svelte'

const { scanProgress, queueRows } = vi.hoisted(() => ({
  scanProgress: { emit: null as ((event: Record<string, unknown>) => void) | null },
  queueRows: { rows: [] as { snapshot: OperationSnapshot }[] },
}))

vi.mock('$lib/tauri-commands', () => ({
  estimateOperationCost: vi.fn(() => Promise.resolve([])),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn((cb: (event: Record<string, unknown>) => void) => {
    scanProgress.emit = cb
    return Promise.resolve(() => {})
  }),
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

vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => queueRows.rows,
}))

const ITEMS = [{ name: 'notes.txt', isDirectory: false, isSymlink: false, size: 12 }]

interface DialogOptions {
  isPermanent?: boolean
  supportsTrash?: boolean
  isArchive?: boolean
  cloudOnlineOnly?: 'all' | 'mixed' | null
  cloudFolderMayHoldOnlineOnly?: boolean
}

interface MountedDialog {
  target: HTMLElement
  component: ReturnType<typeof mount>
  confirms: boolean[]
  backgrounds: (string | null)[]
  press: () => DeleteConfirmer
}

function mountDialog(options: DialogOptions = {}): MountedDialog {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const confirms: boolean[] = []
  const backgrounds: (string | null)[] = []
  let confirmer: DeleteConfirmer | null = null
  const component = mount(DeleteDialog, {
    target,
    props: {
      sourceItems: ITEMS,
      sourcePaths: ['/Users/test/notes.txt'],
      sourceFolderPath: '/Users/test',
      isPermanent: options.isPermanent ?? false,
      supportsTrash: options.supportsTrash ?? true,
      isArchive: options.isArchive ?? false,
      cloudOnlineOnly: options.cloudOnlineOnly ?? null,
      cloudFolderMayHoldOnlineOnly: options.cloudFolderMayHoldOnlineOnly ?? false,
      isFromCursor: true,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      onConfirm: (_previewId: string | null, isPermanent: boolean) => confirms.push(isPermanent),
      onConfirmInBackground: (previewId: string | null) => backgrounds.push(previewId),
      registerConfirmer: (confirm: DeleteConfirmer) => {
        confirmer = confirm
        return () => {}
      },
      onCancel: () => {},
    },
  })
  return {
    target,
    component,
    confirms,
    backgrounds,
    press: () => {
      if (!confirmer) throw new Error('the dialog registered no confirmer')
      return confirmer
    },
  }
}

function overlayOf(target: HTMLElement): HTMLElement {
  const overlay = target.querySelector<HTMLElement>('[role="dialog"], [role="alertdialog"]')
  if (!overlay) throw new Error('dialog overlay not found')
  return overlay
}

function typeKey(target: HTMLElement, init: KeyboardEventInit & { type?: 'keydown' | 'keyup' }): void {
  const overlay = overlayOf(target)
  const active = document.activeElement
  const from = active instanceof HTMLElement && overlay.contains(active) ? active : overlay
  from.dispatchEvent(new KeyboardEvent(init.type ?? 'keydown', { bubbles: true, ...init }))
}

/** Found by its spoken name, which carries the visible word in both readings. */
function backgroundButton(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>(
    'button[aria-label="Start this in the background"], button[aria-label="Start this in the operation queue"]',
  )
}

/** Lets the scan-start promise and the confirm's awaits run. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await new Promise((done) => setTimeout(done, 0))
}

function liveRow(id: string): { snapshot: OperationSnapshot } {
  return {
    snapshot: {
      operationId: id,
      operationType: 'copy',
      status: 'running',
      source: '/a',
      destination: '/b',
      supportsRollback: true,
      reverses: null,
      error: null,
    },
  }
}

beforeEach(() => {
  document.body.innerHTML = ''
  queueRows.rows = []
  scanProgress.emit = null
})

afterEach(() => {
  vi.clearAllMocks()
})

describe('trashing in the background', () => {
  it('F2 trashes in the background, with the scan preview Enter would hand over', async () => {
    const { target, confirms, backgrounds } = mountDialog()
    await settle()

    typeKey(target, { key: 'F2' })
    await settle()

    expect(backgrounds).toEqual(['preview-1'])
    expect(confirms).toEqual([])
  })

  it('the button does the same', async () => {
    const { target, backgrounds } = mountDialog()
    await settle()

    backgroundButton(target)?.click()
    await settle()

    expect(backgrounds).toEqual(['preview-1'])
  })

  it('an MCP confirm asking for the background presses the same button', async () => {
    const { press, backgrounds, confirms } = mountDialog()
    await settle()

    expect(press()({ startInBackground: true })).toBe('pressed')
    await settle()

    expect(backgrounds).toEqual(['preview-1'])
    expect(confirms).toEqual([])
  })

  it('reads "Background" with nothing else running, and "Queue" once something is', async () => {
    const quiet = mountDialog()
    await tick()
    expect(backgroundButton(quiet.target)?.textContent.trim()).toBe('Background')

    document.body.innerHTML = ''
    queueRows.rows = [liveRow('other')]
    const busy = mountDialog()
    await tick()
    expect(backgroundButton(busy.target)?.textContent.trim()).toBe('Queue')
  })
})

describe('never a permanent delete', () => {
  /** Neither F2 nor the MCP press may start anything, and the button is gone. */
  async function expectNoBackground(dialog: MountedDialog): Promise<void> {
    await settle()
    expect(backgroundButton(dialog.target)).toBeNull()
    typeKey(dialog.target, { key: 'F2' })
    expect(dialog.press()({ startInBackground: true }), 'the MCP press says why').toBe('refusedPermanentDelete')
    await settle()
    expect(dialog.backgrounds).toEqual([])
    expect(dialog.confirms).toEqual([])
  }

  it('not once the switch says delete permanently', async () => {
    const dialog = mountDialog()
    await settle()
    const trashSwitch = dialog.target.querySelector<HTMLElement>('[role="switch"], input[type="checkbox"]')
    if (!trashSwitch) throw new Error('trash switch not found')
    trashSwitch.click()
    await tick()

    await expectNoBackground(dialog)
  })

  it('not while Shift is held, and Shift+F2 starts nothing', async () => {
    const dialog = mountDialog()
    await settle()

    typeKey(dialog.target, { key: 'Shift', shiftKey: true })
    await tick()
    expect(backgroundButton(dialog.target)).toBeNull()
    typeKey(dialog.target, { key: 'F2', shiftKey: true })
    await settle()

    expect(dialog.backgrounds).toEqual([])
    expect(dialog.confirms).toEqual([])
  })

  it('not on a Shift+F8 dialog', async () => {
    await expectNoBackground(mountDialog({ isPermanent: true }))
  })

  it('not for online-only cloud content', async () => {
    await expectNoBackground(mountDialog({ isPermanent: true, supportsTrash: false, cloudOnlineOnly: 'all' }))
  })

  it('not inside an archive', async () => {
    await expectNoBackground(mountDialog({ isPermanent: true, supportsTrash: false, isArchive: true }))
  })

  it('not on a volume without a trash', async () => {
    await expectNoBackground(mountDialog({ supportsTrash: false }))
  })

  it('hands the press back when the walk turns the trash into a delete mid-confirm', async () => {
    const dialog = mountDialog({ cloudFolderMayHoldOnlineOnly: true })
    await settle()

    typeKey(dialog.target, { key: 'F2' })
    await settle()
    // The confirm is waiting on the walk's online-only answer; it says yes.
    scanProgress.emit?.({
      previewId: 'preview-1',
      filesFound: 1,
      dirsFound: 0,
      bytesFound: 12,
      currentDir: null,
      onlineOnlyFound: true,
    })
    await settle()

    expect(dialog.backgrounds).toEqual([])
    expect(dialog.confirms).toEqual([])
    expect(backgroundButton(dialog.target)).toBeNull()
  })
})

describe('F2 is the dialog’s only while it’s open', () => {
  it('stops at the dialog while it is up, and reaches file.rename again once it closes', async () => {
    const dialog = mountDialog()
    await settle()
    // Stand-in for the app's global key handler, where F2 is `file.rename`.
    const globalRename = vi.fn()
    const onGlobalKeydown = (event: KeyboardEvent): void => {
      if (event.key === 'F2') globalRename()
    }
    window.addEventListener('keydown', onGlobalKeydown)
    try {
      typeKey(dialog.target, { key: 'F2' })
      expect(globalRename, 'the open dialog keeps F2 to itself').not.toHaveBeenCalled()

      await unmount(dialog.component)
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2' }))
      expect(globalRename, 'F2 is file.rename again once the dialog is gone').toHaveBeenCalledOnce()
    } finally {
      window.removeEventListener('keydown', onGlobalKeydown)
    }
  })
})
