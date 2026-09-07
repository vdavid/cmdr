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
 * The scheme root of a path on a volume with no local mount
 * (`sftp://ada@nas:22/srv` → `sftp://ada@nas:22`), or `null` for a plain path.
 *
 * ❗ This is where the walk-up STOPS on such a path, and what it answers there.
 * Every `pathExists` probe on a remote path is false, so the plain walk chops the
 * scheme itself (`sftp:/`, then `sftp:`), falls through to `~`, and lands the
 * pane on the boot disk. `null` is the same failure spelled differently: four of
 * this module's callers hand it to `navigateToFallback`, which turns it into `~`
 * on the root volume. A restored server tab has to come back on its server.
 */
function schemeRootOf(path: string): string | null {
  const match = SCHEME_ROOT_RE.exec(path)
  return match ? match[0] : null
}

/**
 * `<scheme>:` plus its authority, up to the first path separator after it.
 *
 * ❗ The second slash is OPTIONAL, and that is the whole guard: a one-slash
 * `sftp:/srv/data` is not a shape anything in the app writes, but a guard keyed
 * on `://` misses it, and what it misses is not "an odd-looking path". The walk
 * chops it to `sftp:`, then to `/`, and lands the pane on the boot disk — exactly
 * the failure this module exists to prevent. Recognizing it costs one character
 * and stops the walk on the scheme either way. `app-status-store.ts`'s
 * never-probe test carries the same shape for the same reason.
 */
const SCHEME_ROOT_RE = /^[a-z][a-z\d+.-]*:\/\/?[^/]*/i

/**
 * Where the walk stops on a scheme path, and the answer once it gets there:
 * `null` for a plain path.
 *
 * ❗ The caller's `volumeRoot` wins when it is on the same scheme, because a
 * volume can sit BELOW its scheme root: an MTP storage is
 * `mtp://<device>/<storage>`, and stopping at `mtp://<device>` would leave the
 * pane off its own volume.
 */
function schemeFloorFor(targetPath: string, volumeRoot: string | undefined): string | null {
  const schemeRoot = schemeRootOf(targetPath)
  if (!schemeRoot) return null
  return volumeRoot && schemeRootOf(volumeRoot) === schemeRoot ? volumeRoot : schemeRoot
}

/**
 * Resolves a path to a valid existing path by walking up the parent tree.
 * Each step has a timeout to prevent hanging on dead mounts (default 1s).
 * Fallback chain: parent tree (up to volumeRoot) → user home (~) → filesystem root (/).
 * Returns null if even the root doesn't exist (volume unmounted).
 *
 * ❗ On a `<scheme>://` path the walk stops at the scheme root and RETURNS IT,
 * never `~`, `/`, or `null` (`schemeRootOf`).
 */
export async function resolveValidPath(targetPath: string, options?: ResolveValidPathOptions): Promise<string | null> {
  const checkFn = options?.pathExistsFn ?? pathExists
  const timeoutMs = options?.timeoutMs ?? 1000
  const volumeRoot = options?.volumeRoot

  const check = (p: string): Promise<boolean> =>
    timeoutMs > 0 ? withTimeout(checkFn(p), timeoutMs, false) : checkFn(p)

  const schemeFloor = schemeFloorFor(targetPath, volumeRoot)

  const walked = await walkUp(targetPath, check, { volumeRoot, schemeFloor })
  if (walked !== null) return walked
  // A scheme path stops here: `~` is on another volume entirely.
  if (schemeFloor) return schemeFloor
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

/**
 * Walks `targetPath` up its parents until something answers, returning that path
 * or `null` when the walk ran out of bounds without one.
 *
 * Two floors, and a scheme floor RETURNS rather than falling through: standing on
 * a server's root is the right answer for a place that has to be dialed before
 * anything under it can answer, while a local volume root that doesn't exist
 * means the volume is gone and the caller's `~` fallback is right.
 */
async function walkUp(
  targetPath: string,
  check: (path: string) => Promise<boolean>,
  bounds: { volumeRoot?: string; schemeFloor: string | null },
): Promise<string | null> {
  let path = targetPath
  while (path !== '/' && path !== '') {
    if (await check(path)) {
      return path
    }
    if (bounds.schemeFloor && path === bounds.schemeFloor) {
      return bounds.schemeFloor
    }
    // Don't walk above the volume root: that crosses into a different volume
    if (bounds.volumeRoot && path === bounds.volumeRoot) {
      break
    }
    // Go to parent
    const lastSlash = path.lastIndexOf('/')
    path = lastSlash > 0 ? path.substring(0, lastSlash) : '/'
  }
  return null
}
