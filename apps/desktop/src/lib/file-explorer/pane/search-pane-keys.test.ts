/**
 * Tests for `search-pane-keys.ts`, the search-results pane's keyboard side-effect
 * wiring. The pure dispatcher (`computeSearchPaneKeyAction`) is tested separately;
 * here it's mocked so each action kind can be driven and its effect asserted:
 * open-cursor, view/edit file (skipping directories), toggle at cursor, toggle-and-
 * advance (with clamp), and move-cursor (with/without shift-extend). Also pins the
 * null-action no-op and the preventDefault/stopPropagation on a handled key.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'

const { computeSpy, openFileViewerSpy, openInEditorSpy } = vi.hoisted(() => ({
  computeSpy: vi.fn(),
  openFileViewerSpy: vi.fn(),
  openInEditorSpy: vi.fn(),
}))

vi.mock('./search-results-keys', () => ({ computeSearchPaneKeyAction: computeSpy }))
vi.mock('$lib/file-viewer/open-viewer', () => ({ openFileViewer: openFileViewerSpy }))
vi.mock('$lib/tauri-commands', () => ({ openInEditor: openInEditorSpy }))

import { createSearchPaneKeys, type SearchPaneKeysDeps } from './search-pane-keys'

function setup(over: Partial<SearchPaneKeysDeps> = {}) {
  const spies = {
    setCursorIndex: vi.fn(),
    extendSelection: vi.fn(),
    toggleSelectionAt: vi.fn(),
    openCursorItem: vi.fn(),
    getSnapshotEntryAt: vi.fn(() => ({ path: '/f.txt', isDirectory: false })) as Mock,
  }
  const deps: SearchPaneKeysDeps = {
    getCursorIndex: () => 3,
    getSearchResultsCount: () => 10,
    getVisibleItemsCount: () => 20,
    getSnapshotEntryAt: spies.getSnapshotEntryAt,
    setCursorIndex: spies.setCursorIndex,
    extendSelection: spies.extendSelection,
    toggleSelectionAt: spies.toggleSelectionAt,
    openCursorItem: spies.openCursorItem,
    ...over,
  }
  return { keys: createSearchPaneKeys(deps), spies }
}

function fakeEvent() {
  const preventDefault = vi.fn()
  const stopPropagation = vi.fn()
  return { e: { preventDefault, stopPropagation } as unknown as KeyboardEvent, preventDefault, stopPropagation }
}

describe('createSearchPaneKeys', () => {
  beforeEach(() => vi.clearAllMocks())

  it('does nothing on a null action (key not handled)', () => {
    computeSpy.mockReturnValue(null)
    const { keys, spies } = setup()
    const { e, preventDefault } = fakeEvent()
    keys.handleSearchResultsKeyDown(e)
    expect(preventDefault).not.toHaveBeenCalled()
    expect(spies.setCursorIndex).not.toHaveBeenCalled()
  })

  it('prevents default + stops propagation on a handled key', () => {
    computeSpy.mockReturnValue({ kind: 'noop' })
    const { keys } = setup()
    const { e, preventDefault, stopPropagation } = fakeEvent()
    keys.handleSearchResultsKeyDown(e)
    expect(preventDefault).toHaveBeenCalled()
    expect(stopPropagation).toHaveBeenCalled()
  })

  it('open-cursor opens the entry under the cursor', () => {
    computeSpy.mockReturnValue({ kind: 'open-cursor' })
    const { keys, spies } = setup()
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(spies.openCursorItem).toHaveBeenCalled()
  })

  it('view-file opens the viewer for a file, skips a directory', () => {
    computeSpy.mockReturnValue({ kind: 'view-file' })
    const { keys } = setup()
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(openFileViewerSpy).toHaveBeenCalledWith('/f.txt')

    openFileViewerSpy.mockClear()
    const { keys: keys2 } = setup({ getSnapshotEntryAt: () => ({ path: '/dir', isDirectory: true }) })
    keys2.handleSearchResultsKeyDown(fakeEvent().e)
    expect(openFileViewerSpy).not.toHaveBeenCalled()
  })

  it('edit-file opens the editor for a file', () => {
    computeSpy.mockReturnValue({ kind: 'edit-file' })
    const { keys } = setup()
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(openInEditorSpy).toHaveBeenCalledWith('/f.txt')
  })

  it('toggle-selection-at-cursor toggles when there are rows, no-ops when empty', () => {
    computeSpy.mockReturnValue({ kind: 'toggle-selection-at-cursor' })
    const { keys, spies } = setup()
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(spies.toggleSelectionAt).toHaveBeenCalledWith(3)

    spies.toggleSelectionAt.mockClear()
    const { keys: keys2, spies: spies2 } = setup({ getSearchResultsCount: () => 0 })
    keys2.handleSearchResultsKeyDown(fakeEvent().e)
    expect(spies2.toggleSelectionAt).not.toHaveBeenCalled()
  })

  it('toggle-selection-and-advance toggles then advances the cursor (clamped)', () => {
    computeSpy.mockReturnValue({ kind: 'toggle-selection-and-advance' })
    const { keys, spies } = setup({ getCursorIndex: () => 9, getSearchResultsCount: () => 10 })
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(spies.toggleSelectionAt).toHaveBeenCalledWith(9)
    expect(spies.setCursorIndex).toHaveBeenCalledWith(9) // clamped at last row
  })

  it('move-cursor with shift extends the selection then moves', () => {
    computeSpy.mockReturnValue({ kind: 'move-cursor', index: 7, overflow: false, shiftKey: true })
    const { keys, spies } = setup({ getCursorIndex: () => 3 })
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(spies.extendSelection).toHaveBeenCalledWith(3, 7, false)
    expect(spies.setCursorIndex).toHaveBeenCalledWith(7)
  })

  it('move-cursor without shift only moves the cursor', () => {
    computeSpy.mockReturnValue({ kind: 'move-cursor', index: 7, overflow: false, shiftKey: false })
    const { keys, spies } = setup()
    keys.handleSearchResultsKeyDown(fakeEvent().e)
    expect(spies.extendSelection).not.toHaveBeenCalled()
    expect(spies.setCursorIndex).toHaveBeenCalledWith(7)
  })
})
