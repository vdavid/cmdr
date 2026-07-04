import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TabManager } from '../tabs/tab-state-manager.svelte'

const {
  getTabCountSpy,
  getAllTabsSpy,
  closeTabRecordingSpy,
  closeOtherTabsRecordingSpy,
  pinTabSpy,
  unpinTabSpy,
  tabOpsNewTabSpy,
  tabOpsReopenSpy,
} = vi.hoisted(() => ({
  getTabCountSpy: vi.fn<() => number>(),
  getAllTabsSpy: vi.fn<() => { id: string; pinned: boolean }[]>(),
  closeTabRecordingSpy: vi.fn(),
  closeOtherTabsRecordingSpy: vi.fn(),
  pinTabSpy: vi.fn(),
  unpinTabSpy: vi.fn(),
  tabOpsNewTabSpy: vi.fn<() => boolean>(),
  tabOpsReopenSpy: vi.fn(),
}))

vi.mock('../tabs/tab-state-manager.svelte', () => ({
  getTabCount: getTabCountSpy,
  getAllTabs: getAllTabsSpy,
  closeTabRecording: closeTabRecordingSpy,
  closeOtherTabsRecording: closeOtherTabsRecordingSpy,
  pinTab: pinTabSpy,
  unpinTab: unpinTabSpy,
}))
vi.mock('./tab-operations', () => ({
  newTab: tabOpsNewTabSpy,
  reopenLastClosedTabInPane: tabOpsReopenSpy,
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createMcpTabAction, type McpTabActionDeps } from './mcp-tab-action'

function setup(opts: { focused?: 'left' | 'right'; activeTabId?: string } = {}) {
  const mgr = { activeTabId: opts.activeTabId ?? 'active' } as unknown as TabManager
  const deps: McpTabActionDeps = {
    getFocusedPane: () => opts.focused ?? 'left',
    getTabMgr: () => mgr,
    getClosedTabsCap: () => 10,
    saveTabsForPaneSide: vi.fn(),
    syncPinTabMenu: vi.fn(),
    syncReopenMenuState: vi.fn(),
    reopenLastClosedTab: vi.fn(),
    switchToTab: vi.fn(),
    snapshotHistory: (h) => h,
  }
  return { handler: createMcpTabAction(deps).handleMcpTabAction, deps, mgr }
}

describe('createMcpTabAction', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getTabCountSpy.mockReturnValue(3)
    getAllTabsSpy.mockReturnValue([{ id: 'active', pinned: false }])
    tabOpsNewTabSpy.mockReturnValue(true)
  })

  it('close refuses to close the last tab', () => {
    getTabCountSpy.mockReturnValue(1)
    const { handler } = setup()
    handler('left', 'close')
    expect(closeTabRecordingSpy).not.toHaveBeenCalled()
  })

  it('close records + persists and syncs menus for the focused pane', () => {
    const { handler, deps } = setup({ focused: 'left' })
    handler('left', 'close', 'tab-2')
    expect(closeTabRecordingSpy).toHaveBeenCalledWith(expect.anything(), 'tab-2', 10)
    expect(deps.saveTabsForPaneSide).toHaveBeenCalledWith('left')
    expect(deps.syncPinTabMenu).toHaveBeenCalled()
    expect(deps.syncReopenMenuState).toHaveBeenCalled()
  })

  it('close on a NON-focused pane persists but does not sync the focused-pane menus', () => {
    const { handler, deps } = setup({ focused: 'right' })
    handler('left', 'close', 'tab-2')
    expect(deps.saveTabsForPaneSide).toHaveBeenCalledWith('left')
    expect(deps.syncPinTabMenu).not.toHaveBeenCalled()
    expect(deps.syncReopenMenuState).not.toHaveBeenCalled()
  })

  it('reopen on the focused pane uses the cheap focused helper', () => {
    const { handler, deps } = setup({ focused: 'left' })
    handler('left', 'reopen')
    expect(deps.reopenLastClosedTab).toHaveBeenCalled()
    expect(tabOpsReopenSpy).not.toHaveBeenCalled()
  })

  it('reopen on a non-focused pane calls the tab-operations helper for that pane', () => {
    const { handler, deps } = setup({ focused: 'right' })
    handler('left', 'reopen')
    expect(tabOpsReopenSpy).toHaveBeenCalledWith('left', expect.any(Function))
    expect(deps.reopenLastClosedTab).not.toHaveBeenCalled()
  })

  it('activate switches to the given tab', () => {
    const { handler, deps } = setup()
    handler('right', 'activate', 'tab-9')
    expect(deps.switchToTab).toHaveBeenCalledWith('right', 'tab-9')
  })

  it('set_pinned pins an unpinned tab', () => {
    getAllTabsSpy.mockReturnValue([{ id: 'active', pinned: false }])
    const { handler, deps } = setup({ activeTabId: 'active' })
    handler('left', 'set_pinned', undefined, true)
    expect(pinTabSpy).toHaveBeenCalledWith(expect.anything(), 'active')
    expect(unpinTabSpy).not.toHaveBeenCalled()
    expect(deps.saveTabsForPaneSide).toHaveBeenCalledWith('left')
  })

  it('set_pinned unpins an already-pinned tab', () => {
    getAllTabsSpy.mockReturnValue([{ id: 'active', pinned: true }])
    const { handler } = setup({ activeTabId: 'active' })
    handler('left', 'set_pinned', undefined, false)
    expect(unpinTabSpy).toHaveBeenCalledWith(expect.anything(), 'active')
    expect(pinTabSpy).not.toHaveBeenCalled()
  })

  it('new warns when the tab limit is reached', () => {
    tabOpsNewTabSpy.mockReturnValue(false)
    const { handler } = setup()
    handler('left', 'new')
    expect(tabOpsNewTabSpy).toHaveBeenCalled()
    // No throw; the warn path is exercised.
  })
})
