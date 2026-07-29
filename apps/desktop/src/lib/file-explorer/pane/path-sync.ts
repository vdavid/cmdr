/**
 * What a pane does when its props move under it: the parent re-renders it with a
 * new `initialPath`, a new `volumeId`, or a cleared `unreachable` flag, and the
 * pane has to decide whether that means "load this directory", "just remember
 * the path", or "nothing".
 *
 * Both decisions are pure so the truth table is checkable. The `$effect`s in
 * `FilePane.svelte` read the props and apply the answer.
 */

import { isMtpVolumeId } from '$lib/mtp'
import type { UnreachableState } from '../tabs/tab-types'

export interface InitialPathSyncInput {
  /** The path the parent wants this pane on. */
  initialPath: string
  /** Where the pane actually is (user navigation moves this without the prop). */
  currentPath: string
  /** The volume id from the previous run of this decision. */
  prevVolumeId: string
  volumeId: string
  isSearchResultsView: boolean
  isNetworkView: boolean
  isMtpDeviceOnly: boolean
}

export type InitialPathAction =
  /** An MTP device finished connecting; load the path on the now-browsable volume. */
  | { kind: 'mtp-connected'; path: string }
  /** Commit the path and load its listing. */
  | { kind: 'load'; path: string }
  /** Commit the path only: this pane's data doesn't come from a listing (yet). */
  | { kind: 'sync-path'; path: string }
  | { kind: 'none' }

/**
 * One decision for two overlapping triggers (persistence restore and MTP
 * connection completion), so they can't both fire a `loadDirectory` for the same
 * change. The MTP arm takes priority: the device just became browsable, so it
 * loads even at an unchanged path.
 */
export function resolveInitialPathAction(input: InitialPathSyncInput): InitialPathAction {
  const { initialPath, currentPath } = input

  // Case 1: MTP device just connected (device-only → storage-specific).
  const wasDeviceOnly = isMtpVolumeId(input.prevVolumeId) && !input.prevVolumeId.includes(':')
  const isNowConnected = isMtpVolumeId(input.volumeId) && input.volumeId.includes(':')
  if (wasDeviceOnly && isNowConnected) {
    return { kind: 'mtp-connected', path: initialPath }
  }

  if (initialPath === currentPath) return { kind: 'none' }

  // Case 2: search-results panes get their data from the snapshot store, not a
  // real listing, so we sync `currentPath` without a backend `list_directory`.
  if (input.isSearchResultsView) return { kind: 'sync-path', path: initialPath }

  // Case 3: device-only MTP syncs the path only; the auto-connect flow handles
  // the transition to a browsable storage volume.
  if (input.isMtpDeviceOnly) return { kind: 'sync-path', path: initialPath }

  // The network view owns its own data (NetworkBrowser / ShareBrowser).
  if (input.isNetworkView) return { kind: 'none' }

  return { kind: 'load', path: initialPath }
}

export interface ReachableAgainInput {
  /** The `unreachable` value from the previous run of this decision. */
  prevUnreachable: UnreachableState | null
  unreachable: UnreachableState | null
  initialPath: string
  currentPath: string
}

/**
 * A tab whose volume timed out at startup shows the unreachable banner; a
 * successful Retry clears it and nothing else would trigger the listing load.
 *
 * Only when the path stayed the same: the banner's "Open home folder" recovery
 * changes `initialPath`, and the path decision above already loads that.
 */
export function shouldReloadAfterReachable(input: ReachableAgainInput): boolean {
  return input.prevUnreachable !== null && input.unreachable === null && input.initialPath === input.currentPath
}
