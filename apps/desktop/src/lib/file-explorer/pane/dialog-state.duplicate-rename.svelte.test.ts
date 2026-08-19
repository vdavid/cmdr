/**
 * The settled-transfer tail that names a duplicate, wired through the real
 * `createDialogState` AND the real settle watch.
 *
 * `duplicate-rename.test.ts` pins the decision itself; this file pins what only
 * the wiring can answer:
 *
 * - The operation id is read while the progress dialog still owns it (it
 *   releases the slot as it unmounts, so an id read a beat later is `null`).
 * - The journal is asked only after `write-settled` for that operation, which is
 *   when its rows become readable at all. The settle watch is the REAL module
 *   here, driven off a fake event stream, because a stub for it is exactly what
 *   would hide a read that happens too early.
 * - Only the route that COMPLETED gets an editor.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import { setForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
import { initSettledOperationsWatch, destroySettledOperationsWatch } from '$lib/file-operations/settled-operations'
import type { TransferProgressPropsData } from './dialog-props'
import type { FilePaneAPI } from './types'

const { getOperationLogDetail, moveCursorToNewFolder, onWriteSettled } = vi.hoisted(() => ({
  getOperationLogDetail: vi.fn(),
  moveCursorToNewFolder: vi.fn(() => Promise.resolve()),
  onWriteSettled: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
  getOperationLogDetail,
  onWriteSettled,
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder }))

const FOLDER = '/Users/me/photos'

function makeState() {
  const startRename = vi.fn()
  const pane = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    getCurrentPath: vi.fn(() => FOLDER),
    hasParentEntry: vi.fn(() => true),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
    startRename,
  } as unknown as FilePaneAPI

  const dialogs = createDialogState({
    getLeftPaneRef: () => pane,
    getRightPaneRef: () => pane,
    getFocusedPaneRef: () => pane,
    getFocusedPaneSide: () => 'left',
    getShowHiddenFiles: () => false,
    onRefocus: vi.fn(),
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, startRename }
}

/** What ⌘V of one file into the folder it already lives in dispatches. */
function pasteDuplicateProps(overrides: Partial<TransferProgressPropsData> = {}): TransferProgressPropsData {
  return {
    operationType: 'copy',
    sourcePaths: [`${FOLDER}/photo.jpg`],
    sourceFolderPath: FOLDER,
    sourcePaneSide: 'right',
    destinationPath: FOLDER,
    direction: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    duplicateFollowUp: 'openRenameEditor',
    ...overrides,
  }
}

/** Feeds the settle watch, the way the backend's `write-settled` stream would. */
let emitSettled: (event: { operationId: string; operationType: 'copy' }) => void = () => {}

/** Lets the unawaited settle wait, journal read, and cursor land drain. */
async function drain(): Promise<void> {
  for (let i = 0; i < 6; i++) await Promise.resolve()
}

beforeEach(async () => {
  vi.clearAllMocks()
  setForegroundOperationId(null)
  onWriteSettled.mockImplementation((callback: (event: { operationId: string; operationType: 'copy' }) => void) => {
    emitSettled = callback
    return Promise.resolve(() => {})
  })
  await initSettledOperationsWatch()
  getOperationLogDetail.mockResolvedValue({
    operation: {},
    items: [
      {
        seq: 1,
        entryType: 'file',
        rowRole: 'rollbackUnit',
        sourceVolumeId: 'root',
        sourcePath: `${FOLDER}/photo.jpg`,
        destVolumeId: 'root',
        destPath: `${FOLDER}/photo (1).jpg`,
        size: 1,
        mtime: null,
        outcome: 'done',
        overwrote: false,
        rollbackSkipReason: null,
      },
    ],
    totalItems: 1,
  })
})

afterEach(() => {
  destroySettledOperationsWatch()
})

describe('a completed duplicate the trigger asked to name', () => {
  it('waits for the settle, then opens the editor on the name the journal reports', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(1, 0, 1024)
    await drain()

    // The journal has nothing readable for this op until it settles, so nothing
    // has been asked yet.
    expect(getOperationLogDetail).not.toHaveBeenCalled()

    emitSettled({ operationId: 'op-1', operationType: 'copy' })
    await drain()

    expect(getOperationLogDetail).toHaveBeenCalledExactlyOnceWith('op-1', 1, 0)
    expect(startRename).toHaveBeenCalledExactlyOnceWith({
      suppressExtensionWarning: true,
      expectedName: 'photo (1).jpg',
    })
  })

  it('opens the editor when the settle landed BEFORE the completion handling did', async () => {
    // The ordinary case: `write-settled` follows its terminal event by
    // microseconds while the dialog holds its completion for `MIN_DISPLAY_MS`.
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')
    emitSettled({ operationId: 'op-1', operationType: 'copy' })

    dialogs.handleTransferComplete(1, 0, 1024)
    await drain()

    expect(startRename).toHaveBeenCalledExactlyOnceWith({
      suppressExtensionWarning: true,
      expectedName: 'photo (1).jpg',
    })
  })

  it('is left alone when the trigger did not ask (the Duplicate command and drag)', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps({ duplicateFollowUp: 'nothing' }))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(1, 0, 1024)
    emitSettled({ operationId: 'op-1', operationType: 'copy' })
    await drain()

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(startRename).not.toHaveBeenCalled()
  })

  it('is left alone when the operation was CANCELLED rather than completed', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')

    dialogs.handleTransferCancelled(0)
    emitSettled({ operationId: 'op-1', operationType: 'copy' })
    await drain()

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(startRename).not.toHaveBeenCalled()
  })

  it('is left alone when the operation FAILED', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')

    dialogs.handleTransferError({ type: 'permission_denied', path: `${FOLDER}/photo.jpg`, message: 'nope' })
    emitSettled({ operationId: 'op-1', operationType: 'copy' })
    await drain()

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(startRename).not.toHaveBeenCalled()
  })
})
