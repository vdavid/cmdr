/**
 * The funnel's SQL against a REAL SQLite with every migration, fed through the same INSERTs
 * `/heartbeat` runs. The route tests mock D1 and can only prove assembly; only an engine can prove
 * the install summary, the day offsets, and the country filter select the right rows.
 */
import { describe, expect, it } from 'vitest'
import { createSqliteD1, type SqliteD1 } from '../sqlite-d1-test-helper'
import { insertHeartbeatSql, upsertInstallSummarySql } from '../telemetry/heartbeat'
import { assembleCountries, queryCountryRows } from './funnel-countries'
import { assembleFunnel, buildDateList, queryFunnelRows, shiftDate, type CountryDay, type FunnelDay } from './funnel'

type DatabaseSync = SqliteD1['sqlite']

interface Beat {
  id: string
  at: string
  country?: string | null
  channel?: string | null
}

function beat(db: DatabaseSync, b: Beat): void {
  const { id, at, country = null, channel = null } = b
  db.prepare(insertHeartbeatSql).run(id, '1.2.3', '26.0', 'aarch64', 'release', null, null, country, channel, at)
  db.prepare(upsertInstallSummarySql).run(id, at, country, channel)
}

function download(db: DatabaseSync, at: string, country: string, uaFamily: string, source = 'website'): void {
  db.prepare(
    `INSERT INTO downloads (created_at, app_version, arch, country, continent, source, ua_family) VALUES (?, '1.2.3', 'aarch64', ?, 'EU', ?, ?)`,
  ).run(at, country, source, uaFamily)
}

/** "Today" for most tests: the 2026-06-01 cohort is 39 days old, so its D7 and D28 are both known. */
const today = '2026-07-10'

/**
 * - `anal_a`: first beat 2026-06-01 from Sweden via Homebrew; back on day one, day seven, and day 28.
 * - `anal_b`: first beat 2026-06-01 with no country, then Germany; installed directly; back on day eight
 *   and day 29, each one past its window. Its country didn't come with the first beat, so it counts
 *   under "(unknown)", never Germany.
 * - `anal_c`: first beat before the window, so it's active on 2026-06-01 but not new.
 * - `anal_d`: first beat 2026-06-20 from Sweden, 20 days before `today`; back yesterday (day 19).
 * - Downloads: two from Sweden (one a non-macOS bot), one from Cloudflare's `XX`, one stored as `unknown`.
 *
 * `until` (a `YYYY-MM-DD` "today") leaves out everything after that day, as production never has rows
 * from the future.
 */
function seeded(until = today): D1Database & { sqlite: DatabaseSync } {
  const { d1, sqlite } = createSqliteD1()
  const end = shiftDate(until, 1)
  const beats: Beat[] = [
    { id: 'anal_a', at: '2026-06-01 09:00:00', country: 'SE', channel: 'homebrew' },
    { id: 'anal_a', at: '2026-06-02 09:00:00', country: 'SE', channel: 'homebrew' },
    { id: 'anal_a', at: '2026-06-08 23:59:59', country: 'SE', channel: 'homebrew' },
    { id: 'anal_a', at: '2026-06-29 12:00:00', country: 'SE', channel: 'homebrew' },
    { id: 'anal_b', at: '2026-06-01 20:00:00', channel: 'direct' },
    { id: 'anal_b', at: '2026-06-01 21:00:00', country: 'DE', channel: 'direct' },
    { id: 'anal_b', at: '2026-06-09 00:00:00', country: 'DE', channel: 'direct' },
    { id: 'anal_b', at: '2026-06-30 00:00:00', country: 'DE', channel: 'direct' },
    { id: 'anal_c', at: '2026-05-20 09:00:00' },
    { id: 'anal_c', at: '2026-06-01 10:00:00' },
    { id: 'anal_d', at: '2026-06-20 09:00:00', country: 'SE' },
    { id: 'anal_d', at: '2026-07-09 09:00:00', country: 'SE' },
  ]
  for (const b of beats) if (b.at < end) beat(sqlite, b)
  download(sqlite, '2026-05-30 08:00:00', 'SE', 'human', 'homebrew')
  download(sqlite, '2026-06-01 08:00:00', 'SE', 'bot')
  download(sqlite, '2026-06-01 08:00:00', 'XX', 'human')
  download(sqlite, '2026-06-02 08:00:00', 'unknown', 'unknown')
  return Object.assign(d1, { sqlite })
}

/** Rows as plain objects: `node:sqlite` hands back null-prototype ones. */
function plain<T>(rows: T[]): T[] {
  return rows.map((row) => ({ ...row }))
}

const byKeys = (a: object, b: object) => JSON.stringify(a).localeCompare(JSON.stringify(b))

describe('the per-day funnel queries against real SQLite', () => {
  it('counts new installs per first-beat day, channel, and first-beat country', async () => {
    const rows = await queryFunnelRows(seeded(), { sinceDate: '2026-06-01', today, country: null })
    expect(plain(rows.installsByChannel).sort(byKeys)).toEqual(
      [
        { date: '2026-06-01', channel: 'direct', country: null, count: 1 },
        { date: '2026-06-01', channel: 'homebrew', country: 'SE', count: 1 },
        { date: '2026-06-20', channel: null, country: 'SE', count: 1 },
      ].sort(byKeys),
    )
  })

  it('reads retention at day seven, day 28, and yesterday for a cohort too young for D28', async () => {
    const rows = await queryFunnelRows(seeded(), { sinceDate: '2026-06-01', today, country: null })
    // `anal_a`'s day-one beat is no window's business once the cohort is old; `anal_b` misses both windows.
    expect(plain(rows.retention).sort(byKeys)).toEqual(
      [
        { cohortDate: '2026-06-01', dayOffset: 7, country: 'SE', retained: 1 },
        { cohortDate: '2026-06-01', dayOffset: 28, country: 'SE', retained: 1 },
        { cohortDate: '2026-06-20', dayOffset: 19, country: 'SE', retained: 1 },
      ].sort(byKeys),
    )
  })

  it('counts an install once when yesterday is also its day seven', async () => {
    // Today 2026-06-09: the 2026-06-01 cohort is eight days old, so yesterday is day seven.
    const rows = await queryFunnelRows(seeded('2026-06-09'), {
      sinceDate: '2026-06-01',
      today: '2026-06-09',
      country: null,
    })
    expect(plain(rows.retention).filter((r) => r.cohortDate === '2026-06-01')).toEqual([
      { cohortDate: '2026-06-01', dayOffset: 7, country: 'SE', retained: 1 },
    ])
  })

  it('reads the day-one beat as retention so far while the cohort is two days old', async () => {
    const rows = await queryFunnelRows(seeded('2026-06-03'), {
      sinceDate: '2026-06-01',
      today: '2026-06-03',
      country: null,
    })
    expect(plain(rows.retention)).toEqual([{ cohortDate: '2026-06-01', dayOffset: 1, country: 'SE', retained: 1 }])
  })

  it('counts every install that beat as active, new or not', async () => {
    const rows = await queryFunnelRows(seeded(), { sinceDate: '2026-06-01', today, country: null })
    expect(plain(rows.dau).find((r) => r.date === '2026-06-01')).toEqual({ date: '2026-06-01', dau: 3 })
  })

  it("filters installs, DAU, and retention by the install's first-beat country", async () => {
    const sweden = await queryFunnelRows(seeded(), { sinceDate: '2026-06-01', today, country: 'SE' })
    expect(plain(sweden.installsByChannel).sort(byKeys)).toEqual(
      [
        { date: '2026-06-01', channel: 'homebrew', country: 'SE', count: 1 },
        { date: '2026-06-20', channel: null, country: 'SE', count: 1 },
      ].sort(byKeys),
    )
    expect(plain(sweden.dau).find((r) => r.date === '2026-06-01')).toEqual({ date: '2026-06-01', dau: 1 })
    expect(sweden.retention).toHaveLength(3)

    // `anal_b` gained Germany only on a later beat, so Germany has no installs at all.
    const germany = await queryFunnelRows(seeded(), { sinceDate: '2026-06-01', today, country: 'DE' })
    expect(germany.installsByChannel).toEqual([])
    expect(germany.dau).toEqual([])
    expect(germany.retention).toEqual([])
  })

  it("filters downloads by the download's own country, and tags each row with it", async () => {
    const rows = await queryFunnelRows(seeded(), { sinceDate: '2026-05-30', today, country: 'SE' })
    expect(plain(rows.downloadsBySource).sort(byKeys)).toEqual(
      [
        { date: '2026-05-30', source: 'homebrew', country: 'SE', count: 1 },
        { date: '2026-06-01', source: 'website', country: 'SE', count: 1 },
      ].sort(byKeys),
    )
    for (const set of [rows.downloadsByRef, rows.downloadsByReferer, rows.downloadsByUa]) {
      expect(set.reduce((sum, r) => sum + r.count, 0)).toBe(2)
    }
    expect(rows.downloadsByUa.every((r) => r.country === 'SE')).toBe(true)
  })
})

describe('the per-day funnel assembled from real SQLite', () => {
  const now = new Date(`${today}T12:00:00Z`)

  async function funnelDays(): Promise<FunnelDay[]> {
    const dates = buildDateList(45, now)
    const rows = await queryFunnelRows(seeded(), { sinceDate: dates[0], today, country: null })
    return assembleFunnel(dates, rows, null, now)
  }

  it("sums each day's country slices to the day's own numbers", async () => {
    const days = await funnelDays()
    const sum = (day: FunnelDay, pick: (c: CountryDay) => number | null) =>
      Object.values(day.byCountry).reduce((total, c) => total + (pick(c) ?? 0), 0)
    for (const day of days) {
      expect(sum(day, (c) => c.serverDownloads)).toBe(day.downloads)
      expect(sum(day, (c) => c.humanInstalls)).toBe(day.humanInstalls)
      for (const source of ['website', 'homebrew', 'other'] as const) {
        expect(sum(day, (c) => c.downloadsBySource[source])).toBe(day.downloadsBySource[source])
      }
      expect(sum(day, (c) => c.newInstalls)).toBe(day.newInstalls)
      for (const channel of ['homebrew', 'direct', 'unknown'] as const) {
        expect(sum(day, (c) => c.newInstallsByChannel[channel])).toBe(day.newInstallsByChannel[channel])
      }
      expect(sum(day, (c) => c.d7Retained)).toBe(day.d7Retained ?? 0)
      expect(sum(day, (c) => c.d28Retained)).toBe(day.d28Retained ?? 0)
      expect(sum(day, (c) => c.retentionSoFar?.retained ?? null)).toBe(day.retentionSoFar?.retained ?? 0)
      expect(sum(day, (c) => c.d28SoFar?.retained ?? null)).toBe(day.d28SoFar?.retained ?? 0)
    }
  })

  it('splits the 2026-06-01 cohort into Sweden and "(unknown)", with D7 and D28', async () => {
    const day = (await funnelDays()).find((d) => d.date === '2026-06-01')
    expect(day).toMatchObject({ newInstalls: 2, d7Retained: 1, d28Retained: 1, d28Retention: 0.5 })
    expect(day?.byCountry).toEqual({
      SE: {
        serverDownloads: 1,
        humanInstalls: 0,
        downloadsBySource: { website: 1, homebrew: 0, other: 0 },
        newInstalls: 1,
        newInstallsByChannel: { homebrew: 1, direct: 0, unknown: 0 },
        d7Retained: 1,
        d28Retained: 1,
        retentionSoFar: null,
        d28SoFar: null,
      },
      '(unknown)': {
        serverDownloads: 1,
        humanInstalls: 1,
        downloadsBySource: { website: 1, homebrew: 0, other: 0 },
        newInstalls: 1,
        newInstallsByChannel: { homebrew: 0, direct: 1, unknown: 0 },
        d7Retained: 0,
        d28Retained: 0,
        retentionSoFar: null,
        d28SoFar: null,
      },
    })
  })

  it('reports D28 so far for a cohort 20 days old, from its beat yesterday', async () => {
    const day = (await funnelDays()).find((d) => d.date === '2026-06-20')
    expect(day).toMatchObject({ d28Retained: null, d28Retention: null, d28SoFar: { day: 19, retained: 1 } })
    expect(day?.byCountry.SE).toMatchObject({ d28Retained: null, d28SoFar: { day: 19, retained: 1 } })
  })

  it('omits a country with nothing that day', async () => {
    const day = (await funnelDays()).find((d) => d.date === '2026-06-20')
    expect(Object.keys(day?.byCountry ?? {})).toEqual(['SE'])
    expect((await funnelDays()).find((d) => d.date === '2026-07-01')?.byCountry).toEqual({})
  })
})

describe('the per-country funnel against real SQLite', () => {
  it('sums each country, folds non-countries into "(unknown)", and sorts by installs, then downloads', async () => {
    const countries = assembleCountries(await queryCountryRows(seeded(), { sinceDate: '2026-05-30', today }))
    // `anal_b` gained Germany on a later beat, so Germany isn't in the table at all.
    expect(countries).toEqual([
      {
        country: 'SE',
        serverDownloads: 2,
        humanInstalls: 1,
        newInstalls: 2,
        d7Cohort: 2,
        d7Retained: 1,
        d28Cohort: 1,
        d28Retained: 1,
      },
      {
        country: '(unknown)',
        serverDownloads: 2,
        humanInstalls: 2,
        newInstalls: 1,
        d7Cohort: 1,
        d7Retained: 0,
        d28Cohort: 1,
        d28Retained: 0,
      },
    ])
  })

  it('leaves cohorts too young for D7 or D28 out of those counts, but not out of new installs', async () => {
    // Today 2026-06-05: the 2026-06-01 cohort is four days old.
    const countries = assembleCountries(
      await queryCountryRows(seeded('2026-06-05'), { sinceDate: '2026-06-01', today: '2026-06-05' }),
    )
    expect(countries.find((c) => c.country === 'SE')).toMatchObject({
      newInstalls: 1,
      d7Cohort: 0,
      d7Retained: 0,
      d28Cohort: 0,
      d28Retained: 0,
    })
  })

  it('counts D7 but not D28 for a cohort between 8 and 28 days old', async () => {
    // Today 2026-06-20: the 2026-06-01 cohort is 19 days old.
    const countries = assembleCountries(
      await queryCountryRows(seeded('2026-06-20'), { sinceDate: '2026-06-01', today: '2026-06-20' }),
    )
    expect(countries.find((c) => c.country === 'SE')).toMatchObject({
      d7Cohort: 1,
      d7Retained: 1,
      d28Cohort: 0,
      d28Retained: 0,
    })
  })

  it('counts an install from before the window nowhere', async () => {
    const countries = assembleCountries(await queryCountryRows(seeded(), { sinceDate: '2026-06-01', today }))
    // `anal_c` (no country, first beat 2026-05-20) would otherwise add to "(unknown)", which holds only `anal_b`.
    expect(countries.find((c) => c.country === '(unknown)')).toMatchObject({ newInstalls: 1 })
  })
})
