/**
 * The dialog-failure recovery path in `createDialogState`.
 *
 * A dialog that throws mid-render puts nothing on screen, but the `show*` flag
 * that opened it is already true, and `isConfirmationDialogOpen()` suppresses
 * the pane's keyboard while it is. That's the wedge: a user pressed F6, saw
 * nothing, and had no working keys and nothing to escape from.
 *
 * `dismissAllAfterDialogFailure` is the close `DialogManager`'s `DialogBoundary`
 * calls. It must clear EVERY dialog (not only the confirmation ones
 * `closeConfirmationDialog` covers) and give the pane its focus back. Reporting
 * and the toast are the boundary's (`#lib/ui/boundary/DialogBoundary.svelte.test.ts`).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import type { FilePaneAPI } from './types'

const { addToast, logError } = vi.hoisted(() => ({ addToast: vi.fn(), logError: vi.fn() }))

vi.mock('#lib/tauri-commands/index.js', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  forgetContainerPassword: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))
// Partial: the settings registry pulls `availableLocales` off this module at import time.
vi.mock('#lib/intl/messages.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/intl/messages.svelte.js')>()),
  tString: (key: string) => key,
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: logError }),
}))
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('#lib/file-operations/mkdir/new-folder-operations.js', () => ({ moveCursorToNewFolder: vi.fn() }))

const onRefocus = vi.fn()

function makeState() {
  const paneRef = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  } as unknown as FilePaneAPI
  return createDialogState({
    getLeftPaneRef: () => paneRef,
    getRightPaneRef: () => paneRef,
    getFocusedPaneRef: () => paneRef,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    // No pane navigation in these suites; the trash toast is the only consumer.
    getExplorer: () => undefined,
    onRefocus,
    skipsConfirmations: () => false,
    onOpenInEditor: vi.fn(),
  })
}

describe('dialog render-failure recovery', () => {
  beforeEach(() => {
    onRefocus.mockReset()
    addToast.mockReset()
    logError.mockReset()
  })

  it('un-suppresses the pane keyboard after a confirmation dialog throws', () => {
    const dialogs = makeState()
    dialogs.showNewFolder({
      currentPath: '/tmp',
      listingId: 'listing-1',
      showHiddenFiles: false,
      initialName: '',
      volumeId: 'root',
    })
    expect(dialogs.isConfirmationDialogOpen()).toBe(true)

    dialogs.dismissAllAfterDialogFailure()

    expect(dialogs.isConfirmationDialogOpen()).toBe(false)
    expect(dialogs.showNewFolderDialog).toBe(false)
    expect(onRefocus).toHaveBeenCalledTimes(1)
  })

  it('clears every dialog, including the ones closeConfirmationDialog leaves alone', () => {
    const dialogs = makeState()
    dialogs.showAlert('Heads up', 'Something to say')
    expect(dialogs.showAlertDialog).toBe(true)

    dialogs.dismissAllAfterDialogFailure()

    expect(dialogs.showAlertDialog).toBe(false)
    expect(dialogs.showTransferDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.showTransferErrorDialog).toBe(false)
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.showDeleteDialog).toBe(false)
    expect(dialogs.showNewFolderDialog).toBe(false)
    expect(dialogs.showNewFileDialog).toBe(false)
  })
})
