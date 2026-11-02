/**
 * Focus nobody at the keyboard asked for. Someone mid-sentence in the rail would otherwise type into a
 * pane, where Enter opens a file and F8 trashes one, both as themselves. So `switch_pane`, `swap_panes`,
 * and `move_cursor` move the panes and leave DOM focus where the person has it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { registerFocusHome } from '#lib/ui/focus-restore.js'
import explorerSource from './DualPaneExplorer.svelte?raw'
import { followFocusedPane, type ActivePaneFocusDeps } from './active-pane-focus'
import { moveCursorToTarget } from './move-cursor'
import { createSwapPanes } from './swap-panes'
import type { FilePaneAPI } from './types'

let explorer: HTMLDivElement
let list: HTMLDivElement
let rail: HTMLTextAreaElement
let unregisterHome: () => void

const focusDeps: ActivePaneFocusDeps = {
  getPaneRef: () =>
    ({
      focusList: () => {
        list.focus()
        return true
      },
    }) as unknown as FilePaneAPI,
  getFocusedPane: () => 'left',
  getContainerElement: () => explorer,
}

beforeEach(() => {
  explorer = document.createElement('div')
  explorer.tabIndex = -1
  list = document.createElement('div')
  list.tabIndex = -1
  explorer.append(list)
  rail = document.createElement('textarea')
  document.body.append(explorer, rail)
  unregisterHome = registerFocusHome({
    root: () => explorer,
    refocus: () => {
      list.focus()
    },
  })
})

afterEach(() => {
  unregisterHome()
  explorer.remove()
  rail.remove()
})

describe('followFocusedPane', () => {
  it('leaves the rail alone', () => {
    rail.focus()
    followFocusedPane(focusDeps)
    expect(document.activeElement).toBe(rail)
  })

  it('leaves an open dialog alone', () => {
    const dialog = document.createElement('div')
    dialog.tabIndex = -1
    document.body.append(dialog)
    dialog.focus()
    followFocusedPane(focusDeps)
    expect(document.activeElement).toBe(dialog)
    dialog.remove()
  })

  it('leaves a field inside the explorer alone: the rename editor would blur, and cancel', () => {
    const field = document.createElement('input')
    explorer.append(field)
    field.focus()
    followFocusedPane(focusDeps)
    expect(document.activeElement).toBe(field)
  })

  it('moves focus while it is the explorer that has it', () => {
    const otherList = document.createElement('div')
    otherList.tabIndex = -1
    explorer.append(otherList)
    otherList.focus()
    followFocusedPane(focusDeps)
    expect(document.activeElement).toBe(list)
  })

  it('moves focus when nothing holds it', () => {
    ;(document.activeElement as HTMLElement | null)?.blur()
    followFocusedPane(focusDeps)
    expect(document.activeElement).toBe(list)
  })
})

describe('the tools that move the panes', () => {
  const follow = () => {
    followFocusedPane(focusDeps)
  }

  it('`move_cursor` leaves the rail alone, and still moves the store and the cursor', async () => {
    rail.focus()
    const setFocusedPane = vi.fn()
    const setCursorIndex = vi.fn(() => Promise.resolve())
    const pane = {
      whenLoadSettles: () => Promise.resolve(),
      isInNetworkView: () => false,
      getEffectiveTotalCount: () => 3,
      setCursorIndex,
      syncStateToMcpNow: () => Promise.resolve(),
    } as unknown as FilePaneAPI

    await moveCursorToTarget('right', 1, {
      getPaneRef: () => pane,
      setFocusedPane,
      moveCursorByName: () => Promise.resolve(true),
      followFocusedPane: follow,
    })

    expect(setFocusedPane).toHaveBeenCalledExactlyOnceWith('right')
    expect(setCursorIndex).toHaveBeenCalledExactlyOnceWith(1)
    expect(document.activeElement).toBe(rail)
  })

  it('`swap_panes` leaves the rail alone', () => {
    rail.focus()
    const pane = {
      isLoading: () => false,
      getSwapState: () => ({}),
      adoptListing: () => {},
    } as unknown as FilePaneAPI
    const tabs = { tabs: [{ id: 't', pinned: false }], activeTabId: 't' }
    const swap = createSwapPanes({
      getPaneRef: () => pane,
      getLeftTabMgr: () => tabs as never,
      getRightTabMgr: () => tabs as never,
      isAnyTransferDialogOpen: () => false,
      followFocusedPane: follow,
    })

    swap.swapPanes()

    expect(document.activeElement).toBe(rail)
  })

  it('`switch_pane` goes through the same rule', () => {
    // The explorer is too much to mount here, and `pane.switch` has no other seam: pinned on source.
    const body = explorerSource.slice(explorerSource.indexOf('export function switchPane()'))
    const end = body.indexOf('\n    }\n')
    expect(body.slice(0, end)).toContain('followFocusedPane()')
    expect(body.slice(0, end)).not.toContain('focusActivePane()')
  })
})
