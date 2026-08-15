/**
 * How long after launch a real folder size first appears on a folder the user
 * opened.
 *
 * This is the wow moment covering a drive in phases is FOR: not "the index
 * finished" (nobody watches that), but "I opened a folder and it told me how big
 * it is". It is also the one number nobody could answer before, because the
 * moment lives on screen and nothing on screen was timed.
 *
 * ❌ Deliberately NOT the same question as "did the index complete": a folder the
 * walker reached ten seconds in answers here, while the drive keeps going for
 * minutes. `covering` separates the two cohorts — a launch with a first index
 * running against one with an index that was already there — since without it a
 * machine that has been indexed for weeks would drown the measurement in zeroes.
 *
 * Fires at most ONCE per launch, and goes inert the moment it has: everything
 * after the first hit is one boolean. ❌ Nothing here carries a path or a name.
 *
 * ⚠️ **Both list modes feed it**, and they have to: it is called from
 * `views/full-list-cache.svelte.ts` AND `views/BriefList.svelte`, at the two
 * points in each where rows the user is looking at gain sizes (a window fetch
 * landing, and an `index-dir-updated` refresh resolving). A hook in only one of
 * them would make the population "launches that opened a folder in THAT mode",
 * which reads as "launches" to anyone who doesn't know, and nothing in the
 * numbers would say otherwise. ❌ Don't drop a call site without saying so in
 * `src-tauri/src/analytics/DETAILS.md` § The first-index events, where whoever
 * reads the dashboard will look.
 */

import { trackEvent } from '$lib/tauri-commands'
import type { FileEntry } from '$lib/file-explorer/types'
import { isVolumeCoveredInPhases } from './index-state.svelte'

/**
 * When this launch started, as close as the frontend can see it: the first
 * evaluation of this module, which happens while the app is booting. The gap to
 * process start is well under a second, against a measurement whose interesting
 * range is seconds to minutes.
 */
const LAUNCHED_AT = Date.now()

/** Whether the one event has already gone out this launch. */
let reported = false

/**
 * Take one window of rows the user is looking at, and report the moment if this
 * is the first one carrying a real folder size.
 *
 * `recursiveSize` is the honest-size field: `null` is the `<dir>` placeholder, so
 * a number here is exactly what the user sees in the size column.
 */
export function noteRenderedFolderSizes(entries: readonly FileEntry[], volumeId: string): void {
  if (reported) return
  if (!entries.some((entry) => entry.isDirectory && entry.recursiveSize != null)) return
  reported = true
  void trackEvent('first_folder_size_shown', {
    seconds_bucket: secondsBucket(Date.now() - LAUNCHED_AT),
    covering: isVolumeCoveredInPhases(volumeId),
  })
}

/**
 * The elapsed time as a coarse bucket. Fine at the short end, where the claim
 * lives: a drive whose priority roots are covered first should be answering in
 * the first two buckets on a launch that is still indexing.
 */
export function secondsBucket(elapsedMs: number): string {
  const seconds = elapsedMs / 1000
  if (seconds < 5) return '<5s'
  if (seconds < 15) return '5-15s'
  if (seconds < 60) return '15-60s'
  if (seconds < 300) return '1-5m'
  return '5m+'
}

/** Test seam: forget that this launch already reported. ❌ Not for production. */
export function resetFirstSizeTimingForTest(): void {
  reported = false
}
