/**
 * Pure utility functions for path navigation logic.
 * Extracted from DualPaneExplorer.svelte to improve modularity.
 *
 * All pathExists calls use frontend timeouts to prevent hangs on slow/unresponsive volumes.
 * The Rust backend also enforces its own timeout per pathExists call: 2 s, or 10 s on a
 * volume with a live session.
 */

import { constructAdbPath, parseAdbPath } from '$lib/adb/adb-path-utils'
import { isPathOnVolume } from '$lib/path/canonical'
import { pathExists } from '$lib/tauri-commands'
import { getLastUsedPathForVolume } from '$lib/app-status-store'
import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'
import { withTimeout } from '$lib/utils/timing'
import type { ConnectionState } from '../types'
import { probeTimeoutMs } from './connection-state'

export { withTimeout }

export interface OtherPaneState {
  otherPaneVolumeId: string
  otherPanePath: string
}

/** Arguments for `determineNavigationPath`: the switching volume plus the other pane's state. */
export interface DetermineNavigationPathArgs {
  volumeId: string
  volumePath: string
  targetPath: string
  otherPane: OtherPaneState
  /**
   * Where the volume lands when nothing is remembered about it, when that isn't
   * `volumePath`: a server place's start folder (`VolumeInfo.landingPath`).
   */
  landingPath?: string | null
  /**
   * The switched-to volume's session state. `direct` waits out a busy server's
   * slow `stat` (`probeTimeoutMs`), or the pane lands on the share root instead
   * of the folder the user left there.
   */
  connectionState?: ConnectionState | null
}

/**
 * Determines which path to navigate to when switching volumes.
 * Runs checks in parallel with 500ms frontend timeouts per check (longer on a live
 * session, `probeTimeoutMs`). The switch itself already landed on the volume root,
 * so the wait holds up only this background correction, never the UI.
 * Priority order:
 * 1. Favorite path (if targetPath !== volumePath)
 * 2. Other pane's path (if the other pane is on the same volume)
 * 3. Stored lastUsedPath for this volume
 * 4. Default: ~ for main volume, the volume's landing for others (`firstLandingOn`)
 */
export async function determineNavigationPath(args: DetermineNavigationPathArgs): Promise<string> {
  const { volumeId, volumePath, targetPath, otherPane, landingPath, connectionState } = args
  const pathExistsTimeoutMs = probeTimeoutMs(connectionState, 500)

  // User navigated to a favorite, so go to the favorite's path directly
  if (targetPath !== volumePath) {
    return targetPath
  }

  // Run both checks in parallel with timeouts, asking the volume being switched to:
  // without its id the backend asks the boot disk, which says "gone" for every path
  // on a phone or server.
  const [otherPaneValid, lastUsedResult] = await Promise.all([
    otherPane.otherPaneVolumeId === volumeId
      ? withTimeout(pathExists(otherPane.otherPanePath, volumeId), pathExistsTimeoutMs, false)
      : Promise.resolve(false),
    getLastUsedPathForVolume(volumeId).then((p) =>
      p && isPathOnVolume(p, volumePath)
        ? withTimeout(pathExists(p, volumeId), pathExistsTimeoutMs, false).then((ok) => (ok ? p : null))
        : null,
    ),
  ])

  if (otherPaneValid) return otherPane.otherPanePath
  if (lastUsedResult) return lastUsedResult

  // Default: ~ for main volume (root), the volume's landing for others
  return volumeId === DEFAULT_VOLUME_ID ? '~' : firstLandingOn(volumePath, landingPath)
}

/**
 * Where a volume with nothing remembered about it opens: its landing when it has
 * one, else its root.
 *
 * ❗ A server place's landing is the start folder its owner chose
 * (`VolumeInfo.landingPath`, minted in Rust). The ROOT stays the ceiling, one
 * Backspace away, and a remembered path (arm 3) still wins over it.
 *
 * ❗ A phone opens at `/sdcard`, not at `/`: an Android device root is a kernel
 * filesystem (`acct`, `apex`, `proc`, forty entries a person mostly cannot
 * read), and the user's own files live one level in. The ROOT is unchanged and
 * one Backspace away, and the breadcrumb shows it, so nothing is hidden — this
 * is a landing rule, ❌ never a different volume root.
 *
 * ❌ `/data` is deliberately NOT hidden or special-cased anywhere: it answers
 * `PermissionDenied` on a locked phone and is exactly what someone came for on a
 * rooted or debuggable one.
 *
 * MTP is not folded in: an MTP volume is already rooted at one STORAGE, so its
 * root is the media tree rather than a kernel filesystem.
 */
function firstLandingOn(volumePath: string, landingPath: string | null | undefined): string {
  if (landingPath) return landingPath
  const parsed = parseAdbPath(volumePath)
  if (!parsed || parsed.path !== '') return volumePath
  return constructAdbPath(parsed.serial, ADB_FIRST_LANDING)
}

/** Where the user's own files are on every Android device. */
const ADB_FIRST_LANDING = 'sdcard'
