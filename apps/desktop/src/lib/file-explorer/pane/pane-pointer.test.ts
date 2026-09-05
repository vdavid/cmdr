/**
 * Tests for `pane-pointer.ts`, what the mouse does to a file pane. They pin:
 * - plain click moves the cursor, focuses the pane, and ends any range gesture,
 * - Shift extends the range and Cmd toggles, with Shift winning when both are
 *   held (Finder parity),
 * - a right-click inside the current selection acts on the whole selection, and
 *   outside it acts on the one entry,
 * - the `..` row gets its own one-item menu, and none at all on a snapshot pane,
 * - opening any context menu cancels an in-flight type-to-jump,
 * - a click inside the inline rename editor does NOT steal focus (that would
 *   blur the input and end the rename mid-edit),
 * - the background double-click goes up one folder, only when the setting is on,
 *   only on real background, only when there is a parent, and raises its
 *   one-time explainer toast exactly once.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import type { FileEntry } from '../types'

const { ipc, settings, toast, background } = vi.hoisted<{
  ipc: { showFileContextMenu: Mock; showParentRowContextMenu: Mock; getPathsAtIndices: Mock }
  settings: { getSetting: Mock; setSetting: Mock }
  toast: { addToast: Mock }
  background: { isFileListBackgroundClick: Mock }
}>(() => ({
  ipc: { showFileContextMenu: vi.fn(), showParentRowContextMenu: vi.fn(), getPathsAtIndices: vi.fn() },
  settings: { getSetting: vi.fn(), setSetting: vi.fn() },
  toast: { addToast: vi.fn() },
  background: { isFileListBackgroundClick: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({
  showFileContextMenu: ipc.showFileContextMenu,
  showParentRowContextMenu: ipc.showParentRowContextMenu,
  getPathsAtIndices: ipc.getPathsAtIndices,
}))
vi.mock('$lib/settings', () => ({ getSetting: settings.getSetting, setSetting: settings.setSetting }))
vi.mock('$lib/ui/toast', () => ({ addToast: toast.addToast }))
vi.mock('./pane-background-dblclick', () => ({ isFileListBackgroundClick: background.isFileListBackgroundClick }))
vi.mock('./DoubleClickPaneHintToastContent.svelte', () => ({ default: {} }))

import { createPanePointer, type PanePointerDeps } from './pane-pointer'

function entryOf(over: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'a.txt',
    path: '/dir/a.txt',
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
    ...over,
  }
}

describe('createPanePointer', () => {
  let deps: PanePointerDeps
  let calls: Record<string, Mock>
  let state: { cursorIndex: number; hasParent: boolean; listingId: string; volumeId: string; selected: number[] }

  beforeEach(() => {
    vi.clearAllMocks()
    ipc.showFileContextMenu.mockResolvedValue(undefined)
    ipc.showParentRowContextMenu.mockResolvedValue(undefined)
    ipc.getPathsAtIndices.mockResolvedValue([])
    background.isFileListBackgroundClick.mockReturnValue(true)
    settings.getSetting.mockReturnValue(true)
    state = { cursorIndex: 2, hasParent: true, listingId: 'listing-1', volumeId: 'root', selected: [] }
    calls = {
      setCursorIndex: vi.fn((i: number) => {
        state.cursorIndex = i
      }),
      onRequestFocus: vi.fn(),
      fetchCursorEntry: vi.fn(),
      extendSelectionFromMouse: vi.fn(),
      toggleSelectionAt: vi.fn(),
      clearRangeState: vi.fn(),
      clearJump: vi.fn(),
      navigateToParent: vi.fn(),
    }
    deps = {
      getCursorIndex: () => state.cursorIndex,
      setCursorIndex: calls.setCursorIndex,
      getHasParent: () => state.hasParent,
      getListingId: () => state.listingId,
      getIncludeHidden: () => true,
      getVolumeId: () => state.volumeId,
      getSelectedIndices: () => state.selected,
      onRequestFocus: calls.onRequestFocus,
      fetchCursorEntry: calls.fetchCursorEntry,
      extendSelectionFromMouse: calls.extendSelectionFromMouse,
      toggleSelectionAt: calls.toggleSelectionAt,
      clearRangeState: calls.clearRangeState,
      clearJump: calls.clearJump,
      navigateToParent: calls.navigateToParent,
    }
  })

  describe('clicking a row', () => {
    it('moves the cursor, focuses the pane, and refreshes the cursor entry', () => {
      createPanePointer(deps).handleSelect({ index: 5 })
      expect(calls.setCursorIndex).toHaveBeenCalledWith(5)
      expect(calls.onRequestFocus).toHaveBeenCalledTimes(1)
      expect(calls.fetchCursorEntry).toHaveBeenCalledTimes(1)
    })

    it('ends the range gesture on a plain click', () => {
      createPanePointer(deps).handleSelect({ index: 5 })
      expect(calls.clearRangeState).toHaveBeenCalledTimes(1)
      expect(calls.extendSelectionFromMouse).not.toHaveBeenCalled()
    })

    it('extends the range on Shift+click, from the cursor', () => {
      createPanePointer(deps).handleSelect({ index: 5, shiftKey: true })
      expect(calls.extendSelectionFromMouse).toHaveBeenCalledWith({ index: 5, cursorIndex: 2, hasParent: true })
      expect(calls.toggleSelectionAt).not.toHaveBeenCalled()
    })

    it('toggles on Cmd+click and drops the anchor', () => {
      createPanePointer(deps).handleSelect({ index: 5, shiftKey: false, metaKey: true })
      expect(calls.toggleSelectionAt).toHaveBeenCalledWith(5, true)
      expect(calls.clearRangeState).toHaveBeenCalledTimes(1)
    })

    it('lets Shift win when both modifiers are held (Finder parity)', () => {
      createPanePointer(deps).handleSelect({ index: 5, shiftKey: true, metaKey: true })
      expect(calls.extendSelectionFromMouse).toHaveBeenCalledTimes(1)
      expect(calls.toggleSelectionAt).not.toHaveBeenCalled()
    })
  })

  describe('the context menu', () => {
    it('acts on the whole selection when the right-clicked row is part of it', async () => {
      state.selected = [1, 2]
      ipc.getPathsAtIndices.mockResolvedValue(['/dir/a.txt', '/dir/b.txt'])
      await createPanePointer(deps).handleContextMenu(entryOf())
      expect(ipc.showFileContextMenu).toHaveBeenCalledWith(
        '/dir/a.txt',
        'a.txt',
        false,
        ['/dir/a.txt', '/dir/b.txt'],
        { listingId: 'listing-1', canOpenTerminalHere: true },
      )
    })

    it('acts on the one entry when the right-clicked row is outside the selection', async () => {
      state.selected = [7]
      ipc.getPathsAtIndices.mockResolvedValue(['/dir/other.txt'])
      await createPanePointer(deps).handleContextMenu(entryOf())
      expect(ipc.showFileContextMenu).toHaveBeenCalledWith(
        '/dir/a.txt',
        'a.txt',
        false,
        ['/dir/a.txt'],
        { listingId: 'listing-1', canOpenTerminalHere: true },
      )
    })

    it('falls back to the single entry when the selection lookup throws', async () => {
      state.selected = [1]
      ipc.getPathsAtIndices.mockRejectedValue(new Error('gone'))
      await createPanePointer(deps).handleContextMenu(entryOf())
      expect(ipc.showFileContextMenu).toHaveBeenCalledWith(
        '/dir/a.txt',
        'a.txt',
        false,
        ['/dir/a.txt'],
        { listingId: 'listing-1', canOpenTerminalHere: true },
      )
    })

    it('greys out "Open terminal here" on a pane whose volume has no OS-visible paths', async () => {
      // The item acts on the pane's folder, so a phone offers nothing to open.
      state.volumeId = 'mtp-1'
      await createPanePointer(deps).handleContextMenu(entryOf())
      expect(ipc.showFileContextMenu).toHaveBeenCalledWith(
        '/dir/a.txt',
        'a.txt',
        false,
        ['/dir/a.txt'],
        { listingId: 'listing-1', canOpenTerminalHere: false },
      )
    })

    it('gives the `..` row its own one-item menu', async () => {
      await createPanePointer(deps).handleContextMenu(entryOf({ name: '..', path: '/dir', isDirectory: true }))
      expect(ipc.showParentRowContextMenu).toHaveBeenCalledWith('/dir')
      expect(ipc.showFileContextMenu).not.toHaveBeenCalled()
    })

    it('shows no `..` menu on a snapshot pane, which has no real parent to favorite', async () => {
      state.volumeId = 'search-results'
      await createPanePointer(deps).handleContextMenu(entryOf({ name: '..', path: '/dir', isDirectory: true }))
      expect(ipc.showParentRowContextMenu).not.toHaveBeenCalled()
    })

    it('cancels an in-flight type-to-jump', async () => {
      await createPanePointer(deps).handleContextMenu(entryOf())
      expect(calls.clearJump).toHaveBeenCalledTimes(1)
    })
  })

  describe('clicking the pane', () => {
    it('focuses the pane', () => {
      createPanePointer(deps).handlePaneClick({ target: null } as unknown as MouseEvent)
      expect(calls.onRequestFocus).toHaveBeenCalledTimes(1)
    })

    it('leaves focus alone inside the inline rename editor', () => {
      const target = { closest: (sel: string) => (sel === '.rename-input' ? {} : null) }
      Object.setPrototypeOf(target, Element.prototype)
      createPanePointer(deps).handlePaneClick({ target } as unknown as MouseEvent)
      expect(calls.onRequestFocus).not.toHaveBeenCalled()
    })
  })

  describe('double-clicking the pane background', () => {
    function dblClick() {
      return { target: null } as unknown as MouseEvent
    }

    it('goes up one folder and explains itself the first time', () => {
      settings.getSetting.mockImplementation((id: string) => id === 'behavior.doubleClickPaneNavigatesToParent')
      createPanePointer(deps).handlePaneBackgroundDblClick(dblClick())
      expect(calls.navigateToParent).toHaveBeenCalledTimes(1)
      expect(settings.setSetting).toHaveBeenCalledWith('behavior.doubleClickOnPaneNotificationSeen', true)
      expect(toast.addToast).toHaveBeenCalledTimes(1)
    })

    it('stays quiet on later double-clicks', () => {
      settings.getSetting.mockReturnValue(true)
      createPanePointer(deps).handlePaneBackgroundDblClick(dblClick())
      expect(calls.navigateToParent).toHaveBeenCalledTimes(1)
      expect(toast.addToast).not.toHaveBeenCalled()
    })

    it('does nothing when the setting is off', () => {
      settings.getSetting.mockReturnValue(false)
      createPanePointer(deps).handlePaneBackgroundDblClick(dblClick())
      expect(calls.navigateToParent).not.toHaveBeenCalled()
    })

    it('ignores a double-click that landed on a row', () => {
      settings.getSetting.mockImplementation((id: string) => id === 'behavior.doubleClickPaneNavigatesToParent')
      background.isFileListBackgroundClick.mockReturnValue(false)
      createPanePointer(deps).handlePaneBackgroundDblClick(dblClick())
      expect(calls.navigateToParent).not.toHaveBeenCalled()
    })

    it('does nothing at a volume root, where there is nothing above', () => {
      settings.getSetting.mockImplementation((id: string) => id === 'behavior.doubleClickPaneNavigatesToParent')
      state.hasParent = false
      createPanePointer(deps).handlePaneBackgroundDblClick(dblClick())
      expect(calls.navigateToParent).not.toHaveBeenCalled()
      expect(toast.addToast).not.toHaveBeenCalled()
    })
  })
})
