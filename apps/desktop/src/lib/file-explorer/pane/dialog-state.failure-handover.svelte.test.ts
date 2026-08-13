/**
 * The failure handover in `createDialogState`: the seam between the foreground
 * error dialog and the ambient surfaces that would otherwise repeat it.
 *
 * The backend retains every failure unconditionally (it can't know a modal is
 * up), and the progress dialog releases its operation slot as it unmounts, a
 * beat BEFORE the retained row reaches the snapshot. So `handleTransferError`
 * hands the id to the second slot while it still can, and
 * `handleTransferErrorClose` releases it and drops the retained row. Break
 * either half and the user gets a toast plus a corner mark for the failure
 * they're reading, or a queue row nobody ever dismisses.
 *
 * The slots here are the REAL module: what's under test is the handover, and a
 * fake would happily pass with the ordering broken.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState, type TransferProgressPropsData } from './dialog-state.svelte'
import {
  clearForegroundOperation,
  getForegroundFailureId,
  setForegroundFailureId,
  setForegroundOperationId,
} from '$lib/file-operations/foreground-operation.svelte'
import type { WriteOperationError } from '../types'
import type { FilePaneAPI } from './types'

const { dismissFailedOperation } = vi.hoisted(() => ({
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  dismissFailedOperation,
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

/** Minimal `FilePaneAPI` stub: the transfer paths call into it on every route. */
function makePaneRef(): FilePaneAPI {
  return {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    // The pane is where the operation was born, which is what the settled-transfer
    // tail checks before it touches a selection: a pane that has navigated since
    // holds one the user made somewhere else.
    getCurrentPath: vi.fn(() => '/Users/me/photos'),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  } as unknown as FilePaneAPI
}

function makeState() {
  const pane = makePaneRef()
  return createDialogState({
    getLeftPaneRef: () => pane,
    getRightPaneRef: () => pane,
    getFocusedPaneRef: () => pane,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    onRefocus: vi.fn(),
    onOpenInEditor: vi.fn(),
  })
}

function copyProps(): TransferProgressPropsData {
  return {
    operationType: 'copy',
    sourcePaths: ['/Users/me/photos/a.raw'],
    sourceFolderPath: '/Users/me/photos',
    sourcePaneSide: 'right',
    destinationPath: '/Volumes/Naspolya/Backup',
    direction: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: 'preview-1',
    sourceVolumeId: 'root',
    destVolumeId: 'naspolya',
    conflictResolution: 'stop',
  }
}

const ioError: WriteOperationError = { type: 'io_error', path: '/Users/me/photos/a.raw', message: 'disk went away' }
const needsPassword: WriteOperationError = {
  type: 'archive_needs_password',
  path: '/Users/me/secret.zip/a.raw',
  wrongAttempt: false,
}

/**
 * Runs a copy up to the moment it fails, exactly as the app does: the progress
 * dialog owns the operation, the error lands, and THEN the dialog unmounts and
 * releases the operation slot (Svelte tears it down after the handler returns).
 */
function failInForeground(dialogs: ReturnType<typeof makeState>, operationId: string, error = ioError): void {
  dialogs.startTransferProgress(copyProps())
  setForegroundOperationId(operationId)
  dialogs.handleTransferError(error)
  clearForegroundOperation(operationId)
}

beforeEach(() => {
  vi.clearAllMocks()
  setForegroundOperationId(null)
  setForegroundFailureId(null)
})

describe('foreground failure handover', () => {
  it('claims the failure while the progress dialog still owns the operation', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-7')

    // The operation slot is gone with the progress dialog; the failure slot is
    // what keeps the corner chip and the failure toast quiet from here on.
    expect(getForegroundFailureId()).toBe('op-7')
    expect(dialogs.showTransferErrorDialog).toBe(true)
  })

  it('claims nothing when no dialog owned the operation', () => {
    // A failure the foreground never owned (nothing in the slot) must not park a
    // stale id: the next Close would dismiss a retained row the user never read.
    const dialogs = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferError(ioError)

    expect(getForegroundFailureId()).toBeNull()
  })

  it('releases the slot and dismisses the retained row when the dialog closes', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-7')

    dialogs.handleTransferErrorClose()

    expect(dismissFailedOperation).toHaveBeenCalledWith('op-7')
    expect(getForegroundFailureId()).toBeNull()
    expect(dialogs.showTransferErrorDialog).toBe(false)
  })

  it('dismisses nothing when the closing dialog owned no failure', () => {
    // A retained row belonging to a BACKGROUNDED failure only goes away on an
    // explicit Dismiss. Closing an unrelated error dialog must not take it.
    const dialogs = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(ioError)

    dialogs.handleTransferErrorClose()

    expect(dismissFailedOperation).not.toHaveBeenCalled()
  })

  it('does NOT claim on the archive-password prompt', () => {
    // `ArchiveNeedsPassword` is a recoverable prompt, not a failure: the backend
    // retains nothing for it (typed exclusion in `record_failure`), so a claim
    // here would park an id that no row will ever match — and silence the next
    // real failure of that same operation.
    const dialogs = makeState()
    dialogs.startTransferProgress(copyProps())
    setForegroundOperationId('op-7')

    dialogs.handleTransferError(needsPassword)

    expect(dialogs.showArchivePasswordDialog).toBe(true)
    expect(getForegroundFailureId()).toBeNull()
  })

  it('lets a second failure take the slot, so Close drops the row on screen', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-7')
    failInForeground(dialogs, 'op-8')

    expect(getForegroundFailureId()).toBe('op-8')

    dialogs.handleTransferErrorClose()

    expect(dismissFailedOperation).toHaveBeenCalledTimes(1)
    expect(dismissFailedOperation).toHaveBeenCalledWith('op-8')
  })
})
