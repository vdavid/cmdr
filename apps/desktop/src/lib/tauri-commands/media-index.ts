// Media-index (image-ML) commands: the OCR-search read surface, the honest
// per-volume enrichment state, and the `cmdr-media://` thumbnail-token helpers the
// search-results grid uses to reuse the EXISTING viewer preview path (plan Decision 5).
// Every wrapper delegates to the typed `commands.*` bindings.

import {
  commands,
  type CoveredCount,
  type MediaIndexVolumeState,
  type OcrHit,
  type ReclaimPreview,
  type ReclaimResult,
  type SimilarImage,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { CoveredCount, MediaIndexVolumeState, OcrHit, ReclaimPreview, ReclaimResult, SimilarImage }

/**
 * Search a volume's image OCR text for `query`, returning up to `limit` hits (backend
 * default 200, capped at 1,000), each with a highlighted `snippet` — the matched text
 * with `[` / `]` around the matched terms (the "why matched" reason). An empty query, an
 * un-enriched volume, or an offline `media.db` returns `[]` rather than erroring, so the
 * caller distinguishes "no matches" from "not indexed" via `mediaIndexVolumeState`.
 */
export async function mediaIndexSearchOcr(
  volumeId: string,
  query: string,
  limit: number | null = null,
): Promise<OcrHit[]> {
  const res = await commands.mediaIndexSearchOcr(volumeId, query, limit)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * The honest per-volume enrichment state (master toggle on/off, a pass running now, and
 * how many images are already enriched). The grid reads this to voice its own coverage
 * rather than showing a confident-looking empty result that's really "not indexed yet".
 */
export async function mediaIndexVolumeState(volumeId: string): Promise<MediaIndexVolumeState> {
  const res = await commands.mediaIndexVolumeState(volumeId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Mint a `cmdr-media://` token for a local image `path`, or `null` when the path isn't a
 * renderable image. The grid builds the thumbnail URL from the token via `mediaUrl`.
 * Every minted token MUST later be dropped via {@link mediaIndexDropThumbnailTokens}, or
 * the backend token map leaks path mappings (unlike a viewer session, the grid has no
 * window-close choke point).
 */
export async function mediaIndexThumbnailToken(path: string): Promise<string | null> {
  const res = await commands.mediaIndexThumbnailToken(path)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Drop `cmdr-media://` thumbnail tokens once the grid no longer shows them. Best-effort. */
export async function mediaIndexDropThumbnailTokens(tokens: string[]): Promise<void> {
  if (tokens.length === 0) return
  await commands.mediaIndexDropThumbnailTokens(tokens)
}

/**
 * Opt a network (SMB) volume in or out of background image enrichment (network enrichment). Off by
 * default: turning on the master toggle does NOT auto-enrich network drives. Enabling kicks
 * an immediate pass so the user sees progress without waiting for the next scan. The FE also
 * persists `mediaIndex.networkVolumes`; both happen together in `network-volume-prefs.ts`.
 */
export async function mediaIndexSetNetworkVolumeEnabled(volumeId: string, enabled: boolean): Promise<void> {
  await commands.mediaIndexSetNetworkVolumeEnabled(volumeId, enabled)
}

/**
 * Set (or clear) a whole-volume "always index" override: enrich regardless of the importance
 * threshold (a rarely-browsed NAS scores low, so without this its photos defer forever — plan
 * Decision 6). Enabling kicks an immediate pass. The FE also persists
 * `mediaIndex.alwaysIndexVolumes`.
 */
export async function mediaIndexSetAlwaysIndexVolume(volumeId: string, always: boolean): Promise<void> {
  await commands.mediaIndexSetAlwaysIndexVolume(volumeId, always)
}

/**
 * Set (or clear) a per-folder image-search EXCLUSION (the privacy veto): no image at or under
 * `folder` is enriched, beating any "always index" override. Excluding also retro-deletes the
 * folder's already-indexed rows backend-side; un-excluding just clears the veto. The FE also
 * persists `mediaIndex.excludedFolders`; both happen together in `excluded-folders.ts`. Rejects
 * if the backend rejects, so the caller can roll the persisted value back.
 */
export async function mediaIndexSetExcludedFolder(folder: string, excluded: boolean): Promise<void> {
  await commands.mediaIndexSetExcludedFolder(folder, excluded)
}

/**
 * Set the folder-importance threshold the image scheduler enriches by (the importance slider's
 * typed `0.0..=1.0` value, clamped backend-side). Below-threshold folders are deferred;
 * an "always index" override still forces enrichment. Live-applied via the
 * `settings-applier.ts` passthrough after the FE persists `mediaIndex.importanceThreshold`
 * (the `mediaIndex.enabled` precedent). Fire-and-forget: a failed push only leaves the
 * running scheduler one threshold behind until the next change or restart re-seeds it.
 */
export async function setImageImportanceThreshold(threshold: number): Promise<void> {
  await commands.mediaIndexSetImportanceThreshold(threshold)
}

/**
 * The live preview behind the importance slider: across the ENABLED volumes in `volumeIds` (master
 * on AND local-or-opted-in-SMB; the backend filters out non-opted-in SMB / MTP), how many
 * folders score at or above `threshold` and how many images they hold. `pending` is `true`
 * when any requested enabled volume isn't ready yet (still scanning / not yet scored), so
 * the UI voices "still scanning" rather than a confident wrong number. Debounce-friendly:
 * the per-folder image counts are cached, so a drag only re-runs the cheap importance filter.
 */
export async function mediaIndexCoveredCount(threshold: number, volumeIds: string[]): Promise<CoveredCount> {
  const res = await commands.mediaIndexCoveredCount(threshold, volumeIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Preview the reclaim-space split across `volumeIds` at the CURRENT `threshold` (the
 * settled slider value): how many stored image rows fall inside the setting
 * (`coveredStored`, still searchable) vs outside it (`doomedCount`, what a prune would
 * delete), and the bytes that would free. `totalStored === coveredStored + doomedCount`
 * always, so the reclaim copy adds up. `pending` is `true` when a requested volume isn't
 * ready (scanning / not yet scored), so the UI hides the reclaim line rather than acting
 * on a lower bound. Answers offline from `media.db`.
 */
export async function mediaIndexReclaimPreview(threshold: number, volumeIds: string[]): Promise<ReclaimPreview> {
  const res = await commands.mediaIndexReclaimPreview(threshold, volumeIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Delete the stored image rows OUTSIDE the current `threshold` across `volumeIds` (the
 * user-explicit reclaim). Selects the doomed set with the same precedence enrichment
 * uses, deletes it through each volume's one writer thread, `VACUUM`s to reclaim the
 * disk, and drops the derived caches. Returns the rows deleted and bytes freed for the
 * honest "Freed X" toast. Recoverable: a later pass re-indexes anything still covered.
 */
export async function mediaIndexPruneBelowThreshold(threshold: number, volumeIds: string[]): Promise<ReclaimResult> {
  const res = await commands.mediaIndexPruneBelowThreshold(threshold, volumeIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Find the images most similar to the one at `sourcePath` on `volumeId` (feature-print
 * cosine), highest first, excluding the source (plan "find similar"). Answers from
 * `media.db` + the resident vector cache even when the volume is offline. `limit` caps the
 * result count (backend default when `null`).
 */
export async function mediaIndexFindSimilar(
  volumeId: string,
  sourcePath: string,
  limit: number | null = null,
): Promise<SimilarImage[]> {
  const res = await commands.mediaIndexFindSimilar(volumeId, sourcePath, limit)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

// A typed wrapper for the per-folder override (`media_index_set_always_index_folder`) is
// deliberately omitted this slice: its trigger is a folder right-click action in the native
// (Rust) file context menu, a small backend follow-up. The raw `commands.*` binding is ready.
