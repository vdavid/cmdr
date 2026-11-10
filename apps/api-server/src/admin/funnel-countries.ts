import { Hono } from 'hono'
import { verifyAdminAuth } from '../admin-auth'
import type { Bindings } from '../types'
import { resolveUaFamily } from '../user-agent'
import { buildDateList, invalidDaysMessage, normalizeCountry, parseDays, unknownCountry } from './funnel'

/**
 * `/admin/funnel/countries`: the funnel's window summed per country, for the dashboard's country table
 * and its country picker. Downloads count by the download's own country, installs and D7 by the
 * install's (`install_summary.country`, its first non-NULL one). `DETAILS.md` § Per-country funnel.
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
}

interface D7CountryRow {
  country: string | null
  d7Retained: number
}

export interface CountryRows {
  downloads: DownloadCountryRow[]
  installs: InstallCountryRow[]
  d7Retained: D7CountryRow[]
}

/**
 * The three per-country row sets. `?2` is the first day whose cohort is still too young for D7
 * (today minus seven): a first beat before it is at least eight days old.
 */
export async function queryCountryRows(db: D1Database, sinceDate: string, d7CutoffDate: string): Promise<CountryRows> {
  const [downloads, installs, d7Retained] = await Promise.all([
    db
      .prepare(
        `SELECT country, ua_family AS uaFamily, user_agent AS userAgent, COUNT(*) AS count
           FROM downloads
           WHERE date(created_at) >= ?1
           GROUP BY country, ua_family, user_agent`,
      )
      .bind(sinceDate)
      .all<DownloadCountryRow>(),
    db
      .prepare(
        `SELECT country, COUNT(*) AS newInstalls, SUM(CASE WHEN first_beat_at < ?2 THEN 1 ELSE 0 END) AS d7Cohort
           FROM install_summary
           WHERE first_beat_at >= ?1
           GROUP BY country`,
      )
      .bind(sinceDate, d7CutoffDate)
      .all<InstallCountryRow>(),
    db
      .prepare(
        `SELECT s.country AS country, COUNT(DISTINCT s.anal_id) AS d7Retained
           FROM install_summary s
           JOIN heartbeat h ON h.anal_id = s.anal_id
            AND h.created_at >= date(s.first_beat_at, '+7 days')
            AND h.created_at <  date(s.first_beat_at, '+8 days')
          WHERE s.first_beat_at >= ?1 AND s.first_beat_at < ?2
          GROUP BY s.country`,
      )
      .bind(sinceDate, d7CutoffDate)
      .all<D7CountryRow>(),
  ])
  return { downloads: downloads.results, installs: installs.results, d7Retained: d7Retained.results }
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
    const created = { country, serverDownloads: 0, humanInstalls: 0, newInstalls: 0, d7Cohort: 0, d7Retained: 0 }
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
  }
  for (const row of rows.d7Retained) entry(row.country).d7Retained += row.d7Retained

  const isUnknown = (c: FunnelCountry) => (c.country === unknownCountry ? 1 : 0)
  return [...byCountry.values()].sort(
    (a, b) =>
      b.newInstalls - a.newInstalls ||
      b.serverDownloads - a.serverDownloads ||
      isUnknown(a) - isUnknown(b) ||
      a.country.localeCompare(b.country),
  )
}

/** GET /admin/funnel/countries?days=N: `FunnelCountry[]` over the last N UTC days (default 30, 1..400). */
funnelCountries.get('/admin/funnel/countries', async (c) => {
  const authError = await verifyAdminAuth(c)
  if (authError) return authError

  const days = parseDays(c.req.query('days'))
  if (days === null) return c.json({ error: invalidDaysMessage }, 400)

  const now = new Date()
  const sinceDate = buildDateList(days, now)[0]
  // Today minus seven: the youngest cohort day with a fully passed D7 window is the day before it.
  const d7CutoffDate = buildDateList(8, now)[0]

  const rows = await queryCountryRows(c.env.TELEMETRY_DB, sinceDate, d7CutoffDate)
  return c.json(assembleCountries(rows))
})

export { funnelCountries }
