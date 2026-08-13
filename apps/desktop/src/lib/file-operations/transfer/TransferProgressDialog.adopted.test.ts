/**
 * What a dialog that ADOPTED a running operation says it is working on.
 *
 * Show, from the operation queue, hands over an `operationId` and the two paths
 * the registry snapshot knows — and nothing pane-relative, because the snapshot
 * names paths, not panes. The user crosses from a queue row reading
 * "big → dest" to this dialog, so it has to name the same two ends; a bare
 * "Copying..." over an unnamed transfer is the one thing it must not be.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot } from '$lib/tauri-commands'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import TransferProgressDialog from './TransferProgressDialog.svelte'

const ADOPTED = 'op-9'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  copyFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  listOperations: vi.fn(() =>
    Promise.resolve<OperationSnapshot[]>([
      {
        operationId: ADOPTED,
        operationType: 'copy',
        status: 'running',
        source: '/Users/test/big',
        destination: '/Volumes/backup/dest',
        supportsRollback: true,
        error: null,
      },
    ]),
  ),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

beforeEach(async () => {
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

/** Mounts the dialog exactly as `DialogManager` does on the Show path: the id
 *  and the snapshot's two paths, with no `direction`. */
async function mountAdopted(): Promise<{ component: ReturnType<typeof mount>; target: HTMLDivElement }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      adoptOperationId: ADOPTED,
      operationType: 'copy' as const,
      sourceFolderPath: '/Users/test/big',
      destinationPath: '/Volumes/backup/dest',
      onComplete: vi.fn(),
      onCancelled: vi.fn(),
      onError: vi.fn(),
      onQueue: vi.fn(),
    },
  })
  await flushPromises()
  return { component, target }
}

describe('TransferProgressDialog, adopting a running operation', () => {
  it('names both ends of the transfer even with no pane-relative direction', async () => {
    const { component, target } = await mountAdopted()

    const source = target.querySelector('.folder-name.source')?.textContent.trim()
    const destination = target.querySelector('.folder-name.destination')?.textContent.trim()
    expect(source, 'the adopted dialog names where the files come from').toBe('big')
    expect(destination, 'and where they are going').toBe('dest')

    void unmount(component)
  })

  it('reads the two ends in source-to-destination order', async () => {
    // Without a pane to be relative to, the arrow means "from → to", the same
    // thing the queue row the user just came from shows.
    const { component, target } = await mountAdopted()

    const names = [...target.querySelectorAll('.direction-indicator > span')].map((el) => el.textContent.trim())
    expect(names).toEqual(['big', '→', 'dest'])

    void unmount(component)
  })
})
