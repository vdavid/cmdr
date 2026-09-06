/**
 * The volume a search-results pane's rows actually live on.
 *
 * A snapshot pane's own `volumeId` is the virtual `search-results`, which no
 * operation can be dispatched against, so every source-side op has to name a real
 * volume instead. It is NOT always the boot drive: a search covers exactly one
 * volume, and any volume with a persisted `index-{volume_id}.db` is searchable,
 * including an SMB share and an MTP storage (`src-tauri/src/search/volumes.rs`).
 * So the rows can sit on an exFAT stick that answers `supportsTrash: false`, or on
 * a volume whose deletes must route through its own backend rather than the local
 * filesystem.
 *
 * Resolution is the frontend half of `transfer-entry::resolveSourceVolumeId`:
 * longest-prefix match per path against the registered volume roots, favorites
 * excluded, and the answer only stands if every path agrees. Anything else is
 * `root`, the honest unknown. There is no backend round-trip here, unlike the drag
 * path: these paths came out of ONE volume's index, so the volume list settles it,
 * and staying synchronous keeps the delete and transfer openers synchronous.
 *
 * `supportsTrash` stays OPTIMISTIC on a miss (`!== false`, matching the normal
 * pane), because a `false` here forces the dialog into a permanent delete. A
 * resolution miss must not do that; a trash the backend can't perform fails
 * honestly and reversibly, a permanent delete does not.
 */

import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'
import { findVolumeIdForPath } from '../drag/drop-operation'
import type { VolumeInfo } from '../types'

/** What a snapshot pane's source-side ops need to know about where its rows live. */
export interface SnapshotSourceVolume {
  /** The volume id to dispatch against. `root` when the rows can't be placed. */
  volumeId: string
  /** Whether that volume has a trash to move rows into. */
  supportsTrash: boolean
}

/** Resolves [`SnapshotSourceVolume`] for a set of absolute snapshot row paths. */
export function resolveSnapshotSourceVolume(
  paths: readonly string[],
  volumes: readonly VolumeInfo[],
): SnapshotSourceVolume {
  const volumeId = resolveVolumeId(paths, volumes)
  const info = volumes.find((v) => v.id === volumeId)
  return { volumeId, supportsTrash: info?.supportsTrash !== false }
}

function resolveVolumeId(paths: readonly string[], volumes: readonly VolumeInfo[]): string {
  if (paths.length === 0) return DEFAULT_VOLUME_ID
  // Favorites are picker-only pseudo-volumes the backend can't dispatch against,
  // so a path under one belongs to its backing real volume.
  const realVolumes = volumes.filter((v) => v.category !== 'favorite')
  const first = findVolumeIdForPath(paths[0], realVolumes)
  if (first === null) return DEFAULT_VOLUME_ID
  return paths.every((p) => findVolumeIdForPath(p, realVolumes) === first) ? first : DEFAULT_VOLUME_ID
}
