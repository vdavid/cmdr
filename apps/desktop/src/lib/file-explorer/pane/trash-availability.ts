/**
 * Whether a pane has a Trash to move a file into, which is the one question every
 * surface offering "…and trash the old one" has to ask first.
 *
 * Two facts decide it, and both are per-PANE rather than per-volume: the volume's
 * own `supportsTrash` (a network mount, a FAT stick, and a phone all answer no),
 * and whether the path sits at or inside an archive, where the backend refuses to
 * trash and there is no `.Trash` to refuse into. The archive half is
 * `pathCrossesArchiveBoundary`, the PANE-path predicate (`pane/CLAUDE.md`).
 *
 * **Unknown means yes.** A volume missing from the list answers `true`, matching
 * `resolveSnapshotSourceVolume`: a trash the backend can't perform fails honestly
 * and reversibly, while a `false` here pushes the surface into a permanent delete
 * that nothing can undo. Guessing in the safe direction is the whole point.
 *
 * ❗ This answers CAN it, never SHOULD it. A caller with its own reason to force a
 * permanent delete (an online-only cloud file, whose trash would download it
 * first) folds that in on top.
 */

import type { VolumeInfo } from '../types'
import { pathCrossesArchiveBoundary } from './archive-paths'

/** Whether the pane at `panePath` on `volumeId` can trash rather than delete. */
export function paneOffersTrash(volumeId: string, panePath: string, volumes: readonly VolumeInfo[]): boolean {
  if (pathCrossesArchiveBoundary(panePath)) return false
  return volumes.find((v) => v.id === volumeId)?.supportsTrash !== false
}
