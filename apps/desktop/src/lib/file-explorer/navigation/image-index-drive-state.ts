// Pure mapping helper for the per-drive IMAGE-index dot (the sibling of the filesystem
// `DriveIndexBadge`). Kept free of Svelte/DOM so the state derivation is unit-testable.
//
// Derived from the honest per-volume `MediaIndexVolumeState` (covered/enriched counts) plus
// the live `getVolumeEnrichActivity(volumeId)` activity. No new backend command; this reuses
// data the settings screen and the corner hourglass already read.

import type { MediaIndexVolumeState } from '$lib/tauri-commands'
import type { VolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'

/** The three visible dot states: gray `off`, pulsing-yellow `indexing`, green `done`. */
export type ImageIndexDriveState = 'off' | 'indexing' | 'done'

export interface ImageIndexDriveStateInputs {
  /** The master image-search toggle (`getMediaIndexEnabled()`). */
  enabled: boolean
  /** The honest per-volume state (`mediaIndexVolumeState`). */
  volumeState: MediaIndexVolumeState
  /** This volume's live enrichment activity (`getVolumeEnrichActivity`), or `undefined`. */
  enrichActivity: VolumeEnrichActivity | undefined
}

/**
 * The drive dot's `done / total` coverage for the tooltip, or `null` when importance hasn't
 * scored the volume yet (no honest total — the index is offline / still scanning). `total` is
 * the qualifying images in the COVERED folders (`coveredQualifyingCount`), falling back to the
 * whole-volume `qualifyingCount` before importance narrows it; `done` is the enriched count
 * clamped to `total` (mirrors the `media-enrich-state` snapshot convention). A narrow scope
 * makes `total` the in-scope denominator, so a drive can honestly reach `done` at any scope
 * rather than reading "indexing" forever.
 */
export function imageIndexDriveCoverage(volumeState: MediaIndexVolumeState): { done: number; total: number } | null {
  const total = volumeState.coveredQualifyingCount ?? volumeState.qualifyingCount
  if (total == null) return null
  return { done: Math.min(volumeState.enrichedCount, total), total }
}

/**
 * Map the per-volume state + live activity to the dot's state.
 *
 * - `off`: image search is disabled, or the volume isn't image-indexed yet (no honest total).
 * - `indexing`: a pass is actively enriching this volume, OR the covered set isn't fully
 *   enriched (`done < total`) — there's still work to do.
 * - `done`: idle and every covered image is enriched.
 *
 * A PAUSED pass reads as `indexing`, not `done`: `enrichActivity.paused` is non-null so the
 * "actively enriching" branch is false, but `done < total` keeps it yellow (work remains).
 */
export function imageIndexDriveState({ enabled, volumeState, enrichActivity }: ImageIndexDriveStateInputs): ImageIndexDriveState {
  if (!enabled || !volumeState.enabled) return 'off'
  const coverage = imageIndexDriveCoverage(volumeState)
  if (coverage === null) return 'off'
  const activelyEnriching = enrichActivity !== undefined && enrichActivity.paused === null
  if (activelyEnriching || coverage.done < coverage.total) return 'indexing'
  return 'done'
}
