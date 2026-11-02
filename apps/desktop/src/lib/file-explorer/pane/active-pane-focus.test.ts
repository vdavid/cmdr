/**
 * Focus nobody at the keyboard asked for. Someone mid-sentence in the rail would otherwise type into a
 * pane, where Enter opens a file and F8 trashes one, both as themselves. So `switch_pane`, `swap_panes`,
 * and `move_cursor` move the panes and leave DOM focus where the person has it. A person's own command
 * runs the same handlers and always takes focus.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { registerFocusHome } from '#lib/ui/focus-restore.js'
import {
  followFocusedPane,
  settleFocusAfterPaneMove,
  type ActivePaneFocusDeps,
  type PaneMoveBy,
} from './active-pane-focus'
import { moveCursorToTarget } from './move-cursor'
import { createSwapPanes } from './swap-panes'
import { paneHandlers } from '../../../routes/(main)/command-handlers/pane-handlers'
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

describe('who moved the panes', () => {
  const settle = (by: PaneMoveBy) => {
    settleFocusAfterPaneMove(by, focusDeps)
  }

  function moveCursorDeps() {
    const setCursorIndex = vi.fn(() => Promise.resolve())
    const pane = {
      whenLoadSettles: () => Promise.resolve(),
      isInNetworkView: () => false,
      getEffectiveTotalCount: () => 3,
      setCursorIndex,
      syncStateToMcpNow: () => Promise.resolve(),
    } as unknown as FilePaneAPI
    const deps = {
      getPaneRef: () => pane,
      setFocusedPane: vi.fn(),
      moveCursorByName: () => Promise.resolve(true),
      settleFocus: settle,
    }
    return { deps, setCursorIndex }
  }

  function swapper() {
    const pane = {
      isLoading: () => false,
      getSwapState: () => ({}),
      adoptListing: () => {},
    } as unknown as FilePaneAPI
    const tabs = { tabs: [{ id: 't', pinned: false }], activeTabId: 't' }
    return createSwapPanes({
      getPaneRef: () => pane,
      getLeftTabMgr: () => tabs as never,
      getRightTabMgr: () => tabs as never,
      isAnyTransferDialogOpen: () => false,
      settleFocus: settle,
    })
  }

  it("a tool's `move_cursor` leaves the rail alone, and still moves the store and the cursor", async () => {
    rail.focus()
    const { deps, setCursorIndex } = moveCursorDeps()
    await moveCursorToTarget('right', 1, 'tool', deps)
    expect(deps.setFocusedPane).toHaveBeenCalledExactlyOnceWith('right')
    expect(setCursorIndex).toHaveBeenCalledExactlyOnceWith(1)
    expect(document.activeElement).toBe(rail)
  })

  it("a person's reveal takes focus from the rail", async () => {
    rail.focus()
    await moveCursorToTarget('right', 1, 'person', moveCursorDeps().deps)
    expect(document.activeElement).toBe(list)
  })

  it("a tool's `swap_panes` leaves the rail alone", () => {
    rail.focus()
    swapper().swapPanes('tool')
    expect(document.activeElement).toBe(rail)
  })

  it("a person's Swap panes takes focus from the rail", () => {
    rail.focus()
    swapper().swapPanes('person')
    expect(document.activeElement).toBe(list)
  })

  it("a tool's `switch_pane` leaves the rail alone", () => {
    rail.focus()
    settle('tool')
    expect(document.activeElement).toBe(rail)
  })

  it("a person's Switch pane takes focus from the rail", () => {
    rail.focus()
    settle('person')
    expect(document.activeElement).toBe(list)
  })
})

describe('what a command handler reads as who moved the panes', () => {
  function run(id: 'pane.switch' | 'pane.swap', dispatchArgs: unknown) {
    const explorerRef = { switchPane: vi.fn(), swapPanes: vi.fn() }
    paneHandlers[id]({ explorerRef, dispatchArgs } as never)
    return explorerRef
  }

  it('an absent payload is a person: the menu, the palette, and the keyboard send none', () => {
    expect(run('pane.switch', undefined).switchPane).toHaveBeenCalledExactlyOnceWith('person')
    expect(run('pane.swap', undefined).swapPanes).toHaveBeenCalledExactlyOnceWith('person')
  })

  it('a tool names itself', () => {
    expect(run('pane.switch', { by: 'tool' }).switchPane).toHaveBeenCalledExactlyOnceWith('tool')
    expect(run('pane.swap', { by: 'tool' }).swapPanes).toHaveBeenCalledExactlyOnceWith('tool')
  })
})
