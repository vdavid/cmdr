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
import type { OpKind } from '$lib/ipc/bindings'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import TransferProgressDialog from './TransferProgressDialog.svelte'

const ADOPTED = 'op-9'

const { progressCbs } = vi.hoisted(() => ({ progressCbs: [] as ((event: Record<string, unknown>) => void)[] }))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  copyFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn((cb: (event: Record<string, unknown>) => void) => {
    progressCbs.push(cb)
    return Promise.resolve(() => {})
  }),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
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
        reverses: null,
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
async function mountAdopted(
  over: { operationType?: 'copy' | 'move' | 'delete'; reverses?: OpKind | null } = {},
): Promise<{ component: ReturnType<typeof mount>; target: HTMLDivElement }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      adoptOperationId: ADOPTED,
      operationType: over.operationType ?? ('copy' as const),
      reverses: over.reverses ?? null,
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

/** A reversal's bar has honest totals from its first frame: the backend counts
 *  the journal's rollback rows before the first act, so there is no scan phase. */
async function emitReversalTick(operationType: string, filesTotal: number): Promise<void> {
  for (const cb of [...progressCbs]) {
    cb({
      operationId: ADOPTED,
      operationType,
      phase: 'rolling_back',
      currentFile: 'report.pdf',
      filesDone: 12,
      filesTotal,
      bytesDone: 25,
      bytesTotal: 100,
    })
  }
  await flushPromises()
}

function title(target: HTMLElement): string {
  return target.querySelector('.dialog-title-bar h2')?.textContent.trim() ?? ''
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

describe('TransferProgressDialog, adopting a REVERSAL', () => {
  it('titles the reversal of a move by what it does, with the scope the journal counted', async () => {
    // Undoing a move is registered as a move, so `operationType` alone would
    // title this "Moving..." — the thing the person asked to undo.
    const { component, target } = await mountAdopted({ operationType: 'move', reverses: 'move' })
    await emitReversalTick('move', 1240)

    expect(title(target)).toBe('Putting 1,240 files back...')

    void unmount(component)
  })

  it('never titles a restore as a rollback that deletes', async () => {
    const { component, target } = await mountAdopted({ operationType: 'move', reverses: 'trash' })
    await emitReversalTick('move', 3)

    expect(title(target)).toBe('Putting 3 files back...')
    expect(title(target)).not.toContain('Deleting')
    expect(title(target)).not.toBe('Rolling back...')

    void unmount(component)
  })

  it('titles the reversal of a copy as the delete it honestly is', async () => {
    const { component, target } = await mountAdopted({ operationType: 'delete', reverses: 'copy' })
    await emitReversalTick('delete', 1240)

    expect(title(target)).toBe('Deleting the 1,240 files it created...')

    void unmount(component)
  })

  it('leaves an in-flight rollback with its own title, which is the honest one there', async () => {
    // A CANCELLED copy cleaning up after itself carries no `reverses`. Its bar
    // drains backwards on a dialog that was already full, and it really is
    // deleting the partials.
    const { component, target } = await mountAdopted({ operationType: 'copy', reverses: null })
    await emitReversalTick('copy', 1240)

    expect(title(target)).toBe('Rolling back...')

    void unmount(component)
  })

  it('offers no Rollback button on a reversal: this operation IS the undo', async () => {
    const { component, target } = await mountAdopted({ operationType: 'move', reverses: 'move' })
    await emitReversalTick('move', 1240)

    const buttons = [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).not.toContain('Rollback')
    expect(buttons).not.toContain('Rolling back...')

    void unmount(component)
  })
})
