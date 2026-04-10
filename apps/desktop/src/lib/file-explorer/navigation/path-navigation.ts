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

  // User navigated to a favorite — go to the favorite's path directly
  if (targetPath !== volumePath) {
    return targetPath
  }

  // Run both checks in parallel with timeouts
  const [otherPaneValid, lastUsedResult] = await Promise.all([
    otherPane.otherPaneVolumeId === volumeId
      ? withTimeout(pathExists(otherPane.otherPanePath), pathExistsTimeoutMs, false)
      : Promise.resolve(false),
    getLastUsedPathForVolume(volumeId).then((p) =>
      p ? withTimeout(pathExists(p), pathExistsTimeoutMs, false).then((ok) => (ok ? p : null)) : null,
    ),
  ])

  if (otherPaneValid) return otherPane.otherPanePath
  if (lastUsedResult) return lastUsedResult

  // Default: ~ for main volume (root), volume path for others
  return volumeId === DEFAULT_VOLUME_ID ? '~' : volumePath
}

export interface ResolveValidPathOptions {
  /** Custom path-existence checker. Defaults to the Tauri `pathExists` command. */
  pathExistsFn?: (path: string) => Promise<boolean>
  /** Timeout per step in ms. Set to 0 to skip timeout wrapping. Defaults to 1000. */
  timeoutMs?: number
  /**
   * Volume root path (like "/Volumes/naspi"). When set, the walk-up stops at this
   * boundary instead of continuing to "/" — prevents crossing into a different volume
   * (which would fail for non-local volumes like SmbVolume).
   */
  volumeRoot?: string
}

/**
 * Resolves a path to a valid existing path by walking up the parent tree.
 * Each step has a timeout to prevent hanging on dead mounts (default 1s).
 * Fallback chain: parent tree (up to volumeRoot) → user home (~) → filesystem root (/).
 * Returns null if even the root doesn't exist (volume unmounted).
 */
export async function resolveValidPath(targetPath: string, options?: ResolveValidPathOptions): Promise<string | null> {
  const checkFn = options?.pathExistsFn ?? pathExists
  const timeoutMs = options?.timeoutMs ?? 1000
  const volumeRoot = options?.volumeRoot

  const check = (p: string): Promise<boolean> =>
    timeoutMs > 0 ? withTimeout(checkFn(p), timeoutMs, false) : checkFn(p)

  let path = targetPath
  while (path !== '/' && path !== '') {
    if (await check(path)) {
      return path
    }
    // Don't walk above the volume root — that crosses into a different volume
    if (volumeRoot && path === volumeRoot) {
      break
    }
    // Go to parent
    const lastSlash = path.lastIndexOf('/')
    path = lastSlash > 0 ? path.substring(0, lastSlash) : '/'
  }
  // Try user home before falling back to root (~ is expanded by the backend)
  if (await check('~')) {
    return '~'
  }
  // Check root
  if (await check('/')) {
    return '/'
  }
  return null
}
