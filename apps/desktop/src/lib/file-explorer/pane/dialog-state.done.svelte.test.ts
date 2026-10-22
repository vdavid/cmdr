/**
 * A progress dialog kept open on its done state (#484), through the real
 * `createDialogState`: the completion's pane work runs at once, the dialog stays
 * mounted until the person closes it, and the plain completion toast stays quiet
 * because the dialog already says it. Anything the dialog doesn't say (a refused
 * trash item, unconfirmed durability, a moved item left behind) still raises.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import { setForegroundOperationId } from '#lib/file-operations/foreground-operation.svelte.js'
import type { AdoptedOperationData, TransferCompletePayload, TransferProgressPropsData } from './dialog-props'
import type { FilePaneAPI } from './types'
import type { PaneRevealAPI } from '../navigation/navigate-and-select'
import type { ToastContent, ToastOptions } from '#lib/ui/toast/toast-store.svelte.js'

const { addToast } = vi.hoisted(() => ({
  addToast: vi.fn<(content: ToastContent, options?: ToastOptions) => string>(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  forgetContainerPassword: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
  getOperationLogDetail: vi.fn(() => Promise.resolve(null)),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast, dismissToast: vi.fn() }))
const { rememberTrashForUndo } = vi.hoisted(() => ({ rememberTrashForUndo: vi.fn() }))
vi.mock('#lib/file-operations/delete/trash-undo.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  rememberTrashForUndo,
}))
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('#lib/operation-log/recording.js', () => ({ isOperationLogRecording: () => true }))

const FOLDER = '/Users/me/photos'

const explorer = { getFocusedPane: () => 'left' } as unknown as PaneRevealAPI

function makeState() {
  const clearSelection = vi.fn()
  const pane = {
    clearSelection,
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    getCurrentPath: vi.fn(() => FOLDER),
    hasParentEntry: vi.fn(() => true),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
    startRename: vi.fn(),
  } as unknown as FilePaneAPI
  const onRefocus = vi.fn()

  const dialogs = createDialogState({
    getLeftPaneRef: () => pane,
    getRightPaneRef: () => pane,
    getFocusedPaneRef: () => pane,
    getFocusedPaneSide: () => 'left',
    getShowHiddenFiles: () => false,
    getExplorer: () => explorer,
    onRefocus,
    skipsConfirmations: () => false,
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, clearSelection, onRefocus }
}

function props(operationType: TransferProgressPropsData['operationType']): TransferProgressPropsData {
  return {
    operationType,
    sourcePaths: [`${FOLDER}/photo.jpg`],
    sourceFolderPath: FOLDER,
    sourcePaneSide: 'left',
    previewId: null,
    sourceVolumeId: 'root',
    duplicateFollowUp: 'nothing',
  }
}

const ONE_FILE: TransferCompletePayload = {
  filesProcessed: 1,
  filesSkipped: 0,
  bytesProcessed: 10,
  appearedDuringMove: null,
  topLevelSkipped: null,
  refused: null,
  metadataLeftBehind: null,
  lockedKept: null,
  durabilityUnconfirmed: null,
}

const ADOPTED: AdoptedOperationData = {
  operationId: 'op-42',
  operationType: 'copy',
  sourcePath: '/Volumes/Card/DCIM',
  destinationPath: '/Users/me/import',
  reverses: null,
}

beforeEach(() => {
  vi.clearAllMocks()
  setForegroundOperationId(null)
})

describe('a started operation kept open on its done state', () => {
  it('clears the selection now and keeps the dialog up', () => {
    const { dialogs, clearSelection, onRefocus } = makeState()
    dialogs.startTransferProgress(props('copy'))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(ONE_FILE, { keptOpen: true })

    expect(clearSelection).toHaveBeenCalled()
    expect(dialogs.showTransferProgressDialog).toBe(true)
    expect(dialogs.transferProgressProps).not.toBeNull()
    expect(onRefocus).not.toHaveBeenCalled()
  })

  it('keeps the plain completion toast quiet: the dialog says it', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(props('copy'))

    dialogs.handleTransferComplete(ONE_FILE, { keptOpen: true })

    expect(addToast).not.toHaveBeenCalled()
  })

  it('still remembers a trash for Edit > Undo, without its toast', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(props('trash'))
    setForegroundOperationId('op-1')

    dialogs.handleTransferComplete(ONE_FILE, { keptOpen: true })

    expect(rememberTrashForUndo).toHaveBeenCalledTimes(1)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('still raises what the dialog doesn’t say', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(props('copy'))

    dialogs.handleTransferComplete({ ...ONE_FILE, durabilityUnconfirmed: { volumeName: 'Backup' } }, { keptOpen: true })

    expect(addToast).toHaveBeenCalledTimes(1)
    expect(addToast.mock.calls[0][1]?.level).toBe('warn')
  })

  it('comes down when the person closes it', () => {
    const { dialogs, onRefocus } = makeState()
    dialogs.startTransferProgress(props('copy'))
    dialogs.handleTransferComplete(ONE_FILE, { keptOpen: true })

    dialogs.handleTransferDoneClosed()

    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
    expect(onRefocus).toHaveBeenCalled()
  })
})

describe('an adopted operation kept open on its done state', () => {
  it('stays up until closed, and says nothing the dialog already says', () => {
    const { dialogs } = makeState()
    dialogs.foregroundOperation(ADOPTED)

    dialogs.handleAdoptedComplete(ONE_FILE, { keptOpen: true })

    expect(dialogs.showTransferProgressDialog).toBe(true)
    expect(dialogs.adoptedProgressProps).not.toBeNull()
    expect(addToast).not.toHaveBeenCalled()

    dialogs.handleAdoptedDoneClosed()

    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.adoptedProgressProps).toBeNull()
  })
})
