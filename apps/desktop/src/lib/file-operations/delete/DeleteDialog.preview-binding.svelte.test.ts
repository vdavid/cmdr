/**
 * Confirming a delete before its scan preview has an id.
 *
 * `handleConfirm` used to be fully synchronous while `previewId` was assigned
 * only after `await startScanPreview(...)`, so a fast confirm (MCP auto-confirm,
 * Playwright, a quick Enter) dispatched with `previewId = null`. The transfer
 * side has always guarded this with `await scan.scanStarted`; the delete side
 * had no equivalent, and the progress dialog's own scan-wait quietly absorbed
 * it.
 *
 * With the wait moved into the backend's operation task, a null `previewId`
 * lands in the miss case instead: the operation re-walks the tree CONCURRENTLY
 * with the preview `startScan` already began, and that orphan has no owner and
 * nothing to cancel it (teardown's cleanup is gated on `!confirmed`, and
 * confirming sets that before the id ever arrives).
 *
 * The IPC is driven by an explicit deferred rather than incidental microtask
 * ordering, so the race is the test's rather than the scheduler's.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'

/** Resolved by the test, so "the IPC hasn't answered yet" is a state it holds
 *  rather than a window it hopes for. */
const { scanPreviewDeferred, startScanPreviewMock } = vi.hoisted(() => {
  let resolveIt: (value: { previewId: string }) => void = () => {}
  const promise = new Promise<{ previewId: string }>((resolve) => {
    resolveIt = resolve
  })
  return {
    scanPreviewDeferred: {
      promise,
      resolve: (previewId: string) => {
        resolveIt({ previewId })
      },
    },
    startScanPreviewMock: vi.fn(() => promise),
  }
})

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: startScanPreviewMock,
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

const FOLDER = '/Users/me/Documents'

let target: HTMLElement

function mountDialog(onConfirm: (previewId: string | null, isPermanent: boolean) => void): HTMLElement {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(DeleteDialog, {
    target,
    props: {
      sourceItems: [{ name: 'notes.txt', isDirectory: false, isSymlink: false, size: 12 }],
      sourcePaths: [`${FOLDER}/notes.txt`],
      sourceFolderPath: FOLDER,
      isPermanent: true,
      supportsTrash: false,
      isFromCursor: true,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      onConfirm,
      onCancel: () => {},
    },
  })
  return target
}

async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await Promise.resolve()
    await tick()
  }
}

beforeEach(() => {
  startScanPreviewMock.mockClear()
})

afterEach(() => {
  target.remove()
})

describe('DeleteDialog confirmed before its scan preview answers', () => {
  it('dispatches with a non-null previewId, never null', async () => {
    const seen: (string | null)[] = []
    const confirmButton = mountDialog((previewId) => {
      seen.push(previewId)
    }).querySelector<HTMLButtonElement>('button.danger, button')

    await flushMicrotasks()
    expect(startScanPreviewMock, 'the scan starts on mount').toHaveBeenCalledTimes(1)

    // Confirm while the IPC is still in flight: the id does not exist yet.
    const dialog = target.querySelector<HTMLElement>('[role="dialog"], [role="alertdialog"]') ?? target
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushMicrotasks()

    expect(seen, 'confirm must wait for the id rather than dispatching null').toEqual([])

    scanPreviewDeferred.resolve('preview-1')
    await flushMicrotasks()

    expect(seen).toEqual(['preview-1'])
    expect(confirmButton).not.toBeNull()
  })
})
