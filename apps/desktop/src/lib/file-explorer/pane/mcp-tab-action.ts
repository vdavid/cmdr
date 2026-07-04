/**
 * The MCP `tab` tool's per-pane action dispatch (`tab.mcpAction` on the command
 * bus): new / close / close_others / reopen / activate / set_pinned. Lifted out
 * of `DualPaneExplorer` as an MCP command body, in the `pane-commands` factory
 * shape — it owns the tab-mutation primitives; the dispatch layer forwards the
 * typed args.
 *
 * Menu-sync (pin state, reopen-enabled) and disk persistence are driven the same
 * way the component did: only for the focused pane, and `saveTabsForPaneSide`
 * per mutating branch. The focused-pane `reopen` reuses the component's
 * `reopenLastClosedTab` (which already syncs); non-focused reopen calls the
 * tab-operations helper directly.
 */

import {
  getTabCount,
  getAllTabs,
  closeTabRecording,
  closeOtherTabsRecording,
  pinTab,
  unpinTab,
  type TabManager,
} from '../tabs/tab-state-manager.svelte'
import type { NavigationHistory } from '../navigation/navigation-history'
import { newTab as tabOpsNewTab, reopenLastClosedTabInPane as tabOpsReopenLastClosedTab } from './tab-operations'
import { getAppLogger } from '$lib/logging/logger'
import type { McpTabAction } from '$lib/commands'

const log = getAppLogger('fileExplorer')

export interface McpTabActionDeps {
  getFocusedPane: () => 'left' | 'right'
  getTabMgr: (pane: 'left' | 'right') => TabManager
  getClosedTabsCap: () => number
  saveTabsForPaneSide: (pane: 'left' | 'right') => void
  syncPinTabMenu: () => void
  syncReopenMenuState: () => void
  /** Reopen the last-closed tab in the FOCUSED pane (already menu-syncs). */
  reopenLastClosedTab: () => void
  switchToTab: (pane: 'left' | 'right', tabId: string) => void
  /** `(h) => $state.snapshot(h)` — the component supplies the rune-based snapshot. */
  snapshotHistory: (h: NavigationHistory) => NavigationHistory
}

export interface McpTabActionHandler {
  handleMcpTabAction: (pane: 'left' | 'right', action: McpTabAction, tabId?: string, pinned?: boolean) => void
}

export function createMcpTabAction(deps: McpTabActionDeps): McpTabActionHandler {
  function handleMcpTabAction(pane: 'left' | 'right', action: McpTabAction, tabId?: string, pinned?: boolean): void {
    const mgr = deps.getTabMgr(pane)
    const focusedPane = deps.getFocusedPane()
    const mcpTabHandlers: Record<McpTabAction, () => void> = {
      new: () => {
        if (!tabOpsNewTab(pane, deps.getTabMgr, deps.snapshotHistory)) {
          log.warn(`MCP tab new: tab limit reached in ${pane} pane`)
        }
      },
      close: () => {
        if (getTabCount(mgr) <= 1) {
          log.warn(`MCP tab close: can't close last tab in ${pane} pane`)
          return
        }
        closeTabRecording(mgr, tabId ?? mgr.activeTabId, deps.getClosedTabsCap())
        deps.saveTabsForPaneSide(pane)
        if (pane === focusedPane) deps.syncPinTabMenu()
        if (pane === focusedPane) deps.syncReopenMenuState()
      },
      close_others: () => {
        closeOtherTabsRecording(mgr, tabId ?? mgr.activeTabId, deps.getClosedTabsCap())
        deps.saveTabsForPaneSide(pane)
        if (pane === focusedPane) deps.syncReopenMenuState()
      },
      reopen: () => {
        if (pane === focusedPane) {
          // Cheap path: existing helper handles the focused pane.
          deps.reopenLastClosedTab()
          return
        }
        // For non-focused panes, call the tab-operations helper with the target pane.
        tabOpsReopenLastClosedTab(pane, deps.getTabMgr)
      },
      activate: () => {
        if (tabId) deps.switchToTab(pane, tabId)
      },
      set_pinned: () => {
        const pinId = tabId ?? mgr.activeTabId
        const tab = getAllTabs(mgr).find((t) => t.id === pinId)
        if (!tab) return
        if (pinned && !tab.pinned) pinTab(mgr, pinId)
        else if (!pinned && tab.pinned) unpinTab(mgr, pinId)
        deps.saveTabsForPaneSide(pane)
        if (pane === focusedPane && pinId === mgr.activeTabId) deps.syncPinTabMenu()
      },
    }
    mcpTabHandlers[action]()
  }

  return { handleMcpTabAction }
}
