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

vi.mock('#lib/tauri-commands/index.js', async () => ({
  ...(await import('#lib/file-operations/transfer/test-transfer-progress-harness.svelte.js')).tauriCommandsMock(),
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  forgetContainerPassword: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/file-operations/queue/queue-window.js', async () =>
  (await import('#lib/file-operations/transfer/test-transfer-progress-harness.svelte.js')).queueWindowMock(),
)
// The real store behind a spy, so a case can read what's actually on screen.
vi.mock('#lib/ui/toast/index.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('#lib/ui/toast/index.js')>()
  return { ...actual, addToast: vi.fn(actual.addToast) }
})
vi.mock('#lib/settings/index.js', async () =>
  (await import('#lib/file-operations/transfer/test-transfer-progress-harness.svelte.js')).settingsMock(),
)
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('#lib/file-operations/mkdir/new-folder-operations.js', () => ({ moveCursorToNewFolder: vi.fn() }))

import { createDialogState } from './dialog-state.svelte'
import { listeners, settle, snapshot } from '#lib/file-operations/transfer/test-transfer-progress-harness.svelte.js'
import { copyBetweenVolumes, deleteFiles, listOperations, trashFiles } from '#lib/tauri-commands/index.js'
import { dispatchTransferOperation } from '#lib/file-operations/transfer/transfer-dispatch.js'
import { addToast, clearAllToasts, getToasts } from '#lib/ui/toast/index.js'
import QueueHandoffToastContent from '#lib/file-operations/queue/QueueHandoffToastContent.svelte'
import { emit } from '@tauri-apps/api/event'
import { setForegroundOperationId } from '#lib/file-operations/foreground-operation.svelte.js'
import TrashCompleteToastContent from '#lib/file-operations/delete/TrashCompleteToastContent.svelte'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'
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
  return { dialogs, onRefocus, left }
}

function transferDialogProps(): TransferDialogPropsData {
  return {
    initiator: 'user',
    operationType: 'copy',
    sourcePaths: [`${SOURCE_FOLDER}/a.jpg`, `${SOURCE_FOLDER}/b.jpg`],
    destinationPath: '/Users/me/backup',
    direction: 'right',
    currentVolumeId: 'root',
    fileCount: 2,
    folderCount: 0,
    sourceFolderPath: SOURCE_FOLDER,
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
    initiator: 'user',
    sourceItems: [{ name: 'a.jpg', isDirectory: false, isSymlink: false, size: 10 }],
    sourcePaths: [`${SOURCE_FOLDER}/a.jpg`],
    sourceFolderPath: SOURCE_FOLDER,
    isPermanent: false,
    supportsTrash: true,
    isFromCursor: true,
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
  clearAllToasts()
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

describe('an MCP confirm in the background', () => {
  // The `dialog confirm` round-trip's id rides the press into the start, which
  // answers it with the id the backend minted, as an auto-confirmed `copy` does.
  it('answers its round-trip with the started copy’s operationId', async () => {
    const { dialogs } = makeState()
    dialogs.showTransfer(transferDialogProps())

    dialogs.handleTransferConfirm(confirmPayload({ startInBackground: true, mcpRequestId: 'req-7' }))
    await settle()

    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-7', ok: true, operationId: 'op-1' })
  })

  it('answers its round-trip with the started trash’s operationId', async () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation(deleteDialogProps())

    dialogs.handleTrashInBackground('preview-9', { mcpRequestId: 'req-8' })
    await settle()

    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-8', ok: true, operationId: 'op-1' })
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
      'user',
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

  // A queued trash whose items all left while it waited completes having moved
  // nothing (ERR-962JD); the sources-gone toast says why, and an Undo toast for
  // "Moved 0 files to trash" would offer to undo nothing.
  it('stays quiet for a trash that had nothing left to move', async () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation(deleteDialogProps())
    dialogs.handleTrashInBackground('preview-9')
    await settle()
    vi.mocked(addToast).mockClear()

    listeners.complete?.({
      operationId: 'op-1',
      operationType: 'trash',
      filesProcessed: 0,
      filesSkipped: 2,
      bytesProcessed: 0,
      refused: null,
    })
    await settle()

    expect(addToast).not.toHaveBeenCalled()
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

  // One keypress, one toast: a plain local trash finishes about as fast as the
  // handoff toast appears, and "Sent to the background" is stale once it's done.
  it('replaces the handoff toast with the Undo toast, rather than stacking both', async () => {
    const { dialogs } = makeState()
    dialogs.showDeleteConfirmation(deleteDialogProps())
    dialogs.handleTrashInBackground('preview-9')
    await settle()
    expect(getToasts().map((t) => t.content)).toEqual([QueueHandoffToastContent])

    complete('trash', null)
    await settle()

    expect(getToasts().map((t) => t.content)).toEqual([TrashCompleteToastContent])
  })

  it('keeps the handoff toast for a copy, which finishes without a toast of its own', async () => {
    await startCopyInBackground()

    complete('copy', null)
    await settle()

    expect(getToasts().map((t) => t.content)).toEqual([QueueHandoffToastContent])
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
  it('holds the error dialog while another dialog is up, and opens it once that closes', async () => {
    let refuse: (reason: unknown) => void = () => {}
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(
      () =>
        new Promise((_, reject) => {
          refuse = reject
        }),
    )
    const { dialogs } = makeState()
    dialogs.showTransfer(transferDialogProps())
    dialogs.handleTransferConfirm(confirmPayload({ startInBackground: true }))
    // The person moves on to F8 before the backend answers.
    dialogs.showDeleteConfirmation(deleteDialogProps())

    refuse(Object.assign(new Error('inside'), { type: 'destination_inside_source', path: '/src' }))
    await settle()

    expect(dialogs.showTransferErrorDialog).toBe(false)
    expect(dialogs.showDeleteDialog).toBe(true)

    dialogs.handleDeleteCancel()
    await settle()

    expect(dialogs.showTransferErrorDialog).toBe(true)
  })

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

    expect(dialogs.showUnlockDialog).toBe(true)
    expect(dialogs.showTransferProgressDialog).toBe(false)

    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: false })
    await settle()

    expect(copyBetweenVolumes).toHaveBeenCalledTimes(2)
    expect(vi.mocked(copyBetweenVolumes).mock.calls[1]?.[4]).toEqual(expect.objectContaining({ previewId: null }))
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
  })

  it('holds the prompt while a foreground operation has the slot, says so, and asks once it ends', async () => {
    const { dialogs } = await startCopyInBackground()
    dialogs.showTransfer(transferDialogProps())
    dialogs.handleTransferConfirm(confirmPayload({ destination: '/Users/me/other' }))
    const foreground = dialogs.transferProgressProps

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBe(foreground)
    expect(addToast).toHaveBeenCalledWith(
      '“secret.zip” needs its password. You’ll be asked as soon as the open dialog closes.',
      expect.objectContaining({ level: 'info' }),
    )

    dialogs.handleTransferComplete({
      filesProcessed: 1,
      filesSkipped: 0,
      bytesProcessed: 1,
      appearedDuringMove: null,
      topLevelSkipped: null,
      refused: null,
      metadataLeftBehind: null,
      lockedKept: null,
      durabilityUnconfirmed: null,
      alreadyGone: null,
    })
    await settle()

    expect(dialogs.showUnlockDialog).toBe(true)
    expect(dialogs.unlockProps?.path).toBe('/Users/me/secret.zip')
  })

  it('leaves the selection the person made since alone when the prompt is cancelled', async () => {
    const { dialogs, left } = await startCopyInBackground()
    // Dropped once, at the background start, like a Queue press.
    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest mock, no `this` binding
    expect(left.clearSelection).toHaveBeenCalledOnce()

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()
    dialogs.handleUnlockCancel()
    await settle()

    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest mock, no `this` binding
    expect(left.clearSelection).toHaveBeenCalledOnce()
    expect(dialogs.transferProgressProps).toBeNull()
  })

  it('never stacks the prompt over a setup dialog the person opened meanwhile', async () => {
    const { dialogs, onRefocus } = await startCopyInBackground()
    dialogs.showTransfer(transferDialogProps())
    onRefocus.mockClear()

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.showTransferDialog).toBe(true)
    expect(onRefocus).not.toHaveBeenCalled()

    dialogs.handleTransferCancel()
    await settle()

    expect(dialogs.showUnlockDialog).toBe(true)
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

      expect(dialogs.showUnlockDialog).toBe(true)
      expect(dialogs.showTransferErrorDialog).toBe(false)
      expect(dialogs.showTransferProgressDialog).toBe(false)
    },
  )

  it('leaves a browse password prompt that is already up alone', async () => {
    const { dialogs } = await startCopyInBackground()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse({
      volumeId: 'root',
      path: '/Users/me/other.zip',
      wrongAttempt: false,
      retry,
    })

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.unlockProps?.path).toBe('/Users/me/other.zip')
    expect(dialogs.transferProgressProps).toBeNull()

    // Once that prompt is answered, the held one asks.
    dialogs.handleUnlockCancel()
    await settle()

    expect(dialogs.unlockProps?.path).toBe('/Users/me/secret.zip')
  })

  it('restarts a permanent delete sent off with Queue in the progress dialog, never out of sight', async () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress({
      initiator: 'user',
      operationType: 'delete',
      sourcePaths: [`${SOURCE_FOLDER}/a.jpg`],
      sourceFolderPath: SOURCE_FOLDER,
      sourcePaneSide: 'left',
      previewId: 'preview-1',
      sourceVolumeId: 'root',
      duplicateFollowUp: 'nothing',
    })

    dialogs.handleTransferQueue('op-1')
    listeners.error?.({ operationId: 'op-1', operationType: 'delete', error: needsPassword, progressAtStop: null })
    await settle()
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: false })
    await settle()

    expect(deleteFiles).not.toHaveBeenCalled()
    expect(dialogs.showTransferProgressDialog).toBe(true)
    expect(dialogs.transferProgressProps?.startInBackground).toBeUndefined()
  })

  it('reaches a job sent to the background from the progress dialog, too', async () => {
    const { dialogs } = makeState()
    dialogs.showTransfer(transferDialogProps())
    dialogs.handleTransferConfirm(confirmPayload())

    dialogs.handleTransferQueue('op-1')
    expect(dialogs.transferProgressProps).toBeNull()
    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(dialogs.showUnlockDialog).toBe(true)

    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: false })
    await settle()

    // It lives in the background now, so it starts there again.
    expect(copyBetweenVolumes).toHaveBeenCalledOnce()
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })
})
