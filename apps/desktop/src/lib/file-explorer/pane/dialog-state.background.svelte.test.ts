/**
 * The main window's half of a background start (#381): F2 (or the Background
 * button) in the copy/move/compress or trash dialog.
 *
 * What it must hold, end to end through a real `createDialogState`:
 *
 * - **The same operation Enter would start**, from the same payload, with no
 *   progress dialog EVER mounted and the progress slot left free, so the next
 *   F5 opens at once (TC's F5, F2, F5, F2).
 * - **A trash, never a permanent delete.** The delete dialog's background
 *   confirm carries no mode at all, so there is nothing to get wrong here.
 * - **Every follow-up a foreground start has still lands**, or is absent on
 *   purpose: a refused start opens the error dialog, and its Retry starts in the
 *   background again; an archive-password stop raises the prompt, and the
 *   person's submit re-dispatches in the background again.
 * - **A job sent to the background from the progress dialog gets the same
 *   password follow-up**: the backend doesn't retain that stop, so before this
 *   nothing asked.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { WriteOperationError } from '../types'

vi.mock('$lib/tauri-commands', async () => ({
  ...(await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).tauriCommandsMock(),
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))
vi.mock('$lib/file-operations/queue/queue-window', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).queueWindowMock(),
)
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/settings', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).settingsMock(),
)
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

import { createDialogState } from './dialog-state.svelte'
import { listeners, settle, snapshot } from '$lib/file-operations/transfer/test-transfer-progress-harness.svelte'
import { copyBetweenVolumes, deleteFiles, listOperations, trashFiles } from '$lib/tauri-commands'
import { dispatchTransferOperation } from '$lib/file-operations/transfer/transfer-dispatch'
import { addToast } from '$lib/ui/toast'
import { setForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
import TrashCompleteToastContent from '$lib/file-operations/delete/TrashCompleteToastContent.svelte'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import type { DeleteDialogPropsData, TransferConfirmPayload } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { FilePaneAPI } from './types'

const SOURCE_FOLDER = '/Users/me/photos'

function makePaneRef() {
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

function makeState() {
  const onRefocus = vi.fn()
  const left = makePaneRef()
  const right = makePaneRef()
  const dialogs = createDialogState({
    getLeftPaneRef: () => left,
    getRightPaneRef: () => right,
    getFocusedPaneRef: () => left,
    getFocusedPaneSide: () => 'left',
    getShowHiddenFiles: () => false,
    getExplorer: () => undefined,
    onRefocus,
    skipsConfirmations: () => false,
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, onRefocus }
}

function transferDialogProps(): TransferDialogPropsData {
  return {
    operationType: 'copy',
    sourcePaths: [`${SOURCE_FOLDER}/a.jpg`, `${SOURCE_FOLDER}/b.jpg`],
    destinationPath: '/Users/me/backup',
    direction: 'right',
    currentVolumeId: 'root',
    fileCount: 2,
    folderCount: 0,
    sourceFolderPath: SOURCE_FOLDER,
    sortColumn: 'name',
    sortOrder: 'ascending',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    duplicateFollowUp: 'nothing',
  }
}

function confirmPayload(over: Partial<TransferConfirmPayload> = {}): TransferConfirmPayload {
  return {
    destination: '/Users/me/backup',
    volumeId: 'root',
    previewId: 'preview-1',
    conflictResolution: 'stop',
    operationType: 'copy',
    preKnownConflicts: ['b.jpg'],
    ...over,
  }
}

function deleteDialogProps(): DeleteDialogPropsData {
  return {
    sourceItems: [{ name: 'a.jpg', isDirectory: false, isSymlink: false, size: 10 }],
    sourcePaths: [`${SOURCE_FOLDER}/a.jpg`],
    sourceFolderPath: SOURCE_FOLDER,
    isPermanent: false,
    supportsTrash: true,
    isFromCursor: true,
    sortColumn: 'name',
    sortOrder: 'ascending',
    sourceVolumeId: 'root',
  }
}

const needsPassword: WriteOperationError = {
  type: 'archive_needs_password',
  path: '/Users/me/secret.zip',
  wrongAttempt: false,
}

/** Starts a copy in the background from an open transfer dialog. */
async function startCopyInBackground() {
  const state = makeState()
  state.dialogs.showTransfer(transferDialogProps())
  state.dialogs.handleTransferConfirm(confirmPayload({ startInBackground: true }))
  await settle()
  return state
}

beforeEach(async () => {
  vi.clearAllMocks()
  listeners.error = null
  listeners.complete = null
  vi.mocked(listOperations).mockResolvedValue([snapshot('op-1', 'running')])
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
  setForegroundOperationId(null)
})

describe('F2 in the transfer dialog', () => {
  it('starts exactly the operation Enter would, from the same payload', async () => {
    // What Enter would dispatch: the birth props the progress dialog mounts with.
    const enter = makeState()
    enter.dialogs.showTransfer(transferDialogProps())
    enter.dialogs.handleTransferConfirm(confirmPayload())
    const enterProps = enter.dialogs.transferProgressProps
    if (!enterProps) throw new Error('Enter started nothing')
    await dispatchTransferOperation(enterProps)
    const enterCall = vi.mocked(copyBetweenVolumes).mock.calls[0]
    vi.mocked(copyBetweenVolumes).mockClear()

    await startCopyInBackground()

    expect(copyBetweenVolumes).toHaveBeenCalledOnce()
    expect(vi.mocked(copyBetweenVolumes).mock.calls[0]).toEqual(enterCall)
  })

  it('mounts no progress dialog, closes the setup dialog, and gives the pane its keys back', async () => {
    const { dialogs, onRefocus } = await startCopyInBackground()

    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
    expect(dialogs.showTransferDialog).toBe(false)
    expect(onRefocus).toHaveBeenCalled()
  })

  it('leaves the progress slot free, so the next F5 opens at once', async () => {
    const { dialogs } = await startCopyInBackground()

    dialogs.showTransfer(transferDialogProps())
    expect(dialogs.showTransferDialog).toBe(true)
    dialogs.handleTransferConfirm(confirmPayload({ destination: '/Users/me/second' }))
    expect(dialogs.showTransferProgressDialog).toBe(true)
    // The second operation took the slot, rather than being refused over the first.
    expect(dialogs.transferProgressProps?.destinationPath).toBe('/Users/me/second')
  })
})

describe('F2 in the delete dialog', () => {
  it('trashes in the background, never deletes, and mounts no progress dialog', async () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation(deleteDialogProps())

    dialogs.handleTrashInBackground('preview-9')
    await settle()

    expect(trashFiles).toHaveBeenCalledWith(
      [`${SOURCE_FOLDER}/a.jpg`],
      [10],
      expect.objectContaining({ previewId: 'preview-9' }),
      undefined,
    )
    expect(deleteFiles).not.toHaveBeenCalled()
    expect(dialogs.showDeleteDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })
})

describe('a background job that finishes', () => {
  function complete(operationType: 'trash' | 'copy', refused: { itemCount: number; reason: 'notPermitted' } | null) {
    listeners.complete?.({
      operationId: 'op-1',
      operationType,
      filesProcessed: 1,
      filesSkipped: 0,
      bytesProcessed: 10,
      refused,
    })
  }

  it('says so for a trash, with the toast that carries Undo and Go to trash', async () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation(deleteDialogProps())
    dialogs.handleTrashInBackground('preview-9')
    await settle()
    vi.mocked(addToast).mockClear()

    complete('trash', null)
    await settle()

    expect(addToast).toHaveBeenCalledOnce()
    const [content, options] = vi.mocked(addToast).mock.calls[0] ?? []
    expect(content).toBe(TrashCompleteToastContent)
    expect(options).toMatchObject({
      level: 'success',
      props: { message: 'Moved 1 file to trash', operationId: 'op-1', sourceFolderPath: SOURCE_FOLDER },
    })
  })

  it('names what a partly refused background trash left behind, beside the Undo toast', async () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation(deleteDialogProps())
    dialogs.handleTrashInBackground('preview-9')
    await settle()
    vi.mocked(addToast).mockClear()

    complete('trash', { itemCount: 1, reason: 'notPermitted' })
    await settle()

    expect(addToast).toHaveBeenCalledTimes(2)
    expect(vi.mocked(addToast).mock.calls[0]?.[0]).toBe(TrashCompleteToastContent)
    expect(vi.mocked(addToast).mock.calls[1]?.[1]).toMatchObject({ level: 'warn' })
  })

  it('stays quiet for a copy, as a job sent off with Queue does', async () => {
    await startCopyInBackground()
    vi.mocked(addToast).mockClear()

    complete('copy', null)
    await settle()

    expect(addToast).not.toHaveBeenCalled()
  })
})

describe('a refused background start', () => {
  it('opens the error dialog, and its Retry starts in the background again', async () => {
    const refusal = Object.assign(new Error('inside'), { type: 'destination_inside_source', path: '/src' })
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(refusal))

    const { dialogs } = await startCopyInBackground()

    expect(dialogs.showTransferErrorDialog).toBe(true)
    expect(dialogs.transferErrorProps?.retry?.startInBackground).toBe(true)

    dialogs.handleTransferErrorRetry()
    await settle()

    expect(copyBetweenVolumes).toHaveBeenCalledTimes(2)
    expect(dialogs.showTransferErrorDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })
})

describe('an archive password, for a job with no dialog', () => {
  it('raises the prompt, and the submit re-dispatches in the background with a fresh scan', async () => {
    const { dialogs } = await startCopyInBackground()

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.showArchivePasswordDialog).toBe(true)
    expect(dialogs.showTransferProgressDialog).toBe(false)

    dialogs.handleArchivePasswordSubmit('hunter2')
    await settle()

    expect(copyBetweenVolumes).toHaveBeenCalledTimes(2)
    expect(vi.mocked(copyBetweenVolumes).mock.calls[1]?.[4]).toEqual(expect.objectContaining({ previewId: null }))
    expect(dialogs.showArchivePasswordDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
  })

  it('says so in a toast when a foreground operation holds the slot, and leaves that one alone', async () => {
    const { dialogs } = await startCopyInBackground()
    dialogs.showTransfer(transferDialogProps())
    dialogs.handleTransferConfirm(confirmPayload({ destination: '/Users/me/other' }))
    const foreground = dialogs.transferProgressProps

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.showArchivePasswordDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBe(foreground)
    expect(addToast).toHaveBeenCalledWith(
      '“secret.zip” needs its password. Start the operation again to type it in.',
      expect.objectContaining({ level: 'warn' }),
    )
  })

  it.each([
    ['the Show view reports first', false],
    ['the background watch hears it first', true],
  ])(
    'asks with the prompt, not an error dialog, when the job is being watched through Show (%s)',
    async (_, watchFirst) => {
      const { dialogs } = await startCopyInBackground()
      dialogs.foregroundOperation({
        operationId: 'op-1',
        operationType: 'copy',
        sourcePath: SOURCE_FOLDER,
        destinationPath: '/Users/me/backup',
        reverses: null,
      })
      expect(dialogs.showTransferProgressDialog).toBe(true)
      // What the adopted view does as it mounts.
      setForegroundOperationId('op-1')

      listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
      if (watchFirst) flushSync()
      // The adopted view reports the same stop through its own outcome callback.
      dialogs.handleAdoptedError(needsPassword, null)
      await settle()

      expect(dialogs.showArchivePasswordDialog).toBe(true)
      expect(dialogs.showTransferErrorDialog).toBe(false)
      expect(dialogs.showTransferProgressDialog).toBe(false)
    },
  )

  it('leaves a browse password prompt that is already up alone', async () => {
    const { dialogs } = await startCopyInBackground()
    const retry = vi.fn()
    dialogs.showArchivePasswordForBrowse({
      volumeId: 'root',
      archivePath: '/Users/me/other.zip',
      wrongAttempt: false,
      retry,
    })

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.archivePasswordProps?.archivePath).toBe('/Users/me/other.zip')
    expect(dialogs.transferProgressProps).toBeNull()
  })

  it('reaches a job sent to the background from the progress dialog, too', async () => {
    const { dialogs } = makeState()
    dialogs.showTransfer(transferDialogProps())
    dialogs.handleTransferConfirm(confirmPayload())

    dialogs.handleTransferQueue('op-1')
    expect(dialogs.transferProgressProps).toBeNull()
    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.showArchivePasswordDialog).toBe(true)

    dialogs.handleArchivePasswordSubmit('hunter2')
    await settle()

    // It lives in the background now, so it starts there again.
    expect(copyBetweenVolumes).toHaveBeenCalledOnce()
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })
})
