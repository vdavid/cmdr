/**
 * Walk-up path resolution utility.
 *
 * Lives in its own module (separate from `path-navigation.ts`) so that
 * `app-status-store.ts` can import it without forming a cycle:
 * `path-navigation.ts` imports from `app-status-store.ts` for
 * `getLastUsedPathForVolume`.
 */

import { pathExists } from '$lib/tauri-commands'
import { withTimeout } from '$lib/utils/timing'

export interface ResolveValidPathOptions {
  /** Custom path-existence checker. Defaults to the Tauri `pathExists` command. */
  pathExistsFn?: (path: string) => Promise<boolean>
  /** Timeout per step in ms. Set to 0 to skip timeout wrapping. Defaults to 1000. */
  timeoutMs?: number
  /**
   * Volume root path (like "/Volumes/naspi"). When set, the walk-up stops at this
   * boundary instead of continuing to "/" (prevents crossing into a different volume,
   * which would fail for non-local volumes like SmbVolume).
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
    // Don't walk above the volume root: that crosses into a different volume
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
