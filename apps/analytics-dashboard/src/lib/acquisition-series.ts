/**
 * Client-safe: turns the per-day funnel rows into the inputs each Acquisition trend chart and stats panel
 * reads. One place decides what each metric's numerator and denominator are, so a chart and its stats
 * can't disagree.
 */
import type { FunnelRow } from '#lib/server/sources/funnel.js'
import type { InstallsByChannel, RetentionSoFar } from './funnel.js'
import { addDays, type CountPoint, type RatioPoint } from './acquisition-stats.js'
import type { RateBands } from './daily-chart.js'

/**
 * The rate charts' red / yellow / green bands: under `yellow` is red, `green` and up is green. David's
 * targets for each step; the bot meter has none on purpose (its level is noise, only its jumps matter).
 */
export const rateBands = {
  visitorToClick: { yellow: 0.05, green: 0.12 },
  clickToInstall: { yellow: 0.5, green: 0.7 },
  d7: { yellow: 0.15, green: 0.25 },
  d28: { yellow: 0.1, green: 0.2 },
} as const satisfies Record<string, RateBands>

/** A D7 cohort is final once it's this many days old (the worker's own rule: D7 needs day 7 to be over). */
export const d7FinalAgeDays = 8

/** A D28 cohort is final once it's this many days old (the worker's rule, as for D7). */
export const d28FinalAgeDays = 29

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
  /** Final D28 cohorts only, as for `d7`. */
  d28: RatioPoint[]
  d28Tentative: (TentativePoint | null)[]
  botMeter: RatioPoint[]
  /** Whether any day carries each optional worker field, for the per-section "Not available yet". */
  has: { channels: boolean; retentionSoFar: boolean; d28: boolean; d28SoFar: boolean }
}

/** The first day whose D7 cohort is still young, given `today`. Cohorts before it are final. */
export function d7End(today: string): string {
  return addDays(today, -(d7FinalAgeDays - 1))
}

/** The first day whose D28 cohort is still young, given `today`. Cohorts before it are final. */
export function d28End(today: string): string {
  return addDays(today, -(d28FinalAgeDays - 1))
}

function isFinalCohort(row: FunnelRow, today: string): boolean {
  return row.date < d7End(today) && (row.d7Retention !== null || row.newInstalls === 0)
}

function isFinalD28Cohort(row: FunnelRow, today: string): boolean {
  return row.date < d28End(today) && (row.d28Retention !== null || row.newInstalls === 0)
}

/** A young cohort's grey point: retained so far ÷ the cohort, labeled with the day it's as of. */
function tentative(soFar: RetentionSoFar | null, installs: number | null): TentativePoint | null {
  return soFar && installs ? { value: soFar.retained / installs, label: `D${String(soFar.day)} so far` } : null
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
    // Homebrew installs never click the website's button, so once the channel split exists they stay out:
    // direct plus unknown-channel installs count against clicks. Unknown stays in because it's builds too old
    // to report a channel, nearly all DMGs; counting only `direct` read 0% for every day before the builds
    // that report it. Days without the split (an older worker) fall back to all installs.
    clickToInstall: ratio(
      (r) => (r.newInstallsByChannel ? r.newInstallsByChannel.direct + r.newInstallsByChannel.unknown : r.newInstalls),
      (r) => r.downloadClicks,
    ),
    installs: count((r) => r.newInstalls),
    installsByChannel: rows.map((r) => r.newInstallsByChannel),
    d7: ratio(
      (r) => (isFinalCohort(r, today) ? (r.d7Retained ?? 0) : null),
      (r) => r.newInstalls,
    ),
    d7Tentative: rows.map((r) => (isFinalCohort(r, today) ? null : tentative(r.retentionSoFar, r.newInstalls))),
    d28: ratio(
      (r) => (isFinalD28Cohort(r, today) ? (r.d28Retained ?? 0) : null),
      (r) => r.newInstalls,
    ),
    d28Tentative: rows.map((r) => (isFinalD28Cohort(r, today) ? null : tentative(r.d28SoFar, r.newInstalls))),
    botMeter: ratio(
      (r) => r.serverDownloads,
      (r) => r.newInstalls,
    ),
    has: {
      channels: rows.some((r) => r.newInstallsByChannel !== null),
      retentionSoFar: rows.some((r) => r.retentionSoFar !== null),
      d28: rows.some((r) => r.d28Retained !== null || r.d28SoFar !== null),
      d28SoFar: rows.some((r) => r.d28SoFar !== null),
    },
  }
}
