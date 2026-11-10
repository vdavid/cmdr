/**
 * The funnel's SQL against a REAL SQLite with every migration, fed through the same INSERTs
 * `/heartbeat` runs. The route tests mock D1 and can only prove assembly; only an engine can prove
 * the install summary, the day offsets, and the country filter select the right rows.
 */
import { describe, expect, it } from 'vitest'
import { createSqliteD1, type SqliteD1 } from '../sqlite-d1-test-helper'
import { insertHeartbeatSql, upsertInstallSummarySql } from '../telemetry/heartbeat'
import { assembleCountries, queryCountryRows } from './funnel-countries'
import { queryFunnelRows } from './funnel'

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

/**
 * - `anal_a`: first beat 2026-06-01 from Sweden via Homebrew; back on day one and day seven.
 * - `anal_b`: first beat 2026-06-01 with no country yet, then Germany; installed directly; back only on
 *   day eight, which is past D7.
 * - `anal_c`: first beat before the window, so it's active on 2026-06-01 but not new.
 * - Downloads: two from Sweden (one a non-macOS bot), one from Cloudflare's `XX`, one stored as `unknown`.
 */
function seeded(): D1Database & { sqlite: DatabaseSync } {
  const { d1, sqlite } = createSqliteD1()
  beat(sqlite, { id: 'anal_a', at: '2026-06-01 09:00:00', country: 'SE', channel: 'homebrew' })
  beat(sqlite, { id: 'anal_a', at: '2026-06-02 09:00:00', country: 'SE', channel: 'homebrew' })
  beat(sqlite, { id: 'anal_a', at: '2026-06-08 23:59:59', country: 'SE', channel: 'homebrew' })
  beat(sqlite, { id: 'anal_b', at: '2026-06-01 20:00:00', channel: 'direct' })
  beat(sqlite, { id: 'anal_b', at: '2026-06-01 21:00:00', country: 'DE', channel: 'direct' })
  beat(sqlite, { id: 'anal_b', at: '2026-06-09 00:00:00', country: 'DE', channel: 'direct' })
  beat(sqlite, { id: 'anal_c', at: '2026-05-20 09:00:00' })
  beat(sqlite, { id: 'anal_c', at: '2026-06-01 10:00:00' })
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

describe('the per-day funnel queries against real SQLite', () => {
  it('counts new installs per first-beat day and channel, from the install summary', async () => {
    const rows = await queryFunnelRows(seeded(), '2026-06-01', null)
    expect(plain(rows.installsByChannel).sort((a, b) => String(a.channel).localeCompare(String(b.channel)))).toEqual([
      { date: '2026-06-01', channel: 'direct', count: 1 },
      { date: '2026-06-01', channel: 'homebrew', count: 1 },
    ])
  })

  it('counts retention by day offset 1..7, leaving out a beat on day eight', async () => {
    const rows = await queryFunnelRows(seeded(), '2026-06-01', null)
    expect(plain(rows.retention).sort((a, b) => a.dayOffset - b.dayOffset)).toEqual([
      { cohortDate: '2026-06-01', dayOffset: 1, retained: 1 },
      { cohortDate: '2026-06-01', dayOffset: 7, retained: 1 },
    ])
  })

  it('counts every install that beat as active, new or not', async () => {
    const rows = await queryFunnelRows(seeded(), '2026-06-01', null)
    expect(plain(rows.dau).find((r) => r.date === '2026-06-01')).toEqual({ date: '2026-06-01', dau: 3 })
  })

  it("filters installs, DAU, and retention by the install's first country", async () => {
    const sweden = await queryFunnelRows(seeded(), '2026-06-01', 'SE')
    expect(plain(sweden.installsByChannel)).toEqual([{ date: '2026-06-01', channel: 'homebrew', count: 1 }])
    expect(plain(sweden.dau).find((r) => r.date === '2026-06-01')).toEqual({ date: '2026-06-01', dau: 1 })
    expect(sweden.retention).toHaveLength(2)

    // `anal_b` first beat with no country; its first non-NULL one, Germany, is what it counts under.
    const germany = await queryFunnelRows(seeded(), '2026-06-01', 'DE')
    expect(plain(germany.installsByChannel)).toEqual([{ date: '2026-06-01', channel: 'direct', count: 1 }])
    expect(germany.retention).toEqual([])
  })

  it("filters downloads by the download's own country", async () => {
    const rows = await queryFunnelRows(seeded(), '2026-05-30', 'SE')
    expect(plain(rows.downloadsBySource).sort((a, b) => a.date.localeCompare(b.date))).toEqual([
      { date: '2026-05-30', source: 'homebrew', count: 1 },
      { date: '2026-06-01', source: 'website', count: 1 },
    ])
    for (const set of [rows.downloadsByRef, rows.downloadsByReferer, rows.downloadsByUa]) {
      expect(set.reduce((sum, r) => sum + r.count, 0)).toBe(2)
    }
  })
})

describe('the per-country funnel against real SQLite', () => {
  it('sums each country, folds non-countries into "(unknown)", and sorts by installs, then downloads', async () => {
    // Cohorts before 2026-06-05 have a fully passed D7 window.
    const countries = assembleCountries(await queryCountryRows(seeded(), '2026-05-30', '2026-06-05'))
    // SE and DE tie on installs; SE's downloads break the tie.
    expect(countries).toEqual([
      { country: 'SE', serverDownloads: 2, humanInstalls: 1, newInstalls: 1, d7Cohort: 1, d7Retained: 1 },
      { country: 'DE', serverDownloads: 0, humanInstalls: 0, newInstalls: 1, d7Cohort: 1, d7Retained: 0 },
      { country: '(unknown)', serverDownloads: 2, humanInstalls: 2, newInstalls: 0, d7Cohort: 0, d7Retained: 0 },
    ])
  })

  it('leaves cohorts too young for D7 out of the D7 counts, but not out of new installs', async () => {
    const countries = assembleCountries(await queryCountryRows(seeded(), '2026-06-01', '2026-06-01'))
    const sweden = countries.find((c) => c.country === 'SE')
    expect(sweden).toMatchObject({ newInstalls: 1, d7Cohort: 0, d7Retained: 0 })
  })

  it('counts an install from before the window nowhere', async () => {
    const countries = assembleCountries(await queryCountryRows(seeded(), '2026-06-01', '2026-06-05'))
    // `anal_c` (no country, first beat 2026-05-20) would otherwise land under "(unknown)".
    expect(countries.find((c) => c.country === '(unknown)')).toMatchObject({ newInstalls: 0 })
  })
})
