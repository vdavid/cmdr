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
import { dependOn } from '$lib/utils/reactivity'
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
  /**
   * Pushes both panes' tab sets now and resolves once the backend has them, dropping any
   * pending debounced push. For a caller about to tell an agent the tabs changed (the MCP
   * `tab move` reply): the agent's next state read must not land inside the debounce.
   */
  syncTabsNow: () => Promise<void>
  /** Clears any pending debounce timer. Call from `onDestroy`. */
  cleanup: () => void
}

export function initTabMcpSync(deps: TabMcpSyncDeps): TabMcpSync {
  let tabSyncTimer: ReturnType<typeof setTimeout> | null = null

  function tabsOf(mgr: TabManager) {
    return getAllTabs(mgr).map((t) => ({
      id: t.id,
      path: t.path,
      pinned: t.pinned,
      active: t.id === mgr.activeTabId,
    }))
  }

  async function pushTabs(): Promise<void> {
    await Promise.all([
      updatePaneTabs('left', tabsOf(deps.getLeftTabMgr())),
      updatePaneTabs('right', tabsOf(deps.getRightTabMgr())),
    ])
  }

  function clearTimer(): void {
    if (tabSyncTimer) clearTimeout(tabSyncTimer)
    tabSyncTimer = null
  }

  function syncTabsToBackend(): void {
    clearTimer()
    tabSyncTimer = setTimeout(() => {
      tabSyncTimer = null
      void pushTabs()
    }, TAB_SYNC_DEBOUNCE_MS)
  }

  async function syncTabsNow(): Promise<void> {
    clearTimer()
    await pushTabs()
  }

  // Reactive effect: sync tab structural changes to the MCP backend
  $effect(() => {
    const leftTabMgr = deps.getLeftTabMgr()
    const rightTabMgr = deps.getRightTabMgr()
    // Read reactive values to establish Svelte reactivity dependencies.
    // Include path so MCP state updates when the active tab navigates.
    dependOn(
      getAllTabs(leftTabMgr).map((t) => `${t.id}:${t.pinned ? 'p' : ''}:${t.path}`),
      getAllTabs(rightTabMgr).map((t) => `${t.id}:${t.pinned ? 'p' : ''}:${t.path}`),
      leftTabMgr.activeTabId,
      rightTabMgr.activeTabId,
    )

    if (!deps.getInitialized()) return

    untrack(() => {
      syncTabsToBackend()
    })
  })

  return {
    syncTabsToBackend,
    syncTabsNow,
    cleanup: clearTimer,
  }
}
