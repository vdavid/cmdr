/**
 * Panes, tabs, their history, and remembered paths following a saved server to
 * its new address (`server-place-moved`). The move itself is the backend's
 * (`src-tauri/src/server_move.rs`): by the time this runs, the stores, the
 * password, and the favorites already name the new address, and the session at
 * the old one is gone.
 *
 * The twin of `volume-root-follow.ts`, which follows a place whose ROOT moved
 * under the same id. A move is exactly an id change: every place gets its new id,
 * and every path on it is respelled from the old address prefix to the new one,
 * by whole components. Applied in one order:
 *
 * 0. ❗ A sign-in sheet open for an old place closes as cancelled
 *    (`dismissSignInForPlaces`): its host-key step and its attempt would dial the
 *    OLD address. First, so the pane's redial below can open a sheet of its own.
 * 1. ❗ The volume store's row is re-keyed FIRST (`applyServerPlaceMoved`), and
 *    reads `saved`. The event beats the debounced `volumes-changed`, and
 *    `navigate()` looks the new id up in the list.
 * 2. Every tab's history is respelled in place, so Back from the moved place
 *    lands on it rather than on an id nothing knows.
 * 3. Each pane's active tab moves through `navigate()`, as a terminal
 *    `'fallback'` select with no history push. It lands on the `saved` row, and
 *    `place-connect` dials it there: the ordinary first open, so a new host's key
 *    or a missing password asks in the usual place. A place that kept its id
 *    (a WebDAV base path that moved) needs no navigate: its row going `saved` is
 *    what makes the pane dial again.
 * 4. A tab no pane is showing is respelled in place and its pane's tabs saved.
 * 5. `lastUsedPaths` moves to the new id.
 *
 * Each window's explorer subscribes on its own. Every step is idempotent, so two
 * windows following one move agree.
 */

import type { ServerPlaceMoved } from '$lib/ipc/bindings'
import type { Location } from '$lib/tauri-commands'
import { getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import type { NavigateIntent, NavigateResult } from './navigate'

export interface ServerMoveFollowDeps {
  /** Closes a sign-in sheet open for one of the old place ids. */
  dismissSignIn: (oldVolumeIds: string[]) => void
  /** Re-keys the volume store's rows to the new ids, roots, and names. */
  applyToVolumeList: (moved: ServerPlaceMoved) => void
  getTabMgr: (pane: 'left' | 'right') => TabManager
  navigate: (intent: NavigateIntent) => NavigateResult
  /** Persists one pane's tabs, for a tab no pane is showing. */
  saveTabs: (pane: 'left' | 'right') => void
  getLastUsedPath: (volumeId: string) => Promise<string | undefined>
  saveLastUsedPath: (record: Location) => Promise<void>
  forgetLastUsedPath: (volumeId: string) => Promise<void>
}

/** Moves every id and path held on the moved server's places to its new address. */
export async function followServerMove(moved: ServerPlaceMoved, deps: ServerMoveFollowDeps): Promise<void> {
  deps.dismissSignIn(moved.places.map((place) => place.oldVolumeId))
  deps.applyToVolumeList(moved)
  const newIdOf = new Map(moved.places.map((place) => [place.oldVolumeId, place.newVolumeId]))
  const respell = (location: Location): Location | null => {
    const volumeId = newIdOf.get(location.volumeId)
    if (volumeId === undefined) return null
    return { volumeId, path: pathAfterServerMove(location.path, moved.oldPrefix, moved.newPrefix) }
  }

  for (const pane of ['left', 'right'] as const) {
    const mgr = deps.getTabMgr(pane)
    const activeId = getActiveTab(mgr).id
    let movedBehind = false
    for (const tab of mgr.tabs) {
      for (const entry of tab.history.stack) {
        const next = respell(entry)
        if (next) Object.assign(entry, next)
      }
      const next = respell(tab)
      if (!next || (next.volumeId === tab.volumeId && next.path === tab.path)) continue
      if (tab.id === activeId) {
        deps.navigate({ pane, to: { selectVolume: next }, source: 'fallback', pushHistory: false })
      } else {
        // The same folder at a new address, so the row it remembered is still there.
        tab.volumeId = next.volumeId
        tab.path = next.path
        movedBehind = true
      }
    }
    if (movedBehind) deps.saveTabs(pane)
  }

  for (const place of moved.places) {
    const remembered = await deps.getLastUsedPath(place.oldVolumeId)
    if (remembered === undefined) continue
    const path = pathAfterServerMove(remembered, moved.oldPrefix, moved.newPrefix)
    if (place.newVolumeId === place.oldVolumeId && path === remembered) continue
    await deps.saveLastUsedPath({ volumeId: place.newVolumeId, path })
    if (place.newVolumeId !== place.oldVolumeId) await deps.forgetLastUsedPath(place.oldVolumeId)
  }
}

/**
 * `path` respelled from `oldPrefix` to `newPrefix`, or unchanged when it isn't on
 * the old address. ❗ By whole components, ❌ never a string prefix:
 * `sftp://ada@nas.local:2222` starts with `sftp://ada@nas.local:22` and is another
 * server.
 */
export function pathAfterServerMove(path: string, oldPrefix: string, newPrefix: string): string {
  const old = oldPrefix.replace(/\/+$/, '')
  if (path !== old && !path.startsWith(`${old}/`)) return path
  return newPrefix.replace(/\/+$/, '') + path.slice(old.length)
}
