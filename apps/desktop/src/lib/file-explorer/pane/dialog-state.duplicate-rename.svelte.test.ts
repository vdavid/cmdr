/**
 * The settled-transfer tail that names a duplicate, wired through the real
 * `createDialogState`.
 *
 * `duplicate-rename.test.ts` pins the decision itself; this file pins the two
 * things only the dialog state can answer: that the operation id is read while
 * the progress dialog still owns it (it releases the slot as it unmounts, so an
 * id read a beat later is `null` and there is no editor), and that a duplicate
 * only ever gets an editor on the route that COMPLETED.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import { setForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
import type { TransferProgressPropsData } from './dialog-props'
import type { FilePaneAPI } from './types'

const { getOperationLogDetail, moveCursorToNewFolder } = vi.hoisted(() => ({
  getOperationLogDetail: vi.fn(),
  moveCursorToNewFolder: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
  getOperationLogDetail,
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

/** Lets the unawaited journal read and cursor land settle. */
async function settle(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
}

beforeEach(() => {
  vi.clearAllMocks()
  setForegroundOperationId(null)
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

describe('a completed duplicate the trigger asked to name', () => {
  it('opens the editor on the name the journal reports', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(1, 0, 1024)
    await settle()

    expect(getOperationLogDetail).toHaveBeenCalledExactlyOnceWith('op-1', 1, 0)
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
    await settle()

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(startRename).not.toHaveBeenCalled()
  })

  it('is left alone when the operation was CANCELLED rather than completed', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')

    dialogs.handleTransferCancelled(0)
    await settle()

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(startRename).not.toHaveBeenCalled()
  })

  it('is left alone when the operation FAILED', async () => {
    const { dialogs, startRename } = makeState()
    dialogs.startTransferProgress(pasteDuplicateProps())
    setForegroundOperationId('op-1')

    dialogs.handleTransferError({ type: 'permission_denied', path: `${FOLDER}/photo.jpg`, message: 'nope' })
    await settle()

    expect(getOperationLogDetail).not.toHaveBeenCalled()
    expect(startRename).not.toHaveBeenCalled()
  })
})
