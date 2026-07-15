// Which shape of indexing checklist a volume's run takes, keyed on the volume's
// `category` from the shared volume store (the same source `active-media-volume.ts`
// reads). This is the FE half of the run-kind decision: `deriveSteps` picks the
// ordered step list off `IndexRunKind`, and this predicate decides local vs network.

import type { VolumeInfo } from '$lib/file-explorer/types'
import { ROOT_VOLUME_ID } from './index-state.svelte'

/**
 * Whether a volume indexes over the NETWORK pipeline (its checklist is Find files
 * → Compute folder sizes, with no Save-the-file-list or Catch-up steps) rather
 * than the LOCAL one.
 *
 * Keyed on the volume's `category`, NOT `volumeId !== root`: `network` (SMB) and
 * `mobile_device` (MTP) index inline over a trait scanner (no `saving_entries`
 * sub-phase, no top-level Reconcile phase), so they take the network checklist.
 * Every local drive runs the full jwalk + FSEvents pipeline and takes the local
 * checklist — `main_volume` (the boot disk), `attached_volume` (a USB stick / SD
 * card), and `cloud_drive`. This is the whole point of the category switch: a
 * non-root local drive gets the Save + Catch-up steps, not the wrong network
 * shape a `volumeId !== root` test would have handed it.
 *
 * Falls back to LOCAL when the volume isn't in the list: the boot disk (`root`)
 * is always local, and a network drive that vanishes mid-abort clears its own
 * checklist row (the abort event drops the activity), so the fallback window is
 * inert.
 */
export function isNetworkIndexRun(volumeId: string, volumes: VolumeInfo[]): boolean {
  if (volumeId === ROOT_VOLUME_ID) return false
  const category = volumes.find((v) => v.id === volumeId)?.category
  return category === 'network' || category === 'mobile_device'
}
