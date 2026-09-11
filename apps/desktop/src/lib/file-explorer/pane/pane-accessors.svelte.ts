/**
 * Per-pane derived state + the read/write accessor functions every other
 * factory in `DualPaneExplorer.svelte` is built on (`paneAccess`, `navigateDeps`,
 * `sortOps`, `swapper`, …). Split out of the component to keep it under its
 * length cap; a `*.svelte.ts` factory because it owns `$derived` reads over the
 * explorer store and the volume list, the same reasoning `drag-drop-controller.svelte.ts`
 * and the other `init*`/`create*` factories here already follow. ❌ Never a child
 * component — nothing here renders.
 *
 * The one thing that can't self-contain is `paneRefs`: it's the component's own
 * `$state`, bound directly in the template via `bind:this`, so it's handed in by
 * reference rather than re-derived.
 */
import type { ViewMode } from '$lib/app-status-store'
import { tString } from '$lib/intl/messages.svelte'
import { getSetting } from '$lib/settings'
import { getVolumes as getStoreVolumes } from '$lib/stores/volume-store.svelte'
import { setReopenClosedTabEnabled, updateViewModeMenu } from '$lib/tauri-commands'
import type { NavigationHistory } from '../navigation/navigation-history'
import type { SortColumn, SortOrder } from '../types'
import { explorerState } from './explorer-state.svelte'
import { getActiveTab, getClosedStackSize, type TabManager } from '../tabs/tab-state-manager.svelte'
import type { FilePaneAPI } from './types'

export function createPaneAccessors(paneRefs: Record<'left' | 'right', FilePaneAPI | undefined>) {
  // Live tab-manager holders live in the explorer store. These `$derived`
  // aliases read the live `$state<TabManager>` reference through the store getter,
  // so every reader below keeps tracking both holder swaps (`setTabMgr`) and
  // in-place manager mutations.
  const leftTabMgr = $derived(explorerState.getTabMgr('left'))
  const rightTabMgr = $derived(explorerState.getTabMgr('right'))

  // Derived active tab state: these replace the old scalar variables
  const leftPath = $derived(getActiveTab(leftTabMgr).path)
  const rightPath = $derived(getActiveTab(rightTabMgr).path)
  const leftVolumeId = $derived(getActiveTab(leftTabMgr).volumeId)
  const rightVolumeId = $derived(getActiveTab(rightTabMgr).volumeId)
  const leftViewMode = $derived(getActiveTab(leftTabMgr).viewMode)
  const rightViewMode = $derived(getActiveTab(rightTabMgr).viewMode)
  const leftSortBy = $derived(getActiveTab(leftTabMgr).sortBy)
  const rightSortBy = $derived(getActiveTab(rightTabMgr).sortBy)
  const leftSortOrder = $derived(getActiveTab(leftTabMgr).sortOrder)
  const rightSortOrder = $derived(getActiveTab(rightTabMgr).sortOrder)
  const leftHistory = $derived(getActiveTab(leftTabMgr).history)
  const rightHistory = $derived(getActiveTab(rightTabMgr).history)
  const leftPaneWidthPercent = $derived(explorerState.getLeftPaneWidthPercent())

  // Volumes come from the shared store (pushed by backend via `volumes-changed` event)
  const volumes = $derived(getStoreVolumes())

  // Derived volume paths - handle 'network' virtual volume specially
  const leftVolumePath = $derived(
    leftVolumeId === 'network' ? 'smb://' : (volumes.find((v) => v.id === leftVolumeId)?.path ?? '/'),
  )
  const rightVolumePath = $derived(
    rightVolumeId === 'network' ? 'smb://' : (volumes.find((v) => v.id === rightVolumeId)?.path ?? '/'),
  )
  // Derived volume names for MCP state sync. ❗ The hub row's name comes from the
  // catalog, the one place the switcher label, this push, and Rust's
  // `volume_listing::SERVERS_VOLUME_NAME` agree: `mcp/executor/nav.rs` waits for
  // this pushed name to equal that const before it calls a volume switch done.
  const serversVolumeName = $derived(tString('fileExplorer.navigation.networkVolume'))
  const leftVolumeName = $derived(
    leftVolumeId === 'network' ? serversVolumeName : volumes.find((v) => v.id === leftVolumeId)?.name,
  )
  const rightVolumeName = $derived(
    rightVolumeId === 'network' ? serversVolumeName : volumes.find((v) => v.id === rightVolumeId)?.name,
  )

  function getPaneRef(pane: 'left' | 'right'): FilePaneAPI | undefined {
    return paneRefs[pane]
  }

  function getPanePath(pane: 'left' | 'right'): string {
    return pane === 'left' ? leftPath : rightPath
  }

  function getPaneVolumeId(pane: 'left' | 'right'): string {
    return pane === 'left' ? leftVolumeId : rightVolumeId
  }

  function getPaneHistory(pane: 'left' | 'right'): NavigationHistory {
    return pane === 'left' ? leftHistory : rightHistory
  }

  function getPaneSort(pane: 'left' | 'right'): { sortBy: SortColumn; sortOrder: SortOrder } {
    return pane === 'left'
      ? { sortBy: leftSortBy, sortOrder: leftSortOrder }
      : { sortBy: rightSortBy, sortOrder: rightSortOrder }
  }

  function getTabMgr(pane: 'left' | 'right'): TabManager {
    return explorerState.getTabMgr(pane)
  }

  function setPanePath(pane: 'left' | 'right', path: string) {
    getActiveTab(getTabMgr(pane)).path = path
  }

  function setPaneVolumeId(pane: 'left' | 'right', volumeId: string) {
    getActiveTab(getTabMgr(pane)).volumeId = volumeId
  }

  function setPaneHistory(pane: 'left' | 'right', history: NavigationHistory) {
    getActiveTab(getTabMgr(pane)).history = history
  }

  function setPaneSort(pane: 'left' | 'right', sortBy: SortColumn, sortOrder: SortOrder) {
    const tab = getActiveTab(getTabMgr(pane))
    tab.sortBy = sortBy
    tab.sortOrder = sortOrder
  }

  function setPaneViewMode(pane: 'left' | 'right', viewMode: ViewMode) {
    getActiveTab(getTabMgr(pane)).viewMode = viewMode
  }

  function getPaneViewMode(pane: 'left' | 'right'): ViewMode {
    return pane === 'left' ? leftViewMode : rightViewMode
  }

  /** Pushes the full View menu state (active pane + per-pane modes) to the backend so
   * the per-pane menu items show correct check marks and the keyboard accelerator
   * (⌘1/⌘2 by default) attaches to the active pane's pair. */
  function pushViewMenuState() {
    void updateViewModeMenu(explorerState.getFocusedPane(), getPaneViewMode('left'), getPaneViewMode('right'))
  }

  function getPaneVolumePath(pane: 'left' | 'right'): string {
    return pane === 'left' ? leftVolumePath : rightVolumePath
  }

  function getPaneVolumeName(pane: 'left' | 'right'): string | undefined {
    return pane === 'left' ? leftVolumeName : rightVolumeName
  }

  function getPaneWidth(pane: 'left' | 'right'): number {
    return pane === 'left' ? leftPaneWidthPercent : 100 - leftPaneWidthPercent
  }

  function otherPane(pane: 'left' | 'right'): 'left' | 'right' {
    return pane === 'left' ? 'right' : 'left'
  }

  /** Per-pane closed-tab history cap, lives in `fileExplorer.tabs.closedTabHistorySize` setting. */
  function getClosedTabsCap(): number {
    return getSetting('fileExplorer.tabs.closedTabHistorySize')
  }

  /** Pushes the focused pane's closed-stack-empty state to the backend so the
   *  File menu's "Reopen closed tab" item enables/disables in sync. */
  function syncReopenMenuState() {
    const enabled = getClosedStackSize(getTabMgr(explorerState.getFocusedPane())) > 0
    void setReopenClosedTabEnabled(enabled)
  }

  return {
    get leftTabMgr() {
      return leftTabMgr
    },
    get rightTabMgr() {
      return rightTabMgr
    },
    get leftHistory() {
      return leftHistory
    },
    get rightHistory() {
      return rightHistory
    },
    get leftPath() {
      return leftPath
    },
    get rightPath() {
      return rightPath
    },
    get volumes() {
      return volumes
    },
    getPaneRef,
    getPanePath,
    getPaneVolumeId,
    getPaneHistory,
    getPaneSort,
    getTabMgr,
    setPanePath,
    setPaneVolumeId,
    setPaneHistory,
    setPaneSort,
    setPaneViewMode,
    getPaneViewMode,
    pushViewMenuState,
    getPaneVolumePath,
    getPaneVolumeName,
    getPaneWidth,
    otherPane,
    getClosedTabsCap,
    syncReopenMenuState,
  }
}
