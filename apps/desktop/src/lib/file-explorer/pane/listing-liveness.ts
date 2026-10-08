/**
 * Which backend listings the panes show, so a pane never keeps showing rows its
 * backend listing no longer serves.
 *
 * Two jobs, both keyed by listing id:
 * - **Heartbeat.** A pane can sit on one folder for days with no reads and no FS
 *   events, and the backend's orphan reaper reclaims a listing nothing touched for
 *   six hours. Every `LISTING_HEARTBEAT_MS`, `keepListingsAlive` names each tracked
 *   listing so the reaper leaves it alone.
 * - **Recovery.** When the heartbeat or any listing read (`onListingGone`) learns the
 *   backend lost a tracked listing, its owner hears about it once and re-lists the
 *   folder. Without it, a lost listing left a pane with stale rows, no watcher, and
 *   F3–F6 failing.
 *
 * A pane tracks its listing only once it has LANDED (and stops when it abandons it),
 * so a read racing a navigation, which also answers "gone", never triggers a re-list.
 */
import { keepListingsAlive, onListingGone } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

/** How often the panes name their listings to the backend: well inside its six-hour reaper window. */
export const LISTING_HEARTBEAT_MS = 30 * 60 * 1000

/** Each tracked listing and what its owner does when the backend no longer holds it. */
const live = new Map<string, () => void>()
let timer: ReturnType<typeof setInterval> | null = null
let unsubscribeGone: (() => void) | null = null

/**
 * Starts tracking `listingId` for its owner, replacing any earlier owner (a pane swap
 * hands a listing to the other pane).
 */
export function trackLiveListing(listingId: string, onGone: () => void): void {
  live.set(listingId, onGone)
  unsubscribeGone ??= onListingGone(reportListingGone)
  timer ??= setInterval(() => void heartbeatLiveListings(), LISTING_HEARTBEAT_MS)
}

/** Stops tracking `listingId`: its pane ended it or walked away. */
export function untrackLiveListing(listingId: string): void {
  live.delete(listingId)
  if (live.size > 0) return
  if (timer !== null) clearInterval(timer)
  timer = null
  unsubscribeGone?.()
  unsubscribeGone = null
}

/** Tells a tracked listing's owner, once, that the backend no longer holds it. Untracked ids are ignored. */
function reportListingGone(listingId: string): void {
  const onGone = live.get(listingId)
  if (!onGone) return
  untrackLiveListing(listingId)
  log.warn('Listing {listingId} is gone from the backend; its pane re-lists', { listingId })
  onGone()
}

/** One heartbeat: keeps every tracked listing alive and recovers the ones already lost. */
async function heartbeatLiveListings(): Promise<void> {
  if (live.size === 0) return
  let gone: string[]
  try {
    gone = await keepListingsAlive([...live.keys()])
  } catch (e) {
    log.warn("Couldn't send the listing heartbeat: {error}", { error: String(e) })
    return
  }
  for (const listingId of gone) reportListingGone(listingId)
}
