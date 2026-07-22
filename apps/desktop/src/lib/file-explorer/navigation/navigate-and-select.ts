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
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { isPathOnVolume } from './path-navigation'
import { resolveLocation } from './resolve-location'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'
import type { Location } from '$lib/tauri-commands'

const log = getAppLogger('navigation')

type Pane = 'left' | 'right'

/**
 * Resolve a directory path to a `Location` (volume id + path) at navigation's
 * edge, or show the shared "couldn't reach that location's drive" toast and
 * return `null`. The single resolve-or-toast used by the ⌘G, search-result, and
 * downloads edges (MCP returns a typed `ok: false` instead of toasting, so it
 * calls `resolveLocation` directly). `ResolveLocationResult` can't tell unmounted
 * from nonexistent, and the no-string-matching rule forbids sniffing, so every
 * failure shows the one generic toast.
 */
export async function resolveLocationOrToast(dir: string): Promise<Location | null> {
  const outcome = await resolveLocation(dir)
  if (!outcome.ok) {
    addToast(tString('fileExplorer.navigation.locationUnreachableToast'), { level: 'info' })
    return null
  }
  return outcome.location
}

/**
 * Reveal a search result's file in `pane`: resolve its parent dir to a `Location`
 * (the index isn't scoped to the pane's volume, so the result can live anywhere),
 * navigate there, then move the cursor onto the file. An unresolvable parent
 * shows the shared toast and skips navigation.
 */
export async function revealSearchResultInPane(explorer: ExplorerAPI, pane: Pane, filePath: string): Promise<void> {
  const lastSlash = filePath.lastIndexOf('/')
  const parentDir = lastSlash > 0 ? filePath.slice(0, lastSlash) : '/'
  const fileName = filePath.slice(lastSlash + 1)
  const location = await resolveLocationOrToast(parentDir)
  if (!location) return
  await navigateToFileInPane(explorer, pane, location, fileName)
}

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
 * Navigate `pane` to a directory `Location`. No cursor move — the directory's
 * own normal navigation lands the cursor on the 0th row (`..`). The caller has
 * already resolved the dir's volume (the edge resolver), so `navigate()` lands on
 * the right volume whether or not it matches the pane's current one.
 *
 * Returns whether the pane actually got there. A refusal leaves the pane on its
 * PREVIOUS directory, still holding a perfectly valid listing id, so a caller
 * that reads pane state afterwards can't tell the difference by inspecting it —
 * it has to check this. Callers that only move the user (go-to-path) can ignore
 * the result; the warning above is enough for them.
 */
export async function navigateToDirInPane(explorer: ExplorerAPI, pane: Pane, location: Location): Promise<boolean> {
  const result = explorer.navigate({ pane, to: { goTo: location }, source: 'user' })
  if (result.status === 'refused') {
    log.warn('navigateToDirInPane: navigate refused {pane} {dir}: {reason}', {
      pane,
      dir: location.path,
      reason: result.reason.message,
    })
    return false
  }
  await result.settled
  return true
}

/**
 * Navigate `pane` to a parent-dir `Location`, then move the cursor onto
 * `fileName` so the file is revealed/selected (we do NOT open it).
 */
export async function navigateToFileInPane(
  explorer: ExplorerAPI,
  pane: Pane,
  location: Location,
  fileName: string,
): Promise<void> {
  const result = explorer.navigate({ pane, to: { goTo: location }, source: 'user' })
  if (result.status === 'refused') {
    log.warn('navigateToFileInPane: navigate refused {pane} {parentDir}: {reason}', {
      pane,
      parentDir: location.path,
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
export async function revealFileInBestPane(explorer: ExplorerAPI, location: Location, fileName: string): Promise<void> {
  const reused = focusPaneShowing(explorer, location.path)
  if (reused) {
    await explorer.moveCursor(reused, fileName)
    return
  }
  await navigateToFileInPane(explorer, explorer.getFocusedPane(), location, fileName)
}

/**
 * Go to a directory `Location`, reusing a pane that already shows it (same
 * pane-pick as `revealFileInBestPane`, minus the cursor move — a bare directory
 * has no file target). Used by the empty-Downloads toast's "Go to Downloads"
 * action.
 */
export async function navigateToDirInBestPane(explorer: ExplorerAPI, location: Location): Promise<void> {
  if (focusPaneShowing(explorer, location.path)) return
  await navigateToDirInPane(explorer, explorer.getFocusedPane(), location)
}
