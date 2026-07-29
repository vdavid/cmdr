/**
 * Detects that the directory a pane is showing was deleted behind its back, and
 * walks up to the nearest valid parent.
 *
 * It's a poll because macOS FSEvents doesn't report the watched directory's own
 * deletion, so nothing pushes this to us. Two guardrails keep it from evicting a
 * user who's fine: two CONSECUTIVE confirmed misses are required (a single one
 * can be a rename-in-flight or a hiccup), and an "I don't know" answer (syscall
 * timeout, or an SMB volume in `Disconnected`) resets the counter rather than
 * counting against the directory.
 */

import { pathExistsChecked } from '$lib/tauri-commands'
import { isVirtualGitPath } from '../git/path-detection'
import { resolveValidPath } from '../navigation/path-resolution'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

/** How often we ask whether the current directory is still there. */
const POLL_INTERVAL_MS = 2000
/** Consecutive confirmed misses required before navigating away. */
const MISSES_BEFORE_FALLBACK = 2

export interface DeletedDirPollDeps {
  getListingId: () => string
  getLoading: () => boolean
  /** Whether the pane's volume kind has a real backend listing (off `caps`). */
  getHasBackendListing: () => boolean
  getIsMtpView: () => boolean
  getCurrentPath: () => string
  /** The pane volume's mount point, or `/` for the root volume. */
  getVolumePath: () => string
  /** Navigate to the resolved surviving ancestor (`null` when even `/` is gone). */
  navigateToFallback: (path: string | null) => void
}

export interface DeletedDirPoll {
  /** Begin polling. Call from `onMount`. */
  start: () => void
  /** Stop polling. Call from `onDestroy`. */
  stop: () => void
}

export function createDeletedDirPoll(deps: DeletedDirPollDeps): DeletedDirPoll {
  let interval: ReturnType<typeof setInterval> | undefined
  let notExistsCount = 0

  function walkUp(currentPath: string, volumePath: string): void {
    void resolveValidPath(currentPath, { volumeRoot: volumePath }).then((validPath) => {
      deps.navigateToFallback(validPath)
    })
  }

  function poll(): void {
    // Network / search-results panes have no real `currentPath` on disk
    // to poll — that folds into `!hasBackendListing`. The MTP skip STAYS:
    // MTP has a backend listing (`hasBackendListing: true`) but no real
    // on-disk path for `pathExists` to stat, so it's an MTP-path-specific
    // skip, not a capability question.
    if (!deps.getListingId() || deps.getLoading() || !deps.getHasBackendListing() || deps.getIsMtpView()) return
    const currentPath = deps.getCurrentPath()
    // Virtual `.git/<category>/...` paths don't exist on disk, so
    // `pathExists` always returns false and the poll would evict
    // the user back to `.git/`. The git watcher keeps these
    // listings fresh via `git-state-changed` and the
    // `directory-diff` events from `invalidate_virtual_listings`.
    if (isVirtualGitPath(currentPath)) return

    void pathExistsChecked(currentPath).then(({ data: exists, timedOut }) => {
      // `timedOut` covers both a 2s syscall timeout and an SMB volume in
      // `Disconnected` state: in both cases we don't know whether the path
      // exists. Reset the counter and wait for the connection to recover.
      if (timedOut || exists) {
        notExistsCount = 0
        return
      }

      notExistsCount++
      if (notExistsCount < MISSES_BEFORE_FALLBACK) return

      const volumePath = deps.getVolumePath()
      // On an external volume, check whether the volume root itself is gone.
      // If so, skip: the volume unmount handler will manage the transition.
      if (volumePath !== '/') {
        void pathExistsChecked(volumePath).then(({ data: volumeExists, timedOut: volumeTimedOut }) => {
          // If we couldn't tell whether the volume is there, don't walk up.
          if (volumeTimedOut) return
          if (!volumeExists) return
          log.info('Directory {dir} no longer exists, navigating to nearest valid parent under {volume}', {
            dir: currentPath,
            volume: volumePath,
          })
          walkUp(currentPath, volumePath)
        })
      } else {
        log.info('Directory {dir} no longer exists, navigating to nearest valid parent', { dir: currentPath })
        walkUp(currentPath, volumePath)
      }
    })
  }

  return {
    start: () => {
      interval = setInterval(poll, POLL_INTERVAL_MS)
    },
    stop: () => {
      clearInterval(interval)
      interval = undefined
    },
  }
}
