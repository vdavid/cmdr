/**
 * Shared navigation primitives for the "jump somewhere in the focused pane"
 * features: "Go to latest download" (⌘J) and "Go to path" (⌘G). Both want to
 * point a pane at a directory and (for files) land the cursor on a specific
 * entry, with the same careful handling of `navigate()`'s `NavigateResult`.
 *
 * `navigate()` returns `{ status: 'refused', reason }` when navigation can't
 * even start (a snapshot pane on a missing volume, the network/MTP refusals);
 * otherwise `{ status: 'started', settled }` where `settled` resolves when the
 * navigation settles. We await `settled` but report-and-bail on a refusal —
 * without the listing settled, `moveCursor` would race against an empty cache.
 *
 * NOTE (L2-adjacent): for the cross-volume snapshot arm, `settled` resolves
 * BEFORE the new listing loads (it resolves when the volume-switch commit is
 * done). `navigateToFileInPane` relies on `moveCursor`'s own internal
 * `whenLoadSettles` to bridge that gap — `settled` is the navigation-started
 * gate, `whenLoadSettles` is the real listing gate. Don't try to collapse them.
 */

import { getAppLogger } from '$lib/logging/logger'
import { isPathOnVolume } from './path-navigation'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'

const log = getAppLogger('navigation')

type Pane = 'left' | 'right'

/**
 * True when `pane`'s active tab already shows `dir` on a REAL local volume.
 *
 * Volume-safe, mirroring `commitPathFromListing`'s drop-foreign-listings policy:
 * a virtual volume (`network`, `search-results`) never counts, and a real volume
 * must actually contain the path (`isPathOnVolume`). An MTP or network pane that
 * happens to report a same-looking local path string thus never matches — its
 * volume mount path is an `mtp://…` / `smb://` URL the local dir isn't on.
 */
function paneShowsLocalDir(explorer: ExplorerAPI, pane: Pane, dir: string): boolean {
  const { volumeId, volumePath, path } = explorer.getPaneLocation(pane)
  if (volumeId === 'network' || volumeId === 'search-results') return false
  if (path !== dir) return false
  return isPathOnVolume(dir, volumePath)
}

/**
 * Navigate `pane` to a directory. No cursor move — the directory's own normal
 * navigation lands the cursor on the 0th row (`..`).
 */
export async function navigateToDirInPane(explorer: ExplorerAPI, pane: Pane, dir: string): Promise<void> {
  const result = explorer.navigate({ pane, to: { path: dir }, source: 'user' })
  if (result.status === 'refused') {
    log.warn('navigateToDirInPane: navigate refused {pane} {dir}: {reason}', {
      pane,
      dir,
      reason: result.reason.message,
    })
    return
  }
  await result.settled
}

/**
 * Navigate `pane` to `parentDir`, then move the cursor onto `fileName` so the
 * file is revealed/selected (we do NOT open it).
 */
export async function navigateToFileInPane(
  explorer: ExplorerAPI,
  pane: Pane,
  parentDir: string,
  fileName: string,
): Promise<void> {
  const result = explorer.navigate({ pane, to: { path: parentDir }, source: 'user' })
  if (result.status === 'refused') {
    log.warn('navigateToFileInPane: navigate refused {pane} {parentDir}: {reason}', {
      pane,
      parentDir,
      reason: result.reason.message,
    })
    return
  }
  await result.settled
  await explorer.moveCursor(pane, fileName)
}

/**
 * Pick the pane that already shows `dir` (focused first, then the other), focus
 * it, and return it. Returns `null` when neither pane shows `dir`, signalling the
 * caller to fall back to navigation. Re-evaluates pane contents live (don't cache
 * the result across an await — the empty-toast action runs this at click time).
 */
function focusPaneShowing(explorer: ExplorerAPI, dir: string): Pane | null {
  const focused = explorer.getFocusedPane()
  if (paneShowsLocalDir(explorer, focused, dir)) return focused

  const other: Pane = focused === 'left' ? 'right' : 'left'
  if (paneShowsLocalDir(explorer, other, dir)) {
    explorer.setFocusedPane(other)
    return other
  }
  return null
}

/**
 * Reveal `fileName` in `parentDir`, reusing a pane that already shows the dir
 * instead of duplicating the view. Used by the downloads "jump to file" flow
 * (⌘J, the toast, the global hotkey). Picks the target pane in priority order:
 *
 * 1. Focused pane already shows `parentDir` → no navigation, just move the cursor.
 * 2. Other pane already shows `parentDir` → shift focus there, move the cursor.
 * 3. Neither → navigate the focused pane to `parentDir`, then move the cursor.
 *
 * "Already shows" is volume-safe and the active tab only (see `paneShowsLocalDir`).
 * The shared `navigateToFileInPane` primitive keeps its always-navigate-the-given-
 * pane contract for "Go to path" (⌘G); only the downloads flow reuses panes.
 */
export async function revealFileInBestPane(explorer: ExplorerAPI, parentDir: string, fileName: string): Promise<void> {
  const reused = focusPaneShowing(explorer, parentDir)
  if (reused) {
    await explorer.moveCursor(reused, fileName)
    return
  }
  await navigateToFileInPane(explorer, explorer.getFocusedPane(), parentDir, fileName)
}

/**
 * Go to `dir`, reusing a pane that already shows it (same pane-pick as
 * `revealFileInBestPane`, minus the cursor move — a bare directory has no file
 * target). Used by the empty-Downloads toast's "Go to Downloads" action.
 */
export async function navigateToDirInBestPane(explorer: ExplorerAPI, dir: string): Promise<void> {
  if (focusPaneShowing(explorer, dir)) return
  await navigateToDirInPane(explorer, explorer.getFocusedPane(), dir)
}
