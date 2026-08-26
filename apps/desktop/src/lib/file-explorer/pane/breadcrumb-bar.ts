/**
 * The pane header's path readout: what it displays, and what its three
 * interactions do (clicking an ancestor segment, right-clicking the bar, and
 * picking a different volume from the chooser).
 *
 * The segment splitting and the per-segment click targets already live in
 * `../navigation/path-segments` and `../navigation/breadcrumb-navigation`; this
 * module owns the string those two start from, plus the handlers.
 */

import { showBreadcrumbContextMenu } from '$lib/tauri-commands'
import type { VolumeInfo } from '../types'
import { isMtpVolumeId, getMtpDisplayPath } from '$lib/mtp'
import { getEffectiveShortcuts } from '$lib/shortcuts/shortcuts-store'
import { toDisplayShortcut } from '$lib/shortcuts/key-capture'
import { isVolumeEjectable } from '../navigation/eject-predicate'
import { getVolumes as getStoreVolumes } from '$lib/stores/volume-store.svelte'
import type { VolumeChangePayload, VolumeSpaceWatchArgs } from './types'

export interface BreadcrumbDisplayPathInput {
  currentPath: string
  volumeId: string
  /** The pane volume's mount point, or `/` for the root volume. */
  volumePath: string
  /** The user's home dir, or `''` before it resolves on mount. */
  userHomePath: string
  isSearchResultsView: boolean
  /** The snapshot's friendly label, on a search-results pane. */
  searchLabel: string | undefined
}

/**
 * The path shown after the volume name. On the root volume the home prefix
 * becomes `~`; on any other volume the path is shown relative to its mount
 * point; MTP has its own display form.
 *
 * R3 B6: the search-results pane shows the snapshot's friendly label (the AI
 * title / filename pattern / regex pattern) AS the path. The volume selector
 * itself reads the generic "Search results" so the slots map cleanly:
 * volume-kind on the left, query-specific label on the right. Don't invert this
 * (label on the left, no path on the right) — see `lib/search/CLAUDE.md`
 * § "Search-specific UI behavior".
 */
export function breadcrumbDisplayPath(input: BreadcrumbDisplayPathInput): string {
  const { currentPath, volumeId, volumePath, userHomePath } = input

  if (input.isSearchResultsView) {
    return input.searchLabel ?? 'Search'
  }
  if (isMtpVolumeId(volumeId)) return getMtpDisplayPath(currentPath)

  // For non-root volumes, strip the volume path prefix
  if (volumePath !== '/') {
    return currentPath.startsWith(volumePath) ? currentPath.slice(volumePath.length) || '/' : currentPath
  }

  // Root volume: paths starting with ~ are already user-friendly
  if (currentPath.startsWith('~')) return currentPath

  // Root volume with absolute path: replace home dir prefix with ~
  if (userHomePath && currentPath.startsWith(userHomePath)) {
    const rest = currentPath.slice(userHomePath.length)
    return rest ? '~' + rest : '~'
  }

  // Root volume, outside home dir: show absolute path as-is
  return currentPath
}

export interface BreadcrumbHandlerDeps {
  /** The live `VolumeInfo` for the pane's volume, for the eject menu item. */
  getCurrentVolumeInfo: () => VolumeInfo | null
  navigateToPath: (path: string) => Promise<void>
  setCurrentPath: (path: string) => void
  onVolumeChange: (change: VolumeChangePayload) => void
  onRequestFocus: () => void
  loadDirectory: (path: string) => void
  refreshSpace: () => void
  watchSpace: (args: VolumeSpaceWatchArgs) => void
  unwatchSpace: () => void
  clearSpace: () => void
}

export interface BreadcrumbHandlers {
  /** Navigate to a breadcrumb ancestor. Errors surface via the pane's error pipeline. */
  handleSegmentClick: (target: string) => void
  handleContextMenu: (event: MouseEvent) => void
  handleVolumeChange: (change: VolumeChangePayload) => void
}

export function createBreadcrumbHandlers(deps: BreadcrumbHandlerDeps): BreadcrumbHandlers {
  function handleSegmentClick(target: string): void {
    void deps.navigateToPath(target).catch(() => {})
  }

  function handleContextMenu(event: MouseEvent): void {
    event.preventDefault()
    deps.onRequestFocus()
    const shortcuts = getEffectiveShortcuts('file.copyCurrentDirectoryPath')
    // Pass eject info when the pane's volume is ejectable so the menu can
    // include an "Eject ({name})" item. Same gate as the row/header eject
    // buttons; the volume-context-action listener in DualPaneExplorer
    // dispatches the click to `ejectVolume`.
    const v = deps.getCurrentVolumeInfo()
    const ejectable = v && isVolumeEjectable(v)
    void showBreadcrumbContextMenu(
      toDisplayShortcut(shortcuts[0] ?? ''),
      ejectable ? v.id : undefined,
      ejectable ? v.name : undefined,
    )
  }

  function handleVolumeChange(change: VolumeChangePayload): void {
    const { volumeId: newVolumeId, targetPath } = change
    // Navigate to the target path (may differ from volume root for favorites)
    // Note: We intentionally don't call onPathChange here - the volume change handler
    // in DualPaneExplorer takes care of saving both the old volume's path and the new path.
    // Calling onPathChange would save the new path under the OLD volume ID (race condition).
    deps.setCurrentPath(targetPath)
    deps.onVolumeChange(change)

    // Don't load directory for network views (they handle their own data)
    // or device-only MTP views (they need connection first via auto-connect effect)
    // But DO load for connected MTP views (storage-specific volume ID contains ":")
    const isDeviceOnlyMtp = isMtpVolumeId(newVolumeId) && !newVolumeId.includes(':')
    if (newVolumeId !== 'network' && !isDeviceOnlyMtp) {
      deps.loadDirectory(targetPath)
      deps.unwatchSpace()
      // Disk images have no meaningful free space: skip the poll, the bottom bar, and the
      // SelectionInfo free/total text. Read the flag off the NEW volume directly — the
      // `volumeId` prop (and so the pane's disk-image derived) hasn't updated yet this tick.
      const newIsDiskImage = getStoreVolumes().find((v) => v.id === newVolumeId)?.isDiskImage === true
      if (newIsDiskImage) {
        deps.clearSpace()
      } else {
        deps.refreshSpace()
        deps.watchSpace({ volumeId: newVolumeId, path: targetPath })
      }
    } else {
      // Leaving a physical volume: stop watching
      deps.unwatchSpace()
    }
  }

  return { handleSegmentClick, handleContextMenu, handleVolumeChange }
}
