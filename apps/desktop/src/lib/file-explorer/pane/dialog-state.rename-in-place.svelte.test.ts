/**
 * A Move dialog confirmed with `renameInPlace` hands the item to the SOURCE pane's
 * rename flow instead of starting a transfer: F2's conflict and extension asks, a
 * case-only change on a case-folding volume, and one undo row, on every volume.
 * It falls back to the transfer whenever the pane can't take it.
 */

import { describe, it, expect, vi } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import type { TransferConfirmPayload } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { FilePaneAPI } from './types'

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
}))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))

const FOLDER = '/Users/me/notes'

function pane(currentPath: string) {
  return {
    startRename: vi.fn(),
    getCurrentPath: vi.fn(() => currentPath),
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    hasParentEntry: vi.fn(() => true),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  }
}

function setup(sourcePanePath = FOLDER) {
  // F6 from the LEFT pane: `direction: 'right'` points at the destination.
  const left = pane(sourcePanePath)
  const right = pane('/Users/me/elsewhere')
  const dialogs = createDialogState({
    getLeftPaneRef: () => left as unknown as FilePaneAPI,
    getRightPaneRef: () => right as unknown as FilePaneAPI,
    getFocusedPaneRef: () => left as unknown as FilePaneAPI,
    getFocusedPaneSide: () => 'left',
    getShowHiddenFiles: () => false,
    getExplorer: () => undefined,
    onRefocus: vi.fn(),
    skipsConfirmations: () => false,
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, left, right }
}

function moveProps(overrides: Partial<TransferDialogPropsData> = {}): TransferDialogPropsData {
  return {
    operationType: 'move',
    sourcePaths: [`${FOLDER}/notes.txt`],
    destinationPath: '/Users/me/elsewhere',
    direction: 'right',
    currentVolumeId: 'root',
    fileCount: 1,
    folderCount: 0,
    sourceFolderPath: FOLDER,
    sortColumn: 'name',
    sortOrder: 'ascending',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    duplicateFollowUp: 'nothing',
    ...overrides,
  }
}

const CONFIRM: TransferConfirmPayload = {
  destination: FOLDER,
  destinationName: 'Notes.txt',
  volumeId: 'root',
  previewId: null,
  conflictResolution: 'stop',
  operationType: 'move',
  preKnownConflicts: [],
  renameInPlace: true,
}

describe('a Move inside its own folder', () => {
  it('renames through the source pane and starts no transfer', () => {
    const { dialogs, left, right } = setup()
    dialogs.showTransfer(moveProps())

    dialogs.handleTransferConfirm(CONFIRM)

    expect(left.startRename).toHaveBeenCalledWith({
      initialName: 'Notes.txt',
      commitTarget: { path: `${FOLDER}/notes.txt`, isDirectory: false },
    })
    expect(right.startRename).not.toHaveBeenCalled()
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.showTransferDialog).toBe(false)
  })

  it('names a folder as a folder', () => {
    const { dialogs, left } = setup()
    dialogs.showTransfer(moveProps({ sourcePaths: [`${FOLDER}/drafts`], fileCount: 0, folderCount: 1 }))

    dialogs.handleTransferConfirm({ ...CONFIRM, destinationName: 'Drafts' })

    expect(left.startRename).toHaveBeenCalledWith({
      initialName: 'Drafts',
      commitTarget: { path: `${FOLDER}/drafts`, isDirectory: true },
    })
  })

  it('falls back to the transfer when the source pane has moved to another folder', () => {
    const { dialogs, left } = setup('/Users/me/somewhere-else')
    dialogs.showTransfer(moveProps())

    dialogs.handleTransferConfirm(CONFIRM)

    expect(left.startRename).not.toHaveBeenCalled()
    expect(dialogs.showTransferProgressDialog).toBe(true)
  })

  it('keeps the transfer for an agent, whose MCP call waits for an operation id', () => {
    const { dialogs, left } = setup()
    dialogs.showTransfer(moveProps({ mcpRequestId: 'req-1', initiator: 'aiClient' }))

    dialogs.handleTransferConfirm(CONFIRM)

    expect(left.startRename).not.toHaveBeenCalled()
    expect(dialogs.showTransferProgressDialog).toBe(true)
  })
})
