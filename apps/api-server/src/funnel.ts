import { Hono } from 'hono'
import { type Bindings, verifyAdminAuth } from './types'

/**
 * Per-day acquisition funnel for the analytics dashboard. One admin endpoint returns a per-UTC-day
 * array for the last N days so the dashboard can show downloads -> installs -> retention -> signups
 * in a single table, instead of stitching it together from the per-metric endpoints.
 *
 * Everything is bucketed by UTC day (D1 `date()` is UTC, and the Listmonk dates are normalized to UTC
 * here), so a "day" means the same window across every column.
 *
 * Auth: the shared ADMIN_API_TOKEN, same pattern as the other admin endpoints.
 */

const funnel = new Hono<{ Bindings: Bindings }>()

/** Bounds on `?days=`: at least 1, at most 90 (keeps the Listmonk page count and SQL scans small). */
const minDays = 1
const maxDays = 90
const defaultDays = 30

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
  /** Installs whose very first heartbeat ever landed that day (a true new-install count). */
  newInstalls: number
  /** Distinct install ids that beat at all that day (true DAU from the heartbeat). */
  dau: number
  /** D7 retention for this cohort: `null` until the cohort is >= 8 days old, else a 0..1 fraction. */
  d7Retention: number | null
  /** Raw count behind `d7Retention` (cohort installs that beat again in [day+7, day+8)). `null` if too young. */
  d7Retained: number | null
  /** Newsletter + beta-list subscribers created that day (Listmonk). `null` when Listmonk is unconfigured. */
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

interface InstallDayRow {
  date: string
  newInstalls: number
}

interface DauRow {
  date: string
  dau: number
}

interface D7Row {
  cohortDate: string
  retained: number
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
 * Per-day server downloads, split by source. Bots are filtered at write time, so this is the raw
 * request count of real DMG fetches. The `source` column is NULL for rows written before migration
 * 0008; those COALESCE to `other`.
 */
async function queryDownloadsBySource(db: D1Database, sinceDate: string): Promise<DownloadBySourceRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COALESCE(source, 'other') AS source, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1
         GROUP BY date, source`,
    )
    .bind(sinceDate)
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
async function queryDownloadsByRef(db: D1Database, sinceDate: string): Promise<DownloadByRefRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COALESCE(ref, '(none)') AS ref, COUNT(*) AS count
         FROM downloads
         WHERE date(created_at) >= ?1
         GROUP BY date, ref`,
    )
    .bind(sinceDate)
    .all<DownloadByRefRow>()
  return results
}

/**
 * New installs per day: the count of install ids whose FIRST-EVER heartbeat fell on that day. We take
 * `MIN(created_at)` per `anal_id` across the WHOLE table (no date filter on the inner query, so an
 * install that first beat months ago never counts as "new" inside the window), then bucket those first
 * beats by UTC day and keep the ones inside the window.
 */
async function queryNewInstalls(db: D1Database, sinceDate: string): Promise<InstallDayRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(firstBeat) AS date, COUNT(*) AS newInstalls
         FROM (SELECT anal_id, MIN(created_at) AS firstBeat FROM heartbeat GROUP BY anal_id)
         WHERE date(firstBeat) >= ?1
         GROUP BY date`,
    )
    .bind(sinceDate)
    .all<InstallDayRow>()
  return results
}

/** Distinct install ids that beat per day (true DAU). */
async function queryDau(db: D1Database, sinceDate: string): Promise<DauRow[]> {
  const { results } = await db
    .prepare(
      `SELECT date(created_at) AS date, COUNT(DISTINCT anal_id) AS dau
         FROM heartbeat
         WHERE date(created_at) >= ?1
         GROUP BY date`,
    )
    .bind(sinceDate)
    .all<DauRow>()
  return results
}

/**
 * D7 retained count per cohort day. For each install whose first heartbeat was on cohort day X, count
 * it as retained if it has ANY heartbeat in the window [X+7d, X+8d) (i.e. exactly the 7th day after
 * install). The join pairs each install's first-beat day against its later beats; we bucket by the
 * cohort (first-beat) day. Cohorts younger than 8 days are filled with `null` later, not here.
 */
async function queryD7Retained(db: D1Database, sinceDate: string): Promise<D7Row[]> {
  const { results } = await db
    .prepare(
      `WITH firsts AS (
           SELECT anal_id, date(MIN(created_at)) AS cohortDate FROM heartbeat GROUP BY anal_id
         )
         SELECT f.cohortDate AS cohortDate, COUNT(DISTINCT f.anal_id) AS retained
           FROM firsts f
           JOIN heartbeat h ON h.anal_id = f.anal_id
            AND date(h.created_at) >= date(f.cohortDate, '+7 days')
            AND date(h.created_at) <  date(f.cohortDate, '+8 days')
          WHERE f.cohortDate >= ?1
          GROUP BY f.cohortDate`,
    )
    .bind(sinceDate)
    .all<D7Row>()
  return results
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
 * Assemble the per-day funnel array from the raw query rows. Pure so it's unit-testable: pass the date
 * list, the D1 rows, the Listmonk per-day map (or `null` when unconfigured), and `now`. Days with no
 * rows get zeros (not `null`) for the count metrics, because a real query covering the day returned no
 * matches. D7 is `null` for cohorts younger than 8 days (we can't know yet), else the fraction.
 */
export function assembleFunnel(
  dates: string[],
  downloadsBySource: DownloadBySourceRow[],
  downloadsByRef: DownloadByRefRow[],
  newInstalls: InstallDayRow[],
  dau: DauRow[],
  d7Retained: D7Row[],
  signupsByDay: Map<string, number> | null,
  now: Date,
): FunnelDay[] {
  const downloadsMap = new Map<string, { website: number; homebrew: number; other: number }>()
  for (const row of downloadsBySource) {
    const entry = downloadsMap.get(row.date) ?? { website: 0, homebrew: 0, other: 0 }
    if (row.source === 'website') entry.website += row.count
    else if (row.source === 'homebrew') entry.homebrew += row.count
    else entry.other += row.count
    downloadsMap.set(row.date, entry)
  }
  const refMap = new Map<string, Record<string, number>>()
  for (const row of downloadsByRef) {
    const entry = refMap.get(row.date) ?? {}
    entry[row.ref] = (entry[row.ref] ?? 0) + row.count
    refMap.set(row.date, entry)
  }
  const newInstallsMap = new Map(newInstalls.map((r) => [r.date, r.newInstalls]))
  const dauMap = new Map(dau.map((r) => [r.date, r.dau]))
  const d7Map = new Map(d7Retained.map((r) => [r.cohortDate, r.retained]))

  const todayUtcMs = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())

  return dates.map((date) => {
    const bySource = downloadsMap.get(date) ?? { website: 0, homebrew: 0, other: 0 }
    const downloads = bySource.website + bySource.homebrew + bySource.other
    const installs = newInstallsMap.get(date) ?? 0

    // D7 retention is knowable only when BOTH (a) the [day+7, day+8) window has fully passed (cohort day
    // is at least 8 days before today) AND (b) the day actually had a cohort (installs > 0). A day with no
    // new installs has no cohort to retain, so its retention is genuinely undefined (null -> dash), not 0%.
    const cohortMs = Date.parse(`${date}T00:00:00Z`)
    const cohortAgeDays = Math.round((todayUtcMs - cohortMs) / 86_400_000)
    const d7Knowable = cohortAgeDays >= 8 && installs > 0
    const retained = d7Knowable ? (d7Map.get(date) ?? 0) : null
    const d7Retention = d7Knowable ? (retained as number) / installs : null

    return {
      date,
      downloads,
      downloadsBySource: bySource,
      downloadsByRef: refMap.get(date) ?? {},
      newInstalls: installs,
      dau: dauMap.get(date) ?? 0,
      d7Retention,
      d7Retained: retained,
      newsletterSignups: signupsByDay ? (signupsByDay.get(date) ?? 0) : null,
    }
  })
}

/**
 * GET /admin/funnel?days=N
 *
 * Returns `FunnelDay[]`, oldest day first, for the last N UTC days (default 30, clamped to 1..90),
 * including today (a partial day). See `FunnelDay` for the per-column meaning and reliability notes.
 * Listmonk signups degrade to `null` (not 0) when Listmonk is unconfigured or its query fails, so the
 * dashboard can tell "no signups" from "couldn't ask".
 */
funnel.get('/admin/funnel', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const rawDays = parseInt(c.req.query('days') ?? String(defaultDays), 10)
  if (!Number.isFinite(rawDays) || rawDays < minDays || rawDays > maxDays) {
    return c.json({ error: `Invalid days. Use an integer ${String(minDays)}..${String(maxDays)}` }, 400)
  }

  const now = new Date()
  const dates = buildDateList(rawDays, now)
  const sinceDate = dates[0]

  const db = c.env.TELEMETRY_DB
  const [downloadsBySource, downloadsByRef, newInstalls, dau, d7Retained] = await Promise.all([
    queryDownloadsBySource(db, sinceDate),
    queryDownloadsByRef(db, sinceDate),
    queryNewInstalls(db, sinceDate),
    queryDau(db, sinceDate),
    queryD7Retained(db, sinceDate),
  ])

  // Listmonk is best-effort: a failure or missing config makes signups `null`, never breaks the funnel.
  let signupsByDay: Map<string, number> | null = null
  const listmonk = resolveListmonkRead(c.env)
  if (listmonk) {
    try {
      signupsByDay = await fetchListmonkSignupsByDay(listmonk, sinceDate)
    } catch (e) {
      console.error('Funnel: Listmonk signups query failed:', e)
      signupsByDay = null
    }
  }

  const funnelDays = assembleFunnel(
    dates,
    downloadsBySource,
    downloadsByRef,
    newInstalls,
    dau,
    d7Retained,
    signupsByDay,
    now,
  )
  return c.json(funnelDays)
})

export { funnel }
