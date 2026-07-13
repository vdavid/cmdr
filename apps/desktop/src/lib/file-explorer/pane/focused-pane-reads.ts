/**
 * Focused-pane read helpers over the explorer store.
 *
 * These wrap the `getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane()))`
 * chain so consumers outside `DualPaneExplorer` (the Go-to-path dialog, the
 * Search dialog, command dispatch) read the focused pane's path / volume id /
 * searchable folder directly from the store instead of through `explorerRef`
 * getters. Each one is a READ: a live, reactive read over store-owned tab state
 * (P1 — touches only the focused pane, never both). They keep live-reference
 * semantics, so a call inside a `$derived` / template expression keeps tracking
 * when the active tab or the focused pane changes — no snapshot severs the seam.
 *
 * They mirror the `getFocusedPane*` getters `DualPaneExplorer` still exposes on
 * `ExplorerAPI` for the write-coupled call sites (navigation) that retire later.
 */

import { resolveSearchableFolder } from '$lib/search/searchable-folder'
import { resolveImageSearchVolume, type ImageSearchVolume } from '$lib/search/active-media-volume'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { getActiveTab } from '../tabs/tab-state-manager.svelte'
import { explorerState } from './explorer-state.svelte'

/** The focused pane's current directory path. Reactive. */
export function getFocusedPanePath(): string {
  return getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane())).path
}

/** The focused pane's active-tab volume id. Reactive. */
export function getFocusedPaneVolumeId(): string {
  return getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane())).volumeId
}

/**
 * The "current folder" the Search dialog's `Search in → Use current folder`
 * action should act on. When the focused pane is a `search-results://` snapshot
 * its path isn't a real folder, so this walks the pane's history back for the
 * most recent real folder; when none is reachable it surfaces a disabled state
 * with a tooltip. Delegates to the pure `resolveSearchableFolder`. Reactive.
 */
export function getFocusedPaneSearchableFolder(): {
  path: string | null
  disabled: boolean
  disabledReason: string
} {
  const tab = getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane()))
  return resolveSearchableFolder({
    currentPath: tab.path,
    history: tab.history.stack.map((e) => e.path),
  })
}

/**
 * The volume the Search dialog's image-OCR grid should search: the focused
 * pane's current volume, resolved against the live volume list for its mount
 * root + network flag. So browsing the NAS surfaces its photos, browsing local
 * surfaces local. Delegates to the pure `resolveImageSearchVolume`. Reactive.
 */
export function getFocusedPaneImageSearchVolume(): ImageSearchVolume {
  return resolveImageSearchVolume(getVolumes(), getFocusedPaneVolumeId())
}
