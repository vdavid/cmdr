/**
 * Tests for `pane-key-router.ts`, the file pane's keydown routing. They pin the
 * order and the bails that used to need a mounted pane to exercise:
 * - an active rename swallows everything (the inline editor owns the keyboard),
 * - the network and search-results views take over before any file-list handling,
 * - Enter / ⌘↓ opens the entry under the cursor, ⌘↓ over nothing is swallowed
 *   rather than falling through to a cursor move or the document dispatcher,
 * - Backspace / ⌘↑ goes to the parent, but only when there is a `..` row,
 * - the ⌘-variants stop propagation so the document dispatcher can't run the
 *   same command a second time (⌘↑ → grandparent, ⌘↓ → double-open),
 * - bare `+` / `-` bubble the Selection dialog commands out of the pane,
 * - the five selection keys act and stop propagation, and Space also raises the
 *   one-time Quick Look hint,
 * - the leftovers reach the Brief or Full cursor handler per the view mode,
 * - key-up on Shift ends the mouse range-anchor gesture.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import type { FileEntry } from '../types'

const { shortcuts, quickLook, renameActivation } = vi.hoisted<{
  shortcuts: { eventMatchesCommand: Mock }
  quickLook: { maybeShowQuickLookHint: Mock }
  renameActivation: { cancelClickToRename: Mock }
}>(() => ({
  shortcuts: { eventMatchesCommand: vi.fn() },
  quickLook: { maybeShowQuickLookHint: vi.fn() },
  renameActivation: { cancelClickToRename: vi.fn() },
}))

vi.mock('$lib/shortcuts', () => ({ eventMatchesCommand: shortcuts.eventMatchesCommand }))
vi.mock('../quick-look/quick-look-hint', () => ({ maybeShowQuickLookHint: quickLook.maybeShowQuickLookHint }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: renameActivation.cancelClickToRename }))

import { createPaneKeyRouter, type PaneKeyRouterDeps } from './pane-key-router'

const entry: FileEntry = {
  name: 'a.txt',
  path: '/dir/a.txt',
  isDirectory: false,
  isSymlink: false,
  permissions: 0o644,
  owner: 'user',
  group: 'staff',
  iconId: 'file',
  extendedMetadataLoaded: true,
}

/**
 * A keyboard event whose `preventDefault` / `stopPropagation` we can observe.
 *
 * The return type `Omit`s those two off `KeyboardEvent` before intersecting the spies in:
 * leaving the real DOM signatures there makes `expect(e.preventDefault)` read as an
 * unbound method to `@typescript-eslint/unbound-method`.
 */
function keyEvent(init: Partial<KeyboardEvent> = {}) {
  return {
    key: 'x',
    metaKey: false,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    ...init,
  } as unknown as Omit<KeyboardEvent, 'preventDefault' | 'stopPropagation'> & {
    preventDefault: Mock
    stopPropagation: Mock
  }
}

/** Route only the named command to `true`, everything else to `false`. */
function onlyCommand(commandId: string) {
  shortcuts.eventMatchesCommand.mockImplementation((_e: KeyboardEvent, id: string) => id === commandId)
}

describe('createPaneKeyRouter', () => {
  let deps: {
    [K in keyof PaneKeyRouterDeps]: PaneKeyRouterDeps[K] extends (...args: never[]) => unknown
      ? Mock
      : PaneKeyRouterDeps[K]
  }
  let state: {
    renameActive: boolean
    isNetworkView: boolean
    isSearchResultsView: boolean
    viewMode: 'brief' | 'full'
    hasParent: boolean
    cursorEntry: FileEntry | undefined
  }

  function router() {
    return createPaneKeyRouter(deps)
  }

  beforeEach(() => {
    vi.clearAllMocks()
    shortcuts.eventMatchesCommand.mockReturnValue(false)
    state = {
      renameActive: false,
      isNetworkView: false,
      isSearchResultsView: false,
      viewMode: 'full',
      hasParent: true,
      cursorEntry: entry,
    }
    deps = {
      getRenameActive: vi.fn(() => state.renameActive),
      getIsNetworkView: vi.fn(() => state.isNetworkView),
      getIsSearchResultsView: vi.fn(() => state.isSearchResultsView),
      getViewMode: vi.fn(() => state.viewMode),
      getHasParent: vi.fn(() => state.hasParent),
      getEntryUnderCursor: vi.fn(() => state.cursorEntry),
      handleNetworkKeyDown: vi.fn(),
      handleSearchResultsKeyDown: vi.fn(),
      handleBriefModeKeys: vi.fn(),
      handleFullModeKeys: vi.fn(),
      openEntry: vi.fn(),
      navigateToParent: vi.fn(),
      onCommand: vi.fn(),
      toggleSelectionAtCursor: vi.fn(),
      toggleSelectionAndMoveDown: vi.fn(),
      selectAll: vi.fn(),
      deselectAll: vi.fn(),
      invertSelection: vi.fn(),
      clearRangeState: vi.fn(),
    }
  })

  describe('the bails at the top', () => {
    it('swallows everything while a rename is active', () => {
      state.renameActive = true
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(renameActivation.cancelClickToRename).not.toHaveBeenCalled()
      expect(deps.handleFullModeKeys).not.toHaveBeenCalled()
    })

    it('cancels a pending click-to-rename on any other keystroke', () => {
      router().handleKeyDown(keyEvent())
      expect(renameActivation.cancelClickToRename).toHaveBeenCalledTimes(1)
    })

    it('hands the whole event to the network view', () => {
      state.isNetworkView = true
      onlyCommand('nav.open')
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.handleNetworkKeyDown).toHaveBeenCalledWith(e)
      expect(deps.openEntry).not.toHaveBeenCalled()
    })

    it('hands the whole event to the search-results view', () => {
      state.isSearchResultsView = true
      onlyCommand('nav.open')
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.handleSearchResultsKeyDown).toHaveBeenCalledWith(e)
      expect(deps.openEntry).not.toHaveBeenCalled()
    })
  })

  describe('open and parent', () => {
    it('opens the entry under the cursor on Enter', () => {
      onlyCommand('nav.open')
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.openEntry).toHaveBeenCalledWith(entry)
      expect(e.preventDefault).toHaveBeenCalled()
      // Bare Enter isn't in the document dispatch map, so nothing to stop.
      expect(e.stopPropagation).not.toHaveBeenCalled()
    })

    it('stops propagation for ⌘↓ so the document dispatcher cannot double-open', () => {
      onlyCommand('nav.open')
      const e = keyEvent({ metaKey: true })
      router().handleKeyDown(e)
      expect(deps.openEntry).toHaveBeenCalledWith(entry)
      expect(e.stopPropagation).toHaveBeenCalled()
    })

    it('swallows ⌘↓ with nothing under the cursor', () => {
      state.cursorEntry = undefined
      onlyCommand('nav.open')
      const e = keyEvent({ metaKey: true })
      router().handleKeyDown(e)
      expect(deps.openEntry).not.toHaveBeenCalled()
      expect(e.preventDefault).toHaveBeenCalled()
      expect(e.stopPropagation).toHaveBeenCalled()
      expect(deps.handleFullModeKeys).not.toHaveBeenCalled()
    })

    it('lets bare Enter with nothing under the cursor fall through to the cursor handler', () => {
      state.cursorEntry = undefined
      onlyCommand('nav.open')
      router().handleKeyDown(keyEvent())
      expect(deps.handleFullModeKeys).toHaveBeenCalled()
    })

    it('goes to the parent on Backspace / ⌘↑', () => {
      onlyCommand('nav.parent')
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.navigateToParent).toHaveBeenCalledTimes(1)
      expect(e.preventDefault).toHaveBeenCalled()
      expect(e.stopPropagation).toHaveBeenCalled()
    })

    it('falls through at a volume root, where there is no `..` row', () => {
      state.hasParent = false
      onlyCommand('nav.parent')
      router().handleKeyDown(keyEvent())
      expect(deps.navigateToParent).not.toHaveBeenCalled()
      expect(deps.handleFullModeKeys).toHaveBeenCalled()
    })
  })

  describe('the Selection dialog keys', () => {
    it('bubbles `selection.selectFiles` for a bare `+`', () => {
      const e = keyEvent({ key: '+' })
      router().handleKeyDown(e)
      expect(deps.onCommand).toHaveBeenCalledWith('selection.selectFiles')
      expect(e.preventDefault).toHaveBeenCalled()
      expect(e.stopPropagation).toHaveBeenCalled()
      expect(deps.handleFullModeKeys).not.toHaveBeenCalled()
    })

    it('bubbles `selection.deselectFiles` for a bare `-`', () => {
      router().handleKeyDown(keyEvent({ key: '-' }))
      expect(deps.onCommand).toHaveBeenCalledWith('selection.deselectFiles')
    })

    it('ignores `+` carrying a modifier', () => {
      router().handleKeyDown(keyEvent({ key: '+', metaKey: true }))
      expect(deps.onCommand).not.toHaveBeenCalled()
      expect(deps.handleFullModeKeys).toHaveBeenCalled()
    })
  })

  describe('the selection keys', () => {
    it('toggles at the cursor and raises the one-time Quick Look hint on Space', () => {
      onlyCommand('selection.toggle')
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.toggleSelectionAtCursor).toHaveBeenCalledTimes(1)
      expect(quickLook.maybeShowQuickLookHint).toHaveBeenCalledTimes(1)
      expect(e.stopPropagation).toHaveBeenCalled()
      expect(deps.handleFullModeKeys).not.toHaveBeenCalled()
    })

    it('toggles and moves down on Insert', () => {
      onlyCommand('selection.toggleAndDown')
      router().handleKeyDown(keyEvent())
      expect(deps.toggleSelectionAndMoveDown).toHaveBeenCalledTimes(1)
      expect(quickLook.maybeShowQuickLookHint).not.toHaveBeenCalled()
    })

    it('selects all on ⌘A', () => {
      onlyCommand('selection.selectAll')
      router().handleKeyDown(keyEvent())
      expect(deps.selectAll).toHaveBeenCalledTimes(1)
    })

    it('deselects all on ⌘⇧A', () => {
      onlyCommand('selection.deselectAll')
      router().handleKeyDown(keyEvent())
      expect(deps.deselectAll).toHaveBeenCalledTimes(1)
    })

    it('inverts the selection on ⇧8', () => {
      onlyCommand('selection.invert')
      router().handleKeyDown(keyEvent())
      expect(deps.invertSelection).toHaveBeenCalledTimes(1)
    })
  })

  describe('the view-mode split', () => {
    it('routes leftovers to the Full cursor handler', () => {
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.handleFullModeKeys).toHaveBeenCalledWith(e)
      expect(deps.handleBriefModeKeys).not.toHaveBeenCalled()
    })

    it('routes leftovers to the Brief cursor handler in Brief mode', () => {
      state.viewMode = 'brief'
      const e = keyEvent()
      router().handleKeyDown(e)
      expect(deps.handleBriefModeKeys).toHaveBeenCalledWith(e)
      expect(deps.handleFullModeKeys).not.toHaveBeenCalled()
    })
  })

  describe('key up', () => {
    it('ends the mouse range-anchor gesture when Shift is released', () => {
      router().handleKeyUp(keyEvent({ key: 'Shift' }))
      expect(deps.clearRangeState).toHaveBeenCalledTimes(1)
    })

    it('ignores every other key', () => {
      router().handleKeyUp(keyEvent({ key: 'a' }))
      expect(deps.clearRangeState).not.toHaveBeenCalled()
    })
  })
})
