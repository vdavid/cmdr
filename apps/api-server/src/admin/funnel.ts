import { Hono } from 'hono'
import { verifyAdminAuth } from '../admin-auth'
import type { Bindings } from '../types'
import { resolveUaFamily } from '../user-agent'

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
 * Auth: the shared ADMIN_API_TOKEN, same pattern as the other admin endpoints.
 */

const funnel = new Hono<{ Bindings: Bindings }>()

/** Bounds on `?days=`: at least 1, at most 400 (a bit over a year, for year-over-year views). */
const minDays = 1
const maxDays = 400
const defaultDays = 30

/** The bucket for a download or install with no usable country. */
export const unknownCountry = '(unknown)'
/** Cloudflare's codes that aren't countries: `XX` (no country) and `T1` (Tor). */
const nonCountryCodes = new Set(['XX', 'T1'])
const countryPattern = /^[A-Z]{2}$/

/** A stored country as the dashboard shows it: ISO-2 as-is, anything else (`unknown`, `XX`, NULL) under `(unknown)`. */
export function normalizeCountry(value: string | null): string {
  return value !== null && countryPattern.test(value) && !nonCountryCodes.has(value) ? value : unknownCountry
}

/** One UTC day of funnel data. `null` (not 0) means "we can't know this yet", e.g. retention too young. */
export interface FunnelDay {
  /** UTC day, `YYYY-MM-DD`. */
  date: string
  /** Server-side DMG downloads logged that day (raw request count, bots already filtered at write time). */
  downloads: number
  /** Server downloads split by attribution source. Rows before 2026-06-11 land in `other` (NULL source). */
  downloadsBySource: { website: number; homebrew: number; other: number }
  /**
   * Server downloads split by first-touch channel (`ref`): a map of ref value -> count for that day.
   * Downloads with no ref (Homebrew, direct links, return visits in a later session, and rows before
   * migration 0009 which predate the column) are keyed under `"(none)"`. An empty object means the day
   * had no downloads at all.
   */
  downloadsByRef: Record<string, number>
  /**
   * Server downloads split by the `Referer` host of the `/download` hit: a map of host -> count for that
   * day. This illuminates the `(none)` first-touch bucket: a direct link to `/download` (shared on
   * AlternativeTo, a directory, GitHub, Reddit, a forum) carries no `ref` but does arrive with a `Referer`,
   * so its host names the page that drove it. Downloads with no referer (typed URL, privacy browser,
   * referrer-policy strip, Homebrew/curl, and rows before migration 0010) are keyed under `"(none)"`.
   */
  downloadsByReferer: Record<string, number>
  /**
   * Server downloads split by User-Agent family for that day. Cmdr is macOS-only, so a non-macOS client
   * fetching the `.dmg` cannot be a real install. `human` = a Mac browser, Homebrew, or curl/wget (a
   * possible install); `bot` = a Windows/Android/Linux/X11 UA (provably impossible, excluded from
   * `humanInstalls`); `unknown` = anything else, including a NULL UA on pre-capture rows (we can't tell,
   * so it stays counted). See `classifyUaFamily`. Note: the scraper spoofs Mac browser UAs too, so
   * `human` is "could be a real install", not "is one"; only `bot` is the high-confidence exclusion.
   */
  downloadsByUaFamily: { human: number; bot: number; unknown: number }
  /**
   * Downloads minus the provably-impossible `bot` (non-macOS UA) hits, i.e. `human + unknown`. A
   * conservative install signal that drops only the clearly-fake downloads and keeps every ambiguous one.
   */
  humanInstalls: number
  /** Installs whose very first heartbeat ever landed that day (a true new-install count). */
  newInstalls: number
  /**
   * `newInstalls` split by the install's channel (its first non-NULL `installChannel`), summing to
   * `newInstalls`. `unknown` holds installs from clients older than the field.
   */
  newInstallsByChannel: { homebrew: number; direct: number; unknown: number }
  /** Distinct install ids that beat at all that day (true DAU from the heartbeat). */
  dau: number
  /** D7 retention for this cohort: `null` until the cohort is >= 8 days old, else a 0..1 fraction. */
  d7Retention: number | null
  /** Raw count behind `d7Retention` (cohort installs that beat again in [day+7, day+8)). `null` if too young. */
  d7Retained: number | null
  /**
   * The tentative stand-in for D7 while the cohort is 2–7 days old: `day` is the latest fully passed day
   * index n (1..6) and `retained` the cohort installs that beat in [day+n, day+n+1). `null` otherwise,
   * and for a day with no new installs.
   */
  retentionSoFar: { day: number; retained: number } | null
  /**
   * Newsletter + beta-list subscribers created that day (Listmonk). `null` when Listmonk is unconfigured,
   * and under a `country` filter (Listmonk has no country).
   */
  newsletterSignups: number | null
}

interface DownloadBySourceRow {
  date: string
  source: string
  count: number
}

interface DownloadByRefRow {
  date: string
  ref: string
  count: number
}

interface DownloadByRefererRow {
  date: string
  referer: string
  count: number
}

interface DownloadByUaRow {
  date: string
  /** The family stored at write time. NULL on rows written before migration 0013. */
  uaFamily: string | null
  /** The raw UA. NULL once the retention sweep has cleared it, and on pre-0010 rows. */
  userAgent: string | null
  count: number
}

interface InstallsByChannelRow {
  date: string
  /** The install's stored channel; NULL when it never reported one. */
  channel: string | null
  count: number
}

interface DauRow {
  date: string
  dau: number
}

interface RetentionRow {
  cohortDate: string
  /** Whole days from the cohort day to the beat's day, 1..7. */
  dayOffset: number
  retained: number
}

/** Every D1 row set the per-day funnel is assembled from. */
export interface FunnelRows {
  downloadsBySource: DownloadBySourceRow[]
  downloadsByRef: DownloadByRefRow[]
  downloadsByReferer: DownloadByRefererRow[]
  downloadsByUa: DownloadByUaRow[]
  installsByChannel: InstallsByChannelRow[]
  dau: DauRow[]
  retention: RetentionRow[]
}

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
 * every query below).
 */
async function queryDownloadsBySource(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<DownloadBySourceRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COALESCE(source, 'other') AS source, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, source`,
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
 * downloads) and collapses to one row per family per day once the sweep has cleared the raw UAs.
 */
async function queryDownloadsByUa(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<DownloadByUaRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, ua_family AS uaFamily, user_agent AS userAgent, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, ua_family, user_agent`,
    )
    .bind(sinceDate, country)
    .all<DownloadByUaRow>()
  return results
}

/**
 * New installs per day and channel: installs whose FIRST-EVER beat (`install_summary.first_beat_at`)
 * fell inside the window. The summary holds one row per install, so this reads installs, not beats,
 * and an install that first beat months ago never counts as "new" here. `first_beat_at >= 'YYYY-MM-DD'`
 * compares as text against `YYYY-MM-DD HH:MM:SS`, which keeps the index usable.
 */
async function queryInstallsByChannel(
  db: D1Database,
  sinceDate: string,
  country: string | null,
): Promise<InstallsByChannelRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(first_beat_at) AS date, install_channel AS channel, COUNT(*) AS count
         FROM install_summary
         WHERE first_beat_at >= ?1 AND (?2 IS NULL OR country = ?2)
         GROUP BY date, install_channel`,
    )
    .bind(sinceDate, country)
    .all<InstallsByChannelRow>()
  return results
}

/** Distinct install ids that beat per day (true DAU); under a country filter, installs from that country. */
async function queryDau(db: D1Database, sinceDate: string, country: string | null): Promise<DauRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COUNT(DISTINCT anal_id) AS dau
         FROM heartbeat
         WHERE created_at >= ?1
           AND (?2 IS NULL OR anal_id IN (SELECT anal_id FROM install_summary WHERE country = ?2))
         GROUP BY date`,
    )
    .bind(sinceDate, country)
    .all<DauRow>()
  return results
}

/**
 * Retained installs per cohort day and day offset 1..7. An install whose first beat was on day X is
 * retained at offset n if it has ANY beat in [X+n, X+n+1). Offset 7 is D7; offsets 1..6 feed the
 * tentative `retentionSoFar` for cohorts too young for D7. `idx_heartbeat_anal_created` bounds each
 * install's lookup to its first week.
 */
async function queryRetention(db: D1Database, sinceDate: string, country: string | null): Promise<RetentionRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(s.first_beat_at) AS cohortDate,
              CAST(julianday(date(h.created_at)) - julianday(date(s.first_beat_at)) AS INTEGER) AS dayOffset,
              COUNT(DISTINCT s.anal_id) AS retained
         FROM install_summary s
         JOIN heartbeat h ON h.anal_id = s.anal_id
          AND h.created_at >= date(s.first_beat_at, '+1 day')
          AND h.created_at <  date(s.first_beat_at, '+8 days')
        WHERE s.first_beat_at >= ?1 AND (?2 IS NULL OR s.country = ?2)
        GROUP BY cohortDate, dayOffset`,
    )
    .bind(sinceDate, country)
    .all<RetentionRow>()
  return results
}

/** Every D1 query the per-day funnel needs, run in parallel. */
export async function queryFunnelRows(db: D1Database, sinceDate: string, country: string | null): Promise<FunnelRows> {
  const [downloadsBySource, downloadsByRef, downloadsByReferer, downloadsByUa, installsByChannel, dau, retention] =
    await Promise.all([
      queryDownloadsBySource(db, sinceDate, country),
      queryDownloadsByRef(db, sinceDate, country),
      queryDownloadsByReferer(db, sinceDate, country),
      queryDownloadsByUa(db, sinceDate, country),
      queryInstallsByChannel(db, sinceDate, country),
      queryDau(db, sinceDate, country),
      queryRetention(db, sinceDate, country),
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

/** Fold rows into one entry per day, created by `empty` and filled by `add`. The shape every breakdown below shares. */
function byDay<Row extends { date: string }, Entry>(
  rows: Row[],
  empty: () => Entry,
  add: (args: { entry: Entry; row: Row }) => void,
): Map<string, Entry> {
  const map = new Map<string, Entry>()
  for (const row of rows) {
    const entry = map.get(row.date) ?? empty()
    add({ entry, row })
    map.set(row.date, entry)
  }
  return map
}

/** A day's D7 and tentative retention, from its cohort size and age. `retained(n)` looks up offset n. */
function cohortRetention(
  installs: number,
  cohortAgeDays: number,
  retained: (dayOffset: number) => number,
): Pick<FunnelDay, 'd7Retention' | 'd7Retained' | 'retentionSoFar'> {
  // D7 retention is knowable only when BOTH (a) the [day+7, day+8) window has fully passed (cohort day
  // is at least 8 days before today) AND (b) the day actually had a cohort (installs > 0). A day with no
  // new installs has no cohort to retain, so its retention is genuinely undefined (null -> dash), not 0%.
  if (installs === 0) return { d7Retention: null, d7Retained: null, retentionSoFar: null }
  if (cohortAgeDays >= 8) {
    const d7Retained = retained(7)
    return { d7Retention: d7Retained / installs, d7Retained, retentionSoFar: null }
  }
  // Too young for D7 but old enough for day 1: the latest day index whose window has fully passed.
  const day = cohortAgeDays - 1
  const retentionSoFar = day >= 1 ? { day, retained: retained(day) } : null
  return { d7Retention: null, d7Retained: null, retentionSoFar }
}

/**
 * Assemble the per-day funnel array from the raw query rows. Pure so it's unit-testable: pass the date
 * list, the D1 rows, the Listmonk per-day map (or `null` when unconfigured or filtered by country), and
 * `now`. Days with no rows get zeros (not `null`) for the count metrics, because a real query covering
 * the day returned no matches. D7 is `null` for cohorts younger than 8 days (we can't know yet), else
 * the fraction.
 */
export function assembleFunnel(
  dates: string[],
  rows: FunnelRows,
  signupsByDay: Map<string, number> | null,
  now: Date,
): FunnelDay[] {
  const downloadsMap = byDay(
    rows.downloadsBySource,
    () => ({ website: 0, homebrew: 0, other: 0 }),
    ({ entry, row }) => {
      if (row.source === 'website') entry.website += row.count
      else if (row.source === 'homebrew') entry.homebrew += row.count
      else entry.other += row.count
    },
  )
  const refMap = byDay(
    rows.downloadsByRef,
    (): Record<string, number> => ({}),
    ({ entry, row }) => {
      entry[row.ref] = (entry[row.ref] ?? 0) + row.count
    },
  )
  const refererMap = byDay(
    rows.downloadsByReferer,
    (): Record<string, number> => ({}),
    ({ entry, row }) => {
      entry[row.referer] = (entry[row.referer] ?? 0) + row.count
    },
  )
  const uaFamilyMap = byDay(
    rows.downloadsByUa,
    () => ({ human: 0, bot: 0, unknown: 0 }),
    ({ entry, row }) => {
      entry[resolveUaFamily(row)] += row.count
    },
  )
  const channelMap = byDay(
    rows.installsByChannel,
    () => ({ homebrew: 0, direct: 0, unknown: 0 }),
    ({ entry, row }) => {
      if (row.channel === 'homebrew') entry.homebrew += row.count
      else if (row.channel === 'direct') entry.direct += row.count
      else entry.unknown += row.count
    },
  )
  const dauMap = new Map(rows.dau.map((r) => [r.date, r.dau]))
  const retentionMap = new Map(rows.retention.map((r) => [`${r.cohortDate}:${String(r.dayOffset)}`, r.retained]))

  const todayUtcMs = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())

  return dates.map((date) => {
    const bySource = downloadsMap.get(date) ?? { website: 0, homebrew: 0, other: 0 }
    const downloads = bySource.website + bySource.homebrew + bySource.other
    const uaFamily = uaFamilyMap.get(date) ?? { human: 0, bot: 0, unknown: 0 }
    const byChannel = channelMap.get(date) ?? { homebrew: 0, direct: 0, unknown: 0 }
    const installs = byChannel.homebrew + byChannel.direct + byChannel.unknown

    const cohortAgeDays = Math.round((todayUtcMs - Date.parse(`${date}T00:00:00Z`)) / 86_400_000)
    const retention = cohortRetention(
      installs,
      cohortAgeDays,
      (dayOffset) => retentionMap.get(`${date}:${String(dayOffset)}`) ?? 0,
    )

    return {
      date,
      downloads,
      downloadsBySource: bySource,
      downloadsByRef: refMap.get(date) ?? {},
      downloadsByReferer: refererMap.get(date) ?? {},
      downloadsByUaFamily: uaFamily,
      humanInstalls: uaFamily.human + uaFamily.unknown,
      newInstalls: installs,
      newInstallsByChannel: byChannel,
      dau: dauMap.get(date) ?? 0,
      ...retention,
      newsletterSignups: signupsByDay ? (signupsByDay.get(date) ?? 0) : null,
    }
  })
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

  const rows = await queryFunnelRows(c.env.TELEMETRY_DB, sinceDate, country)

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
