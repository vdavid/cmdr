/**
 * Client-safe: turns the per-day funnel rows into the inputs each Acquisition trend chart and stats panel
 * reads. One place decides what each metric's numerator and denominator are, so a chart and its stats
 * can't disagree.
 */
import type { FunnelRow } from '#lib/server/sources/funnel.js'
import type { InstallsByChannel } from './funnel.js'
import { addDays, type CountPoint, type RatioPoint } from './acquisition-stats.js'

/** A D7 cohort is final once it's this many days old (the worker's own rule: D7 needs day 7 to be over). */
export const d7FinalAgeDays = 8

/** A young cohort's grey "so far" point: its retained share as of `day`, with its label. */
export interface TentativePoint {
  value: number
  label: string
}

export interface TrendInputs {
  days: string[]
  visitors: CountPoint[]
  pageviewsPerVisitor: RatioPoint[]
  clicks: CountPoint[]
  visitorToClick: RatioPoint[]
  clickToInstall: RatioPoint[]
  installs: CountPoint[]
  installsByChannel: (InstallsByChannel | null)[]
  /** Final cohorts only; a cohort too young for D7 has `num: null`. */
  d7: RatioPoint[]
  d7Tentative: (TentativePoint | null)[]
  botMeter: RatioPoint[]
  /** Whether any day carries each optional worker field, for the per-section "Not available yet". */
  has: { channels: boolean; retentionSoFar: boolean }
}

/** The first day whose D7 cohort is still young, given `today`. Cohorts before it are final. */
export function d7End(today: string): string {
  return addDays(today, -(d7FinalAgeDays - 1))
}

function isFinalCohort(row: FunnelRow, today: string): boolean {
  return row.date < d7End(today) && (row.d7Retention !== null || row.newInstalls === 0)
}

export function buildTrendInputs(rows: FunnelRow[], today: string): TrendInputs {
  const count = (pick: (r: FunnelRow) => number | null): CountPoint[] =>
    rows.map((r) => ({ date: r.date, value: pick(r) }))
  const ratio = (num: (r: FunnelRow) => number | null, den: (r: FunnelRow) => number | null): RatioPoint[] =>
    rows.map((r) => ({ date: r.date, num: num(r), den: den(r) }))

  return {
    days: rows.map((r) => r.date),
    visitors: count((r) => r.visitors),
    pageviewsPerVisitor: ratio(
      (r) => r.pageviews,
      (r) => r.visitors,
    ),
    clicks: count((r) => r.downloadClicks),
    visitorToClick: ratio(
      (r) => r.downloadClicks,
      (r) => r.visitors,
    ),
    clickToInstall: ratio(
      (r) => r.newInstalls,
      (r) => r.downloadClicks,
    ),
    installs: count((r) => r.newInstalls),
    installsByChannel: rows.map((r) => r.newInstallsByChannel),
    d7: ratio(
      (r) => (isFinalCohort(r, today) ? (r.d7Retained ?? 0) : null),
      (r) => r.newInstalls,
    ),
    d7Tentative: rows.map((r) =>
      !isFinalCohort(r, today) && r.retentionSoFar && r.newInstalls
        ? { value: r.retentionSoFar.retained / r.newInstalls, label: `D${String(r.retentionSoFar.day)} so far` }
        : null,
    ),
    botMeter: ratio(
      (r) => r.serverDownloads,
      (r) => r.newInstalls,
    ),
    has: {
      channels: rows.some((r) => r.newInstallsByChannel !== null),
      retentionSoFar: rows.some((r) => r.retentionSoFar !== null),
    },
  }
}
