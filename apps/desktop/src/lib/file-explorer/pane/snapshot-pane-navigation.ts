/**
 * Pure helpers for the snapshot pane (`volumeId === 'search-results'`)
 * cross-volume navigation rule.
 *
 * The rule: when the active pane is on the snapshot volume and the user
 * navigates to a real path, route through the volume-change machinery
 * (`onVolumeChange` / `handleVolumeChange`) so the pane switches to the
 * resolved real volume FIRST, then loads the target path. Symmetric for the
 * search-dialog "navigate to a file" path: `DualPaneExplorer.navigateToPath`
 * must do the same conversion when its current `volumeId` is `search-results`.
 *
 * Why this matters: a bare `loadDirectory(realPath)` from a snapshot pane
 * leaves `volumeId === 'search-results'` but `path` pointing to a real
 * filesystem location. The pane then re-renders `SearchResultsView`, tries to
 * extract a snapshot id from a path that doesn't start with
 * `search-results://`, gets `null`, and falls through to "Search results no
 * longer available". The IPC also kicks off a real listing under the wrong
 * `volume_id`, which the `applyPathChange` guard then drops with a "Dropping
 * stale onPathChange on search-results pane" warning.
 *
 * `isCrossVolumeNavigation` answers the trigger question: "is the upcoming
 * navigation crossing out of the snapshot volume?". Keep this pure so both
 * call sites (FilePane.handleNavigate, DualPaneExplorer.navigateToPath) read
 * the same single source of truth.
 */

export const SEARCH_RESULTS_VOLUME_ID = 'search-results'
export const SEARCH_RESULTS_PATH_PREFIX = 'search-results://'

/**
 * Returns true when navigating from the given `currentVolumeId` to `targetPath`
 * is leaving the snapshot volume for a real filesystem path. The caller MUST
 * route through the volume-change machinery in that case (resolve the real
 * volume, then `onVolumeChange` / `handleVolumeChange`); a bare
 * `loadDirectory(targetPath)` would leave the pane's `volumeId` stuck on
 * `search-results` with a real `path`.
 *
 * Returns false in every other case: same-volume nav, network volume, MTP
 * volume, normal local volume, and the (very unusual) case of an internal
 * `search-results://` to `search-results://` navigation (those don't need a
 * volume switch).
 */
export function isCrossVolumeNavigation(currentVolumeId: string, targetPath: string): boolean {
  return currentVolumeId === SEARCH_RESULTS_VOLUME_ID && !targetPath.startsWith(SEARCH_RESULTS_PATH_PREFIX)
}
