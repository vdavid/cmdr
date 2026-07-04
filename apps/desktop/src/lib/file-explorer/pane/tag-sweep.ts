/**
 * Background Finder-tag backfill for a whole listing.
 *
 * The visible-range enrich (`fetchVisibleRange`) keeps on-screen rows' tags
 * correct; this fills the off-screen rows after a listing loads so scrolling is
 * instant and future tag-aware features see every row. Extracted from
 * `FilePane.svelte` to keep the pane lean (it's already `file-length`-flagged)
 * and to make the chunk/cancel logic unit-testable.
 *
 * Cancelable by construction: each chunk bails when `isStale()` reports the work
 * is abandoned (pane destroyed, a newer load started, or the pane swapped
 * listings). The per-listing AtomicBool/Notify cancel machinery doesn't reach
 * this detached loop, so `isStale` is the stop signal. `enrich_tags` already
 * no-ops on non-local backends and emits diffs only for rows that changed, so a
 * still-running-but-abandoned chunk is cheap.
 */

import { enrichTags, getFileRange } from '$lib/tauri-commands'

/** Low-priority chunk size for the sweep (paths per `enrich_tags` call). */
export const TAG_SWEEP_CHUNK = 500

export interface TagSweepOptions {
  listingId: string
  /** Backend visible-entry count for the listing (excludes the synthetic `..`). */
  totalCount: number
  includeHidden: boolean
  /** Returns true once the sweep should stop (nav away, unmount, listing swap). */
  isStale: () => boolean
}

export async function sweepListingTags(opts: TagSweepOptions): Promise<void> {
  const { listingId, totalCount, includeHidden, isStale } = opts
  for (let start = 0; start < totalCount; start += TAG_SWEEP_CHUNK) {
    if (isStale()) return
    let chunk
    try {
      chunk = await getFileRange(listingId, start, TAG_SWEEP_CHUNK, includeHidden)
    } catch {
      return
    }
    if (isStale()) return
    if (chunk.length === 0) break
    try {
      await enrichTags(
        listingId,
        chunk.map((e) => e.path),
      )
    } catch {
      // Best-effort backfill; a failed chunk just leaves those rows to the
      // visible-range enrich when they scroll in.
    }
  }
}
