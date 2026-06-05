import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { FileEntry } from '../types'
import type { SelectionAction } from '../../../routes/(main)/explorer-api'

const { findFileIndexSpy } = vi.hoisted(() => ({
  findFileIndexSpy: vi.fn<() => Promise<number | null>>(),
}))

vi.mock('$lib/tauri-commands', () => ({ findFileIndex: findFileIndexSpy }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() }),
}))

import { createPaneCommands } from './pane-commands'

/**
 * Builds a `FilePaneAPI` stub. Every method is a spy so the action-routing and
 * select-mode tests can assert exactly which pane method fired. Reads override
 * the defaults via the config bag.
 */
function buildPaneRef(
  overrides: Partial<{
    listingId: string
    hasParent: boolean
    isRenaming: boolean
    isInNetworkView: boolean
    volumeId: string
    filenameUnderCursor: string | undefined
    pathUnderCursor: string | undefined
    selectedIndices: number[]
    entriesSnapshot: FileEntry[]
    entriesCursorIndex: number
  }> = {},
) {
  const stub = {
    getListingId: () => overrides.listingId ?? 'listing-1',
    hasParentEntry: () => overrides.hasParent ?? false,
    isRenaming: () => overrides.isRenaming ?? false,
    isInNetworkView: () => overrides.isInNetworkView ?? false,
    getVolumeId: () => overrides.volumeId ?? 'root',
    getFilenameUnderCursor: () => ('filenameUnderCursor' in overrides ? overrides.filenameUnderCursor : 'file.txt'),
    getPathUnderCursor: () => overrides.pathUnderCursor,
    getSelectedIndices: () => overrides.selectedIndices ?? [],
    getEntriesSnapshot: () => Promise.resolve(overrides.entriesSnapshot ?? []),
    getEntriesCursorIndex: () => overrides.entriesCursorIndex ?? 0,
    // Action / select spies
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    toggleSelectionAtCursor: vi.fn(),
    toggleSelectionAndMoveDownAtCursor: vi.fn(),
    selectRange: vi.fn(),
    setSelectedIndices: vi.fn(),
    applyIndices: vi.fn(),
    setCursorIndex: vi.fn(() => Promise.resolve()),
    findNetworkItemIndex: vi.fn(() => -1),
    // Key-route spies
    handleKeyDown: vi.fn(),
    handleJumpKeystroke: vi.fn(),
    clearJumpState: vi.fn(),
    // Delegate spies
    toggleVolumeChooser: vi.fn(),
    openVolumeChooser: vi.fn(),
    closeVolumeChooser: vi.fn(),
    openCursorItem: vi.fn(() => Promise.resolve()),
    refreshView: vi.fn(),
    refreshNetworkHosts: vi.fn(),
    injectError: vi.fn(),
    navigateToPath: vi.fn(() => Promise.resolve()),
  }
  return stub
}

/**
 * Object-literal stub type (function-valued spies, not interface methods) so
 * `expect(ref.clearSelection)` doesn't trip `@typescript-eslint/unbound-method`.
 * Cast to `FilePaneAPI` only at the `PaneAccess` boundary.
 */
type PaneRefStub = ReturnType<typeof buildPaneRef>

const asPaneRef = (stub: PaneRefStub | undefined): FilePaneAPI | undefined => stub as unknown as FilePaneAPI

/** Non-undefined cast for the `moveCursorByName*` call sites that always pass a real stub. */
const refOf = (stub: PaneRefStub): FilePaneAPI => stub as unknown as FilePaneAPI

interface AccessConfig {
  focusedPane?: 'left' | 'right'
  paneRefs?: Partial<Record<'left' | 'right', PaneRefStub | undefined>>
  volumeIds?: Partial<Record<'left' | 'right', string>>
  paths?: Partial<Record<'left' | 'right', string>>
  showHiddenFiles?: boolean
}

function buildAccess(config: AccessConfig = {}): PaneAccess {
  const otherPane = (pane: 'left' | 'right'): 'left' | 'right' => (pane === 'left' ? 'right' : 'left')
  const defaultRef = buildPaneRef()
  return {
    getPaneRef: (pane) => asPaneRef(config.paneRefs && pane in config.paneRefs ? config.paneRefs[pane] : defaultRef),
    getPanePath: (pane) => config.paths?.[pane] ?? (pane === 'left' ? '/left/dir' : '/right/dir'),
    getPaneVolumeId: (pane) => config.volumeIds?.[pane] ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane,
    getShowHiddenFiles: () => config.showHiddenFiles ?? true,
    getVolumes: () => [],
    focusContainer: () => {},
  }
}

const dialogsStub = {
  confirmOpenDialog: vi.fn(),
  handleTransferError: vi.fn(),
}

function create(access: PaneAccess) {
  return createPaneCommands(access, dialogsStub as unknown as Parameters<typeof createPaneCommands>[1])
}

function fileEntry(overrides: Partial<FileEntry> = {}): FileEntry {
  return { name: 'file.txt', path: '/dir/file.txt', isDirectory: false, ...overrides } as unknown as FileEntry
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('handleSelectionAction routing', () => {
  it('routes clear / deselectAll to clearSelection', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))

    cmds.handleSelectionAction('clear')
    cmds.handleSelectionAction('deselectAll')

    expect(ref.clearSelection).toHaveBeenCalledTimes(2)
  })

  it('routes selectAll', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleSelectionAction('selectAll')
    expect(ref.selectAll).toHaveBeenCalledOnce()
  })

  it('routes toggleAtCursor and toggleAtCursorAndMoveDown', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleSelectionAction('toggleAtCursor')
    cmds.handleSelectionAction('toggleAtCursorAndMoveDown')
    expect(ref.toggleSelectionAtCursor).toHaveBeenCalledOnce()
    expect(ref.toggleSelectionAndMoveDownAtCursor).toHaveBeenCalledOnce()
  })

  it('routes selectRange only when both indices are provided', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))

    cmds.handleSelectionAction('selectRange', 2, 5)
    expect(ref.selectRange).toHaveBeenCalledWith(2, 5)

    vi.mocked(ref.selectRange).mockClear()
    cmds.handleSelectionAction('selectRange', 2)
    expect(ref.selectRange).not.toHaveBeenCalled()
  })

  it('no-ops on an unknown action and when no pane is focused', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    // The action param is the closed `SelectionAction` union now; a bogus value
    // can only arrive via a cast. Pin that the switch has no errant default.
    cmds.handleSelectionAction('bogus' as SelectionAction)
    expect(ref.clearSelection).not.toHaveBeenCalled()

    // No pane focused: nothing throws.
    const cmdsNoPane = create(buildAccess({ paneRefs: { left: undefined } }))
    expect(() => {
      cmdsNoPane.handleSelectionAction('selectAll')
    }).not.toThrow()
  })
})

describe('handleMcpSelect modes', () => {
  it('count 0 clears the selection', () => {
    const ref = buildPaneRef({ selectedIndices: [1, 2] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleMcpSelect('left', 0, 0, 'replace')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([])
    expect(ref.selectAll).not.toHaveBeenCalled()
  })

  it("'all' selects all", () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleMcpSelect('left', 0, 'all', 'replace')
    expect(ref.selectAll).toHaveBeenCalledOnce()
    expect(ref.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('replace mode sets the contiguous range from start', () => {
    const ref = buildPaneRef({ selectedIndices: [9] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleMcpSelect('left', 2, 3, 'replace')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([2, 3, 4])
  })

  it('add mode unions the range with the current selection', () => {
    const ref = buildPaneRef({ selectedIndices: [0, 1] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleMcpSelect('left', 2, 2, 'add')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([0, 1, 2, 3])
  })

  it('subtract mode removes the range from the current selection', () => {
    const ref = buildPaneRef({ selectedIndices: [0, 1, 2, 3] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleMcpSelect('left', 1, 2, 'subtract')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([0, 3])
  })

  it('targets the requested pane, not the focused one', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ focusedPane: 'left', paneRefs: { left, right } }))
    cmds.handleMcpSelect('right', 0, 1, 'replace')
    expect(right.setSelectedIndices).toHaveBeenCalledWith([0])
    expect(left.setSelectedIndices).not.toHaveBeenCalled()
  })
})

describe('validateMtpNavigation', () => {
  const cmds = create(buildAccess())

  it('passes (null) for a local path on a local volume', () => {
    expect(cmds.validateMtpNavigation('/Users/x/dir', 'root', 'Macintosh HD')).toBeNull()
  })

  it('passes (null) for an mtp:// path that matches the pane volume', () => {
    expect(cmds.validateMtpNavigation('mtp://dev1/65537/DCIM', 'dev1:65537', 'Phone')).toBeNull()
  })

  it('rejects an mtp:// path whose device/storage does not match the pane volume', () => {
    const result = cmds.validateMtpNavigation('mtp://dev1/65537/DCIM', 'dev1:99999', 'Phone')
    expect(result).toBe('Pane is not on this MTP volume — call select_volume first.')
  })

  it('rejects an mtp:// path with no parseable device/storage', () => {
    const result = cmds.validateMtpNavigation('mtp://garbage', 'dev1:65537', 'Phone')
    expect(result).toBe('Pane is not on this MTP volume — call select_volume first.')
  })

  it('rejects a local path while the pane sits on an MTP volume', () => {
    const result = cmds.validateMtpNavigation('/Users/x/dir', 'mtp-dev1:65537', 'Phone')
    expect(result).toBe('Pane is on the Phone MTP volume. Use select_volume to switch to a local volume first.')
  })
})

describe('getFileAndPathUnderCursor path preference', () => {
  it('prefers the pane-reported path under cursor (snapshot pane)', () => {
    const ref = buildPaneRef({
      filenameUnderCursor: 'test.md',
      pathUnderCursor: '/real/dir/test.md',
    })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, paths: { left: 'search-results://sr-1' } }))
    expect(cmds.getFileAndPathUnderCursor()).toEqual({ path: '/real/dir/test.md', filename: 'test.md' })
  })

  it('falls back to ${currentPath}/${filename} when no pane path is reported', () => {
    const ref = buildPaneRef({ filenameUnderCursor: 'doc.txt', pathUnderCursor: undefined })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, paths: { left: '/Users/x/dir' } }))
    expect(cmds.getFileAndPathUnderCursor()).toEqual({ path: '/Users/x/dir/doc.txt', filename: 'doc.txt' })
  })

  it('returns null for the .. parent entry and when nothing is under the cursor', () => {
    const parentRef = buildPaneRef({ filenameUnderCursor: '..' })
    expect(create(buildAccess({ paneRefs: { left: parentRef } })).getFileAndPathUnderCursor()).toBeNull()

    const emptyRef = buildPaneRef({ filenameUnderCursor: undefined })
    expect(create(buildAccess({ paneRefs: { left: emptyRef } })).getFileAndPathUnderCursor()).toBeNull()
  })
})

describe('routePanelKey type-to-jump intercept mirroring', () => {
  function payload(over: Partial<Parameters<ReturnType<typeof create>['routePanelKey']>[0]> = {}) {
    return { key: 'a', code: 'KeyA', shiftKey: false, metaKey: false, altKey: false, ctrlKey: false, ...over }
  }

  it('routes a printable char to handleJumpKeystroke and does NOT forward to handleKeyDown', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.routePanelKey(payload({ key: 'a' }))
    expect(ref.handleJumpKeystroke).toHaveBeenCalledWith('a')
    expect(ref.handleKeyDown).not.toHaveBeenCalled()
  })

  it('clears the jump buffer on a reset key and falls through to handleKeyDown', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.routePanelKey(payload({ key: 'ArrowDown', code: 'ArrowDown' }))
    expect(ref.clearJumpState).toHaveBeenCalledOnce()
    expect(ref.handleKeyDown).toHaveBeenCalledOnce()
    expect(ref.handleJumpKeystroke).not.toHaveBeenCalled()
  })

  it('forwards a non-jump key (Enter) straight to handleKeyDown after clearing', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.routePanelKey(payload({ key: 'Enter', code: 'Enter' }))
    expect(ref.clearJumpState).toHaveBeenCalledOnce()
    expect(ref.handleKeyDown).toHaveBeenCalledOnce()
  })

  it('skips the type-to-jump intercept while renaming', () => {
    const ref = buildPaneRef({ isRenaming: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.routePanelKey(payload({ key: 'a' }))
    expect(ref.handleJumpKeystroke).not.toHaveBeenCalled()
    expect(ref.handleKeyDown).toHaveBeenCalledOnce()
  })
})

describe('getFocusedPaneEntries snapshot shape', () => {
  it('returns entries + cursorIndex and flags search-results panes', async () => {
    const entries = [fileEntry({ name: 'a' }), fileEntry({ name: 'b' })]
    const ref = buildPaneRef({ entriesSnapshot: entries, entriesCursorIndex: 1, volumeId: 'search-results' })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await expect(cmds.getFocusedPaneEntries()).resolves.toEqual({
      entries,
      cursorIndex: 1,
      isSnapshotPane: true,
    })
  })

  it('reports isSnapshotPane false for a regular pane', async () => {
    const ref = buildPaneRef({ volumeId: 'root', entriesCursorIndex: 0 })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    const result = await cmds.getFocusedPaneEntries()
    expect(result.isSnapshotPane).toBe(false)
  })

  it('returns the empty default when no pane is focused', async () => {
    const cmds = create(buildAccess({ paneRefs: { left: undefined } }))
    await expect(cmds.getFocusedPaneEntries()).resolves.toEqual({
      entries: [],
      cursorIndex: 0,
      isSnapshotPane: false,
    })
  })
})

describe('moveCursorByNameInFileListing parent offset', () => {
  it('adds +1 to the backend index when the pane has a .. parent row', async () => {
    findFileIndexSpy.mockResolvedValue(4)
    const ref = buildPaneRef({ hasParent: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.moveCursorByNameInFileListing(refOf(ref), 'target')
    expect(ref.setCursorIndex).toHaveBeenCalledWith(5)
  })

  it('uses the backend index unchanged when the pane has no parent row', async () => {
    findFileIndexSpy.mockResolvedValue(4)
    const ref = buildPaneRef({ hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.moveCursorByNameInFileListing(refOf(ref), 'target')
    expect(ref.setCursorIndex).toHaveBeenCalledWith(4)
  })

  it('does nothing when the backend reports no match', async () => {
    findFileIndexSpy.mockResolvedValue(null)
    const ref = buildPaneRef({ hasParent: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.moveCursorByNameInFileListing(refOf(ref), 'missing')
    expect(ref.setCursorIndex).not.toHaveBeenCalled()
  })

  it('passes showHiddenFiles through to findFileIndex', async () => {
    findFileIndexSpy.mockResolvedValue(0)
    const ref = buildPaneRef({ hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, showHiddenFiles: false }))
    await cmds.moveCursorByNameInFileListing(refOf(ref), 'target')
    expect(findFileIndexSpy).toHaveBeenCalledWith('listing-1', 'target', false)
  })
})

describe('delegating commands', () => {
  it('confirmDialog forwards dialogType + onConflict to the dialog state', () => {
    const cmds = create(buildAccess())
    cmds.confirmDialog('transfer-confirmation', 'overwrite')
    expect(dialogsStub.confirmOpenDialog).toHaveBeenCalledWith('transfer-confirmation', 'overwrite')
  })

  it('triggerTransferError builds a synthetic error carrying the friendly title', () => {
    const cmds = create(buildAccess())
    const friendly = { title: 'Boom' } as Parameters<ReturnType<typeof create>['triggerTransferError']>[0]
    cmds.triggerTransferError(friendly)
    expect(dialogsStub.handleTransferError).toHaveBeenCalledWith(
      { type: 'io_error', path: '/debug/preview', message: 'Boom' },
      friendly,
    )
  })

  it('toggleVolumeChooser closes the other pane and toggles the target', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left, right } }))
    cmds.toggleVolumeChooser('left')
    expect(right.closeVolumeChooser).toHaveBeenCalledOnce()
    expect(left.toggleVolumeChooser).toHaveBeenCalledOnce()
  })

  it('openVolumeChooser opens the focused pane after closing the other', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ focusedPane: 'left', paneRefs: { left, right } }))
    cmds.openVolumeChooser()
    expect(right.closeVolumeChooser).toHaveBeenCalledOnce()
    expect(left.openVolumeChooser).toHaveBeenCalledOnce()
  })

  it('closeVolumeChooser closes both panes', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left, right } }))
    cmds.closeVolumeChooser()
    expect(left.closeVolumeChooser).toHaveBeenCalledOnce()
    expect(right.closeVolumeChooser).toHaveBeenCalledOnce()
  })

  it('sendKeyToFocusedPane synthesises a keydown for the focused pane', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.sendKeyToFocusedPane('Enter')
    expect(ref.handleKeyDown).toHaveBeenCalledOnce()
    const event = ref.handleKeyDown.mock.calls[0]?.[0] as KeyboardEvent | undefined
    expect(event?.key).toBe('Enter')
  })

  it('openItemUnderCursor awaits the pane and throws without a focused pane', async () => {
    const ref = buildPaneRef()
    await create(buildAccess({ paneRefs: { left: ref } })).openItemUnderCursor()
    expect(ref.openCursorItem).toHaveBeenCalledOnce()

    await expect(create(buildAccess({ paneRefs: { left: undefined } })).openItemUnderCursor()).rejects.toThrow(
      'Focused pane is not available',
    )
  })

  it('getFocusedPane reads the focused pane', () => {
    const cmds = create(buildAccess({ focusedPane: 'right' }))
    expect(cmds.getFocusedPane()).toBe('right')
  })

  it('applyIndicesToFocusedPane forwards indices + mode', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.applyIndicesToFocusedPane([1, 3], 'remove')
    expect(ref.applyIndices).toHaveBeenCalledWith([1, 3], 'remove')
  })

  it('scrollTo sets the cursor index on the requested pane', () => {
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { right } }))
    cmds.scrollTo('right', 42)
    expect(right.setCursorIndex).toHaveBeenCalledWith(42)
  })

  it('refreshPane refreshes the focused pane view', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.refreshPane()
    expect(ref.refreshView).toHaveBeenCalledOnce()
  })

  it('refreshNetworkHosts refreshes the focused pane', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.refreshNetworkHosts()
    expect(ref.refreshNetworkHosts).toHaveBeenCalledOnce()
  })

  it('injectError injects into the named pane', () => {
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { right } }))
    const friendly = { title: 'oops' } as Parameters<ReturnType<typeof create>['injectError']>[1]
    cmds.injectError('right', friendly)
    expect(right.injectError).toHaveBeenCalledWith(friendly)
  })

  it("resetError re-navigates both panes for 'both', one pane otherwise", () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left, right }, paths: { left: '/l', right: '/r' } }))

    cmds.resetError('both')
    expect(left.navigateToPath).toHaveBeenCalledWith('/l')
    expect(right.navigateToPath).toHaveBeenCalledWith('/r')

    vi.mocked(left.navigateToPath).mockClear()
    vi.mocked(right.navigateToPath).mockClear()
    cmds.resetError('left')
    expect(left.navigateToPath).toHaveBeenCalledOnce()
    expect(right.navigateToPath).not.toHaveBeenCalled()
  })
})

describe('moveCursorByName network-vs-listing dispatch', () => {
  it('uses the network item index in a network view', async () => {
    const ref = buildPaneRef({ isInNetworkView: true })
    vi.mocked(ref.findNetworkItemIndex).mockReturnValue(3)
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.moveCursorByName(refOf(ref), 'host-a')
    expect(ref.findNetworkItemIndex).toHaveBeenCalledWith('host-a')
    expect(ref.setCursorIndex).toHaveBeenCalledWith(3)
    expect(findFileIndexSpy).not.toHaveBeenCalled()
  })

  it('falls to the file-listing path when not in a network view', async () => {
    findFileIndexSpy.mockResolvedValue(2)
    const ref = buildPaneRef({ isInNetworkView: false, hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.moveCursorByName(refOf(ref), 'file')
    expect(findFileIndexSpy).toHaveBeenCalledOnce()
    expect(ref.setCursorIndex).toHaveBeenCalledWith(2)
  })
})
