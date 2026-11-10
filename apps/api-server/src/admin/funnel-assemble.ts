import { resolveUaFamily } from '../user-agent'

/**
 * The per-day funnel's contract (`FunnelDay`), the row sets its queries return (`FunnelRows`), and the
 * pure assembly from one to the other. The queries and the route live in `funnel.ts`.
 */

/** The bucket for a download or install with no usable country. */
export const unknownCountry = '(unknown)'
/** Cloudflare's codes that aren't countries: `XX` (no country) and `T1` (Tor). */
const nonCountryCodes = new Set(['XX', 'T1'])
export const countryPattern = /^[A-Z]{2}$/

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
  retentionSoFar: RetentionSoFar | null
  /** D28 retention, as `d7Retention` with 28 for 7: `null` until the cohort is >= 29 days old. */
  d28Retention: number | null
  /** Raw count behind `d28Retention` (cohort installs that beat in [day+28, day+29)). `null` if too young. */
  d28Retained: number | null
  /** The tentative stand-in for D28 while the cohort is 8–28 days old, as `retentionSoFar` (`day` 7..27). */
  d28SoFar: RetentionSoFar | null
  /**
   * Newsletter + beta-list subscribers created that day (Listmonk). `null` when Listmonk is unconfigured,
   * and under a `country` filter (Listmonk has no country).
   */
  newsletterSignups: number | null
  /**
   * The day's numbers per country: ISO-2 or `"(unknown)"`, a country with nothing that day omitted. The
   * slices sum to the day's own fields, so the dashboard can switch countries without a request. A
   * download counts under its own country, an install under its first beat's.
   */
  byCountry: Record<string, CountryDay>
}

/** The latest fully passed day index n of a cohort too young for its horizon, and the installs back that day. */
export interface RetentionSoFar {
  day: number
  retained: number
}

/** One country's slice of a `FunnelDay`, every field meaning what the same-named `FunnelDay` one does. */
export interface CountryDay {
  /** `FunnelDay.downloads` for this country. */
  serverDownloads: number
  humanInstalls: number
  downloadsBySource: { website: number; homebrew: number; other: number }
  newInstalls: number
  newInstallsByChannel: { homebrew: number; direct: number; unknown: number }
  d7Retained: number | null
  d28Retained: number | null
  retentionSoFar: RetentionSoFar | null
  d28SoFar: RetentionSoFar | null
}

export interface DownloadBySourceRow {
  date: string
  source: string
  /** The download's own stored country, before `normalizeCountry`. */
  country: string | null
  count: number
}

export interface DownloadByRefRow {
  date: string
  ref: string
  count: number
}

export interface DownloadByRefererRow {
  date: string
  referer: string
  count: number
}

export interface DownloadByUaRow {
  date: string
  /** The family stored at write time. NULL on rows written before migration 0013. */
  uaFamily: string | null
  /** The raw UA. NULL once the retention sweep has cleared it, and on pre-0010 rows. */
  userAgent: string | null
  country: string | null
  count: number
}

export interface InstallsByChannelRow {
  date: string
  /** The install's stored channel; NULL when it never reported one. */
  channel: string | null
  /** The install's first-beat country; NULL when that beat carried none. */
  country: string | null
  count: number
}

export interface DauRow {
  date: string
  dau: number
}

export interface RetentionRow {
  cohortDate: string
  /** Whole days from the cohort day to the beat's day: 7, 28, or the cohort's latest fully passed day. */
  dayOffset: number
  /** The install's first-beat country; NULL when that beat carried none. */
  country: string | null
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

/** Fold rows into one entry per day, created by `empty` and filled by `add`. The ref and referer breakdowns share it. */
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

/** One day's raw counts for one country, before any derivation. */
interface Tally {
  bySource: { website: number; homebrew: number; other: number }
  uaFamily: { human: number; bot: number; unknown: number }
  byChannel: { homebrew: number; direct: number; unknown: number }
  /** Installs of this day's cohort retained at each day offset the retention query read. */
  retained: Map<number, number>
}

function emptyTally(): Tally {
  return {
    bySource: { website: 0, homebrew: 0, other: 0 },
    uaFamily: { human: 0, bot: 0, unknown: 0 },
    byChannel: { homebrew: 0, direct: 0, unknown: 0 },
    retained: new Map(),
  }
}

/** The tallies summed, so a day's own numbers are its country slices added up and can't drift from them. */
function sumTallies(tallies: Iterable<Tally>): Tally {
  const total = emptyTally()
  for (const tally of tallies) {
    for (const key of ['website', 'homebrew', 'other'] as const) total.bySource[key] += tally.bySource[key]
    for (const key of ['human', 'bot', 'unknown'] as const) total.uaFamily[key] += tally.uaFamily[key]
    for (const key of ['homebrew', 'direct', 'unknown'] as const) total.byChannel[key] += tally.byChannel[key]
    for (const [offset, count] of tally.retained) total.retained.set(offset, (total.retained.get(offset) ?? 0) + count)
  }
  return total
}

/** Retention at one horizon: the known count and rate, or the tentative stand-in while the cohort is too young. */
interface HorizonRetention {
  retained: number | null
  rate: number | null
  soFar: RetentionSoFar | null
}

/**
 * Retention at `horizon` days (7 for D7, 28 for D28) for a cohort of `installs` that is `cohortAgeDays`
 * old. Known once [day+horizon, day+horizon+1) has fully passed, so at age horizon + 1. Before that,
 * from age `soFromDay + 1`, `soFar` stands in at the latest fully passed day n = age − 1. A day with no
 * installs has no cohort, so every field is `null` (a dash), never 0%.
 */
function horizonRetention(args: {
  installs: number
  cohortAgeDays: number
  horizon: number
  soFarFromDay: number
  retained: Map<number, number>
}): HorizonRetention {
  const { installs, cohortAgeDays, horizon, soFarFromDay, retained } = args
  if (installs === 0) return { retained: null, rate: null, soFar: null }
  if (cohortAgeDays >= horizon + 1) {
    const count = retained.get(horizon) ?? 0
    return { retained: count, rate: count / installs, soFar: null }
  }
  const day = cohortAgeDays - 1
  const soFar = day >= soFarFromDay ? { day, retained: retained.get(day) ?? 0 } : null
  return { retained: null, rate: null, soFar }
}

/** A tally's derived numbers: what both `FunnelDay` and `CountryDay` are built from. */
function derive(tally: Tally, cohortAgeDays: number) {
  const { bySource, uaFamily, byChannel, retained } = tally
  const installs = byChannel.homebrew + byChannel.direct + byChannel.unknown
  return {
    downloads: bySource.website + bySource.homebrew + bySource.other,
    humanInstalls: uaFamily.human + uaFamily.unknown,
    installs,
    d7: horizonRetention({ installs, cohortAgeDays, horizon: 7, soFarFromDay: 1, retained }),
    // D28's stand-in starts where D7 becomes known, so a cohort always shows exactly one of the two.
    d28: horizonRetention({ installs, cohortAgeDays, horizon: 28, soFarFromDay: 7, retained }),
  }
}

function countryDay(tally: Tally, cohortAgeDays: number): CountryDay {
  const { downloads, humanInstalls, installs, d7, d28 } = derive(tally, cohortAgeDays)
  return {
    serverDownloads: downloads,
    humanInstalls,
    downloadsBySource: tally.bySource,
    newInstalls: installs,
    newInstallsByChannel: tally.byChannel,
    d7Retained: d7.retained,
    d28Retained: d28.retained,
    retentionSoFar: d7.soFar,
    d28SoFar: d28.soFar,
  }
}

/** Every row's counts filed under its day and normalized country. */
function tallyRows(rows: FunnelRows): Map<string, Map<string, Tally>> {
  const days = new Map<string, Map<string, Tally>>()
  const tally = (date: string, rawCountry: string | null): Tally => {
    const countries = days.get(date) ?? new Map<string, Tally>()
    days.set(date, countries)
    const country = normalizeCountry(rawCountry)
    const existing = countries.get(country)
    if (existing) return existing
    const created = emptyTally()
    countries.set(country, created)
    return created
  }
  for (const row of rows.downloadsBySource) {
    const { bySource } = tally(row.date, row.country)
    if (row.source === 'website') bySource.website += row.count
    else if (row.source === 'homebrew') bySource.homebrew += row.count
    else bySource.other += row.count
  }
  for (const row of rows.downloadsByUa) tally(row.date, row.country).uaFamily[resolveUaFamily(row)] += row.count
  for (const row of rows.installsByChannel) {
    const { byChannel } = tally(row.date, row.country)
    if (row.channel === 'homebrew') byChannel.homebrew += row.count
    else if (row.channel === 'direct') byChannel.direct += row.count
    else byChannel.unknown += row.count
  }
  for (const row of rows.retention) {
    const { retained } = tally(row.cohortDate, row.country)
    retained.set(row.dayOffset, (retained.get(row.dayOffset) ?? 0) + row.retained)
  }
  return days
}

/**
 * Assemble the per-day funnel array from the raw query rows. Pure so it's unit-testable: pass the date
 * list, the D1 rows, the Listmonk per-day map (or `null` when unconfigured or filtered by country), and
 * `now`. Days with no rows get zeros (not `null`) for the count metrics, because a real query covering
 * the day returned no matches. D7 and D28 are `null` while the cohort is too young (we can't know yet).
 * A day's own numbers are its `byCountry` slices summed, so the two always agree.
 */
export function assembleFunnel(
  dates: string[],
  rows: FunnelRows,
  signupsByDay: Map<string, number> | null,
  now: Date,
): FunnelDay[] {
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
  const dauMap = new Map(rows.dau.map((r) => [r.date, r.dau]))
  const tallies = tallyRows(rows)
  const todayUtcMs = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())

  return dates.map((date) => {
    const countries = tallies.get(date) ?? new Map<string, Tally>()
    const cohortAgeDays = Math.round((todayUtcMs - Date.parse(`${date}T00:00:00Z`)) / 86_400_000)
    const total = sumTallies(countries.values())
    const { downloads, humanInstalls, installs, d7, d28 } = derive(total, cohortAgeDays)
    const byCountry: Record<string, CountryDay> = {}
    for (const [country, tally] of countries) byCountry[country] = countryDay(tally, cohortAgeDays)

    return {
      date,
      downloads,
      downloadsBySource: total.bySource,
      downloadsByRef: refMap.get(date) ?? {},
      downloadsByReferer: refererMap.get(date) ?? {},
      downloadsByUaFamily: total.uaFamily,
      humanInstalls,
      newInstalls: installs,
      newInstallsByChannel: total.byChannel,
      dau: dauMap.get(date) ?? 0,
      d7Retention: d7.rate,
      d7Retained: d7.retained,
      retentionSoFar: d7.soFar,
      d28Retention: d28.rate,
      d28Retained: d28.retained,
      d28SoFar: d28.soFar,
      newsletterSignups: signupsByDay ? (signupsByDay.get(date) ?? 0) : null,
      byCountry,
    }
  })
}
