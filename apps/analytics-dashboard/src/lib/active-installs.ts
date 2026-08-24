/**
 * Daily active installs as a RANGE rather than a point estimate.
 *
 * The opt-out rate is deliberately unmeasurable: an install that opts out of analytics sends
 * nothing at all, not even an "I opted out" bit, and that stays true. So a heartbeat-derived DAU
 * undercounts by an unknown amount, and printing it as one number quietly claims a precision we
 * don't have. Two independent signals bracket it instead:
 *
 * - **Floor**: distinct install ids that beat that day. Every one of these definitely ran the app.
 * - **Reach**: distinct IPs that checked for updates that day. Update checks ride a separate
 *   consent, so opted-out installs appear here.
 *
 * The reach is NOT a strict upper bound, and the UI says so. IPs aren't installs: NAT collapses a
 * household or an office to one, a dynamic IP inflates one install across days, and an install with
 * `updates.autoCheck` off never shows up at all. It's the best evidence we have for "at least this
 * many", nothing more.
 */

import type { HeartbeatDauRow, UpdateActivityRow } from './server/sources/cloudflare.js'

export interface ActiveInstallBound {
  day: string
  /** Distinct heartbeat installs: what we can prove ran. */
  floor: number
  /** Distinct update-checking IPs, or null on a day the update data doesn't cover. */
  reach: number | null
}

/**
 * One bound per heartbeat day, oldest first. The two endpoints answer different ranges, so a day
 * the update data doesn't reach gets a null reach rather than a zero, which would read as "nobody".
 */
export function boundsByDay(dau: HeartbeatDauRow[], updateActivity: UpdateActivityRow[]): ActiveInstallBound[] {
  const reachByDay = new Map<string, number>()
  for (const row of updateActivity) {
    reachByDay.set(row.day, (reachByDay.get(row.day) ?? 0) + row.updaters)
  }
  return [...dau]
    .sort((a, b) => a.date.localeCompare(b.date))
    .map((row) => ({ day: row.date, floor: row.dau, reach: reachByDay.get(row.date) ?? null }))
}

/** The most recent day, or null when there's nothing yet. */
export function latestBound(bounds: ActiveInstallBound[]): ActiveInstallBound | null {
  return bounds.length === 0 ? null : bounds[bounds.length - 1]
}

/**
 * The bound as a range, or as a lower bound alone when the update-check count lands at or below the
 * heartbeat count. That happens (NAT, or an update check that didn't fire that day) and it means
 * the reach isn't telling us anything above the floor, ❌ never that the fleet shrank.
 */
export function formatBound(bound: ActiveInstallBound | null): string {
  if (bound === null) return '–'
  if (bound.reach === null || bound.reach <= bound.floor) return `${String(bound.floor)}+`
  return `${String(bound.floor)}–${String(bound.reach)}`
}

/** The widest gap the window shows, as a share of the floor, for the "how blind are we" line. */
export function largestUnseenShare(bounds: ActiveInstallBound[]): number | null {
  let widest: number | null = null
  for (const bound of bounds) {
    if (bound.reach === null || bound.floor === 0 || bound.reach <= bound.floor) continue
    const share = (bound.reach - bound.floor) / bound.reach
    if (widest === null || share > widest) widest = share
  }
  return widest
}
