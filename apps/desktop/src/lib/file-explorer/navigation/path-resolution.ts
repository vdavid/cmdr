/**
 * Walk-up path resolution utility.
 *
 * Lives in its own module (separate from `path-navigation.ts`) so that
 * `app-status-store.ts` can import it without forming a cycle:
 * `path-navigation.ts` imports from `app-status-store.ts` for
 * `getLastUsedPathForVolume`.
 */

import { pathExists, pathExistsChecked } from '#lib/tauri-commands/index.js'
import { withTimeout } from '#lib/utils/timing.js'
import type { ConnectionState } from '../types'
import { probeTimeoutMs } from './connection-state'
import { parentOrRoot } from '#lib/path/grammar.js'

export interface ResolveValidPathOptions {
  /**
   * Custom path-existence checker, asked about every rung (the volume's and `~` / `/`).
   * Defaults to the Tauri `pathExists` command.
   */
  pathExistsFn?: (path: string) => Promise<boolean>
  /**
   * Timeout per step in ms. Set to 0 to skip timeout wrapping. Defaults to 1000, stretched by
   * `connectionState` on the volume's own rungs (`probeTimeoutMs`).
   */
  timeoutMs?: number
  /**
   * The `volumeId` volume's session state. `direct` gives its rungs the session wait, so a busy
   * NAS's slow `stat` of a live parent isn't skipped as missing. `~` and `/` keep `timeoutMs`.
   */
  connectionState?: ConnectionState | null
  /**
   * Volume root path (like "/Volumes/naspi"). When set, the walk-up stops at this
   * boundary instead of continuing to "/" (prevents crossing into a different volume,
   * which would fail for non-local volumes like SmbVolume).
   */
  volumeRoot?: string
  /**
   * The volume the walk asks about its own rungs. ❗ Without it the backend asks
   * `root`, the boot disk, which says "gone" for every path on a phone or server,
   * so the walk lands on the scheme floor instead of the nearest parent that's
   * still there. `~` and `/` always go to the boot disk: they're its rungs.
   */
  volumeId?: string
  /**
   * When the walk reaches `volumeRoot` and the root couldn't tell (the step timeout, or the
   * backend's `timedOut`), answer the root instead of going on to `~` / `/`. For callers
   * that should STAY on the volume: a slow mount is still there, and leaving it showed the
   * boot disk's home folder as if it were the picked drive (cmdr-reports#4). A root that
   * says "gone" still falls through. ❌ Not for the SMB cancel and disconnect handlers,
   * which walk to LEAVE a volume that stopped answering.
   */
  keepSilentVolume?: boolean
}

/** One rung's answer. `unknown` is a probe that couldn't tell: a timeout, not a "no". */
type ProbeAnswer = 'yes' | 'no' | 'unknown'

/**
 * The scheme root of a path on a volume with no local mount
 * (`sftp://ada@nas:22/srv` → `sftp://ada@nas:22`), or `null` for a plain path.
 *
 * ❗ This is where the walk-up STOPS on such a path, and what it answers there.
 * A remote path's probes can all say no (asked of the boot disk, or of a server
 * that isn't connected), and then the plain walk chops the scheme itself
 * (`sftp:/`, then `sftp:`), falls through to `~`, and lands the pane on the boot
 * disk. `null` is the same failure spelled differently: four of this module's
 * callers hand it to `navigateToFallback`, which turns it into `~` on the root
 * volume. A restored server tab has to come back on its server.
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
 * Resolves a path to a valid existing path by walking up the parent tree, asking
 * `volumeId` about each parent. Each step has a timeout to prevent hanging on dead
 * mounts (default 1s, longer on a live session: `probeTimeoutMs`).
 * Fallback chain: parent tree (up to volumeRoot), then on the boot disk `/` → `~`, and past another
 * volume's root `~` → `/`.
 * Returns null if even the root doesn't exist (volume unmounted).
 *
 * ❗ On a `<scheme>://` path the walk stops at the scheme root and RETURNS IT,
 * never `~`, `/`, or `null` (`schemeRootOf`). With `keepSilentVolume`, a volume
 * root that couldn't tell is answered too, ❌ never walked past.
 */
export async function resolveValidPath(
  targetPath: string,
  options: ResolveValidPathOptions = {},
): Promise<string | null> {
  const timeoutMs = options.timeoutMs ?? 1000
  const volumeRoot = options.volumeRoot
  const onVolume = volumeProbe(options.pathExistsFn, options.volumeId)
  const onBootDisk = options.pathExistsFn ?? ((p: string) => pathExists(p))

  const onVolumeTimeoutMs = timeoutMs > 0 ? probeTimeoutMs(options.connectionState, timeoutMs) : 0

  const bounded = <T>(probe: Promise<T>, fallback: T, ms = timeoutMs): Promise<T> =>
    ms > 0 ? withTimeout(probe, ms, fallback) : probe

  const schemeFloor = schemeFloorFor(targetPath, volumeRoot)

  const walked = await walkUp(targetPath, (p) => bounded(onVolume(p), 'unknown', onVolumeTimeoutMs), {
    volumeRoot,
    schemeFloor,
    keepSilentRoot: options.keepSilentVolume === true,
  })
  if (walked !== null) return walked
  // A scheme path stops here: `~` is on another volume entirely.
  if (schemeFloor) return schemeFloor
  // On the boot disk, `/` is the nearest surviving parent of a deleted top-level folder, so it comes
  // before `~`. A walk that stopped at another volume's root (an ejected drive) has no parent on
  // that volume left, so home is the friendlier landing there (`~` is expanded by the backend).
  const fallbacks = !volumeRoot || volumeRoot === '/' ? ['/', '~'] : ['~', '/']
  for (const fallback of fallbacks) {
    if (await bounded(onBootDisk(fallback), false)) return fallback
  }
  return null
}

/**
 * Asks the volume about one rung. The default keeps the backend's `timedOut` as
 * `unknown` (`keepSilentVolume` reads it); an injected `pathExistsFn` answers
 * yes or no.
 */
function volumeProbe(
  pathExistsFn: ((path: string) => Promise<boolean>) | undefined,
  volumeId: string | undefined,
): (path: string) => Promise<ProbeAnswer> {
  if (pathExistsFn) return (p) => pathExistsFn(p).then((exists) => (exists ? 'yes' : 'no'))
  return (p) =>
    pathExistsChecked(p, volumeId).then(({ data, timedOut }) => (data ? 'yes' : timedOut ? 'unknown' : 'no'))
}

/**
 * Walks `targetPath` up its parents until something answers, returning that path
 * or `null` when the walk ran out of bounds without one.
 *
 * Two floors, and a scheme floor RETURNS rather than falling through: standing on
 * a server's root is the right answer for a place that has to be dialed before
 * anything under it can answer, while a local volume root that doesn't exist
 * means the volume is gone and the caller's `~` fallback is right.
 *
 * ❗ A probe that couldn't tell (the backend's `timedOut`, or the step timeout)
 * is SKIPPED, ❌ never landed on: the answer is where a caller navigates, so only
 * a "yes" may end the walk, and standing on a place that didn't answer re-fails
 * its listing further from where the user was than any parent that did answer.
 * The gate for "couldn't tell" sits BEFORE the walk instead: `listing-loader.ts`
 * and `deleted-dir-poll.ts` start one only after a confirmed miss, and the SMB
 * cancel and disconnect handlers walk to LEAVE a volume that stopped answering,
 * which stopping on it would defeat.
 *
 * The one opt-in exception is a silent volume ROOT (`keepSilentRoot`, the
 * caller's `keepSilentVolume`): it's the volume itself, not a parent, and past it
 * the walk leaves for another drive.
 */
async function walkUp(
  targetPath: string,
  check: (path: string) => Promise<ProbeAnswer>,
  bounds: { volumeRoot?: string; schemeFloor: string | null; keepSilentRoot: boolean },
): Promise<string | null> {
  let path = targetPath
  while (path !== '/' && path !== '') {
    const answer = await check(path)
    if (answer === 'yes') {
      return path
    }
    if (bounds.schemeFloor && path === bounds.schemeFloor) {
      return bounds.schemeFloor
    }
    // Don't walk above the volume root: that crosses into a different volume
    if (bounds.volumeRoot && path === bounds.volumeRoot) {
      return bounds.keepSilentRoot && answer === 'unknown' ? path : null
    }
    // Go to parent. A drive or share root (`C:/`) is its own parent: the walk ends there.
    const parent = parentOrRoot(path)
    if (parent === path) break
    path = parent
  }
  return null
}
