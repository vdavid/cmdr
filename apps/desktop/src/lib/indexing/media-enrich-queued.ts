/**
 * Pure predicate for the indicator's "image indexing starts after the drive
 * scan" line: image indexing is ON, and some enrichment-eligible volume is
 * mid-drive-index without an enrichment pass of its own yet.
 *
 * Why this line exists: flipping "Index image contents" on while a drive scan
 * runs is a designed no-op on the backend (a pass only starts on a ready index;
 * the scan's completion edge kicks it automatically). Without a visible queued
 * state, the toggle looks broken. The line renders under the drive rows that
 * are ALREADY keeping the corner hourglass up, so it never pins the surface on
 * its own (the same discipline as paused enrichment rows never lighting the
 * gate).
 *
 * `eligibleVolumeIds` comes from `getEnabledMediaIndexVolumeIds` (local root +
 * opted-in SMB) so eligibility policy stays single-sourced — a USB stick's scan
 * must never claim image indexing is coming (LocalExternal isn't enriched).
 * Replay (roll-on) rows are excluded by the caller: a quick update isn't the
 * scan-completion edge the promise is about.
 */
export function isEnrichQueued(
  imageIndexEnabled: boolean,
  eligibleVolumeIds: readonly string[],
  indexingVolumeIds: readonly string[],
  enrichingVolumeIds: readonly string[],
): boolean {
  if (!imageIndexEnabled) return false
  const eligible = new Set(eligibleVolumeIds)
  const enriching = new Set(enrichingVolumeIds)
  return indexingVolumeIds.some((id) => eligible.has(id) && !enriching.has(id))
}
