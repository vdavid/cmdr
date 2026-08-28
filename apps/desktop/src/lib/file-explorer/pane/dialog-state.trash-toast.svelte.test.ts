/**
 * What a completed trash puts on screen, wired through the real
 * `createDialogState`.
 *
 * The trash toast is the one completion toast that carries actions, and both of
 * them need the journaled operation id. That id is read while the progress dialog
 * still owns the foreground slot (it releases on unmount), so the wiring is the
 * only place that can answer whether the toast actually gets one — which is why
 * this pins the props, not just the fact that a toast appeared.
 *
 * Copy, move, and delete keep the plain string toast: none of them has an undo to
 * offer, and a delete never will.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import { setForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
import TrashCompleteToastContent from '$lib/file-operations/delete/TrashCompleteToastContent.svelte'
import type { TransferProgressPropsData } from './dialog-props'
import type { FilePaneAPI } from './types'
import type { PaneRevealAPI } from '../navigation/navigate-and-select'
import type { ToastContent, ToastOptions } from '$lib/ui/toast/toast-store.svelte'

const { addToast } = vi.hoisted(() => ({
  addToast: vi.fn<(content: ToastContent, options?: ToastOptions) => string>(),
}))

/** The `addToast` call this completion raised, typed so option reads aren't `any`. */
function raisedToast(): { content: ToastContent; options: ToastOptions } {
  const call = addToast.mock.calls[0]
  expect(call).toBeDefined()
  return { content: call[0], options: call[1] ?? {} }
}

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
  getOperationLogDetail: vi.fn(() => Promise.resolve(null)),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/ui/toast', () => ({ addToast }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))

const FOLDER = '/Users/me/photos'

const explorer = { getFocusedPane: () => 'left' } as unknown as PaneRevealAPI

function makeState() {
  const pane = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    getCurrentPath: vi.fn(() => FOLDER),
    hasParentEntry: vi.fn(() => true),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
    startRename: vi.fn(),
  } as unknown as FilePaneAPI

  return createDialogState({
    getLeftPaneRef: () => pane,
    getRightPaneRef: () => pane,
    getFocusedPaneRef: () => pane,
    getFocusedPaneSide: () => 'left',
    getShowHiddenFiles: () => false,
    getExplorer: () => explorer,
    onRefocus: vi.fn(),
    onOpenInEditor: vi.fn(),
  })
}

function props(operationType: TransferProgressPropsData['operationType']): TransferProgressPropsData {
  return {
    operationType,
    sourcePaths: [`${FOLDER}/photo.jpg`],
    sourceFolderPath: FOLDER,
    sourcePaneSide: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'root',
    duplicateFollowUp: 'nothing',
  }
}

/** The completion payload for one file, nothing skipped. */
const ONE_FILE = { filesProcessed: 1, filesSkipped: 0, bytesProcessed: 10 }

beforeEach(() => {
  vi.clearAllMocks()
  setForegroundOperationId(null)
})

describe('a completed trash', () => {
  it('raises the toast that carries Undo and Go to trash, with the operation to act on', () => {
    const dialogs = makeState()
    dialogs.startTransferProgress(props('trash'))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(ONE_FILE)

    expect(addToast).toHaveBeenCalledTimes(1)
    const { content, options } = raisedToast()
    expect(content).toBe(TrashCompleteToastContent)
    expect(options.props).toMatchObject({
      message: 'Moved 1 file to trash',
      operationId: 'op-1',
      sourceFolderPath: FOLDER,
      explorer,
    })
  })

  it('gives the user longer to reach the buttons than a plain toast gets', () => {
    const dialogs = makeState()
    dialogs.startTransferProgress(props('trash'))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(ONE_FILE)

    expect(raisedToast().options.timeoutMs).toBeGreaterThan(7000)
  })

  it('leaves the toast app-global, so neither pane can eat it on the next navigation', () => {
    const dialogs = makeState()
    dialogs.startTransferProgress(props('trash'))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(ONE_FILE)

    expect(raisedToast().options.originPane).toBeUndefined()
  })

  it('falls back to the plain sentence when there is no operation to act on', () => {
    // Both actions need the journaled id. Buttons that would do nothing are worse
    // than no buttons.
    const dialogs = makeState()
    dialogs.startTransferProgress(props('trash'))

    dialogs.handleTransferComplete(ONE_FILE)

    expect(addToast).toHaveBeenCalledWith('Moved 1 file to trash', expect.objectContaining({ level: 'success' }))
  })
})

describe('every other completed operation', () => {
  it.each([['copy'], ['move'], ['delete']] as const)('keeps the plain string toast for %s', (operationType) => {
    const dialogs = makeState()
    dialogs.startTransferProgress(props(operationType))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(ONE_FILE)

    expect(addToast).toHaveBeenCalledTimes(1)
    expect(typeof raisedToast().content).toBe('string')
  })
})
