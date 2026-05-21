/**
 * Pure utility functions for path navigation logic.
 * Extracted from DualPaneExplorer.svelte to improve modularity.
 *
 * All pathExists calls use frontend timeouts to prevent hangs on slow/unresponsive volumes.
 * The Rust backend also enforces a 2-second timeout per pathExists call.
 */

import { pathExists } from '$lib/tauri-commands'
import { getLastUsedPathForVolume } from '$lib/app-status-store'
import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'
import { withTimeout } from '$lib/utils/timing'

export { withTimeout }

export interface OtherPaneState {
  otherPaneVolumeId: string
  otherPanePath: string
}

/**
 * True when `path` equals `volumePath` or is a descendant of it. Used to drop
 * stale or corrupted paths that don't belong on the given volume — for example
 * a local `/Users/...` path that ended up persisted under an SMB volumeId from
 * a previous bug.
 */
export function isPathOnVolume(path: string, volumePath: string): boolean {
  if (path === volumePath) return true
  const prefix = volumePath.endsWith('/') ? volumePath : volumePath + '/'
  return path.startsWith(prefix)
}

/**
 * Determines which path to navigate to when switching volumes.
 * Runs checks in parallel with 500ms frontend timeouts per check.
 * Priority order:
 * 1. Favorite path (if targetPath !== volumePath)
 * 2. Other pane's path (if the other pane is on the same volume)
 * 3. Stored lastUsedPath for this volume
 * 4. Default: ~ for main volume, volume root for others
 */
export async function determineNavigationPath(
  volumeId: string,
  volumePath: string,
  targetPath: string,
  otherPane: OtherPaneState,
): Promise<string> {
  const pathExistsTimeoutMs = 500

  // User navigated to a favorite, so go to the favorite's path directly
  if (targetPath !== volumePath) {
    return targetPath
  }

  // Run both checks in parallel with timeouts
  const [otherPaneValid, lastUsedResult] = await Promise.all([
    otherPane.otherPaneVolumeId === volumeId
      ? withTimeout(pathExists(otherPane.otherPanePath), pathExistsTimeoutMs, false)
      : Promise.resolve(false),
    getLastUsedPathForVolume(volumeId).then((p) =>
      p && isPathOnVolume(p, volumePath)
        ? withTimeout(pathExists(p), pathExistsTimeoutMs, false).then((ok) => (ok ? p : null))
        : null,
    ),
  ])

  if (otherPaneValid) return otherPane.otherPanePath
  if (lastUsedResult) return lastUsedResult

  // Default: ~ for main volume (root), volume path for others
  return volumeId === DEFAULT_VOLUME_ID ? '~' : volumePath
}
