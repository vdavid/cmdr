import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { FileEntry } from '../types'
import type { SelectionAction } from '../../../routes/(main)/explorer-api'

const { findFileIndexSpy, findFileIndicesSpy, refreshListingSpy, addToastSpy, snapshotSpy, toggleTagsSpy } = vi.hoisted(
  () => ({
    snapshotSpy: vi.fn(),
    toggleTagsSpy: vi.fn(() => Promise.resolve()),
    findFileIndexSpy: vi.fn<() => Promise<number | null>>(),
    findFileIndicesSpy: vi.fn<() => Promise<Record<string, number>>>(),
    addToastSpy: vi.fn(),
    refreshListingSpy: vi.fn<() => Promise<{ data: null; timedOut: boolean }>>(() =>
      Promise.resolve({ data: null, timedOut: false }),
    ),
  }),
)

vi.mock('#lib/tauri-commands/index.js', async () => ({
  asListingLookupError: (
    await vi.importActual<typeof import('#lib/tauri-commands/listing-gone.js')>('#lib/tauri-commands/listing-gone.js')
  ).asListingLookupError,
  getSelectionSnapshot: snapshotSpy,
  toggleTags: toggleTagsSpy,
  findFileIndex: findFileIndexSpy,
  findFileIndices: findFileIndicesSpy,
  refreshListing: refreshListingSpy,
}))

// `capabilitiesFor` (used by `isSnapshotPane`) resolves fsType/category from the
// volume store for real ids; the two virtual ids short-circuit before the lookup.
// Empty store ⇒ a real id ('root', 'mtp-…') falls to the listable `local`/`mtp`
// rows (hasBackendListing: true ⇒ not a snapshot pane).
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [] }))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: addToastSpy }))

vi.mock('#lib/logging/logger.js', () => ({
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
    isJumpActive: boolean
    isInNetworkView: boolean
    volumeId: string
    filenameUnderCursor: string | undefined
    pathUnderCursor: string | undefined
    /** What a fresh re-read finds under the cursor; defaults to the cached name and path. */
    freshEntry: { name: string; path: string } | null
    selectedIndices: number[]
    allSelected: boolean
    rowStateReady: boolean
    lastSequence: number
    entriesSnapshot: FileEntry[]
    entriesCursorIndex: number
    cursorIndex: number
    effectiveTotalCount: number
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
    refreshCursorEntry: () => {
      if ('freshEntry' in overrides) return Promise.resolve(overrides.freshEntry)
      const name = 'filenameUnderCursor' in overrides ? overrides.filenameUnderCursor : 'file.txt'
      if (name === undefined) return Promise.resolve(null)
      return Promise.resolve({ name, path: overrides.pathUnderCursor ?? `/listing/${name}`, isDirectory: false })
    },
    getSelectedIndices: () => overrides.selectedIndices ?? [],
    isAllSelected: () => overrides.allSelected ?? false,
    getCursorIndex: () => overrides.cursorIndex ?? 0,
    getEffectiveTotalCount: () => overrides.effectiveTotalCount ?? 0,
    isRowStateReady: () => overrides.rowStateReady ?? true,
    getLastSequence: () => overrides.lastSequence ?? 0,
    getEntriesSnapshot: () => Promise.resolve(overrides.entriesSnapshot ?? []),
    getEntriesCursorIndex: () => overrides.entriesCursorIndex ?? 0,
    // Action / select spies
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    invertSelection: vi.fn(),
    toggleSelectionAtCursor: vi.fn(),
    toggleSelectionAndMoveDownAtCursor: vi.fn(),
    selectRange: vi.fn(),
    setSelectedIndices: vi.fn(),
    applyIndices: vi.fn(),
    setCursorIndex: vi.fn(() => Promise.resolve()),
    findNetworkItemIndex: vi.fn(() => -1),
    whenLoadSettles: vi.fn(() => Promise.resolve()),
    syncStateToMcpNow: vi.fn(() => Promise.resolve()),
    // Key-route spies
    handleKeyDown: vi.fn(),
    handleJumpKeystroke: vi.fn(),
    isJumpActive: () => overrides.isJumpActive ?? false,
    clearJumpState: vi.fn(),
    isQuickFilterMode: vi.fn(() => false),
    isQuickFilterActive: vi.fn(() => false),
    appendQuickFilter: vi.fn(),
    backspaceQuickFilter: vi.fn(),
    clearQuickFilter: vi.fn(),
    isFilterTypingLive: vi.fn(() => false),
    endFilterTyping: vi.fn(),
    noteFilterKeyRanCommand: vi.fn(),
    // Delegate spies
    toggleVolumeChooser: vi.fn(),
    openVolumeChooser: vi.fn(),
    toggleFavoritesMenu: vi.fn(),
    closeHeaderMenu: vi.fn(),
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
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane,
    getShowHiddenFiles: () => config.showHiddenFiles ?? true,
    getVolumes: () => [],
    focusActivePane: () => {},
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

    cmds.handleSelectionAction({ action: 'clear' })
    cmds.handleSelectionAction({ action: 'deselectAll' })

    expect(ref.clearSelection).toHaveBeenCalledTimes(2)
  })

  it('routes selectAll', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleSelectionAction({ action: 'selectAll' })
    expect(ref.selectAll).toHaveBeenCalledOnce()
  })

  it('routes invert', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleSelectionAction({ action: 'invert' })
    expect(ref.invertSelection).toHaveBeenCalledOnce()
  })

  it('routes toggleAtCursor and toggleAtCursorAndMoveDown', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.handleSelectionAction({ action: 'toggleAtCursor' })
    cmds.handleSelectionAction({ action: 'toggleAtCursorAndMoveDown' })
    expect(ref.toggleSelectionAtCursor).toHaveBeenCalledOnce()
    expect(ref.toggleSelectionAndMoveDownAtCursor).toHaveBeenCalledOnce()
  })

  it('routes selectRange only when both indices are provided', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))

    cmds.handleSelectionAction({ action: 'selectRange', startIndex: 2, endIndex: 5 })
    expect(ref.selectRange).toHaveBeenCalledWith(2, 5)

    vi.mocked(ref.selectRange).mockClear()
    cmds.handleSelectionAction({ action: 'selectRange', startIndex: 2 })
    expect(ref.selectRange).not.toHaveBeenCalled()
  })

  it('no-ops on an unknown action and when no pane is focused', () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    // The action param is the closed `SelectionAction` union now; a bogus value
    // can only arrive via a cast. Pin that the switch has no errant default.
    cmds.handleSelectionAction({ action: 'bogus' as SelectionAction })
    expect(ref.clearSelection).not.toHaveBeenCalled()

    // No pane focused: nothing throws.
    const cmdsNoPane = create(buildAccess({ paneRefs: { left: undefined } }))
    expect(() => {
      cmdsNoPane.handleSelectionAction({ action: 'selectAll' })
    }).not.toThrow()
  })
})

describe('handleMcpSelect modes', () => {
  it('count 0 clears the selection', async () => {
    const ref = buildPaneRef({ selectedIndices: [1, 2] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelect('left', 0, 0, 'replace')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([])
    expect(ref.selectAll).not.toHaveBeenCalled()
  })

  it("'all' selects all", async () => {
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelect('left', 0, 'all', 'replace')
    expect(ref.selectAll).toHaveBeenCalledOnce()
    expect(ref.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('replace mode sets the contiguous range from start', async () => {
    const ref = buildPaneRef({ selectedIndices: [9] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelect('left', 2, 3, 'replace')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([2, 3, 4])
  })

  it('flushes the new selection to the MCP store before returning', async () => {
    // The flush-before-reply contract: a follow-up copy/move/delete reads the
    // backend PaneStateStore, which lags FE state until the debounced sync. select
    // (and now move_cursor) must push synchronously so the file-op pre-check sees
    // the fresh selection instead of rejecting with "Nothing to copy". Pinning it
    // here guards both paths from a future "drop the flush" regression.
    const ref = buildPaneRef({ selectedIndices: [9] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelect('left', 2, 3, 'replace')
    expect(ref.syncStateToMcpNow).toHaveBeenCalled()
  })

  it('add mode unions the range with the current selection', async () => {
    const ref = buildPaneRef({ selectedIndices: [0, 1] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelect('left', 2, 2, 'add')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([0, 1, 2, 3])
  })

  it('subtract mode removes the range from the current selection', async () => {
    const ref = buildPaneRef({ selectedIndices: [0, 1, 2, 3] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelect('left', 1, 2, 'subtract')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([0, 3])
  })

  it('targets the requested pane, not the focused one', async () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ focusedPane: 'left', paneRefs: { left, right } }))
    await cmds.handleMcpSelect('right', 0, 1, 'replace')
    expect(right.setSelectedIndices).toHaveBeenCalledWith([0])
    expect(left.setSelectedIndices).not.toHaveBeenCalled()
  })
})

describe('getFileAndPathUnderCursor', () => {
  it("answers the entry's own path (a snapshot pane's rows carry real paths)", async () => {
    const ref = buildPaneRef({ filenameUnderCursor: 'test.md', pathUnderCursor: '/real/dir/test.md' })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, paths: { left: 'search-results://sr-1' } }))
    expect(await cmds.getFileAndPathUnderCursor()).toEqual({ path: '/real/dir/test.md', filename: 'test.md' })
  })

  // Regression: a quick filter put CLAUDE.md on the row `backups` had held, and
  // F4 acted on the cached `backups`.
  it('acts on a fresh read of the cursor row, never the cached one', async () => {
    const ref = buildPaneRef({
      filenameUnderCursor: 'backups',
      pathUnderCursor: '/Users/x/.claude/backups',
      freshEntry: { name: 'CLAUDE.md', path: '/Users/x/.claude/CLAUDE.md' },
    })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    expect(await cmds.getFileAndPathUnderCursor()).toEqual({
      path: '/Users/x/.claude/CLAUDE.md',
      filename: 'CLAUDE.md',
    })
  })

  it('returns null for the .. parent entry and when nothing is under the cursor', async () => {
    const parentRef = buildPaneRef({ filenameUnderCursor: '..' })
    expect(await create(buildAccess({ paneRefs: { left: parentRef } })).getFileAndPathUnderCursor()).toBeNull()

    const emptyRef = buildPaneRef({ filenameUnderCursor: undefined })
    expect(await create(buildAccess({ paneRefs: { left: emptyRef } })).getFileAndPathUnderCursor()).toBeNull()
  })
})

describe('getPathToCopyUnderCursor', () => {
  it('returns the cursor entry path for a regular row', async () => {
    const ref = buildPaneRef({ filenameUnderCursor: 'doc.txt', pathUnderCursor: '/Users/x/dir/doc.txt' })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, paths: { left: '/Users/x/dir' } }))
    expect(await cmds.getPathToCopyUnderCursor()).toBe('/Users/x/dir/doc.txt')
  })

  it('copies a fresh read of the cursor row, never the cached one', async () => {
    const ref = buildPaneRef({
      filenameUnderCursor: 'backups',
      pathUnderCursor: '/Users/x/backups',
      freshEntry: { name: 'notes.md', path: '/Users/x/notes.md' },
    })
    expect(await create(buildAccess({ paneRefs: { left: ref } })).getPathToCopyUnderCursor()).toBe('/Users/x/notes.md')
  })

  it("returns the pane's own directory when the cursor sits on ..", async () => {
    const ref = buildPaneRef({ filenameUnderCursor: '..' })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, paths: { left: '/Users/x/Downloads' } }))
    expect(await cmds.getPathToCopyUnderCursor()).toBe('/Users/x/Downloads')
  })

  it('reads the FOCUSED pane, not always the left one', async () => {
    const cmds = create(
      buildAccess({
        focusedPane: 'right',
        paneRefs: { left: buildPaneRef(), right: buildPaneRef({ filenameUnderCursor: '..' }) },
        paths: { right: '/Volumes/naspi/papers' },
      }),
    )
    expect(await cmds.getPathToCopyUnderCursor()).toBe('/Volumes/naspi/papers')
  })

  it('returns null when no row is under the cursor', async () => {
    const ref = buildPaneRef({ filenameUnderCursor: undefined })
    expect(await create(buildAccess({ paneRefs: { left: ref } })).getPathToCopyUnderCursor()).toBeNull()
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

  it('while a jump is ACTIVE, a punctuation key extends the buffer instead of forwarding', () => {
    // `-` is not a base jump char, but once the buffer has content it must extend
    // the jump (jump to `my-file`) rather than open the deselect dialog.
    const ref = buildPaneRef({ isJumpActive: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.routePanelKey(payload({ key: '-', code: 'Minus' }))
    expect(ref.handleJumpKeystroke).toHaveBeenCalledWith('-')
    expect(ref.handleKeyDown).not.toHaveBeenCalled()
  })

  it('while jump is INACTIVE, a punctuation key falls through to handleKeyDown', () => {
    const ref = buildPaneRef({ isJumpActive: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    cmds.routePanelKey(payload({ key: '-', code: 'Minus' }))
    expect(ref.handleJumpKeystroke).not.toHaveBeenCalled()
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

describe('resolveMultiRenameTarget', () => {
  const NO_FILE_TOAST = [
    'No file selected',
    expect.objectContaining({ level: 'error', timeoutMs: 5000, report: false }),
  ] as const

  beforeEach(() => {
    addToastSpy.mockClear()
  })

  it('hands over the selection as backend rows in row order, with the sequence they belong to', () => {
    const ref = buildPaneRef({ hasParent: true, selectedIndices: [5, 0, 2], effectiveTotalCount: 10, lastSequence: 9 })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    // Row 0 is `..`: it never renames, and the rest shift down by one.
    expect(cmds.resolveMultiRenameTarget()).toEqual({
      listingId: 'listing-1',
      selectedIndices: [1, 4],
      expectedSequence: 9,
    })
  })

  // #724: like F5, F6, F8, and ⇧F6, an unselected press acts on one file. It once took the
  // whole folder, and a press meant for one file renamed thousands.
  it.each([
    { hasParent: true, cursorIndex: 3, row: 2 },
    { hasParent: false, cursorIndex: 3, row: 3 },
    { hasParent: false, cursorIndex: 0, row: 0 },
  ])('with nothing selected, takes the file under the cursor only: %o', ({ hasParent, cursorIndex, row }) => {
    const ref = buildPaneRef({ hasParent, cursorIndex, effectiveTotalCount: 10, lastSequence: 3 })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    expect(cmds.resolveMultiRenameTarget()).toEqual({
      listingId: 'listing-1',
      selectedIndices: [row],
      expectedSequence: 3,
    })
    expect(addToastSpy).not.toHaveBeenCalled()
  })

  it('takes every row when everything is selected, without shipping the indices', () => {
    const all = buildPaneRef({ allSelected: true, selectedIndices: [0, 1, 2], effectiveTotalCount: 3 })
    const cmds = create(buildAccess({ paneRefs: { left: all } }))
    expect(cmds.resolveMultiRenameTarget()).toMatchObject({ selectedIndices: null })
  })

  it('opens nothing and says so on the .. row, which is all an empty subfolder has', () => {
    const ref = buildPaneRef({ hasParent: true, cursorIndex: 0, effectiveTotalCount: 1 })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    expect(cmds.resolveMultiRenameTarget()).toBeNull()
    expect(addToastSpy).toHaveBeenCalledExactlyOnceWith(...NO_FILE_TOAST)
  })

  it('opens nothing and says so in an empty folder with no .. row', () => {
    const ref = buildPaneRef({ hasParent: false, cursorIndex: 0, effectiveTotalCount: 0 })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    expect(cmds.resolveMultiRenameTarget()).toBeNull()
    expect(addToastSpy).toHaveBeenCalledExactlyOnceWith(...NO_FILE_TOAST)
  })

  it.each([
    ['a selection', { selectedIndices: [1] }],
    ['the cursor row', { cursorIndex: 1 }],
  ])('opens nothing while %s is still settling, and says why', (_label, rows) => {
    const settling = buildPaneRef({ ...rows, effectiveTotalCount: 5, rowStateReady: false })
    const cmds = create(buildAccess({ paneRefs: { left: settling } }))
    expect(cmds.resolveMultiRenameTarget()).toBeNull()
    expect(addToastSpy).toHaveBeenCalledExactlyOnceWith(expect.any(String), { level: 'info' })
  })

  it('opens nothing, silently, on a pane with no real listing', () => {
    const search = buildPaneRef({ volumeId: 'search-results' })
    expect(create(buildAccess({ paneRefs: { left: search } })).resolveMultiRenameTarget()).toBeNull()
    expect(addToastSpy).not.toHaveBeenCalled()
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
    const found = await cmds.moveCursorByNameInFileListing(refOf(ref), 'target')
    expect(ref.setCursorIndex).toHaveBeenCalledWith(5)
    expect(found).toBe(true)
  })

  it('uses the backend index unchanged when the pane has no parent row', async () => {
    findFileIndexSpy.mockResolvedValue(4)
    const ref = buildPaneRef({ hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    const found = await cmds.moveCursorByNameInFileListing(refOf(ref), 'target')
    expect(ref.setCursorIndex).toHaveBeenCalledWith(4)
    expect(found).toBe(true)
  })

  it('reports not-found when the backend reports no match', async () => {
    // Pre-fix the caller had no way to tell, so MCP move_cursor returned a
    // false-positive OK for nonexistent filenames.
    findFileIndexSpy.mockResolvedValue(null)
    const ref = buildPaneRef({ hasParent: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    const found = await cmds.moveCursorByNameInFileListing(refOf(ref), 'missing')
    expect(ref.setCursorIndex).not.toHaveBeenCalled()
    expect(found).toBe(false)
  })

  it('passes showHiddenFiles through to findFileIndex', async () => {
    findFileIndexSpy.mockResolvedValue(0)
    const ref = buildPaneRef({ hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref }, showHiddenFiles: false }))
    await cmds.moveCursorByNameInFileListing(refOf(ref), 'target')
    expect(findFileIndexSpy).toHaveBeenCalledWith('listing-1', 'target', false)
  })

  /** A pane whose listing moves on the moment the lookup is sent, the way a fast `..` does. */
  function paneThatMovesOnWhileAsked() {
    let listingId = 'listing-1'
    const ref = { ...buildPaneRef({ hasParent: false }), getListingId: () => listingId }
    const moveOn = () => {
      listingId = 'listing-2'
    }
    return { ref, moveOn }
  }

  it('answers not-found, quietly, when the listing it asked about was dropped meanwhile', async () => {
    // The backend drops a listing the pane walks away from, so the lookup comes back
    // refused. A caller that fires and forgets (the tab-switch cursor restore) once
    // turned that refusal into an unhandled rejection.
    const { ref, moveOn } = paneThatMovesOnWhileAsked()
    findFileIndexSpy.mockImplementation(() => {
      moveOn()
      return Promise.reject(new Error('the listing is gone'))
    })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await expect(cmds.moveCursorByNameInFileListing(refOf(ref), 'target')).resolves.toBe(false)
    expect(ref.setCursorIndex).not.toHaveBeenCalled()
  })

  it('leaves the cursor alone when the pane moved to another listing while the lookup ran', async () => {
    // The index belongs to the listing that was asked, not the one on screen now.
    const { ref, moveOn } = paneThatMovesOnWhileAsked()
    findFileIndexSpy.mockImplementation(() => {
      moveOn()
      return Promise.resolve(4)
    })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await expect(cmds.moveCursorByNameInFileListing(refOf(ref), 'target')).resolves.toBe(false)
    expect(ref.setCursorIndex).not.toHaveBeenCalled()
  })

  it('still reports a lookup that fails while its listing is the one on screen', async () => {
    findFileIndexSpy.mockRejectedValue(new Error('no answer'))
    const ref = buildPaneRef({ hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await expect(cmds.moveCursorByNameInFileListing(refOf(ref), 'target')).rejects.toThrow('no answer')
  })
})

describe('delegating commands', () => {
  it('confirmDialog forwards dialogType, onConflict, and the background option to the dialog state', () => {
    const cmds = create(buildAccess())
    void cmds.confirmDialog('transfer-confirmation', 'overwrite', { startInBackground: true })
    expect(dialogsStub.confirmOpenDialog).toHaveBeenCalledWith('transfer-confirmation', 'overwrite', {
      startInBackground: true,
    })
  })

  it('toggleVolumeChooser closes the other pane and toggles the target', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left, right } }))
    cmds.toggleVolumeChooser('left')
    expect(right.closeHeaderMenu).toHaveBeenCalledOnce()
    expect(left.toggleVolumeChooser).toHaveBeenCalledOnce()
  })

  it('openVolumeChooser opens the focused pane after closing the other', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ focusedPane: 'left', paneRefs: { left, right } }))
    cmds.openVolumeChooser()
    expect(right.closeHeaderMenu).toHaveBeenCalledOnce()
    expect(left.openVolumeChooser).toHaveBeenCalledOnce()
  })

  // The one-menu-at-a-time rule crosses panes as well as menus: ⌃D over one pane must
  // leave nothing open over the other.
  it('toggleFavoritesMenu toggles the focused pane after closing the other', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ focusedPane: 'left', paneRefs: { left, right } }))
    cmds.toggleFavoritesMenu()
    expect(right.closeHeaderMenu).toHaveBeenCalledOnce()
    expect(left.toggleFavoritesMenu).toHaveBeenCalledOnce()
  })

  it('closeHeaderMenus closes both panes', () => {
    const left = buildPaneRef()
    const right = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left, right } }))
    cmds.closeHeaderMenus()
    expect(left.closeHeaderMenu).toHaveBeenCalledOnce()
    expect(right.closeHeaderMenu).toHaveBeenCalledOnce()
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

  it('refreshPane forces a backend re-read, then re-renders', async () => {
    // Pre-fix this only bumped the render counter — a stale cache stayed stale.
    // The `true` is the honesty half: on a volume whose watcher claims to see
    // every writer (SMB), an unforced refresh answers out of the cache.
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.refreshPane()
    expect(refreshListingSpy).toHaveBeenCalledExactlyOnceWith('listing-1', true)
    expect(ref.refreshView).toHaveBeenCalledOnce()
  })

  it('refreshPane re-scans hosts when the pane shows the network browser', async () => {
    // ⌘R means "refresh what I'm looking at", and the network browser has no
    // listing to re-read.
    const ref = buildPaneRef({ isInNetworkView: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.refreshPane()
    expect(ref.refreshNetworkHosts).toHaveBeenCalledOnce()
    expect(refreshListingSpy).not.toHaveBeenCalled()
  })

  it('tells the user and the agent when the re-read outlives its wait', async () => {
    // The read is still running, so the agent must not get an OK — and the person
    // who pressed ⌘R must not be left wondering whether anything happened.
    refreshListingSpy.mockResolvedValueOnce({ data: null, timedOut: true })
    const ref = buildPaneRef()
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await expect(cmds.refreshPane()).rejects.toThrow('still running')
    expect(addToastSpy).toHaveBeenCalledOnce()
    expect(ref.refreshView).not.toHaveBeenCalled()
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
    const found = await cmds.moveCursorByName(refOf(ref), 'file')
    expect(findFileIndexSpy).toHaveBeenCalledOnce()
    expect(ref.setCursorIndex).toHaveBeenCalledWith(2)
    expect(found).toBe(true)
  })

  it('reports not-found for a missing network item without moving the cursor', async () => {
    const ref = buildPaneRef({ isInNetworkView: true })
    vi.mocked(ref.findNetworkItemIndex).mockReturnValue(-1)
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    const found = await cmds.moveCursorByName(refOf(ref), 'no-such-host')
    expect(ref.setCursorIndex).not.toHaveBeenCalled()
    expect(found).toBe(false)
  })
})

describe('handleMcpSelectNames', () => {
  it('maps names to frontend indices (parent offset) and replaces the selection', async () => {
    findFileIndicesSpy.mockResolvedValue({ 'a.txt': 0, 'b.txt': 4 })
    const ref = buildPaneRef({ hasParent: true })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelectNames('left', ['a.txt', 'b.txt'], 'replace')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([1, 5])
  })

  it('throws naming the missing files when any name is not in the listing', async () => {
    findFileIndicesSpy.mockResolvedValue({ 'a.txt': 0 })
    const ref = buildPaneRef({ hasParent: false })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await expect(cmds.handleMcpSelectNames('left', ['a.txt', 'nope.txt', 'gone.md'], 'replace')).rejects.toThrow(
      'Not found in the left pane: nope.txt, gone.md',
    )
    expect(ref.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('merges with the current selection in add mode and removes in subtract mode', async () => {
    findFileIndicesSpy.mockResolvedValue({ 'a.txt': 2 })
    const ref = buildPaneRef({ hasParent: false, selectedIndices: [7] })
    const cmds = create(buildAccess({ paneRefs: { left: ref } }))
    await cmds.handleMcpSelectNames('left', ['a.txt'], 'add')
    expect(ref.setSelectedIndices).toHaveBeenCalledWith([7, 2])

    const ref2 = buildPaneRef({ hasParent: false, selectedIndices: [2, 7] })
    const cmds2 = create(buildAccess({ paneRefs: { left: ref2 } }))
    await cmds2.handleMcpSelectNames('left', ['a.txt'], 'subtract')
    expect(ref2.setSelectedIndices).toHaveBeenCalledWith([7])
  })

  it('throws when the pane is unavailable', async () => {
    const cmds = create(buildAccess({ paneRefs: { left: undefined } }))
    await expect(cmds.handleMcpSelectNames('left', ['a.txt'], 'replace')).rejects.toThrow(
      'The left pane is unavailable',
    )
  })
})

/** A watcher insert above the selection, landing between the pane applying its rows and a tag toggle. */
describe('toggleTagOnFocusedSelection when the rows moved under the press', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  async function movedAccess(catchesUp: boolean) {
    const moved = await import('./moved-listing.test-fixtures')
    const pane = moved.createMovedPane({ onSelection: true })
    snapshotSpy.mockImplementation(
      catchesUp ? moved.catchingUpAfterRefusal(pane, moved.selectionSnapshot) : moved.selectionSnapshot,
    )
    const ref = { ...buildPaneRef({ listingId: moved.MOVED_LISTING_ID }), ...pane }
    return { access: buildAccess({ paneRefs: { left: ref } }), moved }
  }

  it('waits for the pane to apply the diff and tags the file it picked', async () => {
    const { access, moved } = await movedAccess(true)

    await create(access).toggleTagOnFocusedSelection(2)

    expect(toggleTagsSpy).toHaveBeenCalledWith(moved.MOVED_LISTING_ID, [moved.SELECTED_PATH], 2)
  })

  it('refuses with the toast once the rows never settle, tagging nothing', async () => {
    vi.useFakeTimers()
    toggleTagsSpy.mockClear()
    addToastSpy.mockClear()
    const { access } = await movedAccess(false)

    const toggled = create(access).toggleTagOnFocusedSelection(2)
    await vi.advanceTimersByTimeAsync(3_500)
    await toggled

    expect(toggleTagsSpy).not.toHaveBeenCalled()
    expect(addToastSpy).toHaveBeenCalledWith(
      'The list here is still updating, so nothing started. Try again in a moment.',
      { level: 'info' },
    )
  })
})
