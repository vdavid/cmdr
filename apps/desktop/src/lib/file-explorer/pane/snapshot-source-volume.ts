/**
 * The volume a search-results pane's rows live on, and what a source-side op needs
 * to know about it.
 *
 * A snapshot pane's own `volumeId` is the virtual `search-results`, which no
 * operation can be dispatched against, so every source-side op names a real volume
 * instead. That volume is the one the search covered, and the snapshot carries it
 * (`SearchSnapshot.volumeId`, as the backend routed the run): a search covers
 * exactly one volume, and it can be an SMB share, an MTP storage, or a phone over
 * ADB as easily as the boot drive (`src-tauri/src/search/volumes.rs`).
 *
 * ❌ Never re-derive it from the rows' paths. A prefix match against the volume list
 * falls back to `root` the moment the volume leaves the list (a phone unplugged under
 * an open results pane), and a delete or move dispatched against `root` for `adb://`
 * rows goes down the local-filesystem path.
 *
 * `supportsTrash` stays OPTIMISTIC when the volume isn't in the list (`!== false`,
 * matching the normal pane), because a `false` here forces the dialog into a
 * permanent delete. A trash the backend can't perform fails honestly and reversibly;
 * a permanent delete does not.
 */

import type { VolumeInfo } from '../types'

/** What a snapshot pane's source-side ops need to know about where its rows live. */
export interface SnapshotSourceVolume {
  /** The volume id to dispatch against: the one the snapshot's search covered. */
  volumeId: string
  /** Whether that volume has a trash to move rows into. */
  supportsTrash: boolean
}

/** Resolves [`SnapshotSourceVolume`] for the volume a snapshot's search covered. */
export function resolveSnapshotSourceVolume(volumeId: string, volumes: readonly VolumeInfo[]): SnapshotSourceVolume {
  const info = volumes.find((v) => v.id === volumeId)
  return { volumeId, supportsTrash: info?.supportsTrash !== false }
}
