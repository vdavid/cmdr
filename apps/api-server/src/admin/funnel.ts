import { Hono } from 'hono'
import { verifyAdminAuth } from '../admin-auth'
import type { Bindings } from '../types'
import {
  assembleFunnel,
  countryPattern,
  type DauRow,
  type DownloadByRefererRow,
  type DownloadByRefRow,
  type DownloadBySourceRow,
  type DownloadByUaRow,
  type FunnelRows,
  type InstallsByChannelRow,
  type RetentionRow,
} from './funnel-assemble'

export { assembleFunnel, normalizeCountry, unknownCountry } from './funnel-assemble'
export type { CountryDay, FunnelDay, FunnelRows, RetentionSoFar } from './funnel-assemble'

/**
 * Per-day acquisition funnel for the analytics dashboard. One admin endpoint returns a per-UTC-day
 * array for the last N days so the dashboard can show downloads -> installs -> retention -> signups
 * in a single table, instead of stitching it together from the per-metric endpoints.
 *
 * Everything is bucketed by UTC day (D1 `date()` is UTC, and the Listmonk dates are normalized to UTC
 * here), so a "day" means the same window across every column.
 *
 * Per-install facts (first beat, country, channel) come from `install_summary`, one row
 * per install, rather than from grouping every heartbeat: `DETAILS.md` § Install summary.
 *
 * This file holds the queries and the route; the response shape and its assembly are in
 * `funnel-assemble.ts`.
 *
 * Auth: the shared ADMIN_API_TOKEN, same pattern as the other admin endpoints.
 */

const funnel = new Hono<{ Bindings: Bindings }>()

/** Bounds on `?days=`: at least 1, at most 400 (a bit over a year, for year-over-year views). */
const minDays = 1
const maxDays = 400
const defaultDays = 30

/**
 * UTC `YYYY-MM-DD` strings for the last `days` days, oldest first, ending today (UTC).
 * `today` is derived from `now` so it's stable within one request.
 */
export function buildDateList(days: number, now: Date): string[] {
  const dates: string[] = []
  // Midnight UTC of "today".
  const todayUtc = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  for (let i = days - 1; i >= 0; i--) {
    dates.push(new Date(todayUtc - i * 86_400_000).toISOString().slice(0, 10))
  }
  return dates
}

/** The UTC day `days` days after (negative: before) a `YYYY-MM-DD` day. */
export function shiftDate(date: string, days: number): string {
  return new Date(Date.parse(`${date}T00:00:00Z`) + days * 86_400_000).toISOString().slice(0, 10)
}

/** The window every funnel query reads: from `sinceDate` through `today` (UTC days), optionally one country. */
export interface FunnelWindow {
  sinceDate: string
  today: string
  /** ISO-2, or `null` for every country. */
  country: string | null
}

/**
 * `?days=` as an integer in range, or null when malformed. Shared with `/admin/funnel/countries`.
 * `parseInt` alone would read `7abc` as 7, hence the digits-only check.
 */
export function parseDays(raw: string | undefined): number | null {
  if (raw === undefined) return defaultDays
  if (!/^\d+$/.test(raw)) return null
  const days = parseInt(raw, 10)
  return days >= minDays && days <= maxDays ? days : null
}

export const invalidDaysMessage = `Invalid days. Use an integer ${String(minDays)}..${String(maxDays)}`

/** `?country=` as an uppercase ISO-2 code, `null` when absent, or `undefined` when malformed. */
function parseCountryFilter(raw: string | undefined): string | null | undefined {
  if (raw === undefined || raw === '') return null
  const country = raw.toUpperCase()
  return countryPattern.test(country) ? country : undefined
}

/**
 * Per-day server downloads, split by source. Bots are filtered at write time, so this is the raw
 * request count of real DMG fetches. The `source` column is NULL for rows written before migration
 * 0008; those COALESCE to `other`. `?2` is the country filter, NULL for every country (the same in
 * every query below). Grouped by country too, for `FunnelDay.byCountry`.
 */
async function queryDownloadsBySource(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<DownloadBySourceRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COALESCE(source, 'other') AS source, country, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, source, country`,
    )
    .bind(sinceDate, country)
    .all<DownloadBySourceRow>()
  return results
}

/**
 * Per-day server downloads, split by first-touch channel (`ref`). The `ref` column is NULL for
 * Homebrew, direct links, return visits in a later session, and rows written before migration 0009;
 * those COALESCE to the `"(none)"` bucket so the dashboard renders them as a single "no ref" channel
 * (consistent with how it shows other unknowns). The value was already sanitized at write time, so
 * grouping is on the stored value as-is.
 */
async function queryDownloadsByRef(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<DownloadByRefRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COALESCE(ref, '(none)') AS ref, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, ref`,
    )
    .bind(sinceDate, country)
    .all<DownloadByRefRow>()
  return results
}

/**
 * Per-day server downloads, split by the `Referer` host of the `/download` hit. The `referer` column is
 * NULL for hits with no/unparseable `Referer` (typed URL, privacy browser, referrer-policy strip,
 * Homebrew/curl) and for rows before migration 0010; those COALESCE to the `"(none)"` bucket. The host
 * was already sanitized at write time, so grouping is on the stored value as-is. This is a separate
 * signal from `ref`: `ref` is the website's first-touch attribution; `referer` is the raw request host,
 * which is what reveals where the direct (no-`ref`) downloads came from.
 */
async function queryDownloadsByReferer(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<DownloadByRefererRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COALESCE(referer, '(none)') AS referer, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, referer`,
    )
    .bind(sinceDate, country)
    .all<DownloadByRefererRow>()
  return results
}

/**
 * Per-day server downloads, split by install-plausibility family. Rows carry the family recorded at
 * write time (`ua_family`, migration 0013); `assembleFunnel` falls back to `classifyUaFamily` on the
 * raw UA for rows written before that column existed. Keeping the fallback out of SQL keeps the
 * classifier a single pure, unit-tested function and the only place the macOS-only logic lives.
 *
 * Grouping on both columns keeps cardinality low (one row per distinct UA per day over a few hundred
 * downloads) and collapses to one row per family per day and country once the sweep has cleared the
 * raw UAs.
 */
async function queryDownloadsByUa(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<DownloadByUaRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, ua_family AS uaFamily, user_agent AS userAgent, country, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, ua_family, user_agent, country`,
    )
    .bind(sinceDate, country)
    .all<DownloadByUaRow>()
  return results
}

/**
 * New installs per day and channel: installs whose FIRST-EVER beat (`install_summary.first_beat_at`)
 * fell inside the window. The summary holds one row per install, so this reads installs, not beats,
 * and an install that first beat months ago never counts as "new" here. `first_beat_at >= 'YYYY-MM-DD'`
 * compares as text against `YYYY-MM-DD HH:MM:SS`, which keeps the index usable. The country is the
 * first beat's (`first_beat_country`), never one gained later: `DETAILS.md` § Install summary.
 */
async function queryInstallsByChannel(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<InstallsByChannelRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(first_beat_at) AS date, install_channel AS channel, first_beat_country AS country, COUNT(*) AS count
         FROM install_summary
         WHERE first_beat_at >= ?1 AND (?2 IS NULL OR first_beat_country = ?2)
         GROUP BY date, install_channel, first_beat_country`,
    )
    .bind(sinceDate, country)
    .all<InstallsByChannelRow>()
  return results
}

/** Distinct install ids that beat per day (true DAU); under a country filter, installs whose first beat came from it. */
async function queryDau(db: D1Database, sinceDate: string, country: string | null): Promise<DauRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COUNT(DISTINCT anal_id) AS dau
         FROM heartbeat
         WHERE created_at >= ?1
           AND (?2 IS NULL OR anal_id IN (SELECT anal_id FROM install_summary WHERE first_beat_country = ?2))
         GROUP BY date`,
    )
    .bind(sinceDate, country)
    .all<DauRow>()
  return results
}

/**
 * Retained installs per cohort day, day offset, and first-beat country. An install whose first beat was
 * on day X is retained at offset n if it has ANY beat in [X+n, X+n+1). Only three windows are read, so
 * each install's lookup (`idx_heartbeat_anal_created`) touches about three days of beats:
 *
 * - [X+7, X+8): D7.
 * - [X+28, X+29): D28.
 * - Yesterday, for cohorts 2–28 days old: a cohort n+1 days old has fully passed day n exactly at
 *   yesterday, so this one window answers `retentionSoFar` and `d28SoFar` for every young cohort.
 *
 * Windows can overlap (yesterday is day seven for an eight-day-old cohort), hence `COUNT(DISTINCT)`.
 * `?3` is yesterday, `?4` today, `?5` the oldest cohort day still short of D28 (or `?1` if later).
 */
async function queryRetention(db: D1Database, window: FunnelWindow): Promise<RetentionRow[]> {
  const { sinceDate, today, country } = window
  const yesterday = shiftDate(today, -1)
  const youngSince = [sinceDate, shiftDate(today, -28)].sort()[1]
  const horizon = (offset: number) =>
    `SELECT s.anal_id, s.first_beat_country AS country, date(s.first_beat_at) AS cohortDate, ${String(offset)} AS dayOffset
       FROM install_summary s
       JOIN heartbeat h ON h.anal_id = s.anal_id
        AND h.created_at >= date(s.first_beat_at, '+${String(offset)} days')
        AND h.created_at <  date(s.first_beat_at, '+${String(offset + 1)} days')
      WHERE s.first_beat_at >= ?1 AND (?2 IS NULL OR s.first_beat_country = ?2)`
  const { results } = await db
    .prepare(
      `SELECT cohortDate, dayOffset, country, COUNT(DISTINCT anal_id) AS retained
         FROM (
           ${horizon(7)}
           UNION ALL
           ${horizon(28)}
           UNION ALL
           SELECT s.anal_id, s.first_beat_country AS country, date(s.first_beat_at) AS cohortDate,
                  CAST(julianday(?3) - julianday(date(s.first_beat_at)) AS INTEGER) AS dayOffset
             FROM install_summary s
             JOIN heartbeat h ON h.anal_id = s.anal_id AND h.created_at >= ?3 AND h.created_at < ?4
            WHERE s.first_beat_at >= ?5 AND s.first_beat_at < ?3 AND (?2 IS NULL OR s.first_beat_country = ?2)
         )
        GROUP BY cohortDate, dayOffset, country`,
    )
    .bind(sinceDate, country, yesterday, today, youngSince)
    .all<RetentionRow>()
  return results
}

/** Every D1 query the per-day funnel needs, run in parallel. */
export async function queryFunnelRows(db: D1Database, window: FunnelWindow): Promise<FunnelRows> {
  const { sinceDate, country } = window
  const [downloadsBySource, downloadsByRef, downloadsByReferer, downloadsByUa, installsByChannel, dau, retention] =
    await Promise.all([
      queryDownloadsBySource(db, sinceDate, country),
      queryDownloadsByRef(db, sinceDate, country),
      queryDownloadsByReferer(db, sinceDate, country),
      queryDownloadsByUa(db, sinceDate, country),
      queryInstallsByChannel(db, sinceDate, country),
      queryDau(db, sinceDate, country),
      queryRetention(db, window),
    ])
  return {
    downloadsBySource,
    downloadsByRef,
    downloadsByReferer,
    downloadsByUa,
    installsByChannel,
    dau,
    retention,
  }
}

/** Resolved Listmonk config for the read-only subscriber query; `null` when any piece is missing. */
interface ListmonkReadConfig {
  url: string
  user: string
  token: string
  listIds: number[]
}

/**
 * Resolve the Listmonk config for the funnel's signups column. Reuses the same URL/user/token as the
 * beta-signup route. The funnel counts BOTH the newsletter list (3) and the beta list. The beta list
 * id comes from `LISTMONK_BETA_LIST_ID`; the newsletter list id from `LISTMONK_NEWSLETTER_LIST_ID`
 * (defaults to 3, the live "Cmdr newsletter" list). Returns `null` if URL/user/token are missing, so
 * the funnel reports signups as `null` (unknown) rather than 0.
 */
function resolveListmonkRead(env: Bindings): ListmonkReadConfig | null {
  const { LISTMONK_API_URL: url, LISTMONK_API_USER: user, LISTMONK_API_TOKEN: token } = env
  if (!url || !user || !token) return null
  const listIds: number[] = []
  const newsletterId = env.LISTMONK_NEWSLETTER_LIST_ID ?? 3
  if (typeof newsletterId === 'number') listIds.push(newsletterId)
  if (typeof env.LISTMONK_BETA_LIST_ID === 'number') listIds.push(env.LISTMONK_BETA_LIST_ID)
  if (listIds.length === 0) return null
  return { url, user, token, listIds }
}

interface ListmonkSubscriber {
  created_at: string
}

/**
 * Fetch the `created_at` of every subscriber on the tracked lists created on/after `sinceDate`, then
 * bucket them into per-UTC-day counts. Uses one Listmonk SQL `query` filtering by date, paginating in
 * case there are many. `created_at` is the SUBSCRIBER's creation timestamp, not the per-list join time,
 * so a person who joins a second list later is counted only on their original signup day. Acceptable:
 * almost everyone is on a single list, and this is a coarse acquisition signal, not billing.
 */
async function fetchListmonkSignupsByDay(config: ListmonkReadConfig, sinceDate: string): Promise<Map<string, number>> {
  const byDay = new Map<string, number>()
  const headers = { Authorization: `token ${config.user}:${config.token}` }
  // `sinceDate` is a fixed, validated `YYYY-MM-DD`; embedding it in the SQL literal is safe.
  const sql = `subscribers.created_at >= '${sinceDate}'`
  const perPage = 1000
  let page = 1
  for (;;) {
    const params = new URLSearchParams()
    params.set('query', sql)
    params.set('per_page', String(perPage))
    params.set('page', String(page))
    for (const id of config.listIds) params.append('list_id', String(id))
    const res = await fetch(`${config.url}/api/subscribers?${params.toString()}`, { headers })
    if (!res.ok) throw new Error(`Listmonk subscribers query HTTP ${String(res.status)}`)
    const body: { data?: { results?: ListmonkSubscriber[] } } = await res.json()
    const results = body.data?.results ?? []
    for (const sub of results) {
      // Normalize to the UTC day. Listmonk returns ISO8601 with a `Z`/offset, so Date parses it as UTC.
      const day = new Date(sub.created_at).toISOString().slice(0, 10)
      byDay.set(day, (byDay.get(day) ?? 0) + 1)
    }
    if (results.length < perPage) break
    page++
  }
  return byDay
}

/**
 * GET /admin/funnel?days=N&country=CC
 *
 * Returns `FunnelDay[]`, oldest day first, for the last N UTC days (default 30, 1..400), including today
 * (a partial day). See `FunnelDay` for the per-column meaning and reliability notes. With `country`
 * (ISO-2), downloads filter on the download's country and installs, DAU, and retention on the install's;
 * signups are then `null`. Listmonk signups also degrade to `null` (not 0) when Listmonk is unconfigured
 * or its query fails, so the dashboard can tell "no signups" from "couldn't ask".
 */
funnel.get('/admin/funnel', async (c) => {
  const authError = await verifyAdminAuth(c)
  if (authError) return authError

  const days = parseDays(c.req.query('days'))
  if (days === null) return c.json({ error: invalidDaysMessage }, 400)
  const country = parseCountryFilter(c.req.query('country'))
  if (country === undefined) return c.json({ error: 'Invalid country. Use an ISO 3166-1 alpha-2 code' }, 400)

  const now = new Date()
  const dates = buildDateList(days, now)
  const sinceDate = dates[0]
  const today = dates[dates.length - 1]

  const rows = await queryFunnelRows(c.env.TELEMETRY_DB, { sinceDate, today, country })

  // Listmonk is best-effort: a failure or missing config makes signups `null`, never breaks the funnel.
  // It has no country, so a country view has no signups to show.
  let signupsByDay: Map<string, number> | null = null
  const listmonk = country === null ? resolveListmonkRead(c.env) : null
  if (listmonk) {
    try {
      signupsByDay = await fetchListmonkSignupsByDay(listmonk, sinceDate)
    } catch (e) {
      console.error('Funnel: Listmonk signups query failed:', e)
      signupsByDay = null
    }
  }

  return c.json(assembleFunnel(dates, rows, signupsByDay, now))
})

export { funnel }
