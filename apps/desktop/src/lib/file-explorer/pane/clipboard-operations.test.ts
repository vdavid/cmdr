import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { TransferProgressPropsData } from './dialog-state.svelte'

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
  readClipboardFilesSpy: vi.fn<() => Promise<{ paths: string[]; isCut: boolean; isDirectory?: (boolean | null)[] }>>(),
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

// `transfer-entry` (the shared guard chain) imports `getDestinationVolumeInfo`
// from here too, so the mock must export it. Keep it a thin lookup matching the
// real one so the read-only paste guard exercises real behavior off the
// per-test volumes list.
vi.mock('./transfer-operations', () => ({
  getCommonParentPath: getCommonParentPathSpy,
  getDestinationVolumeInfo: (volumeId: string, volumes: { id: string; name: string; isReadOnly?: boolean }[]) => {
    const v = volumes.find((vol) => vol.id === volumeId)
    return v ? { name: v.name, isReadOnly: v.isReadOnly ?? false } : undefined
  },
}))

// The MTP / snapshot refusals read the capability table via `capabilitiesFor`,
// which resolves fsType/category from the volume store for real ids. The two
// virtual ids ('network' / 'search-results') short-circuit before the lookup;
// MTP ids ('mtp-…') classify via `isMtpVolumeId` without needing the store.
// An empty store is enough for every id this suite exercises.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: logErrorSpy, warn: vi.fn(), info: vi.fn(), debug: vi.fn() }),
}))

import { createClipboardOperations } from './clipboard-operations'
// The real classifier (volume store is mocked to empty; virtual ids short-circuit,
// MTP ids classify via `isMtpVolumeId`), used by the equivalence pin below.
import { capabilitiesFor as capabilitiesForReal } from './volume-capabilities'

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
  volumes?: { id: string; name: string; isReadOnly?: boolean }[]
}

function buildAccess(config: AccessConfig = {}): PaneAccess {
  return {
    getPaneRef: () => ('paneRef' in config ? config.paneRef : buildPaneRef()),
    getPanePath: () => config.path ?? '/dest/dir',
    getPaneVolumeId: () => config.volumeId ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane: (pane) => (pane === 'left' ? 'right' : 'left'),
    getShowHiddenFiles: () => config.showHiddenFiles ?? true,
    getVolumes: () => (config.volumes ?? []) as unknown as ReturnType<PaneAccess['getVolumes']>,
    focusContainer: () => {},
  }
}

const dialogsStub = {
  startTransferProgress: vi.fn<(props: TransferProgressPropsData) => void>(),
  showAlert: vi.fn<(title: string, message: string) => void>(),
}

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

  it('refuses copy inside an archive (writable parent) and points at F5/F6', async () => {
    // The pane's volumeId is the writable parent drive; the archive-ness is in the
    // PATH. Without the archive check ⌘C would push unresolvable archive-inner
    // paths onto the OS clipboard.
    const access = buildAccess({ volumeId: 'root', path: '/x/foo.zip/inner' })

    await createClipboardOperations(access, buildDialogs()).copyToClipboard()

    expect(addToastSpy).toHaveBeenCalledWith('To copy files out of an archive, use F5 to copy or F6 to move.', {
      level: 'info',
    })
    expect(copyFilesToClipboardSpy).not.toHaveBeenCalled()
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

  it('refuses cut inside an archive and points at F5/F6', async () => {
    const access = buildAccess({ volumeId: 'root', path: '/x/foo.zip/inner' })

    await createClipboardOperations(access, buildDialogs()).cutToClipboard()

    expect(addToastSpy).toHaveBeenCalledWith('To copy files out of an archive, use F5 to copy or F6 to move.', {
      level: 'info',
    })
    expect(cutFilesToClipboardSpy).not.toHaveBeenCalled()
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

  it('refuses pasting into an archive destination with the archive alert', async () => {
    const access = buildAccess({ volumeId: 'root', path: '/x/foo.zip/inner' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(dialogsStub.showAlert).toHaveBeenCalledWith(
      'Archives are read-only',
      "You can copy files out of an archive, but copying into one isn't possible yet.",
    )
    expect(readClipboardFilesSpy).not.toHaveBeenCalled()
    expect(dialogsStub.startTransferProgress).not.toHaveBeenCalled()
  })

  it('refuses pasting into a read-only destination with the shared "Read-only device" alert', async () => {
    const access = buildAccess({
      volumeId: 'ext-ro',
      volumes: [{ id: 'ext-ro', name: 'Backup', isReadOnly: true }],
    })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(dialogsStub.showAlert).toHaveBeenCalledWith(
      'Read-only device',
      '"Backup" is read-only. You can copy files from it, but not to it.',
    )
    // The shared guard fires before reading the clipboard or queueing anything.
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

  it('threads the file/folder split when every clipboard kind flag is known', async () => {
    readClipboardFilesSpy.mockResolvedValue({
      paths: ['/x/a.txt', '/x/dir1', '/x/dir2'],
      isCut: false,
      isDirectory: [false, true, true],
    })
    getCommonParentPathSpy.mockReturnValue('/x')
    const access = buildAccess({ volumeId: 'root', path: '/dest' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    expect(dialogsStub.startTransferProgress.mock.calls[0][0]).toMatchObject({
      fileCount: 1,
      folderCount: 2,
    })
  })

  it('omits the split (composer falls back) when any clipboard kind flag is unknown', async () => {
    readClipboardFilesSpy.mockResolvedValue({
      paths: ['/x/a.txt', '/x/mystery'],
      isCut: false,
      isDirectory: [false, null],
    })
    getCommonParentPathSpy.mockReturnValue('/x')
    const access = buildAccess({ volumeId: 'root', path: '/dest' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    const props = dialogsStub.startTransferProgress.mock.calls[0][0]
    expect(props.fileCount).toBeUndefined()
    expect(props.folderCount).toBeUndefined()
  })

  it('omits the split when the clipboard carries no kind flags (legacy shape)', async () => {
    readClipboardFilesSpy.mockResolvedValue({ paths: ['/x/a.txt'], isCut: false })
    getCommonParentPathSpy.mockReturnValue('/x')
    const access = buildAccess({ volumeId: 'root', path: '/dest' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    const props = dialogsStub.startTransferProgress.mock.calls[0][0]
    expect(props.fileCount).toBeUndefined()
    expect(props.folderCount).toBeUndefined()
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

/**
 * PR3 / A6: the MTP clipboard refusal moved from `volumeId.startsWith('mtp-')`
 * to the capability table's `kind === 'mtp'`. Pin that the converted gate is
 * byte-equivalent to the old string compare across every volumeId a focused
 * pane can hold when a clipboard op fires — so no user-visible toast changes.
 *
 * The capability MTP arm (`isMtpVolumeId || category === 'mobile_device'`) is
 * BROADER than `startsWith('mtp-')` (it also catches colon-form ids), but no
 * live clipboard-time pane is colon-form-only: real MTP panes carry
 * `mtp-{location}` / `mtp-{device}:{storage}` ids, both `startsWith('mtp-')`.
 * And `network` / `search-results` (which also lack a system clipboard) must
 * NOT be MTP-refused — `kind === 'mtp'` keeps them out, unlike a raw
 * `!supportsSystemClipboard` read would.
 */
describe('MTP clipboard-refusal equivalence (PR3 / A6)', () => {
  // The live set of volumeIds a focused pane can hold when copy/cut/paste fires.
  const liveClipboardPaneIds = [
    'root', // local main volume
    'attached-1', // attached local volume
    'smb-host-share', // mounted SMB share
    'mtp-1234', // MTP device (location-id form)
    'mtp-1234:0x00010001', // MTP storage (device:storage form)
    'network', // the synthetic network browser (paste reaches this; copy/cut bail earlier)
    'search-results', // the snapshot pane (clipboard blocked upstream by dispatch)
  ]

  it('matches the old startsWith("mtp-") gate on the live pane-id set', () => {
    for (const id of liveClipboardPaneIds) {
      const oldGate = id.startsWith('mtp-')
      const newGate = capabilitiesForReal(id).kind === 'mtp'
      expect(newGate, `gate mismatch for ${id}`).toBe(oldGate)
    }
  })

  it('does not MTP-refuse a network paste (byte-identical: network falls through)', async () => {
    readClipboardFilesSpy.mockResolvedValue({ paths: [], isCut: false })
    const access = buildAccess({ volumeId: 'network', path: 'smb://host' })

    await createClipboardOperations(access, buildDialogs()).pasteFromClipboard(false)

    // No MTP toast; the gate falls through to the real clipboard read, which
    // here finds an empty clipboard — exactly the pre-conversion behavior.
    expect(addToastSpy).not.toHaveBeenCalledWith('Use F5 to copy files to MTP devices', { level: 'info' })
    expect(readClipboardFilesSpy).toHaveBeenCalled()
  })
})
