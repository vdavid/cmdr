import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { TransferOperationType } from '../types'
import type { FilePaneAPI } from './types'

// Stubs and fixtures live in `file-operation-commands.test-harness.ts`, shared
// with `file-operation-commands.search-results.test.ts`. Every factory below
// reaches the harness through a lazy `await import`, which is what lets the
// harness stay free of a `vi.hoisted` block.
vi.mock('#lib/tauri-commands/index.js', async () => {
  const { spies } = await import('./file-operation-commands.test-harness')
  const { asListingLookupError } = await vi.importActual<typeof import('#lib/tauri-commands/listing-gone.js')>(
    '#lib/tauri-commands/listing-gone.js',
  )
  return {
    DEFAULT_VOLUME_ID: 'root',
    asListingLookupError,
    getFileAt: spies.getFileAt,
    getSelectionEntries: spies.getSelectionEntries,
    trashRoutingForPaths: spies.trashRoutingForPaths,
  }
})

vi.mock('#lib/ui/toast/index.js', async () => ({
  addToast: (await import('./file-operation-commands.test-harness')).spies.addToast,
}))

vi.mock('#lib/search/snapshot-store.svelte.js', async () => {
  const { spies, resolveSnapshotEntriesStub } = await import('./file-operation-commands.test-harness')
  return {
    getSnapshot: spies.getSnapshot,
    resolveSnapshotEntries: resolveSnapshotEntriesStub,
    // Pure namespace arithmetic with no store state behind it, so the mock keeps
    // the real shape rather than a spy.
    snapshotIdFromPanePath: (path: string) =>
      path.startsWith('search-results://') ? path.slice('search-results://'.length) : null,
  }
})

// Source/dest routing reads the capability table via `capabilitiesFor`, which
// resolves fsType/category from the volume store for real ids. The 'search-results'
// id short-circuits before the lookup; real ids ('root', …) fall to the listable
// `local` default with an empty store. The read-only alerts read `access.getVolumes()`
// (the test's own `volumes` fixture), NOT the store — those stay per-VolumeInfo.
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [] }))

vi.mock('#lib/search/capabilities.js', () => ({
  searchResultsNotAFolderToast: () => 'Search results aren’t a folder. Pick a real destination.',
}))

vi.mock('#lib/file-viewer/open-viewer.js', async () => ({
  openFileViewer: (await import('./file-operation-commands.test-harness')).spies.openFileViewer,
}))

vi.mock('#lib/file-operations/mkdir/new-folder-operations.js', async () => ({
  getInitialFolderName: (await import('./file-operation-commands.test-harness')).spies.getInitialFolderName,
}))

vi.mock('#lib/file-operations/mkfile/new-file-operations.js', async () => ({
  getInitialFileName: (await import('./file-operation-commands.test-harness')).spies.getInitialFileName,
}))

// Keep the pure helpers (`getDestinationVolumeInfo`, `buildTransferPropsFromSnapshot`)
// real so the read-only and snapshot assertions exercise the actual props builders;
// stub only the two listing-id-driven async builders, which would otherwise reach
// into un-mocked tauri-commands. This lets us assert which branch (selection vs
// cursor) ran without standing up a full listing fixture.
vi.mock('./transfer-operations', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./transfer-operations')>()
  const { spies } = await import('./file-operation-commands.test-harness')
  return {
    ...actual,
    buildTransferPropsFromSelection: spies.buildTransferPropsFromSelection,
    buildTransferPropsFromCursor: spies.buildTransferPropsFromCursor,
  }
})

vi.mock('#lib/logging/logger.js', async () => {
  const { spies } = await import('./file-operation-commands.test-harness')
  return { getAppLogger: () => ({ error: vi.fn(), warn: spies.logWarn, info: vi.fn(), debug: spies.logDebug }) }
})

import { createFileOperationCommands } from './file-operation-commands'
import {
  spies,
  buildAccess,
  buildDialogs,
  buildPaneRef,
  fileEntry,
  volume,
  type DialogsStub,
} from './file-operation-commands.test-harness'

const {
  getFileAt: getFileAtSpy,
  getSelectionEntries: getSelectionEntriesSpy,
  openFileViewer: openFileViewerSpy,
  getInitialFolderName: getInitialFolderNameSpy,
  getInitialFileName: getInitialFileNameSpy,
  buildTransferPropsFromSelection: buildFromSelectionSpy,
  buildTransferPropsFromCursor: buildFromCursorSpy,
  trashRoutingForPaths: trashRoutingSpy,
} = spies

function create(access: ReturnType<typeof buildAccess>, dialogs: DialogsStub) {
  return createFileOperationCommands(access, dialogs as unknown as Parameters<typeof createFileOperationCommands>[1])
}

beforeEach(() => {
  vi.clearAllMocks()
  // `clearAllMocks` leaves implementations in place, so the routing answer is
  // re-stated here: an unset one would leak across tests in file order.
  trashRoutingSpy.mockResolvedValue({ routing: 'trash', folderMayHoldOnlineOnly: false })
})

describe('startRename', () => {
  it('refuses on a read-only volume with the exact alert and never starts rename', () => {
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume({ mountIsReadOnly: true })] })
    const dialogs = buildDialogs()

    create(access, dialogs).startRename()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      'This is a read-only volume. Renaming isn’t possible here.',
    )
    expect(startRename).not.toHaveBeenCalled()
  })

  it('starts rename on the focused pane for a writable volume', () => {
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })

    create(access, buildDialogs()).startRename()

    expect(startRename).toHaveBeenCalledTimes(1)
  })

  it('starts rename inside a zip (writable archive, no refusal)', () => {
    // A zip is writable: renaming an entry inside it runs the real managed
    // archive-edit flow, so no refusal alert fires even though the path crosses a
    // `.zip`. The parent drive isn't read-only, so nothing blocks it.
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumes: [volume()],
      paths: { left: '/left/foo.zip/inner' },
    })
    const dialogs = buildDialogs()

    create(access, dialogs).startRename()

    expect(dialogs.showAlert).not.toHaveBeenCalled()
    expect(startRename).toHaveBeenCalledTimes(1)
  })

  it("refuses a .git listing's worktrees row with the portal alert, though the pane is writable (#529)", () => {
    // The row stands in for git's REAL `.git/worktrees/` folder at the same path.
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename, pathUnderCursor: '/left/repo/.git/worktrees' })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()], paths: { left: '/left/repo/.git' } })
    const dialogs = buildDialogs()

    create(access, dialogs).startRename()

    expect(dialogs.showAlert.mock.calls[0]?.[0]).toBe('Read-only git portal')
    expect(startRename).not.toHaveBeenCalled()
  })

  it('starts rename on a real entry under .git', () => {
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename, pathUnderCursor: '/left/repo/.git/description' })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()], paths: { left: '/left/repo/.git' } })

    create(access, buildDialogs()).startRename()

    expect(startRename).toHaveBeenCalledTimes(1)
  })

  it('still refuses rename inside a zip that lives on a read-only volume', () => {
    // A writable-archive path doesn't override a read-only parent VolumeInfo: the
    // zip can't be rewritten in place on a read-only mount, so the volume refusal
    // still fires.
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumes: [volume({ mountIsReadOnly: true })],
      paths: { left: '/left/foo.zip/inner' },
    })
    const dialogs = buildDialogs()

    create(access, dialogs).startRename()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      'This is a read-only volume. Renaming isn’t possible here.',
    )
    expect(startRename).not.toHaveBeenCalled()
  })
})

describe('refuseMultiRename', () => {
  function refusal(path: string, readOnlyMount = false) {
    const access = buildAccess({
      paneRefs: { left: buildPaneRef() },
      volumes: [volume({ mountIsReadOnly: readOnlyMount })],
      paths: { left: path },
    })
    const dialogs = buildDialogs()
    const refused = create(access, dialogs).refuseMultiRename()
    return { refused, alert: dialogs.showAlert.mock.calls[0] as [string, string] | undefined }
  }

  it('lets a writable folder through without an alert', () => {
    expect(refusal('/left/photos')).toEqual({ refused: false, alert: undefined })
  })

  it('refuses inside a read-only archive with the archive alert', () => {
    const { refused, alert } = refusal('/left/backup.tar/inner')
    expect(refused).toBe(true)
    expect(alert?.[0]).toBe('Read-only archive')
  })

  it('refuses inside a zip too: F2 edits a zip, but Multi-rename only renames on a drive', () => {
    const { refused, alert } = refusal('/left/foo.zip/inner')
    expect(refused).toBe(true)
    expect(alert).toEqual([
      'Multi-rename doesn’t work inside archives',
      'To rename files in this zip, rename them one at a time.',
    ])
  })

  it('refuses inside the .git portal with the portal alert', () => {
    const { refused, alert } = refusal('/left/repo/.git/branches/main')
    expect(refused).toBe(true)
    expect(alert?.[0]).toBe('Read-only git portal')
  })

  it('refuses on a read-only volume with the rename alert', () => {
    const { refused, alert } = refusal('/left/photos', true)
    expect(refused).toBe(true)
    expect(alert).toEqual(['Read-only volume', 'This is a read-only volume. Renaming isn’t possible here.'])
  })
})

describe('cancelRename', () => {
  it('cancels rename on both panes', () => {
    const cancelLeft = vi.fn()
    const cancelRight = vi.fn()
    const access = buildAccess({
      paneRefs: {
        left: buildPaneRef({ cancelRename: cancelLeft }),
        right: buildPaneRef({ cancelRename: cancelRight }),
      },
    })

    create(access, buildDialogs()).cancelRename()

    expect(cancelLeft).toHaveBeenCalledTimes(1)
    expect(cancelRight).toHaveBeenCalledTimes(1)
  })
})

describe('isRenaming', () => {
  it('returns true when either pane is renaming', () => {
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ isRenaming: () => false }), right: buildPaneRef({ isRenaming: () => true }) },
    })

    expect(create(access, buildDialogs()).isRenaming()).toBe(true)
  })

  it('returns false when neither pane is renaming', () => {
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ isRenaming: () => false }), right: buildPaneRef({ isRenaming: () => false }) },
    })

    expect(create(access, buildDialogs()).isRenaming()).toBe(false)
  })
})

describe('openNewFolderDialog', () => {
  it('refuses on a read-only volume with the exact alert', async () => {
    const access = buildAccess({ volumes: [volume({ mountIsReadOnly: true })] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFolderDialog()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      'This is a read-only volume. Creating folders isn’t possible here.',
    )
    expect(dialogs.showNewFolder).not.toHaveBeenCalled()
  })

  it('opens the new-folder dialog inside a zip (writable archive)', async () => {
    getInitialFolderNameSpy.mockResolvedValue('seed')
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1' }) },
      volumes: [volume()],
      paths: { left: '/left/foo.zip' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFolderDialog()

    // No refusal: creating a folder inside a zip runs the real managed
    // archive-edit flow, so the dialog opens like any writable destination.
    expect(dialogs.showAlert).not.toHaveBeenCalled()
    expect(dialogs.showNewFolder).toHaveBeenCalledWith({
      currentPath: '/left/foo.zip',
      listingId: 'lst-1',
      showHiddenFiles: true,
      initialName: 'seed',
      volumeId: 'root',
    })
  })

  it('bails when the focused pane has no listing id', async () => {
    const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: null }) }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFolderDialog()

    expect(dialogs.showNewFolder).not.toHaveBeenCalled()
  })

  it('opens the new folder dialog with the cursor-derived initial name', async () => {
    getInitialFolderNameSpy.mockResolvedValue('seed')
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1' }) },
      volumes: [volume()],
      paths: { left: '/left/dir' },
      showHiddenFiles: false,
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFolderDialog()

    expect(dialogs.showNewFolder).toHaveBeenCalledWith({
      currentPath: '/left/dir',
      listingId: 'lst-1',
      showHiddenFiles: false,
      initialName: 'seed',
      volumeId: 'root',
    })
  })
})

describe('openNewFileDialog', () => {
  it('refuses on a read-only volume with the exact alert', async () => {
    const access = buildAccess({ volumes: [volume({ mountIsReadOnly: true })] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFileDialog()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      'This is a read-only volume. Creating files isn’t possible here.',
    )
    expect(dialogs.showNewFile).not.toHaveBeenCalled()
  })

  it('opens the new-file dialog inside a zip (writable archive)', async () => {
    getInitialFileNameSpy.mockResolvedValue('seed')
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1' }) },
      volumes: [volume()],
      paths: { left: '/left/foo.zip' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFileDialog()

    // No refusal: creating a file inside a zip runs the managed archive-edit flow.
    expect(dialogs.showAlert).not.toHaveBeenCalled()
    expect(dialogs.showNewFile).toHaveBeenCalledWith({
      currentPath: '/left/foo.zip',
      listingId: 'lst-1',
      showHiddenFiles: true,
      initialName: 'seed',
      volumeId: 'root',
    })
  })

  it('bails when the focused pane has no listing id', async () => {
    const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: null }) }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFileDialog()

    expect(dialogs.showNewFile).not.toHaveBeenCalled()
  })

  it('opens the new file dialog with the cursor-derived initial name', async () => {
    getInitialFileNameSpy.mockResolvedValue('seed.txt')
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1' }) },
      volumes: [volume()],
      paths: { left: '/left/dir' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFileDialog()

    expect(dialogs.showNewFile).toHaveBeenCalledWith({
      currentPath: '/left/dir',
      listingId: 'lst-1',
      showHiddenFiles: true,
      initialName: 'seed.txt',
      volumeId: 'root',
    })
  })
})

describe('confirmation dialog passthroughs', () => {
  it('forwards closeConfirmationDialog', () => {
    const dialogs = buildDialogs()
    create(buildAccess(), dialogs).closeConfirmationDialog()
    expect(dialogs.closeConfirmationDialog).toHaveBeenCalledTimes(1)
  })

  it('forwards isConfirmationDialogOpen result', () => {
    const dialogs = buildDialogs()
    dialogs.isConfirmationDialogOpen.mockReturnValue(true)
    expect(create(buildAccess(), dialogs).isConfirmationDialogOpen()).toBe(true)
  })
})

describe('openViewerForCursor', () => {
  it('bails when there is no listing id', async () => {
    const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: null }) } })

    await create(access, buildDialogs()).openViewerForCursor()

    expect(getFileAtSpy).not.toHaveBeenCalled()
    expect(openFileViewerSpy).not.toHaveBeenCalled()
  })

  it('does not open the viewer for a directory, the parent entry, or rows that never settled', async () => {
    for (const cursorEntry of [fileEntry({ isDirectory: true }), fileEntry({ name: '..' }), null]) {
      const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: 'lst-1', cursorEntry }) } })

      await create(access, buildDialogs()).openViewerForCursor()
    }

    expect(openFileViewerSpy).not.toHaveBeenCalled()
  })

  it('opens the file the checked cursor read names, never an unchecked row lookup', async () => {
    // What an unchecked read would see right after a watcher insert: the neighbor.
    getFileAtSpy.mockResolvedValue(fileEntry({ path: '/Users/x/dir/neighbor.md' }))
    const access = buildAccess({
      paneRefs: {
        left: buildPaneRef({ listingId: 'lst-1', cursorEntry: fileEntry({ path: '/Users/x/dir/note.md' }) }),
      },
    })

    await create(access, buildDialogs()).openViewerForCursor()

    expect(openFileViewerSpy).toHaveBeenCalledWith('/Users/x/dir/note.md', 'root')
  })

  it('threads a non-root pane volume id so a remote-hosted archive previews through it', async () => {
    const access = buildAccess({
      paneRefs: {
        left: buildPaneRef({
          listingId: 'lst-1',
          volumeId: 'smb-1',
          cursorEntry: fileEntry({ path: '/share/bundle.zip/inner.txt' }),
        }),
      },
    })

    await create(access, buildDialogs()).openViewerForCursor()

    expect(openFileViewerSpy).toHaveBeenCalledWith('/share/bundle.zip/inner.txt', 'smb-1')
  })
})

describe('openTransferDialog', () => {
  it('refuses a read-only destination with the device-specific alert', async () => {
    const access = buildAccess({
      focusedPane: 'left',
      volumeIds: { left: 'root', right: 'mtp-1' },
      volumes: [volume({ id: 'mtp-1', name: 'Pixel SD card', mountIsReadOnly: true })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only device',
      '“Pixel SD card” is read-only. You can copy files from it, but not to it.',
    )
    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })

  it('allows a zip destination (opposite pane inside a zip) and opens the transfer dialog', async () => {
    // Both panes are on the writable root drive; the opposite pane's PATH crosses a
    // zip. A zip is a writable destination now, so no refusal fires and the
    // transfer dialog opens (the backend routes it into the archive-edit flow).
    buildFromCursorSpy.mockResolvedValue({ operationType: 'copy' })
    const access = buildAccess({
      focusedPane: 'left',
      volumeIds: { left: 'root', right: 'root' },
      paths: { left: '/left/dir', right: '/right/foo.zip/inner' },
      volumes: [volume()],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(dialogs.showAlert).not.toHaveBeenCalled()
    expect(dialogs.showTransfer).toHaveBeenCalledWith(expect.objectContaining({ operationType: 'copy' }))
  })

  it('builds transfer props from the selection when items are selected', async () => {
    buildFromSelectionSpy.mockResolvedValue({ operationType: 'copy' })
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [3, 4], hasParent: true })
    const access = buildAccess({ focusedPane: 'left', paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    // Selection branch builds from the selected indices; the cursor branch is untouched.
    expect(buildFromSelectionSpy).toHaveBeenCalled()
    expect(buildFromCursorSpy).not.toHaveBeenCalled()
    expect(dialogs.showTransfer).toHaveBeenCalledTimes(1)
  })

  it('builds transfer props from the cursor when nothing is selected', async () => {
    buildFromCursorSpy.mockResolvedValue({ operationType: 'copy' })
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [], cursorIndex: 1 })
    const access = buildAccess({ focusedPane: 'left', paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(buildFromSelectionSpy).not.toHaveBeenCalled()
    expect(buildFromCursorSpy).toHaveBeenCalled()
    expect(dialogs.showTransfer).toHaveBeenCalledTimes(1)
  })

  it('F5 asks for the rename editor on a single-item duplicate; an MCP copy does not', async () => {
    // The whole trigger split rests on this field. F5 is a person at the keyboard
    // who may want to name the copy; an auto-confirmed copy is an agent's, and an
    // agent must not pull focus into a text field in front of whoever is watching.
    // The Duplicate command and drag answer for themselves at their own call sites.
    buildFromCursorSpy.mockResolvedValue({ operationType: 'copy' })
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [], cursorIndex: 1 })
    const access = buildAccess({ focusedPane: 'left', paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()
    const commands = create(access, dialogs)

    await commands.openTransferDialog('copy')
    expect(dialogs.showTransfer).toHaveBeenLastCalledWith(
      expect.objectContaining({ duplicateFollowUp: 'openRenameEditor' }),
    )

    await commands.openTransferDialog('copy', true, 'overwrite_all', 'mcp-1', 'aiClient')
    expect(dialogs.showTransfer).toHaveBeenLastCalledWith(expect.objectContaining({ duplicateFollowUp: 'nothing' }))
  })

  it('bails on a non-snapshot pane without a listing id', async () => {
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: buildPaneRef({ listingId: null }) },
      volumes: [volume()],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })
})

// F5 / F6 pressed while the pane's rows are still settling (a listing reload, a resort, a
// watcher diff) used to return silently: the key did nothing. It was the top Linux E2E flake
// (12 of 17 retry-rescued flakes in 109 CI runs, #557), and for a user it's a dead key.
describe('openTransferDialog while the source rows settle', () => {
  it('opens the dialog once the selected rows settle, rather than dropping the press', async () => {
    buildFromSelectionSpy.mockResolvedValue({ operationType: 'copy' })
    let ready = false
    let settle: (value: boolean) => void = () => {}
    const paneRef = buildPaneRef({
      listingId: 'lst-1',
      selectedIndices: [3, 4],
      isRowStateReady: () => ready,
      whenRowStateReady: () =>
        new Promise<boolean>((resolve) => {
          settle = (value) => {
            ready = value
            resolve(value)
          }
        }),
    })
    const access = buildAccess({ focusedPane: 'left', paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    const opening = create(access, dialogs).openTransferDialog('copy')
    await Promise.resolve()
    expect(dialogs.showTransfer).not.toHaveBeenCalled()

    settle(true)
    await opening

    expect(buildFromSelectionSpy).toHaveBeenCalledTimes(1)
    expect(dialogs.showTransfer).toHaveBeenCalledTimes(1)
  })

  it('says so with a toast when the rows never settle in time, never a silent no-op', async () => {
    const paneRef = buildPaneRef({
      listingId: 'lst-1',
      selectedIndices: [3, 4],
      isRowStateReady: () => false,
      whenRowStateReady: () => Promise.resolve(false),
    })
    const access = buildAccess({ focusedPane: 'left', paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
    expect(buildFromSelectionSpy).not.toHaveBeenCalled()
    expect(spies.addToast).toHaveBeenCalledWith(
      'The list here is still updating, so nothing started. Try again in a moment.',
      { level: 'info' },
    )
  })

  it('rebuilds from the new rows when they move while the dialog is being prepared', async () => {
    buildFromSelectionSpy.mockResolvedValue({ operationType: 'copy' })
    // The view generation moves once between reading the selection and showing the dialog.
    const generations = [0, 1]
    const paneRef = buildPaneRef({
      listingId: 'lst-1',
      selectedIndices: [3, 4],
      getViewGeneration: () => generations.shift() ?? 1,
    })
    const access = buildAccess({ focusedPane: 'left', paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(buildFromSelectionSpy).toHaveBeenCalledTimes(2)
    expect(dialogs.showTransfer).toHaveBeenCalledTimes(1)
  })
})

describe('openCopyDialog / openMoveDialog', () => {
  it('openCopyDialog delegates with copy semantics', async () => {
    buildFromCursorSpy.mockImplementation((...args: unknown[]) =>
      Promise.resolve({ operationType: args[0] as TransferOperationType }),
    )
    const access = buildAccess({ focusedPane: 'left', volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openCopyDialog()

    expect(buildFromCursorSpy.mock.calls[0]?.[0]).toBe('copy')
    expect(dialogs.showTransfer.mock.calls[0]?.[0]).toMatchObject({ operationType: 'copy' })
  })

  it('openMoveDialog delegates with move semantics', async () => {
    buildFromCursorSpy.mockImplementation((...args: unknown[]) =>
      Promise.resolve({ operationType: args[0] as TransferOperationType }),
    )
    const access = buildAccess({ focusedPane: 'left', volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openMoveDialog()

    expect(buildFromCursorSpy.mock.calls[0]?.[0]).toBe('move')
    expect(dialogs.showTransfer.mock.calls[0]?.[0]).toMatchObject({ operationType: 'move' })
  })
})

describe('openDeleteDialog', () => {
  it('bails when the focused pane has no listing id', async () => {
    const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: null }) }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('refuses on a read-only volume with the exact alert', async () => {
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1' }) },
      volumes: [volume({ mountIsReadOnly: true })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      'This is a read-only volume. Deleting files isn’t possible here.',
    )
    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('opens a PERMANENT delete confirm inside a zip (no trash, archive flag)', async () => {
    // Deleting an entry inside a zip is permanent: there's no Trash inside an
    // archive. Even with the trash preselect (F8, `permanent: false`) and a
    // trash-capable parent drive, the confirm forces permanent, drops trash, and
    // sets the archive flag so the dialog shows the archive warning.
    getSelectionEntriesSpy.mockResolvedValue([fileEntry({ name: 'inner.txt' })])
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] }) },
      volumes: [volume({ supportsTrash: true })],
      paths: { left: '/left/foo.zip/inner' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showAlert).not.toHaveBeenCalled()
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: true,
      supportsTrash: false,
      isArchive: true,
      sourceFolderPath: '/left/foo.zip/inner',
    })
  })

  it('deletes the selection (hasSelection branch) and is not flagged as from-cursor', async () => {
    getSelectionEntriesSpy.mockResolvedValue([fileEntry({ name: 'a.txt' }), fileEntry({ name: 'b.txt' })])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [1, 2], hasParent: true })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumes: [volume()],
      paths: { left: '/left/dir' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: true })

    expect(dialogs.showDeleteConfirmation).toHaveBeenCalledTimes(1)
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isFromCursor: false,
      isPermanent: true,
      sourceFolderPath: '/left/dir',
      sourceVolumeId: 'root',
    })
  })

  it('deletes the cursor item (no-selection branch) flagged as from-cursor', async () => {
    getSelectionEntriesSpy.mockResolvedValue([fileEntry({ name: 'cur.txt' })])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [], cursorIndex: 3 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ isFromCursor: true })
  })

  it('looks up supportsTrash from the source volume', async () => {
    getSelectionEntriesSpy.mockResolvedValue([fileEntry()])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumes: [volume({ supportsTrash: false })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ supportsTrash: false })
  })

  it('bails when getSelectionEntries throws something other than a listing refusal', async () => {
    getSelectionEntriesSpy.mockRejectedValue(new Error('boom'))
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('bails when all fetched entries are the parent ".." entry', async () => {
    getSelectionEntriesSpy.mockResolvedValue([fileEntry({ name: '..' })])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })
})

/** A watcher insert above the selection, landing between the pane applying its
 *  rows and F8: the old row numbers now name the neighbor (`moved-listing.test-fixtures.ts`). */
describe('openDeleteDialog when the rows moved under the press', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  async function pressDelete(options: { onSelection: boolean; catchesUp: boolean }) {
    const moved = await import('./moved-listing.test-fixtures')
    const pane = moved.createMovedPane({ onSelection: options.onSelection })
    getSelectionEntriesSpy.mockImplementation(
      options.catchesUp ? moved.catchingUpAfterRefusal(pane, moved.selectionEntries) : moved.selectionEntries,
    )
    const paneRef = { ...buildPaneRef({ listingId: moved.MOVED_LISTING_ID }), ...pane } as unknown as FilePaneAPI
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()
    await create(access, dialogs).openDeleteDialog({ permanent: true })
    return { dialogs, moved }
  }

  it.each([true, false])(
    'waits for the pane to apply the diff and offers the file it picked (selection: %s)',
    async (onSelection) => {
      const { dialogs, moved } = await pressDelete({ onSelection, catchesUp: true })

      expect(dialogs.showDeleteConfirmation).toHaveBeenCalledTimes(1)
      expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ sourcePaths: [moved.SELECTED_PATH] })
      expect(spies.addToast).not.toHaveBeenCalled()
    },
  )

  it('refuses with the toast once the rows never settle within the budget', async () => {
    vi.useFakeTimers()
    const pressed = pressDelete({ onSelection: true, catchesUp: false })
    await vi.advanceTimersByTimeAsync(3_500)
    const { dialogs } = await pressed

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
    expect(spies.addToast).toHaveBeenCalledWith(
      'The list here is still updating, so nothing started. Try again in a moment.',
      { level: 'info' },
    )
  })
})

/** Online-only content in a cloud-storage folder can't reach the Trash without
 *  being downloaded first, so F8 there runs the permanent delete instead. The
 *  backend decides (`cloud_trash.rs`); this is how the answer reaches the dialog. */
describe('openDeleteDialog in a cloud-storage folder', () => {
  const cloudEntry = (name: string) => fileEntry({ name, path: `/Users/x/Library/CloudStorage/Dropbox/Work/${name}` })

  function cloudAccess() {
    return buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1', selectedIndices: [0, 1] }) },
      volumes: [volume({ supportsTrash: true })],
      paths: { left: '/Users/x/Library/CloudStorage/Dropbox/Work' },
    })
  }

  beforeEach(() => {
    getSelectionEntriesSpy.mockResolvedValue([cloudEntry('a.pkg'), cloudEntry('b.pkg')])
  })

  it('turns F8 into the permanent delete, and says why', async () => {
    trashRoutingSpy.mockResolvedValue({ routing: 'permanentDeleteAllOnlineOnly', folderMayHoldOnlineOnly: false })
    const dialogs = buildDialogs()

    await create(cloudAccess(), dialogs).openDeleteDialog({ permanent: false })

    expect(trashRoutingSpy).toHaveBeenCalledWith([
      '/Users/x/Library/CloudStorage/Dropbox/Work/a.pkg',
      '/Users/x/Library/CloudStorage/Dropbox/Work/b.pkg',
    ])
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: true,
      supportsTrash: false,
      cloudOnlineOnly: 'all',
    })
  })

  /** Shift+F8 was already permanent, but the in-dialog switch back to trash would
   *  hit the same refusal, so the trash stays off here too. */
  it('keeps the trash switch away from a Shift+F8 dialog too', async () => {
    trashRoutingSpy.mockResolvedValue({ routing: 'permanentDeleteAllOnlineOnly', folderMayHoldOnlineOnly: false })
    const dialogs = buildDialogs()

    await create(cloudAccess(), dialogs).openDeleteDialog({ permanent: true })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: true,
      supportsTrash: false,
      cloudOnlineOnly: 'all',
    })
  })

  /** The banner's two wordings differ in what they can offer as a way out, so the
   *  routing variant has to reach the dialog rather than collapsing to a boolean. */
  it('tells the dialog when only part of the selection is online-only', async () => {
    trashRoutingSpy.mockResolvedValue({ routing: 'permanentDeleteMixedOnlineOnly', folderMayHoldOnlineOnly: false })
    const dialogs = buildDialogs()

    await create(cloudAccess(), dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: true,
      supportsTrash: false,
      cloudOnlineOnly: 'mixed',
    })
  })

  /** Nothing evicted anywhere in the selection: the backend says `trash` and
   *  nothing about the dialog changes. */
  it('leaves everything else on the trash', async () => {
    trashRoutingSpy.mockResolvedValue({ routing: 'trash', folderMayHoldOnlineOnly: false })
    const dialogs = buildDialogs()

    await create(cloudAccess(), dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: false,
      supportsTrash: true,
      cloudOnlineOnly: null,
    })
  })

  /** A selected FOLDER can't carry the flag itself, so the backend says "not yet"
   *  and the dialog's own scan walk finishes the answer. The dialog needs to know
   *  to wait, so the flag has to reach it. */
  it('forwards the unfinished answer for a folder, so the dialog waits on its walk', async () => {
    trashRoutingSpy.mockResolvedValue({ routing: 'trash', folderMayHoldOnlineOnly: true })
    const dialogs = buildDialogs()

    await create(cloudAccess(), dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: false,
      supportsTrash: true,
      cloudOnlineOnly: null,
      cloudFolderMayHoldOnlineOnly: true,
    })
  })

  it('keeps the trash when the question itself fails', async () => {
    trashRoutingSpy.mockRejectedValue(new Error('IPC gone'))
    const dialogs = buildDialogs()

    await create(cloudAccess(), dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isPermanent: false,
      supportsTrash: true,
      cloudOnlineOnly: null,
    })
  })
})
