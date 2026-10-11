/**
 * F2 and the Background / Queue button in the copy / move / compress dialog
 * (#381): Total Commander's "F2 Queue", so F5, F2 sends a copy off without a
 * progress dialog.
 *
 * F2 is Enter plus one flag. Same guards (a destination that refuses writes, a
 * source that can't be read), same wait for the scan preview to start, same
 * payload; only `startInBackground` differs. It's dialog-scoped like the
 * progress dialog's F2: `ModalDialog`'s overlay stops every keydown, so it never
 * reaches the global `file.rename` while the dialog is up, and does again once
 * it closes.
 */

import { describe, it, expect, vi } from 'vitest'
import { tick, unmount } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import type { TransferConfirmPayload, TransferConfirmer } from '$lib/file-explorer/pane/dialog-props'
import {
  deferred,
  destinationWriteAccessMock,
  flushMicrotasks,
  mountDialog,
  startScanPreviewMock,
  type ConfirmFn,
} from './test-transfer-dialog-harness'

const { queueRows } = vi.hoisted(() => ({ queueRows: { rows: [] as { snapshot: OperationSnapshot }[] } }))

vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => queueRows.rows,
}))

function overlayOf(target: HTMLElement): HTMLElement {
  const overlay = target.querySelector<HTMLElement>('.modal-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return overlay
}

function pressKey(target: HTMLElement, init: KeyboardEventInit): void {
  overlayOf(target).dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, ...init }))
}

/** Found by its spoken name, which carries the visible word in both readings. */
function backgroundButton(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>(
    'button[aria-label="Start this in the background"], button[aria-label="Start this in the operation queue"]',
  )
}

/** The write-access probe runs on a 300 ms debounce. */
async function settleExistsCheck(): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, 350))
  await flushMicrotasks()
}

async function confirmWith(key: KeyboardEventInit): Promise<TransferConfirmPayload[]> {
  const payloads: TransferConfirmPayload[] = []
  const target = mountDialog({ onConfirm: (payload) => payloads.push(payload) })
  await flushMicrotasks()
  pressKey(target, key)
  await flushMicrotasks()
  return payloads
}

function liveRow(): { snapshot: OperationSnapshot } {
  return {
    snapshot: {
      operationId: 'other',
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

describe('F2 in the transfer dialog', () => {
  it('confirms with exactly the payload Enter sends, plus startInBackground', async () => {
    const [enter] = await confirmWith({ key: 'Enter' })
    document.body.innerHTML = ''
    const [f2] = await confirmWith({ key: 'F2' })

    expect(enter).toBeDefined()
    expect(enter.startInBackground).toBeUndefined()
    expect(f2).toEqual({ ...enter, startInBackground: true })
  })

  it('waits for the scan preview to start, like Enter, so the job claims it', async () => {
    const scanStart = deferred<{ previewId: string }>()
    startScanPreviewMock.mockReturnValueOnce(scanStart.promise)
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ onConfirm })
    await flushMicrotasks()

    pressKey(target, { key: 'F2' })
    await flushMicrotasks()
    expect(onConfirm, 'nothing goes before the preview has an id').not.toHaveBeenCalled()

    scanStart.resolve({ previewId: 'preview-late' })
    await flushMicrotasks()
    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({ previewId: 'preview-late', startInBackground: true }),
    )
  })

  it('is refused while the destination refuses writes, and the button reads disabled', async () => {
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unwritable', reason: 'noPermission' })
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ destinationPath: '/Users/other/private', onConfirm })
    await settleExistsCheck()

    pressKey(target, { key: 'F2' })
    await flushMicrotasks()

    expect(onConfirm).not.toHaveBeenCalled()
    expect(backgroundButton(target)?.disabled).toBe(true)
  })

  it('ignores ⇧F2 and ⌘F2, which are other combos', async () => {
    expect(await confirmWith({ key: 'F2', shiftKey: true })).toEqual([])
    document.body.innerHTML = ''
    expect(await confirmWith({ key: 'F2', metaKey: true })).toEqual([])
  })

  it('starts in the background from the button too', async () => {
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ onConfirm })
    await flushMicrotasks()

    backgroundButton(target)?.click()
    await flushMicrotasks()

    expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ startInBackground: true }))
  })

  it('starts in the background from an MCP confirm that asks for it', async () => {
    const captured: { press: TransferConfirmer | null } = { press: null }
    const onConfirm = vi.fn<ConfirmFn>()
    mountDialog({
      onConfirm,
      registerConfirmer: (confirm) => {
        captured.press = confirm
        return () => {}
      },
    })
    await flushMicrotasks()

    captured.press?.('skip', { startInBackground: true })
    await flushMicrotasks()

    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({ conflictResolution: 'skip', startInBackground: true }),
    )
  })

  it('reads "Background" with nothing else running, and "Queue" once something is', async () => {
    queueRows.rows = []
    const quiet = mountDialog()
    await tick()
    expect(backgroundButton(quiet)?.textContent.trim()).toBe('Background')

    document.body.innerHTML = ''
    queueRows.rows = [liveRow()]
    const busy = mountDialog()
    await tick()
    expect(backgroundButton(busy)?.textContent.trim()).toBe('Queue')
    queueRows.rows = []
  })

  it('swallows the held F2’s auto-repeat until the key comes up, so it never renames the cursor file', async () => {
    const target = mountDialog()
    await flushMicrotasks()
    // Stand-in for the app's global key handler, where F2 is `file.rename`.
    const globalRename = vi.fn()
    const onGlobalKeydown = (event: KeyboardEvent): void => {
      if (event.key === 'F2') globalRename()
    }
    window.addEventListener('keydown', onGlobalKeydown)
    try {
      pressKey(target, { key: 'F2' })
      // The dialog is gone (no modal mounts), and the key is still down: its
      // repeats land on whatever has focus now, and bubble.
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', repeat: true, bubbles: true }))
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', repeat: true, bubbles: true }))
      expect(globalRename, 'the held key’s repeats stay with the press that started the job').not.toHaveBeenCalled()

      document.body.dispatchEvent(new KeyboardEvent('keyup', { key: 'F2', bubbles: true }))
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true }))
      expect(globalRename, 'a fresh press is file.rename again').toHaveBeenCalledOnce()
    } finally {
      window.removeEventListener('keydown', onGlobalKeydown)
    }
  })

  it('stops at the dialog while it is up, and is file.rename again once it closes', async () => {
    const { mount } = await import('svelte')
    const TransferDialog = (await import('./TransferDialog.svelte')).default
    const target = document.createElement('div')
    document.body.appendChild(target)
    const dialog = mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/notes.txt'],
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
    await flushMicrotasks()
    // Stand-in for the app's global key handler, where F2 is `file.rename`.
    const globalRename = vi.fn()
    const onGlobalKeydown = (event: KeyboardEvent): void => {
      if (event.key === 'F2') globalRename()
    }
    window.addEventListener('keydown', onGlobalKeydown)
    try {
      pressKey(target, { key: 'F2' })
      expect(globalRename, 'the open dialog keeps F2 to itself').not.toHaveBeenCalled()

      await unmount(dialog)
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2' }))
      expect(globalRename, 'F2 is file.rename again once the dialog is gone').toHaveBeenCalledOnce()
    } finally {
      window.removeEventListener('keydown', onGlobalKeydown)
    }
  })
})
