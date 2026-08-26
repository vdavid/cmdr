/**
 * `go-to-trash.ts`: taking the user to the trash, by volume or by operation.
 *
 * The cases worth pinning are the ones where something is missing: no trash on
 * this drive, no in-trash location recorded, and a cursor target the listing
 * won't show. None of them may surface as a fault, and none may leave the user
 * somewhere other than a trash.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { goToTrash, goToTrashedItems } from './go-to-trash'
import type { Location, OperationLogDetail } from '$lib/tauri-commands'
import type { ToastOptions } from '$lib/ui/toast/toast-store.svelte'
import type { PaneRevealAPI } from '$lib/file-explorer/navigation/navigate-and-select'

const {
  getTrashDir,
  getOperationLogDetail,
  whenOperationSettled,
  resolveLocationOrToast,
  navigateToDirInBestPane,
  revealFileInBestPane,
  addToast,
} = vi.hoisted(() => ({
  getTrashDir: vi.fn<(path: string) => Promise<string | null>>(),
  // `limit` and `offset` are both numbers, so they go in as a rest param rather
  // than a confusable positional pair (`cmdr/no-confusable-callback-params`); the
  // assertions only ever read the id.
  getOperationLogDetail: vi.fn<(id: string, ...paging: number[]) => Promise<OperationLogDetail | null>>(),
  whenOperationSettled: vi.fn<(id: string) => Promise<boolean>>(),
  resolveLocationOrToast: vi.fn<(dir: string) => Promise<Location | null>>(),
  navigateToDirInBestPane: vi.fn<(explorer: PaneRevealAPI, location: Location) => Promise<void>>(),
  revealFileInBestPane: vi.fn<(explorer: PaneRevealAPI, location: Location, name: string) => Promise<void>>(),
  addToast: vi.fn<(message: string, options?: ToastOptions) => string>(),
}))

vi.mock('$lib/tauri-commands', () => ({ getTrashDir, getOperationLogDetail }))
vi.mock('../settled-operations', () => ({ whenOperationSettled }))
vi.mock('$lib/file-explorer/navigation/navigate-and-select', () => ({
  resolveLocationOrToast,
  navigateToDirInBestPane,
  revealFileInBestPane,
}))
vi.mock('$lib/ui/toast', () => ({ addToast }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))

const TRASH_LOCATION: Location = { volumeId: 'root', path: '/Users/me/.Trash' }

function explorerStub(): PaneRevealAPI {
  return {
    getFocusedPane: () => 'left',
    setFocusedPane: vi.fn(),
    getPaneLocation: () => ({ volumeId: 'root', volumePath: '/', path: '/Users/me/Documents' }),
    navigate: vi.fn(),
    moveCursor: vi.fn(),
  } as unknown as PaneRevealAPI
}

/** One `rollbackUnit` row carrying where the OS actually put the item. */
function trashedItem(destPath: string | null) {
  return { seq: 0, rowRole: 'rollbackUnit', sourcePath: '/Users/me/Documents/notes.txt', destPath }
}

/** Only the item rows matter here; the header and totals never get read. */
function detail(items: unknown[]): OperationLogDetail {
  return { items } as unknown as OperationLogDetail
}

beforeEach(() => {
  vi.clearAllMocks()
  whenOperationSettled.mockResolvedValue(true)
  resolveLocationOrToast.mockResolvedValue(TRASH_LOCATION)
  getTrashDir.mockResolvedValue('/Users/me/.Trash')
})

describe('goToTrash', () => {
  it('opens the trash of the volume the focused pane is standing on', async () => {
    const explorer = explorerStub()
    await goToTrash(explorer)

    // The pane's own directory picks the volume: standing on an external drive
    // has to open THAT drive's trash, not the boot volume's.
    expect(getTrashDir).toHaveBeenCalledWith('/Users/me/Documents')
    expect(navigateToDirInBestPane).toHaveBeenCalledWith(explorer, TRASH_LOCATION)
  })

  it('says so, and navigates nowhere, when the drive keeps no trash', async () => {
    getTrashDir.mockResolvedValue(null)
    await goToTrash(explorerStub())

    expect(navigateToDirInBestPane).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith(
      'fileOperations.trash.noTrashHere',
      expect.objectContaining({ level: 'info' }),
    )
  })

  it('is a no-op without an explorer (HMR or pre-mount)', async () => {
    await goToTrash(undefined)
    expect(getTrashDir).not.toHaveBeenCalled()
  })
})

describe('goToTrashedItems', () => {
  it('lands on the item at the location the journal recorded', async () => {
    getOperationLogDetail.mockResolvedValue(detail([trashedItem('/Users/me/.Trash/notes.txt')]))
    const explorer = explorerStub()

    await goToTrashedItems(explorer, 'op-1', '/Users/me/Documents')

    expect(resolveLocationOrToast).toHaveBeenCalledWith('/Users/me/.Trash')
    expect(revealFileInBestPane).toHaveBeenCalledWith(explorer, TRASH_LOCATION, 'notes.txt')
  })

  it('waits for the settle before reading the journal', async () => {
    // The journal flushes its buffered item rows in the finalize barrier, so a
    // read at completion time comes back empty. Order matters, not just the call.
    const order: string[] = []
    whenOperationSettled.mockImplementation(() => {
      order.push('settled')
      return Promise.resolve(true)
    })
    getOperationLogDetail.mockImplementation(() => {
      order.push('read')
      return Promise.resolve(detail([trashedItem('/Users/me/.Trash/notes.txt')]))
    })

    await goToTrashedItems(explorerStub(), 'op-1', '/Users/me/Documents')
    expect(order).toEqual(['settled', 'read'])
  })

  it('skips the interior rows and lands on a real top-level item', async () => {
    getOperationLogDetail.mockResolvedValue(
      detail([
        { seq: 0, rowRole: 'searchOnly', sourcePath: '/Users/me/Documents/photos/a.jpg', destPath: null },
        trashedItem('/Users/me/.Trash/photos'),
      ]),
    )

    await goToTrashedItems(explorerStub(), 'op-1', '/Users/me/Documents')
    expect(revealFileInBestPane).toHaveBeenCalledWith(expect.anything(), TRASH_LOCATION, 'photos')
  })

  it('falls back to the source volume trash when no in-trash location was recorded', async () => {
    // Linux records none, and a row can be missing after a crash. Getting the
    // user to the right trash is the point; the cursor is the bonus.
    getOperationLogDetail.mockResolvedValue(detail([trashedItem(null)]))
    const explorer = explorerStub()

    await goToTrashedItems(explorer, 'op-1', '/Users/me/Documents')

    expect(getTrashDir).toHaveBeenCalledWith('/Users/me/Documents')
    expect(navigateToDirInBestPane).toHaveBeenCalledWith(explorer, TRASH_LOCATION)
    expect(revealFileInBestPane).not.toHaveBeenCalled()
  })

  it('falls back the same way when the operation never settles', async () => {
    whenOperationSettled.mockResolvedValue(false)
    await goToTrashedItems(explorerStub(), 'op-1', '/Users/me/Documents')

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(navigateToDirInBestPane).toHaveBeenCalled()
  })

  it('still leaves the user in the trash when the cursor target is hidden', async () => {
    // `moveCursor` THROWS on a name the visible listing doesn't hold, which is
    // what a trashed dotfile is with "show hidden files" off. Navigation already
    // happened by then, so the throw must not escape as a fault.
    getOperationLogDetail.mockResolvedValue(detail([trashedItem('/Users/me/.Trash/.env')]))
    revealFileInBestPane.mockRejectedValue(new Error('".env" not found in the left pane listing'))

    await expect(goToTrashedItems(explorerStub(), 'op-1', '/Users/me/Documents')).resolves.toBeUndefined()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('is a no-op without an explorer (HMR or pre-mount)', async () => {
    await goToTrashedItems(undefined, 'op-1', '/Users/me/Documents')
    expect(whenOperationSettled).not.toHaveBeenCalled()
  })
})
