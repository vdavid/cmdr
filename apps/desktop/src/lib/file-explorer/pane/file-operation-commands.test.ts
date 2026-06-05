import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { SearchSnapshot } from '$lib/search/snapshot-store.svelte'
import type { FileEntry, VolumeInfo, TransferOperationType } from '../types'

const {
  getFileAtSpy,
  getFilesAtIndicesSpy,
  addToastSpy,
  getSnapshotSpy,
  openFileViewerSpy,
  getInitialFolderNameSpy,
  getInitialFileNameSpy,
  buildFromSelectionSpy,
  buildFromCursorSpy,
  logWarnSpy,
  logDebugSpy,
} = vi.hoisted(() => ({
  getFileAtSpy: vi.fn<() => Promise<FileEntry | null>>(),
  getFilesAtIndicesSpy: vi.fn<() => Promise<FileEntry[]>>(),
  addToastSpy: vi.fn<(content: unknown, options?: unknown) => string>(),
  getSnapshotSpy: vi.fn<() => SearchSnapshot | undefined>(),
  openFileViewerSpy: vi.fn<() => Promise<void>>(),
  getInitialFolderNameSpy: vi.fn<() => Promise<string>>(),
  getInitialFileNameSpy: vi.fn<() => Promise<string>>(),
  buildFromSelectionSpy: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
  buildFromCursorSpy: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
  logWarnSpy: vi.fn(),
  logDebugSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  DEFAULT_VOLUME_ID: 'root',
  getFileAt: getFileAtSpy,
  getFilesAtIndices: getFilesAtIndicesSpy,
}))

vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))

vi.mock('$lib/search/snapshot-store.svelte', () => ({ getSnapshot: getSnapshotSpy }))

// Source/dest routing reads the capability table via `capabilitiesFor`, which
// resolves fsType/category from the volume store for real ids. The 'search-results'
// id short-circuits before the lookup; real ids ('root', …) fall to the listable
// `local` default with an empty store. The read-only alerts read `access.getVolumes()`
// (the test's own `volumes` fixture), NOT the store — those stay per-VolumeInfo.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

vi.mock('$lib/search/capabilities', () => ({
  SEARCH_RESULTS_NOT_A_FOLDER_TOAST: "Search results aren't a folder. Pick a real destination.",
}))

vi.mock('$lib/file-viewer/open-viewer', () => ({ openFileViewer: openFileViewerSpy }))

vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ getInitialFolderName: getInitialFolderNameSpy }))

vi.mock('$lib/file-operations/mkfile/new-file-operations', () => ({ getInitialFileName: getInitialFileNameSpy }))

// Keep the pure helpers (`getDestinationVolumeInfo`, `buildTransferPropsFromSnapshot`)
// real so the read-only and snapshot assertions exercise the actual props builders;
// stub only the two listing-id-driven async builders, which would otherwise reach
// into un-mocked tauri-commands. This lets us assert which branch (selection vs
// cursor) ran without standing up a full listing fixture.
vi.mock('./transfer-operations', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./transfer-operations')>()
  return {
    ...actual,
    buildTransferPropsFromSelection: buildFromSelectionSpy,
    buildTransferPropsFromCursor: buildFromCursorSpy,
  }
})

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: vi.fn(), warn: logWarnSpy, info: vi.fn(), debug: logDebugSpy }),
}))

import { createFileOperationCommands } from './file-operation-commands'

/** Builds a `FilePaneAPI` stub exposing only the members the file-operation band reads. */
function buildPaneRef(
  overrides: Partial<{
    listingId: string | null
    hasParent: boolean
    selectedIndices: number[]
    cursorIndex: number
    currentPath: string
    startRename: () => void
    cancelRename: () => void
    isRenaming: () => boolean
  }> = {},
): FilePaneAPI {
  const stub = {
    getListingId: () => ('listingId' in overrides ? overrides.listingId : 'listing-1'),
    hasParentEntry: () => overrides.hasParent ?? false,
    getSelectedIndices: () => overrides.selectedIndices ?? [],
    getCursorIndex: () => overrides.cursorIndex ?? 0,
    getCurrentPath: () => overrides.currentPath ?? '/Users/x/dir',
    startRename: overrides.startRename ?? vi.fn(),
    cancelRename: overrides.cancelRename ?? vi.fn(),
    isRenaming: overrides.isRenaming ?? (() => false),
  }
  return stub as unknown as FilePaneAPI
}

interface AccessConfig {
  focusedPane?: 'left' | 'right'
  paneRefs?: Partial<Record<'left' | 'right', FilePaneAPI | undefined>>
  volumeIds?: Partial<Record<'left' | 'right', string>>
  paths?: Partial<Record<'left' | 'right', string>>
  volumes?: VolumeInfo[]
  showHiddenFiles?: boolean
  focusContainer?: () => void
}

function buildAccess(config: AccessConfig = {}): PaneAccess {
  const otherPane = (pane: 'left' | 'right'): 'left' | 'right' => (pane === 'left' ? 'right' : 'left')
  const defaultRef = buildPaneRef()
  return {
    getPaneRef: (pane) => (config.paneRefs && pane in config.paneRefs ? config.paneRefs[pane] : defaultRef),
    getPanePath: (pane) => config.paths?.[pane] ?? (pane === 'left' ? '/left/dir' : '/right/dir'),
    getPaneVolumeId: (pane) => config.volumeIds?.[pane] ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane,
    getShowHiddenFiles: () => config.showHiddenFiles ?? true,
    getVolumes: () => config.volumes ?? [],
    focusContainer: config.focusContainer ?? (() => {}),
  }
}

interface DialogsStub {
  showAlert: ReturnType<typeof vi.fn>
  showNewFolder: ReturnType<typeof vi.fn>
  showNewFile: ReturnType<typeof vi.fn>
  showTransfer: ReturnType<typeof vi.fn>
  showDeleteConfirmation: ReturnType<typeof vi.fn>
  closeConfirmationDialog: ReturnType<typeof vi.fn>
  isConfirmationDialogOpen: ReturnType<typeof vi.fn>
}

function buildDialogs(): DialogsStub {
  return {
    showAlert: vi.fn(),
    showNewFolder: vi.fn(),
    showNewFile: vi.fn(),
    showTransfer: vi.fn(),
    showDeleteConfirmation: vi.fn(),
    closeConfirmationDialog: vi.fn(),
    isConfirmationDialogOpen: vi.fn(() => false),
  }
}

function create(access: PaneAccess, dialogs: DialogsStub) {
  return createFileOperationCommands(access, dialogs as unknown as Parameters<typeof createFileOperationCommands>[1])
}

/** A minimal VolumeInfo with overridable flags. */
function volume(overrides: Partial<VolumeInfo> = {}): VolumeInfo {
  return {
    id: 'root',
    name: 'Macintosh HD',
    isReadOnly: false,
    supportsTrash: true,
    ...overrides,
  } as unknown as VolumeInfo
}

function snapshotEntry(overrides: Partial<SearchSnapshot['entries'][number]> = {}): SearchSnapshot['entries'][number] {
  return {
    name: 'doc.txt',
    path: '/real/dir/doc.txt',
    parentPath: '/real/dir',
    isDirectory: false,
    size: 42,
    modifiedAt: null,
    iconId: 'ext:txt',
    ...overrides,
  }
}

function snapshot(entries: SearchSnapshot['entries']): SearchSnapshot {
  return { entries } as unknown as SearchSnapshot
}

function fileEntry(overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'doc.txt',
    path: '/Users/x/dir/doc.txt',
    isDirectory: false,
    isSymlink: false,
    size: 10,
    recursiveSize: undefined,
    recursiveFileCount: undefined,
    ...overrides,
  } as unknown as FileEntry
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('startRename', () => {
  it('refuses on a read-only volume with the exact alert and never starts rename', () => {
    const startRename = vi.fn()
    const paneRef = buildPaneRef({ startRename })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume({ isReadOnly: true })] })
    const dialogs = buildDialogs()

    create(access, dialogs).startRename()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      "This is a read-only volume. Renaming isn't possible here.",
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
    const access = buildAccess({ volumes: [volume({ isReadOnly: true })] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFolderDialog()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      "This is a read-only volume. Creating folders isn't possible here.",
    )
    expect(dialogs.showNewFolder).not.toHaveBeenCalled()
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
    const access = buildAccess({ volumes: [volume({ isReadOnly: true })] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openNewFileDialog()

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      "This is a read-only volume. Creating files isn't possible here.",
    )
    expect(dialogs.showNewFile).not.toHaveBeenCalled()
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

  it('does not open the viewer for a directory or the parent entry', async () => {
    getFileAtSpy.mockResolvedValue(fileEntry({ isDirectory: true }))
    const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: 'lst-1', cursorIndex: 1 }) } })

    await create(access, buildDialogs()).openViewerForCursor()

    expect(openFileViewerSpy).not.toHaveBeenCalled()
  })

  it('opens the viewer for a file under the cursor', async () => {
    getFileAtSpy.mockResolvedValue(fileEntry({ path: '/Users/x/dir/note.md' }))
    const access = buildAccess({ paneRefs: { left: buildPaneRef({ listingId: 'lst-1', cursorIndex: 2 }) } })

    await create(access, buildDialogs()).openViewerForCursor()

    expect(openFileViewerSpy).toHaveBeenCalledWith('/Users/x/dir/note.md')
  })
})

describe('openTransferDialog', () => {
  it('warns with the search-results destination toast when the opposite pane is a snapshot', async () => {
    const access = buildAccess({ focusedPane: 'left', volumeIds: { left: 'root', right: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(addToastSpy).toHaveBeenCalledWith("Search results aren't a folder. Pick a real destination.", {
      level: 'warn',
    })
    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })

  it('does not show the search-results toast for a network destination (PR3: kind-scoped)', async () => {
    // A network dest also has `canPasteInto: false`, but the dest-block toast is
    // scoped to the search-results KIND. Historically a network dest fell through
    // here silently; converting the gate to `!canPasteInto` must not start
    // toasting it. The transfer then proceeds past the guard as before.
    const access = buildAccess({
      focusedPane: 'left',
      volumeIds: { left: 'root', right: 'network' },
      paneRefs: { left: buildPaneRef({ listingId: null }) },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(addToastSpy).not.toHaveBeenCalledWith("Search results aren't a folder. Pick a real destination.", {
      level: 'warn',
    })
  })

  it('refuses a read-only destination with the device-specific alert', async () => {
    const access = buildAccess({
      focusedPane: 'left',
      volumeIds: { left: 'root', right: 'mtp-1' },
      volumes: [volume({ id: 'mtp-1', name: 'Pixel SD card', isReadOnly: true })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only device',
      '"Pixel SD card" is read-only. You can copy files from it, but not to it.',
    )
    expect(dialogs.showTransfer).not.toHaveBeenCalled()
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

  it('builds snapshot transfer props for a search-results source pane', async () => {
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ path: '/real/a.txt' }), snapshotEntry({ path: '/real/b.txt' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0, 1] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).toHaveBeenCalledTimes(1)
    expect(dialogs.showTransfer.mock.calls[0][0]).toMatchObject({
      operationType: 'move',
      sourcePaths: ['/real/a.txt', '/real/b.txt'],
    })
  })

  it('does not open a snapshot transfer when the snapshot index is stale (out of range)', async () => {
    getSnapshotSpy.mockReturnValue(snapshot([snapshotEntry()]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [9] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })

  it('does not open a snapshot transfer when the snapshot is missing', async () => {
    getSnapshotSpy.mockReturnValue(undefined)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
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

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('refuses on a read-only volume with the exact alert', async () => {
    const access = buildAccess({
      paneRefs: { left: buildPaneRef({ listingId: 'lst-1' }) },
      volumes: [volume({ isReadOnly: true })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showAlert).toHaveBeenCalledWith(
      'Read-only volume',
      "This is a read-only volume. Deleting files isn't possible here.",
    )
    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('deletes the selection (hasSelection branch) and is not flagged as from-cursor', async () => {
    getFilesAtIndicesSpy.mockResolvedValue([fileEntry({ name: 'a.txt' }), fileEntry({ name: 'b.txt' })])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [1, 2], hasParent: true })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumes: [volume()],
      paths: { left: '/left/dir' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(true)

    expect(dialogs.showDeleteConfirmation).toHaveBeenCalledTimes(1)
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      isFromCursor: false,
      isPermanent: true,
      sourceFolderPath: '/left/dir',
      sourceVolumeId: 'root',
    })
  })

  it('deletes the cursor item (no-selection branch) flagged as from-cursor', async () => {
    getFilesAtIndicesSpy.mockResolvedValue([fileEntry({ name: 'cur.txt' })])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [], cursorIndex: 3 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ isFromCursor: true })
  })

  it('looks up supportsTrash from the source volume', async () => {
    getFilesAtIndicesSpy.mockResolvedValue([fileEntry()])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumes: [volume({ supportsTrash: false })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ supportsTrash: false })
  })

  it('bails when getFilesAtIndices throws', async () => {
    getFilesAtIndicesSpy.mockRejectedValue(new Error('boom'))
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('bails when all fetched entries are the parent ".." entry', async () => {
    getFilesAtIndicesSpy.mockResolvedValue([fileEntry({ name: '..' })])
    const paneRef = buildPaneRef({ listingId: 'lst-1', selectedIndices: [0] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumes: [volume()] })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('builds the delete dialog from the snapshot cursor entry on a search-results pane', async () => {
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'hit.md', path: '/real/hit.md', parentPath: '/real' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', cursorIndex: 0 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(true)

    expect(dialogs.showDeleteConfirmation).toHaveBeenCalledTimes(1)
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourcePaths: ['/real/hit.md'],
      sourceFolderPath: '/real',
      isPermanent: true,
      supportsTrash: true,
      isFromCursor: true,
      sourceVolumeId: 'root',
    })
  })

  it('bails on a search-results pane whose cursor is out of range', async () => {
    getSnapshotSpy.mockReturnValue(snapshot([snapshotEntry()]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', cursorIndex: 9 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('bails on a search-results pane whose snapshot is missing', async () => {
    getSnapshotSpy.mockReturnValue(undefined)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', cursorIndex: 0 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog(false)

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })
})
