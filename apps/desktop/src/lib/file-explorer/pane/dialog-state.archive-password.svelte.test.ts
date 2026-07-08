/**
 * The archive-password interception + re-dispatch path in `createDialogState`.
 *
 * A copy/move whose source is inside an encrypted archive surfaces as a
 * `WriteOperationError` of type `archive_needs_password`. `handleTransferError`
 * must intercept exactly that variant BEFORE the generic error dialog, show the
 * password prompt, and keep `transferProgressProps` alive so a successful unlock
 * re-dispatches the same op and a cancel settles it cleanly. Other variants must
 * still route to the generic error dialog.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState, type TransferProgressPropsData } from './dialog-state.svelte'
import type { WriteOperationError } from '../types'
import type { FilePaneAPI } from './types'

const { setArchivePassword, clearArchivePassword } = vi.hoisted(() => ({
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands', () => ({
  formatBytes: (n: number) => `${String(n)} B`,
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword,
  clearArchivePassword,
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

/** Minimal `FilePaneAPI` stub plus the spies its transfer-path members expose. */
function makePaneRef() {
  const spies = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  }
  return { ref: spies as unknown as FilePaneAPI, spies }
}

const onRefocus = vi.fn()

function makeState() {
  const rightPane = makePaneRef()
  const leftPane = makePaneRef()
  const dialogs = createDialogState({
    getLeftPaneRef: () => leftPane.ref,
    getRightPaneRef: () => rightPane.ref,
    getFocusedPaneRef: () => rightPane.ref,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    onRefocus,
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, rightPane, leftPane }
}

/** A copy op sourced from inside an encrypted zip on the `root` volume. */
function copyProps(): TransferProgressPropsData {
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
    conflictResolution: 'stop',
    scanInProgress: false,
  }
}

const needsPassword = (wrongAttempt: boolean): WriteOperationError => ({
  type: 'archive_needs_password',
  path: '/Users/me/secret.zip/inner/report.pdf',
  wrongAttempt,
})

beforeEach(() => {
  vi.clearAllMocks()
})

describe('archive-password interception', () => {
  it('intercepts archive_needs_password: prompts instead of the generic error dialog', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferError(needsPassword(false))

    expect(dialogs.showArchivePasswordDialog).toBe(true)
    expect(dialogs.showTransferErrorDialog).toBe(false)
    // The progress dialog is unmounted but its props stay alive for a retry.
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).not.toBeNull()

    const props = dialogs.archivePasswordProps
    expect(props).toEqual({
      archiveName: 'secret.zip',
      wrongAttempt: false,
      parentVolumeId: 'root',
      archivePath: '/Users/me/secret.zip/inner/report.pdf',
    })
  })

  it('passes wrongAttempt through so the re-prompt shows its distinct copy', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferError(needsPassword(true))

    expect(dialogs.archivePasswordProps?.wrongAttempt).toBe(true)
  })

  it('does NOT intercept other error variants (they still show the error dialog)', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    const ioError: WriteOperationError = { type: 'io_error', path: '/x', message: 'boom' }
    dialogs.handleTransferError(ioError)

    expect(dialogs.showArchivePasswordDialog).toBe(false)
    expect(dialogs.showTransferErrorDialog).toBe(true)
    expect(dialogs.transferProgressProps).toBeNull()
  })
})

describe('archive-password submit → re-dispatch', () => {
  it('stores the password and re-dispatches the same op with a fresh scan', async () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false))

    dialogs.handleArchivePasswordSubmit('hunter2')

    expect(setArchivePassword).toHaveBeenCalledWith('root', '/Users/me/secret.zip/inner/report.pdf', 'hunter2')
    // The prompt closes immediately; the re-dispatch runs after the store resolves.
    expect(dialogs.showArchivePasswordDialog).toBe(false)
    await vi.waitFor(() => {
      expect(dialogs.showTransferProgressDialog).toBe(true)
    })
    // The consumed preview is dropped so the retry re-scans the archive index.
    expect(dialogs.transferProgressProps?.previewId).toBeNull()
    expect(dialogs.transferProgressProps?.scanInProgress).toBe(false)
    // Same operation otherwise.
    expect(dialogs.transferProgressProps?.sourcePaths).toEqual(['/Users/me/secret.zip/inner/report.pdf'])
  })
})

describe('archive-password cancel → settle', () => {
  it('forgets the password and settles like a dismissed transfer (nothing stuck)', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false))

    dialogs.handleArchivePasswordCancel()

    expect(clearArchivePassword).toHaveBeenCalledWith('root', '/Users/me/secret.zip/inner/report.pdf')
    expect(dialogs.showArchivePasswordDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.archivePasswordProps).toBeNull()
    expect(dialogs.transferProgressProps).toBeNull()
    expect(rightPane.spies.clearSelection).toHaveBeenCalled()
    expect(onRefocus).toHaveBeenCalled()
  })
})
