/**
 * An MCP `dialog confirm` on the delete dialog presses the dialog's OWN confirm
 * (`registerConfirmer`), so it sends what the button would: the scan preview the
 * dialog started, and the mode the dialog shows NOW rather than the one it opened
 * with. The transfer dialog's twin: `pane/dialog-state.transfer-confirm.svelte.test.ts`.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'
import type { DeleteConfirmer } from '$lib/file-explorer/pane/dialog-props'

vi.mock('$lib/tauri-commands', () => ({
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

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

let component: Record<string, unknown> | null = null
let target: HTMLElement

afterEach(() => {
  if (component) {
    void unmount(component)
    component = null
  }
  target.remove()
})

function mountDialog() {
  target = document.createElement('div')
  document.body.appendChild(target)
  const confirms: Array<{ previewId: string | null; isPermanent: boolean }> = []
  const registration: { press: DeleteConfirmer | null; unregistered: boolean } = { press: null, unregistered: false }
  component = mount(DeleteDialog, {
    target,
    props: {
      sourceItems: [{ name: 'notes.txt', isDirectory: false, isSymlink: false, size: 12 }],
      sourcePaths: ['/Users/test/notes.txt'],
      sourceFolderPath: '/Users/test',
      isPermanent: false,
      supportsTrash: true,
      isFromCursor: true,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      onConfirm: (previewId: string | null, isPermanent: boolean) => confirms.push({ previewId, isPermanent }),
      registerConfirmer: (press: DeleteConfirmer) => {
        registration.press = press
        return () => {
          registration.unregistered = true
        }
      },
      onCancel: () => {},
    },
  }) as Record<string, unknown>
  return { confirms, registration }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await Promise.resolve()
    await tick()
  }
}

describe('an MCP confirm on the delete dialog', () => {
  it('sends the scan preview and the mode the switch shows now', async () => {
    const { confirms, registration } = mountDialog()
    await settle()

    // Opened as a trash; the person (or an agent's earlier step) flipped it to permanent.
    target.querySelector<HTMLElement>('[role="switch"]')?.click()
    await tick()

    expect(registration.press, 'the dialog registers its own confirm').not.toBeNull()
    registration.press?.()
    await settle()

    expect(confirms).toEqual([{ previewId: 'preview-1', isPermanent: true }])
  })

  it('takes its confirm back when it closes', async () => {
    const { registration } = mountDialog()
    await settle()

    if (component) void unmount(component)
    component = null

    expect(registration.unregistered).toBe(true)
  })
})
