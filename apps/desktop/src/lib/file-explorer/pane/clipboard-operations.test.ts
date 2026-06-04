import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'

const {
  copyFilesToClipboardSpy,
  cutFilesToClipboardSpy,
  copyPathsToClipboardSpy,
  cutPathsToClipboardSpy,
  readClipboardFilesSpy,
  clearClipboardCutStateSpy,
  addToastSpy,
  resolveSnapshotPathsSpy,
  getCommonParentPathSpy,
  logErrorSpy,
} = vi.hoisted(() => ({
  copyFilesToClipboardSpy: vi.fn<() => Promise<number>>(),
  cutFilesToClipboardSpy: vi.fn<() => Promise<number>>(),
  copyPathsToClipboardSpy: vi.fn<() => Promise<number>>(),
  cutPathsToClipboardSpy: vi.fn<() => Promise<number>>(),
  readClipboardFilesSpy: vi.fn<() => Promise<{ paths: string[]; isCut: boolean }>>(),
  clearClipboardCutStateSpy: vi.fn<() => Promise<void>>(),
  addToastSpy: vi.fn<(content: unknown, options?: unknown) => string>(),
  resolveSnapshotPathsSpy: vi.fn<() => string[]>(),
  getCommonParentPathSpy: vi.fn<() => string>(),
  logErrorSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  DEFAULT_VOLUME_ID: 'root',
  copyFilesToClipboard: copyFilesToClipboardSpy,
  cutFilesToClipboard: cutFilesToClipboardSpy,
  copyPathsToClipboard: copyPathsToClipboardSpy,
  cutPathsToClipboard: cutPathsToClipboardSpy,
  readClipboardFiles: readClipboardFilesSpy,
  clearClipboardCutState: clearClipboardCutStateSpy,
}))

vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))

vi.mock('$lib/search/snapshot-store.svelte', () => ({ resolveSnapshotPaths: resolveSnapshotPathsSpy }))

vi.mock('./transfer-operations', () => ({ getCommonParentPath: getCommonParentPathSpy }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: logErrorSpy, warn: vi.fn(), info: vi.fn(), debug: vi.fn() }),
}))

import { createClipboardOperations } from './clipboard-operations'

/** Builds a `FilePaneAPI` stub exposing only the members the clipboard path reads. */
function buildPaneRef(
  overrides: Partial<{
    listingId: string | null
    hasParent: boolean
    selectedIndices: number[]
    cursorIndex: number
    currentPath: string
  }> = {},
): FilePaneAPI {
  const stub = {
    getListingId: () => ('listingId' in overrides ? overrides.listingId : 'listing-1'),
    hasParentEntry: () => overrides.hasParent ?? false,
    getSelectedIndices: () => overrides.selectedIndices ?? [],
    getCursorIndex: () => overrides.cursorIndex ?? 0,
    getCurrentPath: () => overrides.currentPath ?? '/Users/x/dir',
  }
  return stub as unknown as FilePaneAPI
}

interface AccessConfig {
  focusedPane?: 'left' | 'right'
  paneRef?: FilePaneAPI | undefined
  volumeId?: string
  path?: string
  showHiddenFiles?: boolean
}

function buildAccess(config: AccessConfig = {}): PaneAccess {
  return {
    getPaneRef: () => ('paneRef' in config ? config.paneRef : buildPaneRef()),
    getPanePath: () => config.path ?? '/dest/dir',
    getPaneVolumeId: () => config.volumeId ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane: (pane) => (pane === 'left' ? 'right' : 'left'),
    getShowHiddenFiles: () => config.showHiddenFiles ?? true,
    getVolumes: () => [],
    focusContainer: () => {},
  }
}

const dialogsStub = { startTransferProgress: vi.fn() }

function buildDialogs() {
  return dialogsStub as unknown as Parameters<typeof createClipboardOperations>[1]
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('copyToClipboard', () => {
  it('copies snapshot paths by value and toasts the pluralized count for a search-results pane', async () => {
    resolveSnapshotPathsSpy.mockReturnValue(['/a.txt', '/b.txt'])
    copyPathsToClipboardSpy.mockResolvedValue(2)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1' })
    const access = buildAccess({ paneRef, volumeId: 'search-results' })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(copyPathsToClipboardSpy).toHaveBeenCalledWith(['/a.txt', '/b.txt'])
    expect(copyFilesToClipboardSpy).not.toHaveBeenCalled()
    expect(addToastSpy).toHaveBeenCalledWith('Copied 2 items', { level: 'info' })
  })

  it('uses the singular noun when a single snapshot item is copied', async () => {
    resolveSnapshotPathsSpy.mockReturnValue(['/only.txt'])
    copyPathsToClipboardSpy.mockResolvedValue(1)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1' })
    const access = buildAccess({ paneRef, volumeId: 'search-results' })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(addToastSpy).toHaveBeenCalledWith('Copied 1 item', { level: 'info' })
  })

  it('falls back to the listing-id path when a snapshot resolves to no paths', async () => {
    resolveSnapshotPathsSpy.mockReturnValue([])
    copyFilesToClipboardSpy.mockResolvedValue(3)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1' })
    const access = buildAccess({ paneRef, volumeId: 'search-results' })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(copyPathsToClipboardSpy).not.toHaveBeenCalled()
    expect(copyFilesToClipboardSpy).toHaveBeenCalled()
  })

  it('refuses MTP copy with a toast pointing at F5 and never touches the clipboard IPC', async () => {
    const access = buildAccess({ volumeId: 'mtp-device-1' })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(addToastSpy).toHaveBeenCalledWith('Use F5 to copy files from MTP devices', { level: 'info' })
    expect(copyFilesToClipboardSpy).not.toHaveBeenCalled()
  })

  it('copies via listing id on a regular pane and forwards hasParent + showHiddenFiles', async () => {
    copyFilesToClipboardSpy.mockResolvedValue(5)
    const paneRef = buildPaneRef({ listingId: 'lst-9', hasParent: true, selectedIndices: [1, 2], cursorIndex: 4 })
    const access = buildAccess({ paneRef, volumeId: 'root', showHiddenFiles: false })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(copyFilesToClipboardSpy).toHaveBeenCalledWith('lst-9', [1, 2], 4, true, false)
    expect(addToastSpy).toHaveBeenCalledWith('Copied 5 items', { level: 'info' })
  })

  it('does nothing when the focused pane has no listing id', async () => {
    const access = buildAccess({ paneRef: buildPaneRef({ listingId: null }) })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(copyFilesToClipboardSpy).not.toHaveBeenCalled()
    expect(addToastSpy).not.toHaveBeenCalled()
  })
})

describe('cutToClipboard', () => {
  it('cuts snapshot paths by value and toasts the move-ready wording', async () => {
    resolveSnapshotPathsSpy.mockReturnValue(['/a.txt', '/b.txt'])
    cutPathsToClipboardSpy.mockResolvedValue(2)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1' })
    const access = buildAccess({ paneRef, volumeId: 'search-results' })

    await createClipboardOperations(access, buildDialogs()).cutToClipboard()

    expect(cutPathsToClipboardSpy).toHaveBeenCalledWith(['/a.txt', '/b.txt'])
    expect(addToastSpy).toHaveBeenCalledWith('2 items ready to move. Paste to complete.', { level: 'info' })
  })

  it('refuses MTP cut with a toast pointing at F6', async () => {
    const access = buildAccess({ volumeId: 'mtp-device-1' })

    await createClipboardOperations(access, buildDialogs()).cutToClipboard()

    expect(addToastSpy).toHaveBeenCalledWith('Use F6 to move files from MTP devices', { level: 'info' })
    expect(cutFilesToClipboardSpy).not.toHaveBeenCalled()
  })

  it('cuts via listing id on a regular pane and toasts the singular move-ready wording', async () => {
    cutFilesToClipboardSpy.mockResolvedValue(1)
    const access = buildAccess({ volumeId: 'root' })

    await createClipboardOperations(access, buildDialogs()).cutToClipboard()

    expect(cutFilesToClipboardSpy).toHaveBeenCalled()
    expect(addToastSpy).toHaveBeenCalledWith('1 item ready to move. Paste to complete.', { level: 'info' })
  })
})

describe('pasteFromClipboard', () => {
  it('refuses pasting onto an MTP pane before reading the clipboard', async () => {
    const access = buildAccess({ volumeId: 'mtp-device-1' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(addToastSpy).toHaveBeenCalledWith('Use F5 to copy files to MTP devices', { level: 'info' })
    expect(readClipboardFilesSpy).not.toHaveBeenCalled()
    expect(dialogsStub.startTransferProgress).not.toHaveBeenCalled()
  })

  it('warns and bails when the clipboard is empty', async () => {
    readClipboardFilesSpy.mockResolvedValue({ paths: [], isCut: false })
    const access = buildAccess({ volumeId: 'root' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(addToastSpy).toHaveBeenCalledWith('No files on the clipboard. Copy files first with ⌘C.', {
      level: 'warn',
    })
    expect(dialogsStub.startTransferProgress).not.toHaveBeenCalled()
  })

  it('starts a copy transfer for non-cut clipboard contents without forceMove', async () => {
    readClipboardFilesSpy.mockResolvedValue({ paths: ['/x/a.txt'], isCut: false })
    getCommonParentPathSpy.mockReturnValue('/x')
    const access = buildAccess({ focusedPane: 'left', volumeId: 'root', path: '/dest' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(dialogsStub.startTransferProgress).toHaveBeenCalledTimes(1)
    expect(dialogsStub.startTransferProgress.mock.calls[0][0]).toMatchObject({
      operationType: 'copy',
      sourcePaths: ['/x/a.txt'],
      destinationPath: '/dest',
      direction: 'left',
      sourcePaneSide: 'right',
    })
    expect(clearClipboardCutStateSpy).not.toHaveBeenCalled()
  })

  it('starts a move transfer and clears cut state for cut clipboard contents', async () => {
    readClipboardFilesSpy.mockResolvedValue({ paths: ['/x/a.txt'], isCut: true })
    getCommonParentPathSpy.mockReturnValue('/x')
    const access = buildAccess({ focusedPane: 'right', volumeId: 'root', path: '/dest' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(dialogsStub.startTransferProgress.mock.calls[0][0]).toMatchObject({
      operationType: 'move',
      direction: 'right',
      sourcePaneSide: 'left',
    })
    expect(clearClipboardCutStateSpy).toHaveBeenCalledTimes(1)
  })

  it('forces a move when forceMove is set even for a non-cut clipboard', async () => {
    readClipboardFilesSpy.mockResolvedValue({ paths: ['/x/a.txt'], isCut: false })
    getCommonParentPathSpy.mockReturnValue('/x')
    const access = buildAccess({ volumeId: 'root' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(true)

    expect(dialogsStub.startTransferProgress.mock.calls[0][0]).toMatchObject({ operationType: 'move' })
    expect(clearClipboardCutStateSpy).not.toHaveBeenCalled()
  })
})

describe('getSnapshotClipboardPaths', () => {
  it('resolves snapshot paths for a search-results pane', () => {
    resolveSnapshotPathsSpy.mockReturnValue(['/a.txt'])
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-7', selectedIndices: [0], cursorIndex: 0 })
    const access = buildAccess({ paneRef, volumeId: 'search-results' })

    const result = createClipboardOperations(access, buildDialogs()).getSnapshotClipboardPaths()

    expect(resolveSnapshotPathsSpy).toHaveBeenCalledWith('sr-7', [0], 0)
    expect(result).toEqual({ paths: ['/a.txt'], snapshotId: 'sr-7' })
  })

  it('returns null when the focused pane is not a search-results pane', () => {
    const access = buildAccess({ volumeId: 'root' })

    expect(createClipboardOperations(access, buildDialogs()).getSnapshotClipboardPaths()).toBeNull()
    expect(resolveSnapshotPathsSpy).not.toHaveBeenCalled()
  })

  it('returns null when a search-results pane resolves to no paths', () => {
    resolveSnapshotPathsSpy.mockReturnValue([])
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-7' })
    const access = buildAccess({ paneRef, volumeId: 'search-results' })

    expect(createClipboardOperations(access, buildDialogs()).getSnapshotClipboardPaths()).toBeNull()
  })
})
