/**
 * An MCP `dialog confirm` on the transfer dialog, at each point of its startup.
 *
 * The confirm presses the dialog's OWN confirm (`registerTransferConfirmer`), so
 * it dispatches what the button would: the path in the box (for a compress, the
 * `.zip` the box names, which the props never held), the picked volume, and the
 * scan preview. It can land while the dialog is still starting: resolving the
 * home dir, registering the scan listeners, or waiting on the `startScanPreview`
 * IPC. Like a person's fast Enter, it then waits for the scan start it can see
 * and hands that preview to the operation.
 *
 * `handleTransferConfirm` takes the dialog down in the same tick it starts the
 * operation, and nulls `transferDialogProps` right there, the way
 * `handleDeleteConfirm` does. The dialog's props are live getters into that
 * slot, so anything the dialog still has in flight reads a null object if it
 * reads a prop after the confirm. So these tests mount the REAL `DialogManager`
 * and `TransferDialog`, fed by live getters over a real `createDialogState`, and
 * confirm at each point. None may throw (an unhandled rejection, or a render
 * throw the boundary would turn into "dismiss every dialog"), and none may leave
 * a scan preview that nothing owns.
 *
 * Why the props go null at once rather than a microtask later, and the rule the
 * dialog's async start follows: `file-operations/DETAILS.md` § "A dialog's async
 * start can outlive it".
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount, type ComponentProps } from 'svelte'
import { homeDir } from '@tauri-apps/api/path'
import * as commands from '$lib/tauri-commands'
import DialogManager from './DialogManager.svelte'
import { createDialogState } from './dialog-state.svelte'
import type { TransferConfirmPayload, TransferConfirmer } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { FilePaneAPI } from './types'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/tauri-commands')>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() => Promise.resolve({ data: null, timedOut: false })),
  startScanPreview: vi.fn(),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(),
  onScanPreviewComplete: vi.fn(),
  onScanPreviewError: vi.fn(),
  onScanPreviewCancelled: vi.fn(),
  scanVolumeForConflicts: vi.fn(() => Promise.resolve([])),
  pathExistsChecked: vi.fn(() => Promise.resolve({ data: true, timedOut: false })),
  destinationExists: vi.fn(() => Promise.resolve({ data: true, timedOut: false })),
  destinationWriteAccess: vi.fn(() => Promise.resolve({ kind: 'unknown' })),
  destinationRootEcho: vi.fn(() => Promise.resolve(null)),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
}))

vi.mock('$lib/file-operations/queue/queue-window', () => ({ openQueueWindow: vi.fn(() => Promise.resolve()) }))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/settings')>()),
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))

// A marker stands in for the progress dialog: the real one dispatches a backend
// operation on mount, and what's under test is the confirmation dialog going away.
vi.mock('../../file-operations/transfer/TransferProgressDialog.svelte', async () => ({
  default: (await import('../../../../test/fixtures/dialog-marker-fixture.svelte')).default,
}))

interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
}

function deferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((settle) => {
    resolve = settle
  })
  return { promise, resolve }
}

/** Lets every pending continuation run, and Node report any rejection nobody handled. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((done) => setTimeout(done, 0))
  }
}

const SOURCE_FOLDER = '/Users/me/photos'

function transferProps(operationType: TransferDialogPropsData['operationType'] = 'copy'): TransferDialogPropsData {
  return {
    operationType,
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
    duplicateFollowUp: 'nothing',
  }
}

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

describe('an MCP confirm on the transfer dialog', () => {
  let registrations: Deferred<undefined>
  let unlistens: Array<() => void>
  let rejections: unknown[]
  let component: Record<string, unknown> | null = null
  const onRejection = (reason: unknown): void => {
    rejections.push(reason)
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
    registrations = deferred()
    unlistens = []
    rejections = []
    process.on('unhandledRejection', onRejection)
    vi.mocked(commands.startScanPreview).mockResolvedValue({ previewId: 'preview-1' })
    for (const register of [
      commands.onScanPreviewProgress,
      commands.onScanPreviewComplete,
      commands.onScanPreviewError,
      commands.onScanPreviewCancelled,
    ]) {
      vi.mocked(register).mockImplementation(async () => {
        await registrations.promise
        const unlisten = vi.fn()
        unlistens.push(unlisten)
        return unlisten
      })
    }
  })

  afterEach(() => {
    process.off('unhandledRejection', onRejection)
    if (component) {
      void unmount(component)
      component = null
    }
  })

  /** Opens the transfer dialog through the real dialog state and `DialogManager`. */
  function openTransferDialog(operationType: TransferDialogPropsData['operationType'] = 'copy') {
    const pane = makePaneRef()
    const dialogs = createDialogState({
      getLeftPaneRef: () => pane,
      getRightPaneRef: () => pane,
      getFocusedPaneRef: () => pane,
      getFocusedPaneSide: () => 'right',
      getShowHiddenFiles: () => false,
      getExplorer: () => undefined,
      onRefocus: vi.fn(),
      skipsConfirmations: () => false,
      onOpenInEditor: vi.fn(),
    })
    const onDialogRenderError = vi.fn()
    const noop = (): void => {}
    // Live getters, like `DualPaneExplorer`'s bindings: the dialog's props read
    // through them into the dialog state's slot every time.
    const props: ComponentProps<typeof DialogManager> = {
      onDialogRenderError,
      get showTransferDialog() {
        return dialogs.showTransferDialog
      },
      get transferDialogProps() {
        return dialogs.transferDialogProps
      },
      get showTransferProgressDialog() {
        return dialogs.showTransferProgressDialog
      },
      get transferProgressProps() {
        return dialogs.transferProgressProps
      },
      get adoptedProgressProps() {
        return dialogs.adoptedProgressProps
      },
      showNewFolderDialog: false,
      newFolderDialogProps: null,
      showNewFileDialog: false,
      newFileDialogProps: null,
      showAlertDialog: false,
      alertDialogProps: null,
      showTransferErrorDialog: false,
      transferErrorProps: null,
      showArchivePasswordDialog: false,
      archivePasswordProps: null,
      showDeleteDialog: false,
      deleteDialogProps: null,
      onTransferConfirm: (payload: TransferConfirmPayload) => {
        dialogs.handleTransferConfirm(payload)
      },
      registerTransferConfirmer: (confirm: TransferConfirmer) => dialogs.registerTransferConfirmer(confirm),
      registerDeleteConfirmer: () => noop,
      onTransferCancel: () => {
        dialogs.handleTransferCancel()
      },
      onTransferComplete: noop,
      onTransferCancelled: noop,
      onTransferError: noop,
      onTransferQueue: noop,
      onAdoptedComplete: noop,
      onAdoptedCancelled: noop,
      onAdoptedError: noop,
      onAdoptedQueue: noop,
      onTransferErrorClose: noop,
      onArchivePasswordSubmit: noop,
      onArchivePasswordCancel: noop,
      onNewFolderCreated: noop,
      onNewFolderCancel: noop,
      onNewFileCreated: noop,
      onNewFileCancel: noop,
      onAlertClose: noop,
      onDeleteConfirm: noop,
      onTrashInBackground: noop,
      onDeleteCancel: noop,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    component = mount(DialogManager, { target, props }) as Record<string, unknown>
    dialogs.showTransfer(transferProps(operationType))
    flushSync()
    expect(target.querySelector('[data-dialog-id="transfer-confirmation"]')).not.toBeNull()
    return { dialogs, target, onDialogRenderError }
  }

  /** What an MCP `dialog confirm` does: presses the open dialog's own confirm, through the dialog state. */
  function confirmFromMcp(dialogs: ReturnType<typeof createDialogState>, onConflict?: string): void {
    dialogs.confirmOpenDialog('transfer-confirmation', onConflict)
  }

  /** Waits for the confirm to dispatch: the dialog and its props are gone, and the progress dialog is up. */
  async function operationStarted(dialogs: ReturnType<typeof createDialogState>, target: HTMLElement): Promise<void> {
    await vi.waitFor(() => {
      expect(dialogs.showTransferProgressDialog).toBe(true)
    })
    flushSync()
    expect(target.querySelector('[data-dialog-id="transfer-confirmation"]')).toBeNull()
    expect(dialogs.transferDialogProps).toBeNull()
    expect(target.querySelector('[data-testid="progress-dialog"]')).not.toBeNull()
  }

  it('starts no scan once it confirmed while the dialog was still resolving the home dir', async () => {
    const home = deferred<string>()
    vi.mocked(homeDir).mockReturnValueOnce(home.promise)
    const { dialogs, target, onDialogRenderError } = openTransferDialog()

    confirmFromMcp(dialogs)
    await operationStarted(dialogs, target)
    expect(dialogs.transferProgressProps?.previewId).toBeNull()
    home.resolve('/Users/me')
    registrations.resolve(undefined)
    await settle()

    expect(rejections).toEqual([])
    expect(onDialogRenderError).not.toHaveBeenCalled()
    expect(commands.onScanPreviewProgress).not.toHaveBeenCalled()
    expect(commands.startScanPreview).not.toHaveBeenCalled()
    expect(commands.scanVolumeForConflicts).not.toHaveBeenCalled()
  })

  it('waits for a scan that is still registering its listeners, and hands its preview over', async () => {
    const { dialogs, target, onDialogRenderError } = openTransferDialog()
    await vi.waitFor(() => {
      expect(commands.onScanPreviewProgress).toHaveBeenCalled()
    })

    confirmFromMcp(dialogs)
    await settle()
    expect(dialogs.showTransferProgressDialog).toBe(false)
    registrations.resolve(undefined)
    await operationStarted(dialogs, target)
    await settle()

    expect(rejections).toEqual([])
    expect(onDialogRenderError).not.toHaveBeenCalled()
    expect(dialogs.transferProgressProps?.previewId).toBe('preview-1')
    expect(commands.cancelScanPreview).not.toHaveBeenCalled()
    expect(unlistens.length).toBeGreaterThan(0)
    for (const unlisten of unlistens) expect(unlisten).toHaveBeenCalledOnce()
  })

  it('waits for a preview id still on its way, and hands it over uncancelled', async () => {
    const preview = deferred<{ previewId: string }>()
    vi.mocked(commands.startScanPreview).mockReturnValue(preview.promise)
    registrations.resolve(undefined)
    const { dialogs, target, onDialogRenderError } = openTransferDialog()
    await vi.waitFor(() => {
      expect(commands.startScanPreview).toHaveBeenCalled()
    })

    confirmFromMcp(dialogs)
    await settle()
    expect(dialogs.showTransferProgressDialog).toBe(false)
    preview.resolve({ previewId: 'preview-late' })
    await operationStarted(dialogs, target)
    await settle()

    expect(rejections).toEqual([])
    expect(onDialogRenderError).not.toHaveBeenCalled()
    expect(dialogs.transferProgressProps?.previewId).toBe('preview-late')
    expect(commands.cancelScanPreview).not.toHaveBeenCalled()
  })

  it('carries the policy the agent named', async () => {
    registrations.resolve(undefined)
    const { dialogs, target } = openTransferDialog()

    confirmFromMcp(dialogs, 'overwrite_older_all')
    await operationStarted(dialogs, target)

    expect(dialogs.transferProgressProps?.conflictResolution).toBe('overwrite_older')
  })

  it('skips clashes when the agent named no policy, without waiting on a destination listing still in flight', async () => {
    vi.mocked(commands.scanVolumeForConflicts).mockReturnValue(new Promise(() => {}))
    registrations.resolve(undefined)
    const { dialogs, target } = openTransferDialog()
    await vi.waitFor(() => {
      expect(commands.scanVolumeForConflicts).toHaveBeenCalled()
    })

    confirmFromMcp(dialogs)
    await operationStarted(dialogs, target)

    expect(dialogs.transferProgressProps?.conflictResolution).toBe('skip')
    expect(dialogs.transferProgressProps?.preKnownConflicts).toEqual([])
  })

  it('compresses to the archive path in the box, the one the Compress button sends', async () => {
    registrations.resolve(undefined)
    const { dialogs, target } = openTransferDialog('compress')
    await vi.waitFor(() => {
      expect(commands.checkScanPreviewStatus).toHaveBeenCalledWith('preview-1')
    })

    confirmFromMcp(dialogs)
    await operationStarted(dialogs, target)

    expect(dialogs.transferProgressProps?.destinationPath).toBe('/Users/me/backup/first.jpg.zip')
    expect(dialogs.transferProgressProps?.operationType).toBe('compress')
  })

  it('hands the preview to the operation, uncancelled, when a person confirms', async () => {
    registrations.resolve(undefined)
    const { dialogs, target, onDialogRenderError } = openTransferDialog()
    await vi.waitFor(() => {
      expect(commands.checkScanPreviewStatus).toHaveBeenCalledWith('preview-1')
    })

    target.querySelector<HTMLButtonElement>('.btn-primary')?.click()
    await vi.waitFor(() => {
      expect(dialogs.showTransferProgressDialog).toBe(true)
    })
    flushSync()
    expect(dialogs.transferDialogProps).toBeNull()
    expect(target.querySelector('[data-dialog-id="transfer-confirmation"]')).toBeNull()
    await settle()

    expect(rejections).toEqual([])
    expect(onDialogRenderError).not.toHaveBeenCalled()
    expect(dialogs.transferProgressProps?.previewId).toBe('preview-1')
    expect(commands.cancelScanPreview).not.toHaveBeenCalled()
    expect(unlistens.length).toBeGreaterThan(0)
    for (const unlisten of unlistens) expect(unlisten).toHaveBeenCalledOnce()
  })

  it('starts on F2 with no progress dialog EVER rendered, not even for a frame, and hands the preview over', async () => {
    registrations.resolve(undefined)
    const { dialogs, target, onDialogRenderError } = openTransferDialog()
    await vi.waitFor(() => {
      expect(commands.checkScanPreviewStatus).toHaveBeenCalledWith('preview-1')
    })
    // Catches a progress dialog that mounts and unmounts between two checks.
    let progressDialogSeen = false
    const observer = new MutationObserver(() => {
      if (target.querySelector('[data-testid="progress-dialog"]')) progressDialogSeen = true
    })
    observer.observe(target, { childList: true, subtree: true })

    target
      .querySelector<HTMLElement>('.modal-overlay')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true }))
    await vi.waitFor(() => {
      expect(commands.copyBetweenVolumes).toHaveBeenCalledOnce()
    })
    flushSync()
    await settle()
    observer.disconnect()

    expect(progressDialogSeen).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(target.querySelector('[data-dialog-id="transfer-confirmation"]')).toBeNull()
    expect(vi.mocked(commands.copyBetweenVolumes).mock.calls[0]?.[4]).toEqual(
      expect.objectContaining({ previewId: 'preview-1' }),
    )
    expect(commands.cancelScanPreview).not.toHaveBeenCalled()
    expect(rejections).toEqual([])
    expect(onDialogRenderError).not.toHaveBeenCalled()
  })
})
