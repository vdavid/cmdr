import { Hono } from 'hono'
import { verifyAdminAuth } from '../admin-auth'
import type { Bindings } from '../types'
import { resolveUaFamily } from '../user-agent'
import { invalidDaysMessage, normalizeCountry, parseDays, shiftDate, unknownCountry } from './funnel'

/**
 * `/admin/funnel/countries`: the funnel's window summed per country, for the dashboard's country table
 * and its country picker. Downloads count by the download's own country, installs and retention by the
 * country of the install's first beat (`install_summary.first_beat_country`). `DETAILS.md` § Per-country
 * funnel.
 */

const funnelCountries = new Hono<{ Bindings: Bindings }>()

/** One country's totals over the window. `country` is ISO-2, or `(unknown)` for no usable country. */
export interface FunnelCountry {
  country: string
  /** Server-side DMG downloads from this country. */
  serverDownloads: number
  /** `serverDownloads` minus the provably-impossible non-macOS UAs, as `FunnelDay.humanInstalls`. */
  humanInstalls: number
  /** Installs from this country whose first-ever beat fell in the window. */
  newInstalls: number
  /** Of `newInstalls`, those in cohorts at least 8 days old: the ones whose D7 is knowable. */
  d7Cohort: number
  /** Of `d7Cohort`, those that beat in [first day + 7, first day + 8). */
  d7Retained: number
  /** Of `newInstalls`, those in cohorts at least 29 days old: the ones whose D28 is knowable. */
  d28Cohort: number
  /** Of `d28Cohort`, those that beat in [first day + 28, first day + 29). */
  d28Retained: number
}

interface DownloadCountryRow {
  country: string | null
  uaFamily: string | null
  userAgent: string | null
  count: number
}

interface InstallCountryRow {
  country: string | null
  newInstalls: number
  d7Cohort: number
  d28Cohort: number
}

interface RetainedCountryRow {
  country: string | null
  /** 7 or 28. */
  dayOffset: number
  retained: number
}

export interface CountryRows {
  downloads: DownloadCountryRow[]
  installs: InstallCountryRow[]
  retained: RetainedCountryRow[]
}

/**
 * The three per-country row sets over `[sinceDate, endDate]` (`endDate` defaults to `today`). A cohort's
 * D7 is knowable once its first beat is before today minus seven (`?2`), its D28 once before today minus
 * 28 (`?3`); both cutoffs also stop at the window's end (`?4`, exclusive), so a past window's cohorts stay
 * inside it.
 */
export async function queryCountryRows(
  db: D1Database,
  window: { sinceDate: string; today: string; endDate?: string },
): Promise<CountryRows> {
  const { sinceDate, today } = window
  const until = shiftDate(window.endDate ?? today, 1)
  const earlier = (a: string, b: string) => (a < b ? a : b)
  const d7Cutoff = earlier(shiftDate(today, -7), until)
  const d28Cutoff = earlier(shiftDate(today, -28), until)
  const horizon = (offset: number, cutoff: string) =>
    `SELECT s.anal_id, s.first_beat_country AS country, ${String(offset)} AS dayOffset
       FROM install_summary s
       JOIN heartbeat h ON h.anal_id = s.anal_id
        AND h.created_at >= date(s.first_beat_at, '+${String(offset)} days')
        AND h.created_at <  date(s.first_beat_at, '+${String(offset + 1)} days')
      WHERE s.first_beat_at >= ?1 AND s.first_beat_at < ${cutoff}`
  const [downloads, installs, retained] = await Promise.all([
    db
      .prepare(
        `SELECT country, ua_family AS uaFamily, user_agent AS userAgent, COUNT(*) AS count
           FROM downloads
           WHERE date(created_at) >= ?1 AND date(created_at) < ?2
           GROUP BY country, ua_family, user_agent`,
      )
      .bind(sinceDate, until)
      .all<DownloadCountryRow>(),
    db
      .prepare(
        `SELECT first_beat_country AS country, COUNT(*) AS newInstalls,
                SUM(CASE WHEN first_beat_at < ?2 THEN 1 ELSE 0 END) AS d7Cohort,
                SUM(CASE WHEN first_beat_at < ?3 THEN 1 ELSE 0 END) AS d28Cohort
           FROM install_summary
           WHERE first_beat_at >= ?1 AND first_beat_at < ?4
           GROUP BY first_beat_country`,
      )
      .bind(sinceDate, d7Cutoff, d28Cutoff, until)
      .all<InstallCountryRow>(),
    db
      .prepare(
        `SELECT country, dayOffset, COUNT(DISTINCT anal_id) AS retained
           FROM (${horizon(7, '?2')} UNION ALL ${horizon(28, '?3')})
          GROUP BY country, dayOffset`,
      )
      .bind(sinceDate, d7Cutoff, d28Cutoff)
      .all<RetainedCountryRow>(),
  ])
  return { downloads: downloads.results, installs: installs.results, retained: retained.results }
}

/**
 * Fold the row sets into one entry per country, normalizing every non-country value into `(unknown)`,
 * sorted by new installs, then server downloads, both descending; on a full tie `(unknown)` goes last
 * and the rest by code, for a stable order.
 */
export function assembleCountries(rows: CountryRows): FunnelCountry[] {
  const byCountry = new Map<string, FunnelCountry>()
  const entry = (raw: string | null): FunnelCountry => {
    const country = normalizeCountry(raw)
    const existing = byCountry.get(country)
    if (existing) return existing
    const created = {
      country,
      serverDownloads: 0,
      humanInstalls: 0,
      newInstalls: 0,
      d7Cohort: 0,
      d7Retained: 0,
      d28Cohort: 0,
      d28Retained: 0,
    }
    byCountry.set(country, created)
    return created
  }
  for (const row of rows.downloads) {
    const target = entry(row.country)
    target.serverDownloads += row.count
    if (resolveUaFamily(row) !== 'bot') target.humanInstalls += row.count
  }
  for (const row of rows.installs) {
    const target = entry(row.country)
    target.newInstalls += row.newInstalls
    target.d7Cohort += row.d7Cohort
    target.d28Cohort += row.d28Cohort
  }
  for (const row of rows.retained) {
    const target = entry(row.country)
    if (row.dayOffset === 7) target.d7Retained += row.retained
    else if (row.dayOffset === 28) target.d28Retained += row.retained
  }

  const isUnknown = (c: FunnelCountry) => (c.country === unknownCountry ? 1 : 0)
  return [...byCountry.values()].sort(
    (a, b) =>
      b.newInstalls - a.newInstalls ||
      b.serverDownloads - a.serverDownloads ||
      isUnknown(a) - isUnknown(b) ||
      a.country.localeCompare(b.country),
  )
}

/**
 * `?end=` as a real `YYYY-MM-DD` day no later than `today`, `undefined` when absent, or null when malformed
 * or in the future.
 */
export function parseEndDate(raw: string | undefined, today: string): string | undefined | null {
  if (raw === undefined) return undefined
  // `Date.parse` rejects month 13 outright but rolls 02-30 into March, so the round trip catches both.
  if (!/^\d{4}-\d{2}-\d{2}$/.test(raw) || Number.isNaN(Date.parse(`${raw}T00:00:00Z`))) return null
  if (shiftDate(raw, 0) !== raw) return null
  return raw <= today ? raw : null
}

/**
 * GET /admin/funnel/countries?days=N&end=YYYY-MM-DD: `FunnelCountry[]` over the N UTC days ending `end`
 * (default today, partial; N defaults to 30, 1..400). The `X-Window` header (`since..end`) names the days
 * answered, so the dashboard can tell a past window from an older worker that ignored `end`.
 */
funnelCountries.get('/admin/funnel/countries', async (c) => {
  const authError = await verifyAdminAuth(c)
  if (authError) return authError

  const days = parseDays(c.req.query('days'))
  if (days === null) return c.json({ error: invalidDaysMessage }, 400)
  const today = new Date().toISOString().slice(0, 10)
  const endDate = parseEndDate(c.req.query('end'), today)
  if (endDate === null) return c.json({ error: 'Invalid end. Use a YYYY-MM-DD day no later than today (UTC)' }, 400)

  const end = endDate ?? today
  const sinceDate = shiftDate(end, -(days - 1))
  const rows = await queryCountryRows(c.env.TELEMETRY_DB, { sinceDate, today, endDate: end })
  c.header('X-Window', `${sinceDate}..${end}`)
  return c.json(assembleCountries(rows))
})

export { funnelCountries }
