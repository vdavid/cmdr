/**
 * Foreground: adopting a running operation into the main window's progress
 * dialog, and the two rules that keep it from touching a user's files.
 *
 * 1. **The slot is single-occupancy, and "occupied" includes the invisible
 *    case.** While an archive-password prompt is up there is no progress dialog
 *    shown and a live `transferProgressProps` waiting for the submit. A guard
 *    that tested the SHOWN flag would let an adoption overwrite those props, and
 *    the password submit would then re-dispatch the adopted operation's sources
 *    to the adopted operation's destination. Here the two live in separate
 *    MODULES, so an adoption cannot reach the birth context at all — and it is
 *    refused outright, which this file pins from both ends.
 * 2. **An adopted view has no birth context and must not act as if it had.**
 *    Its outcome handlers touch no pane: no refresh, no selection change, no
 *    operation snapshot. `pane/DETAILS.md` § "Birth context" argues why.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import type { AdoptedOperationData, TransferProgressPropsData } from './dialog-props'
import type { WriteOperationError } from '../types'
import type { FilePaneAPI } from './types'
import { addToast } from '$lib/ui/toast'
import { removeEntryFromAllSnapshots } from '$lib/search/snapshot-store.svelte'
import { refreshListing } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

const SOURCE_FOLDER = '/Users/me/photos'

function makePaneRef(currentPath = SOURCE_FOLDER) {
  const spies = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    getCurrentPath: vi.fn(() => currentPath),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  }
  return { ref: spies as unknown as FilePaneAPI, spies }
}

function makeState(sourcePaneAt = SOURCE_FOLDER) {
  const rightPane = makePaneRef(sourcePaneAt)
  const leftPane = makePaneRef()
  const dialogs = createDialogState({
    getLeftPaneRef: () => leftPane.ref,
    getRightPaneRef: () => rightPane.ref,
    getFocusedPaneRef: () => rightPane.ref,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    onRefocus: vi.fn(),
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, rightPane, leftPane }
}

/** A move the RIGHT pane started, from the folder it is showing. */
function moveProps(): TransferProgressPropsData {
  return {
    operationType: 'move',
    sourcePaths: [`${SOURCE_FOLDER}/a.jpg`, `${SOURCE_FOLDER}/b.jpg`],
    sourceFolderPath: SOURCE_FOLDER,
    sourcePaneSide: 'right',
    destinationPath: '/Users/me/backup',
    direction: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: 'preview-1',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    fileCount: 2,
    folderCount: 0,
  }
}

/** A copy out of an encrypted archive: the path that keeps props alive with no
 *  dialog on screen. */
function archiveCopyProps(): TransferProgressPropsData {
  return {
    operationType: 'copy',
    sourcePaths: ['/Users/me/secret.zip/inner/report.pdf'],
    sourceFolderPath: '/Users/me/secret.zip/inner',
    sourcePaneSide: 'right',
    destinationPath: '/Users/me/out',
    direction: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: 'preview-1',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
  }
}

function adopted(operationId = 'op-42'): AdoptedOperationData {
  return {
    operationId,
    operationType: 'copy',
    sourcePath: '/Volumes/Card/DCIM',
    destinationPath: '/Users/me/import',
  }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('adopting an operation into the progress dialog', () => {
  it('shows the named operation when nothing else is up', () => {
    const { dialogs } = makeState()

    expect(dialogs.foregroundOperation(adopted())).toBe('adopted')

    expect(dialogs.showTransferProgressDialog).toBe(true)
    expect(dialogs.adoptedProgressProps).toEqual(adopted())
    // ❌ Never the birth slot: it is the archive-password re-dispatch's input.
    expect(dialogs.transferProgressProps).toBeNull()
  })

  it('is a no-op when the same operation is already showing', () => {
    const { dialogs } = makeState()
    dialogs.foregroundOperation(adopted())
    const first = dialogs.adoptedProgressProps

    expect(dialogs.foregroundOperation(adopted())).toBe('alreadyShowing')

    // The SAME props object: a replacement would remount the dialog, and a
    // remount disposes the session and builds a second one, with a second ETA
    // smoother that starts from nothing.
    expect(dialogs.adoptedProgressProps).toBe(first)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('refuses while another operation has the dialog, and says so', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(moveProps())

    expect(dialogs.foregroundOperation(adopted())).toBe('busy')

    expect(dialogs.adoptedProgressProps).toBeNull()
    expect(dialogs.transferProgressProps?.sourcePaths).toEqual(moveProps().sourcePaths)
    expect(addToast).toHaveBeenCalledTimes(1)
  })

  it('refuses while a password prompt holds a live operation with NO dialog shown', () => {
    // The wrong-write scenario, from both ends. Nothing is on screen but the
    // prompt, and `transferProgressProps` is what the submit re-dispatches.
    const { dialogs } = makeState()
    dialogs.startTransferProgress(archiveCopyProps())
    dialogs.handleTransferError({
      type: 'archive_needs_password',
      path: '/Users/me/secret.zip/inner/report.pdf',
      wrongAttempt: false,
    })
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).not.toBeNull()

    expect(dialogs.foregroundOperation(adopted())).toBe('busy')

    expect(dialogs.adoptedProgressProps).toBeNull()
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })

  it('leaves the password re-dispatch aimed at the operation the user unlocked', async () => {
    // The other end of the same scenario: after a refused adoption, the submit
    // must still copy the ARCHIVE's file to the archive copy's destination.
    const { dialogs } = makeState()
    dialogs.startTransferProgress(archiveCopyProps())
    dialogs.handleTransferError({
      type: 'archive_needs_password',
      path: '/Users/me/secret.zip/inner/report.pdf',
      wrongAttempt: false,
    })
    dialogs.foregroundOperation(adopted())

    dialogs.handleArchivePasswordSubmit('hunter2')

    await vi.waitFor(() => {
      expect(dialogs.showTransferProgressDialog).toBe(true)
    })
    expect(dialogs.transferProgressProps?.sourcePaths).toEqual(['/Users/me/secret.zip/inner/report.pdf'])
    expect(dialogs.transferProgressProps?.destinationPath).toBe('/Users/me/out')
    expect(dialogs.adoptedProgressProps).toBeNull()
  })

  it('refuses while a confirmation dialog is up', () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation({
      sourceItems: [],
      sourcePaths: [`${SOURCE_FOLDER}/a.jpg`],
      sourceFolderPath: SOURCE_FOLDER,
      isPermanent: false,
      supportsTrash: true,
      isFromCursor: false,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
    })

    expect(dialogs.foregroundOperation(adopted())).toBe('busy')
    expect(dialogs.adoptedProgressProps).toBeNull()
  })
})

describe('an operation born while an adopted one is showing', () => {
  // Birth wins: the adopted operation is still running and still listed in the
  // queue window, which is where it goes back to. ❌ The two must never stack —
  // the started dialog renders from the OTHER slot, so nothing else stops them.

  it('hands the adopted one back rather than stacking a second dialog', () => {
    const { dialogs } = makeState()
    dialogs.foregroundOperation(adopted())

    dialogs.startTransferProgress(moveProps())

    expect(dialogs.adoptedProgressProps).toBeNull()
    expect(dialogs.transferProgressProps?.sourcePaths).toEqual(moveProps().sourcePaths)
    expect(dialogs.showTransferProgressDialog).toBe(true)
  })

  it('does the same for a confirmed delete', () => {
    const { dialogs } = makeState()
    dialogs.foregroundOperation(adopted())
    dialogs.showDeleteConfirmation({
      sourceItems: [],
      sourcePaths: [`${SOURCE_FOLDER}/a.jpg`],
      sourceFolderPath: SOURCE_FOLDER,
      isPermanent: true,
      supportsTrash: true,
      isFromCursor: false,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
    })

    dialogs.handleDeleteConfirm(null, true)

    expect(dialogs.adoptedProgressProps).toBeNull()
    expect(dialogs.transferProgressProps?.operationType).toBe('delete')
  })
})

describe("an adopted view's outcomes touch no pane", () => {
  it('completes without refreshing, clearing, or re-selecting anything', () => {
    const { dialogs, rightPane, leftPane } = makeState()
    dialogs.foregroundOperation(adopted())

    dialogs.handleAdoptedComplete(12, 0, 4096)

    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(rightPane.spies.clearOperationSnapshot).not.toHaveBeenCalled()
    expect(leftPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(refreshListing).not.toHaveBeenCalled()
    // It still says what happened: the counts are facts about the OPERATION.
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.adoptedProgressProps).toBeNull()
  })

  it('purges no search snapshot, and neither does the started family', () => {
    // ❌ Not a gap to fill here. A dialog knows the operation's INTENT, which
    // misses a skip and a cancel and is absent from an adopted view entirely;
    // `$lib/search/snapshot-purge.ts` reads the per-path outcome stream for the
    // whole window instead. This pins that neither family reaches for the store.
    const { dialogs } = makeState()
    dialogs.foregroundOperation({ ...adopted(), operationType: 'move' })
    dialogs.handleAdoptedComplete(3, 0, 128)

    dialogs.startTransferProgress(moveProps())
    dialogs.handleTransferComplete(2, 0, 2048)

    expect(removeEntryFromAllSnapshots).not.toHaveBeenCalled()
  })

  it('cancels without adjusting a selection it did not take', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.foregroundOperation({ ...adopted(), operationType: 'move' })

    dialogs.handleAdoptedCancelled(2)

    expect(rightPane.spies.clearOperationSnapshot).not.toHaveBeenCalled()
    expect(rightPane.spies.selectAll).not.toHaveBeenCalled()
    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(refreshListing).not.toHaveBeenCalled()
    expect(dialogs.adoptedProgressProps).toBeNull()
  })

  it('shows a failure the same way, still without touching a pane', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.foregroundOperation(adopted())
    const error: WriteOperationError = { type: 'io_error', path: '/x', message: 'boom' }

    dialogs.handleAdoptedError(error)

    expect(dialogs.showTransferErrorDialog).toBe(true)
    expect(dialogs.transferErrorProps).toEqual({ operationType: 'copy', error })
    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(refreshListing).not.toHaveBeenCalled()
  })

  it('backgrounds again without touching a pane, and frees the slot for the next one', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.foregroundOperation(adopted())

    dialogs.handleAdoptedQueue()

    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(rightPane.spies.clearOperationSnapshot).not.toHaveBeenCalled()
    expect(dialogs.adoptedProgressProps).toBeNull()
    expect(dialogs.foregroundOperation(adopted('op-77'))).toBe('adopted')
  })
})

describe('a view whose pane has moved on since the operation was born', () => {
  // Started-by-this-view is not the same as fresh context: the axis is whether
  // the pane fields still describe the pane. A source pane that navigated away
  // mid-transfer has a selection the user made SOMEWHERE ELSE.

  it('refreshes and reports, but leaves the new selection alone', () => {
    const { dialogs, rightPane } = makeState('/Users/me/somewhere-else')
    dialogs.startTransferProgress(moveProps())

    dialogs.handleAdoptedQueue() // no-op: nothing adopted
    dialogs.handleTransferComplete(2, 0, 2048)

    expect(refreshListing).toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(rightPane.spies.clearOperationSnapshot).not.toHaveBeenCalled()
  })

  it('still clears the selection when the pane is where the operation started', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.startTransferProgress(moveProps())

    dialogs.handleTransferComplete(2, 0, 2048)

    expect(rightPane.spies.clearSelection).toHaveBeenCalled()
    expect(rightPane.spies.clearOperationSnapshot).toHaveBeenCalled()
  })

  it('leaves a cancelled operation the same way', () => {
    const { dialogs, rightPane } = makeState('/Users/me/somewhere-else')
    dialogs.startTransferProgress(moveProps())

    dialogs.handleTransferCancelled(1)

    expect(refreshListing).toHaveBeenCalled()
    expect(rightPane.spies.clearOperationSnapshot).not.toHaveBeenCalled()
    expect(rightPane.spies.selectAll).not.toHaveBeenCalled()
  })
})
