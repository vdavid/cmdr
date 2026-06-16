/**
 * The shared transfer entry seam: one guard chain and one source-volume
 * resolver that every transfer entry path (F5/F6, drag-and-drop, clipboard
 * paste) runs through, so the three paths prepare a transfer identically.
 *
 * Why this module exists: historically only F5/F6 ran the destination guards
 * (read-only alert, search-results refusal) and only F5/F6 carried a real
 * source volume id. Drag-and-drop hardcoded `sourceVolumeId = destVolumeId` (a
 * placeholder), so dropping MTP↔local stat'd the wrong volume and the dialog
 * showed 0 bytes / 0 files; and neither drop nor paste warned before a doomed
 * write into a read-only destination. Funneling all three through these two
 * functions removes both classes of bug at the source.
 *
 * Both functions are pure (the resolver takes `resolvePathVolume` as a dep), so
 * they're unit-testable without a running app.
 */
import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from '$lib/search/capabilities'
import { findVolumeIdForPath } from '../drag/drop-operation'
import { getCommonParentPath, getDestinationVolumeInfo } from './transfer-operations'
import { capabilitiesFor } from './volume-capabilities'
import { tString } from '$lib/intl/messages.svelte'
import type { PathVolumeResolution } from '$lib/tauri-commands'
import type { VolumeInfo } from '../types'

/** A blocking alert (read-only destination). */
export interface TransferGuardAlert {
  title: string
  message: string
}

/** A blocking toast (search-results destination — not a real folder). */
export interface TransferGuardToast {
  message: string
  level: 'warn'
}

/**
 * Outcome of the destination guard. `ok: true` means the transfer may proceed.
 * Otherwise exactly one of `alert` / `toast` carries the user-facing refusal the
 * caller surfaces through its own dialog/toast plumbing. The copy is the
 * E2E-asserted contract — never reword it here without updating the specs.
 */
export type TransferGuardResult =
  | { ok: true }
  | { ok: false; alert: TransferGuardAlert; toast?: undefined }
  | { ok: false; toast: TransferGuardToast; alert?: undefined }

/**
 * Runs the destination guards shared by every transfer entry path. Order
 * matches the original F5/F6 opener:
 *
 * 1. Search-results destination → not a folder, refuse with a toast. Gated on
 *    `!canPasteInto` SCOPED to the `search-results` kind so the wording stays
 *    correct (a network destination shares the `false` capability but isn't a
 *    misrendered "not a folder").
 * 2. Read-only destination → refuse with an alert. Read off the destination's
 *    `VolumeInfo.isReadOnly` (a per-volume runtime flag, not a kind capability).
 *
 * Returns `ok` when neither fires. An unknown destination volume id (no
 * `VolumeInfo`) is allowed through: we can't prove it's read-only, the backend
 * still rejects a genuinely read-only write, and blocking on "unknown" would
 * break legitimate transfers to a freshly-mounted volume.
 */
export function checkTransferDestinationGuard(destVolumeId: string, volumes: VolumeInfo[]): TransferGuardResult {
  const destCaps = capabilitiesFor(destVolumeId)
  if (!destCaps.canPasteInto && destCaps.kind === 'search-results') {
    return { ok: false, toast: { message: SEARCH_RESULTS_NOT_A_FOLDER_TOAST, level: 'warn' } }
  }

  const destVolume = getDestinationVolumeInfo(destVolumeId, volumes)
  if (destVolume?.isReadOnly) {
    return {
      ok: false,
      alert: {
        title: tString('fileExplorer.readOnly.deviceTitle'),
        message: tString('fileExplorer.readOnly.deviceMessage', { name: destVolume.name }),
      },
    }
  }

  return { ok: true }
}

/**
 * Resolves the real source volume id for a set of source paths, so a dropped or
 * pasted transfer carries the same accurate `sourceVolumeId` an F5/F6 transfer
 * does. NEVER returns a knowingly-wrong id: when resolution is genuinely
 * ambiguous (sources span volumes) or fails, it returns `DEFAULT_VOLUME_ID`
 * (root) — the honest "unknown", which gives today's degraded-but-correct
 * behavior rather than stat'ing the wrong volume.
 *
 * Favorites (`category === 'favorite'`) are EXCLUDED from the candidate set:
 * they're pseudo-volumes that exist only in the volume picker, the backend
 * VolumeManager has no record of them, so resolving a path under a favorite's
 * root to its `fav-*` id makes dispatch fail with "Source volume 'fav-…' not
 * found". A favorite is a location on its BACKING volume (the local fs), so a
 * path under `~/Desktop` must resolve to `root`, not `fav-desktop`. Filtering
 * them out lets longest-prefix fall through to the real local root.
 *
 * Resolution order:
 * 1. Frontend longest-prefix match (`findVolumeIdForPath`) per path against the
 *    BACKEND-REAL volume roots (favorites filtered out) — handles local AND
 *    MTP-shaped paths (MTP volumes
 *    register an `mtp://…` root). If every path matches the SAME volume, use it
 *    (no backend round-trip).
 * 2. If the per-path matches DISAGREE, the sources span volumes — return root.
 * 3. If NO path matched a registered root (all `null`), ask the backend to
 *    resolve the common parent via `resolve_path_volume` (statfs / protocol
 *    dispatch). Use its volume id, else root.
 */
export async function resolveSourceVolumeId(
  paths: string[],
  volumes: readonly VolumeInfo[],
  resolvePathVolume: (path: string) => Promise<PathVolumeResolution>,
): Promise<string> {
  if (paths.length === 0) return DEFAULT_VOLUME_ID

  // Favorites are picker-only pseudo-volumes the backend can't dispatch against;
  // a path under one belongs to its backing real volume, so they never qualify
  // as a resolution candidate.
  const realVolumes = volumes.filter((v) => v.category !== 'favorite')

  const perPath = paths.map((p) => findVolumeIdForPath(p, realVolumes))
  const matched = perPath.filter((id): id is string => id !== null)

  if (matched.length === perPath.length) {
    // Every path resolved on the frontend. Same volume → use it; otherwise the
    // sources span volumes → honest unknown.
    const first = matched[0]
    return matched.every((id) => id === first) ? first : DEFAULT_VOLUME_ID
  }

  if (matched.length > 0) {
    // Some paths matched a registered root and some didn't — a mixed batch we
    // can't honestly pin to one volume.
    return DEFAULT_VOLUME_ID
  }

  // No registered root matched. Ask the backend about the common parent.
  const resolution = await resolvePathVolume(getCommonParentPath(paths))
  return resolution.volume?.id ?? DEFAULT_VOLUME_ID
}
