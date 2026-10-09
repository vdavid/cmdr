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
import { createDialogState } from './dialog-state.svelte'
import type { TransferProgressPropsData } from './dialog-props'
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
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
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
    // No pane navigation in these suites; the trash toast is the only consumer.
    getExplorer: () => undefined,
    onRefocus: vi.fn(),
    skipsConfirmations: () => false,
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
    duplicateFollowUp: 'nothing',
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
  dialogs.handleTransferError(error, null)
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

    dialogs.handleTransferError(ioError, null)

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
    dialogs.handleTransferError(ioError, null)

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

    dialogs.handleTransferError(needsPassword, null)

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

/**
 * The error dialog's Retry (cmdr-reports#17: a retryable error that offered no
 * Retry). It starts the SAME operation again as a new one, and settles the failed
 * one exactly as Close would.
 */
describe('retry from the error dialog', () => {
  it('starts the failed operation again, as a new one with a fresh preview', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-7')
    expect(dialogs.transferErrorProps?.retry?.sourcePaths).toEqual(copyProps().sourcePaths)

    dialogs.handleTransferErrorRetry()

    expect(dialogs.showTransferErrorDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(true)
    const again = dialogs.transferProgressProps
    expect(again?.sourcePaths).toEqual(copyProps().sourcePaths)
    expect(again?.destinationPath).toBe(copyProps().destinationPath)
    expect(again?.conflictResolution).toBe('stop')
    // The backend refuses a second claim on one preview.
    expect(again?.previewId).toBeNull()
  })

  it('settles the failed operation like a close', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-7')

    dialogs.handleTransferErrorRetry()

    expect(dismissFailedOperation).toHaveBeenCalledWith('op-7')
    expect(getForegroundFailureId()).toBeNull()
  })

  it('never answers an MCP round-trip twice', () => {
    const dialogs = makeState()
    dialogs.startTransferProgress({ ...copyProps(), mcpRequestId: 'mcp-1', initiator: 'aiClient' })
    dialogs.handleTransferError(ioError, null)

    dialogs.handleTransferErrorRetry()

    expect(dialogs.transferProgressProps?.mcpRequestId).toBeUndefined()
    expect(dialogs.transferProgressProps?.initiator).toBe('user')
  })
})

/**
 * "Copy anyway" after a space shortfall (#351). The pre-flight's figure is an
 * upper bound, so the person decides: the same copy starts again, told to skip
 * the free-space check.
 */
describe('copy anyway from the error dialog', () => {
  const shortfall: WriteOperationError = {
    type: 'insufficient_space',
    required: 2_000_000_000,
    available: 500_000_000,
    volumeName: 'Naspolya',
  }

  it('starts the same copy again with the free-space check skipped', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-9', shortfall)

    dialogs.handleTransferErrorCopyAnyway()

    expect(dialogs.showTransferErrorDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(true)
    const again = dialogs.transferProgressProps
    expect(again?.sourcePaths).toEqual(copyProps().sourcePaths)
    expect(again?.conflictResolution).toBe('stop')
    expect(again?.spaceShortfall).toBe('proceed')
    expect(again?.previewId).toBeNull()
    expect(dismissFailedOperation).toHaveBeenCalledWith('op-9')
  })

  it('leaves a plain retry asking about space again', () => {
    const dialogs = makeState()
    failInForeground(dialogs, 'op-10', shortfall)

    dialogs.handleTransferErrorRetry()

    expect(dialogs.transferProgressProps?.spaceShortfall).toBeUndefined()
  })
})
