/**
 * The "Skip confirmation" setting at the one place every entry point opens a
 * confirmation: `showTransfer` and `showDeleteConfirmation` start the operation
 * straight away when it's on and the confirmation may be skipped, and show the
 * dialog otherwise. Which confirmations qualify: `confirmation-skip.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import type { DeleteDialogPropsData } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { FilePaneAPI } from './types'

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

const SOURCE_FOLDER = '/Users/me/photos'

function makePaneRef(): FilePaneAPI {
  return {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    getCurrentPath: vi.fn(() => SOURCE_FOLDER),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  } as unknown as FilePaneAPI
}

function makeState(skip: boolean) {
  const pane = makePaneRef()
  return createDialogState({
    getLeftPaneRef: () => pane,
    getRightPaneRef: () => pane,
    getFocusedPaneRef: () => pane,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    getExplorer: () => undefined,
    skipsConfirmations: () => skip,
    onRefocus: vi.fn(),
    onOpenInEditor: vi.fn(),
  })
}

function transferProps(overrides: Partial<TransferDialogPropsData> = {}): TransferDialogPropsData {
  return {
    operationType: 'copy',
    sourcePaths: [`${SOURCE_FOLDER}/first.jpg`],
    destinationPath: '/Users/me/backup',
    direction: 'left',
    currentVolumeId: 'root',
    fileCount: 1,
    folderCount: 0,
    sourceFolderPath: SOURCE_FOLDER,
    sortColumn: 'name',
    sortOrder: 'ascending',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    duplicateFollowUp: 'openRenameEditor',
    ...overrides,
  }
}

function deleteProps(overrides: Partial<DeleteDialogPropsData> = {}): DeleteDialogPropsData {
  return {
    sourceItems: [{ name: 'first.jpg', size: 10, isDirectory: false, isSymlink: false }],
    sourcePaths: [`${SOURCE_FOLDER}/first.jpg`],
    sourceFolderPath: SOURCE_FOLDER,
    isPermanent: false,
    supportsTrash: true,
    isFromCursor: true,
    sortColumn: 'name',
    sortOrder: 'ascending',
    sourceVolumeId: 'root',
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('showTransfer with confirmations skipped', () => {
  it('starts the copy into the other pane without showing the dialog', () => {
    const dialogs = makeState(true)

    dialogs.showTransfer(transferProps())

    expect(dialogs.showTransferDialog).toBe(false)
    expect(dialogs.transferDialogProps).toBeNull()
    expect(dialogs.showTransferProgressDialog).toBe(true)
    expect(dialogs.transferProgressProps).toMatchObject({
      operationType: 'copy',
      sourcePaths: [`${SOURCE_FOLDER}/first.jpg`],
      destinationPath: '/Users/me/backup',
      destVolumeId: 'root',
      previewId: null,
      conflictResolution: 'stop',
      duplicateFollowUp: 'openRenameEditor',
    })
  })

  it('still shows the dialog when this transfer has to show it', () => {
    const dialogs = makeState(true)

    dialogs.showTransfer(transferProps({ operationType: 'compress' }))

    expect(dialogs.showTransferDialog).toBe(true)
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })

  it('shows the dialog when the setting is off', () => {
    const dialogs = makeState(false)

    dialogs.showTransfer(transferProps())

    expect(dialogs.showTransferDialog).toBe(true)
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })
})

describe('showDeleteConfirmation with confirmations skipped', () => {
  it('moves to the trash without showing the dialog', () => {
    const dialogs = makeState(true)

    dialogs.showDeleteConfirmation(deleteProps())

    expect(dialogs.showDeleteDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(true)
    expect(dialogs.transferProgressProps).toMatchObject({
      operationType: 'trash',
      sourcePaths: [`${SOURCE_FOLDER}/first.jpg`],
      previewId: null,
      itemSizes: [10],
    })
  })

  it('always asks before a permanent delete', () => {
    const dialogs = makeState(true)

    dialogs.showDeleteConfirmation(deleteProps({ isPermanent: true }))

    expect(dialogs.showDeleteDialog).toBe(true)
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })
})
