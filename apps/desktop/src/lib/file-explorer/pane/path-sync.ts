/**
 * What a pane does when its props move under it: the parent re-renders it with a
 * new `initialPath`, a new `volumeId`, or a cleared `unreachable` flag, and the
 * pane has to decide whether that means "load this directory", "just remember
 * the path", or "nothing".
 *
 * Both decisions are pure so the truth table is checkable. The `$effect`s in
 * `FilePane.svelte` read the props and apply the answer.
 */

import type { UnreachableState } from '../tabs/tab-types'

export interface InitialPathSyncInput {
  /** The path the parent wants this pane on. */
  initialPath: string
  /** Where the pane actually is (user navigation moves this without the prop). */
  currentPath: string
  isSearchResultsView: boolean
  isNetworkView: boolean
  /**
   * Whether `device-connect.svelte.ts` is holding this pane while it opens a
   * phone. ❗ A load here would dial the SAME phone a second time through
   * `resolve_path_to_volume`, and the pane's Cancel aims at the other one.
   */
  deviceIsConnecting: boolean
}

export type InitialPathAction =
  /** Commit the path and load its listing. */
  | { kind: 'load'; path: string }
  /** Commit the path only: this pane's data doesn't come from a listing (yet). */
  | { kind: 'sync-path'; path: string }
  | { kind: 'none' }

/** What a new `initialPath` prop means for the pane: load it, only commit it, or nothing. */
export function resolveInitialPathAction(input: InitialPathSyncInput): InitialPathAction {
  const { initialPath, currentPath } = input

  if (initialPath === currentPath) return { kind: 'none' }

  // Search-results panes get their data from the snapshot store, not a real
  // listing, so we sync `currentPath` without a backend `list_directory`.
  if (input.isSearchResultsView) return { kind: 'sync-path', path: initialPath }

  // A phone being opened over ADB: what resumes the load is the connect
  // factory's own `onConnected`, at the path this arm commits.
  if (input.deviceIsConnecting) return { kind: 'sync-path', path: initialPath }

  // The network view owns its own data (ServersHub / PlacesBrowser), so nothing
  // loads, ❗ but the path still commits: a pane that came from a share kept that
  // share's path, and its header read "Servers ▸ /Volumes/public".
  if (input.isNetworkView) return { kind: 'sync-path', path: initialPath }

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
