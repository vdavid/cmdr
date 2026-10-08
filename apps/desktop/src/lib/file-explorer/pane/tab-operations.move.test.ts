/**
 * Moving a tab, as everything around the state change: what gets persisted, what
 * analytics hears, the cursor a leaving tab carries, and the toast a refused drop shows.
 * The rules themselves (pinned, only tab, cap, who stays active) are pinned on the pure
 * `moveTab` in `../tabs/tab-state-manager.test.ts`.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { savePaneTabsSpy, trackEventSpy, updatePinTabMenuSpy, addToastSpy } = vi.hoisted(() => ({
  savePaneTabsSpy: vi.fn<(pane: string, tabs: { tabs: { id: string }[] }) => Promise<void>>(() => Promise.resolve()),
  trackEventSpy: vi.fn<(name: string, props: Record<string, unknown>) => Promise<void>>(() => Promise.resolve()),
  updatePinTabMenuSpy: vi.fn<(pinned: boolean) => Promise<void>>(() => Promise.resolve()),
  addToastSpy: vi.fn<(message: string, options?: { level?: string }) => void>(),
}))

vi.mock('$lib/app-status-store', () => ({ savePaneTabs: savePaneTabsSpy }))
vi.mock('$lib/tauri-commands', () => ({
  showTabContextMenu: vi.fn(),
  onTabContextAction: vi.fn(() => Promise.resolve(() => {})),
  updatePinTabMenu: updatePinTabMenuSpy,
  trackEvent: trackEventSpy,
}))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createInitialTabState, handleTabDrop, moveTabToPane, type TabMoveDeps } from './tab-operations'
import { createTabManager, MAX_TABS_PER_PANE, type TabManager } from '../tabs/tab-state-manager.svelte'
import type { FilePaneAPI } from './types'

type Pane = 'left' | 'right'

function managerOf(ids: string[], active: string = ids[0], pinned: string[] = []): TabManager {
  const tabs = ids.map((id) => ({
    ...createInitialTabState(`/Users/test/${id}`, 'root'),
    id,
    pinned: pinned.includes(id),
  }))
  const mgr = createTabManager(tabs[0])
  for (const tab of tabs.slice(1)) mgr.tabs.push(tab)
  mgr.activeTabId = active
  return mgr
}

const fullPane = () => managerOf(Array.from({ length: MAX_TABS_PER_PANE }, (_, i) => `x${String(i)}`))

function setup(opts: { left: TabManager; right: TabManager; focused?: Pane; cursor?: Partial<Record<Pane, string>> }) {
  const mgrs = { left: opts.left, right: opts.right }
  const deps: TabMoveDeps = {
    getTabMgr: (pane) => mgrs[pane],
    getPaneRef: (pane) => {
      const name = opts.cursor?.[pane]
      return name === undefined ? undefined : ({ getFilenameUnderCursor: () => name } as unknown as FilePaneAPI)
    },
    getFocusedPane: () => opts.focused ?? 'left',
  }
  return { deps, ...mgrs }
}

const idsOf = (mgr: TabManager) => mgr.tabs.map((t) => t.id)
const savedPanes = () => savePaneTabsSpy.mock.calls.map(([pane]) => pane).sort()

beforeEach(() => {
  vi.clearAllMocks()
})

describe('moveTabToPane', () => {
  it('reorders within a pane, persisting that pane alone', () => {
    const { deps, left } = setup({ left: managerOf(['a', 'b', 'c']), right: managerOf(['x']) })

    const result = moveTabToPane({ fromPane: 'left', tabId: 'a', toPane: 'left', toIndex: 2 }, deps)

    expect(result).toMatchObject({ moved: true, toIndex: 2 })
    expect(idsOf(left)).toEqual(['b', 'c', 'a'])
    expect(savedPanes()).toEqual(['left'])
    expect(trackEventSpy).toHaveBeenCalledExactlyOnceWith('tab_moved', {
      scope: 'samePane',
      outcome: 'moved',
      open_tabs: 3,
    })
  })

  it('moves to the other pane, persisting both', () => {
    const { deps, left, right } = setup({ left: managerOf(['a', 'b']), right: managerOf(['x', 'y']) })

    moveTabToPane({ fromPane: 'left', tabId: 'b', toPane: 'right', toIndex: 1 }, deps)

    expect(idsOf(left)).toEqual(['a'])
    expect(idsOf(right)).toEqual(['x', 'b', 'y'])
    expect(savedPanes()).toEqual(['left', 'right'])
    expect(trackEventSpy).toHaveBeenCalledExactlyOnceWith('tab_moved', {
      scope: 'otherPane',
      outcome: 'moved',
      open_tabs: 3,
    })
  })

  it('persists the order the move produced', () => {
    const { deps } = setup({ left: managerOf(['a', 'b', 'c']), right: managerOf(['x']) })

    moveTabToPane({ fromPane: 'left', tabId: 'c', toPane: 'left', toIndex: 0 }, deps)

    expect(savePaneTabsSpy.mock.calls[0][1].tabs.map((t) => t.id)).toEqual(['c', 'a', 'b'])
  })

  it('gives an active tab that leaves the cursor it had, so it shows the same row when next opened', () => {
    const { deps, right } = setup({
      left: managerOf(['a', 'b'], 'a'),
      right: managerOf(['x']),
      cursor: { left: 'notes.txt', right: 'other.txt' },
    })

    moveTabToPane({ fromPane: 'left', tabId: 'a', toPane: 'right' }, deps)

    expect(right.tabs[1]).toMatchObject({ id: 'a', cursorFilename: 'notes.txt' })
  })

  it("leaves an inactive tab's remembered cursor alone", () => {
    const left = managerOf(['a', 'b'], 'a')
    left.tabs[1].cursorFilename = 'remembered.txt'
    const { deps, right } = setup({ left, right: managerOf(['x']), cursor: { left: 'live.txt' } })

    moveTabToPane({ fromPane: 'left', tabId: 'b', toPane: 'right' }, deps)

    expect(right.tabs[1].cursorFilename).toBe('remembered.txt')
  })

  it("leaves the active tab's remembered cursor alone on a reorder, since its pane stays mounted", () => {
    const left = managerOf(['a', 'b'], 'a')
    const { deps } = setup({ left, right: managerOf(['x']), cursor: { left: 'live.txt' } })

    moveTabToPane({ fromPane: 'left', tabId: 'a', toPane: 'left', toIndex: 1 }, deps)

    expect(left.tabs[1].cursorFilename).toBeNull()
  })

  it('re-syncs the Pin tab menu when the focused pane loses its active tab', () => {
    const { deps } = setup({ left: managerOf(['a', 'b'], 'a', ['b']), right: managerOf(['x']), focused: 'left' })

    moveTabToPane({ fromPane: 'left', tabId: 'a', toPane: 'right' }, deps)

    // `b` took over as active, and it's pinned.
    expect(updatePinTabMenuSpy).toHaveBeenCalledExactlyOnceWith(true)
  })

  it('leaves the Pin tab menu alone when the focused pane keeps its active tab', () => {
    const { deps } = setup({ left: managerOf(['a', 'b'], 'a'), right: managerOf(['x', 'y']), focused: 'right' })

    moveTabToPane({ fromPane: 'left', tabId: 'a', toPane: 'right' }, deps)
    moveTabToPane({ fromPane: 'right', tabId: 'y', toPane: 'right', toIndex: 0 }, deps)

    expect(updatePinTabMenuSpy).not.toHaveBeenCalled()
  })

  it.each([
    { name: 'a pinned tab', left: ['a', 'b'], pinned: ['b'], rightFull: false, tabId: 'b', outcome: 'pinned' },
    { name: "a pane's only tab", left: ['a'], pinned: [], rightFull: false, tabId: 'a', outcome: 'onlyTab' },
    { name: 'a move into a full pane', left: ['a', 'b'], pinned: [], rightFull: true, tabId: 'a', outcome: 'atCap' },
  ])(
    'refuses $name: nothing persisted, the refusal counted',
    ({ left: leftIds, pinned, rightFull, tabId, outcome }) => {
      const { deps, left, right } = setup({
        left: managerOf(leftIds, leftIds[0], pinned),
        right: rightFull ? fullPane() : managerOf(['x']),
      })
      const before = [idsOf(left), idsOf(right)]

      const result = moveTabToPane({ fromPane: 'left', tabId, toPane: 'right' }, deps)

      expect(result.moved).toBe(false)
      expect([idsOf(left), idsOf(right)]).toEqual(before)
      expect(savePaneTabsSpy).not.toHaveBeenCalled()
      expect(trackEventSpy).toHaveBeenCalledExactlyOnceWith('tab_moved', {
        scope: 'otherPane',
        outcome,
        open_tabs: right.tabs.length,
      })
    },
  )

  it('does nothing at all for a tab dropped back where it was', () => {
    const { deps } = setup({ left: managerOf(['a', 'b']), right: managerOf(['x']) })

    expect(moveTabToPane({ fromPane: 'left', tabId: 'a', toPane: 'left', toIndex: 0 }, deps)).toEqual({
      moved: false,
      reason: 'unchanged',
    })
    expect(savePaneTabsSpy).not.toHaveBeenCalled()
    expect(trackEventSpy).not.toHaveBeenCalled()
  })

  it('does nothing at all for a tab that closed before the drop', () => {
    const { deps } = setup({ left: managerOf(['a', 'b']), right: managerOf(['x']) })

    expect(moveTabToPane({ fromPane: 'left', tabId: 'gone', toPane: 'right' }, deps)).toEqual({
      moved: false,
      reason: 'notFound',
    })
    expect(savePaneTabsSpy).not.toHaveBeenCalled()
    expect(trackEventSpy).not.toHaveBeenCalled()
  })
})

describe('handleTabDrop', () => {
  it('says nothing when the drop works', () => {
    const { deps } = setup({ left: managerOf(['a', 'b']), right: managerOf(['x']) })
    handleTabDrop({ fromPane: 'left', tabId: 'a', toPane: 'right' }, deps)
    expect(addToastSpy).not.toHaveBeenCalled()
  })

  it('explains a drop on a full pane', () => {
    const { deps } = setup({ left: managerOf(['a', 'b']), right: fullPane() })
    handleTabDrop({ fromPane: 'left', tabId: 'a', toPane: 'right' }, deps)
    expect(addToastSpy).toHaveBeenCalledExactlyOnceWith('Tab limit reached', { level: 'warn' })
  })

  it("says nothing about a pane's only tab, which the mouse can't pick up in the first place", () => {
    const { deps, left } = setup({ left: managerOf(['a']), right: managerOf(['x']) })
    handleTabDrop({ fromPane: 'left', tabId: 'a', toPane: 'right' }, deps)
    expect(addToastSpy).not.toHaveBeenCalled()
    expect(left.tabs.map((tab) => tab.id)).toEqual(['a'])
  })
})
