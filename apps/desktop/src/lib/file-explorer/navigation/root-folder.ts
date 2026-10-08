/**
 * Where Go > Root folder (`nav.goToRoot`, ⌘/) takes a pane: the root of the place
 * the pane SHOWS, which isn't always its `volumeId`'s root.
 *
 * - Inside an archive, the archive's root. The ROUTED pane keeps the parent drive's
 *   `volumeId`, so the volume root would throw the user out of the archive.
 * - Otherwise the root of `paneVolumeOf`'s answer, the volume the header names: a
 *   boot-disk pane that walked into `/Volumes/USB` goes to `/Volumes/USB`, not `/`.
 *   When that root isn't on the pane's own volume, the own volume's root.
 * - A phone or server goes to its root, never its landing (`/sdcard` on Android).
 *
 * `rootFolderOf` is the pure rule; `goToRootFolder` asks the backend which volume
 * holds the pane's path, then navigates.
 */

import { isPathOnVolume } from '$lib/path/canonical'
import { resolvePathVolume } from '$lib/tauri-commands'
import type { NavigateIntent } from '../pane/navigate'
import { archiveRootOf } from '../pane/archive-paths'
import type { VolumeInfo } from '../types'
import { paneVolumeOf } from './pane-volume'

/** Views with no folder tree behind them, so no root to go to. */
const ROOTLESS_VOLUME_IDS = new Set(['network', 'search-results'])

export interface RootFolderQuery {
  volumes: readonly VolumeInfo[]
  volumeId: string
  path: string
  /** The volume the backend says holds `path`, or `null` when unknown. */
  containingVolumeId: string | null
}

/** The root folder for a pane at `path`, or `null` when there's none to go to. */
export function rootFolderOf({ volumes, volumeId, path, containingVolumeId }: RootFolderQuery): string | null {
  if (ROOTLESS_VOLUME_IDS.has(volumeId)) return null
  const archiveRoot = archiveRootOf(path)
  if (archiveRoot !== null) return archiveRoot
  // A favorite's path is no volume's root.
  const own = volumes.find((v) => v.id === volumeId)
  if (!own || own.category === 'favorite') return null
  // ❗ The answer must stay on the pane's OWN volume: the pane drops a listing that
  // isn't (`commitPathFromListing`), and another volume's id would take the switch
  // arm, whose best-path correction trades a root for the remembered folder.
  const shown = paneVolumeOf(volumes, volumeId, path, containingVolumeId)
  return shown && isPathOnVolume(shown.path, own.path) ? shown.path : own.path
}

export interface GoToRootFolderDeps {
  getVolumes: () => readonly VolumeInfo[]
  getPaneVolumeId: (pane: 'left' | 'right') => string
  getPanePath: (pane: 'left' | 'right') => string
  navigate: (intent: NavigateIntent) => unknown
}

/**
 * Sends `pane` to its root folder. A no-op when there's none, when the pane is
 * already there, or when the pane moved on while the backend answered.
 */
export async function goToRootFolder(deps: GoToRootFolderDeps, pane: 'left' | 'right'): Promise<void> {
  const volumeId = deps.getPaneVolumeId(pane)
  const path = deps.getPanePath(pane)
  const containingVolumeId = ROOTLESS_VOLUME_IDS.has(volumeId)
    ? null
    : ((await resolvePathVolume(path)).volume?.id ?? null)
  const root = rootFolderOf({ volumes: deps.getVolumes(), volumeId, path, containingVolumeId })
  if (root === null || root === path) return
  if (deps.getPaneVolumeId(pane) !== volumeId || deps.getPanePath(pane) !== path) return
  // Same `volumeId`, so `navigate` takes the in-place arm: history, the pinned-tab
  // fork, and the drop-foreign-listings policy all apply as for any folder open.
  deps.navigate({ pane, to: { goTo: { volumeId, path: root } }, source: 'user' })
}
