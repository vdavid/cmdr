/**
 * Mirrors each pane's tab STRUCTURE (id / path / pinned / active) into the MCP
 * backend's tab store, debounced ~100 ms trailing. Lifted out of
 * `DualPaneExplorer` as a `*.svelte.ts` factory owning its own reactive `$effect`,
 * debounce timer, and cleanup.
 *
 * This is the MCP backend mirror (L8 / A5): the Rust state store for MCP, a
 * different target and debounce than disk persistence — NOT `app-status.json`
 * (that's `persistence-subscriber` / `tab-operations`). Sibling of
 * `pane-mcp-sync.svelte.ts`, which mirrors the live pane STATE; this one mirrors
 * the tab set.
 *
 * Created synchronously during component init (the `initListingDiffSync` /
 * `initPersistenceSubscriber` pattern) so the `$effect` gets Svelte's tracking
 * context. `syncTabsToBackend()` is also exposed for the one-shot initial push
 * `onMount` fires after persisted state loads; `cleanup()` clears the pending
 * timer from `onDestroy`.
 */

import { untrack } from 'svelte'
import { updatePaneTabs } from '$lib/tauri-commands'
import { getAllTabs, type TabManager } from '../tabs/tab-state-manager.svelte'

const TAB_SYNC_DEBOUNCE_MS = 100

export interface TabMcpSyncDeps {
  getLeftTabMgr: () => TabManager
  getRightTabMgr: () => TabManager
  /** Gates the effect so the load-from-disk tab creation doesn't push before init. */
  getInitialized: () => boolean
}

export interface TabMcpSync {
  /** Schedules a debounced push of both panes' tab sets to the MCP backend. Also
   *  called once from `onMount` for the initial sync after persisted state loads. */
  syncTabsToBackend: () => void
  /** Clears any pending debounce timer. Call from `onDestroy`. */
  cleanup: () => void
}

export function initTabMcpSync(deps: TabMcpSyncDeps): TabMcpSync {
  let tabSyncTimer: ReturnType<typeof setTimeout> | null = null

  function syncTabsToBackend(): void {
    if (tabSyncTimer) clearTimeout(tabSyncTimer)
    tabSyncTimer = setTimeout(() => {
      const leftTabMgr = deps.getLeftTabMgr()
      const rightTabMgr = deps.getRightTabMgr()
      const leftTabs = getAllTabs(leftTabMgr).map((t) => ({
        id: t.id,
        path: t.path,
        pinned: t.pinned,
        active: t.id === leftTabMgr.activeTabId,
      }))
      const rightTabs = getAllTabs(rightTabMgr).map((t) => ({
        id: t.id,
        path: t.path,
        pinned: t.pinned,
        active: t.id === rightTabMgr.activeTabId,
      }))
      void updatePaneTabs('left', leftTabs)
      void updatePaneTabs('right', rightTabs)
    }, TAB_SYNC_DEBOUNCE_MS)
  }

  // Reactive effect: sync tab structural changes to the MCP backend
  $effect(() => {
    const leftTabMgr = deps.getLeftTabMgr()
    const rightTabMgr = deps.getRightTabMgr()
    // Read reactive values to establish Svelte reactivity dependencies.
    // Include path so MCP state updates when the active tab navigates.
    void getAllTabs(leftTabMgr).map((t) => `${t.id}:${t.pinned ? 'p' : ''}:${t.path}`)
    void getAllTabs(rightTabMgr).map((t) => `${t.id}:${t.pinned ? 'p' : ''}:${t.path}`)
    void leftTabMgr.activeTabId
    void rightTabMgr.activeTabId

    if (!deps.getInitialized()) return

    untrack(() => {
      syncTabsToBackend()
    })
  })

  return {
    syncTabsToBackend,
    cleanup: () => {
      if (tabSyncTimer) clearTimeout(tabSyncTimer)
    },
  }
}
