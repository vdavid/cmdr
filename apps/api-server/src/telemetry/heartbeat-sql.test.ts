/**
 * The `/heartbeat` INSERTs, run against a REAL SQLite with the real migrations. The route tests
 * mock D1 and can only prove which values were bound; only an engine can prove the `json_each`
 * unpacking puts each value in the right column. `node:sqlite` ships with Node, so this costs no dependency.
 */
import { describe, expect, it } from 'vitest'
import { applyMigrationsFrom, createSqliteD1, type SqliteD1 } from '../sqlite-d1-test-helper'
import { insertEventsSql, insertHeartbeatSql, upsertInstallSummarySql } from './heartbeat'

function openDb(): DatabaseSync {
  return createSqliteD1().sqlite
}

type DatabaseSync = SqliteD1['sqlite']

const analId = 'anal_0123456789abcdef0123456789abcdef0123'
const eventId = '3b241101-e2bb-4255-8caf-4136c566a962'

interface BeatFacts {
  at: string
  country?: string | null
  channel?: string | null
}

/** Store one beat the way the route does: the beat row, then the summary upsert, same timestamp. */
function beat(db: DatabaseSync, id: string, facts: BeatFacts): void {
  const { at, country = null, channel = null } = facts
  db.prepare(insertHeartbeatSql).run(id, '1.2.3', 'macOS 26.0', 'aarch64', 'release', null, null, country, channel, at)
  db.prepare(upsertInstallSummarySql).run(id, at, country, channel)
}

function summary(db: DatabaseSync, id: string): Record<string, unknown> {
  return {
    ...(db
      .prepare(`SELECT first_beat_at, country, install_channel FROM install_summary WHERE anal_id = ?`)
      .get(id) as Record<string, unknown>),
  }
}

describe('the heartbeat INSERTs against real SQLite', () => {
  it('stores the beat with its uptime, country, channel, and our timestamp', () => {
    const db = openDb()
    db.prepare(insertHeartbeatSql).run(
      analId,
      '1.2.3',
      'macOS 26.0',
      'aarch64',
      'release',
      '{"a":1}',
      5_400,
      'SE',
      'homebrew',
      '2026-11-10 12:00:00',
    )
    expect({
      ...db
        .prepare(`SELECT anal_id, uptime_seconds, config_json, country, install_channel, created_at FROM heartbeat`)
        .get(),
    }).toEqual({
      anal_id: analId,
      uptime_seconds: 5_400,
      config_json: '{"a":1}',
      country: 'SE',
      install_channel: 'homebrew',
      created_at: '2026-11-10 12:00:00',
    })
    db.close()
  })

  it('unpacks every event from the one JSON parameter into its own row and columns', () => {
    const db = openDb()
    const rows = [
      ['search_used', '2026-09-24T10:00:00.000Z', '{"mode":"ai","count":3}', '1.2.3', eventId],
      ['app_launched', '2026-09-24T09:00:00.000Z', '{}', '1.2.2', null],
    ]
    db.prepare(insertEventsSql).run(analId, JSON.stringify(rows))

    const stored = db
      .prepare(
        `SELECT anal_id, app_version, event, occurred_at, properties_json, received_at, event_id FROM analytics_event ORDER BY id`,
      )
      .all() as Record<string, unknown>[]
    expect(stored).toHaveLength(2)
    expect(stored[0]).toMatchObject({
      anal_id: analId,
      app_version: '1.2.3',
      event: 'search_used',
      occurred_at: '2026-09-24T10:00:00.000Z',
      properties_json: '{"mode":"ai","count":3}',
      event_id: eventId,
    })
    expect(stored[1]).toMatchObject({
      event: 'app_launched',
      properties_json: '{}',
      app_version: '1.2.2',
      event_id: null,
    })
    // Ours, not the client's: the retention sweep keys on it.
    expect(stored[0].received_at).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
    db.close()
  })

  it('ignores an event whose id it already has, and keeps every event without one', () => {
    // A beat stored but whose 204 got lost is retried with the same events.
    const db = openDb()
    const rows = JSON.stringify([
      ['search_used', '2026-09-24T10:00:00.000Z', '{}', '1.2.3', eventId],
      ['app_launched', '2026-09-24T09:00:00.000Z', '{}', '1.2.3', null],
    ])
    db.prepare(insertEventsSql).run(analId, rows)
    db.prepare(insertEventsSql).run(analId, rows)
    const counts = db.prepare(`SELECT event, COUNT(*) AS n FROM analytics_event GROUP BY event ORDER BY event`).all()
    expect(counts.map((r) => ({ ...r }))).toEqual([
      { event: 'app_launched', n: 2 },
      { event: 'search_used', n: 1 },
    ])
    db.close()
  })

  it('keeps a property string that looks like JSON as a string', () => {
    const db = openDb()
    db.prepare(insertEventsSql).run(
      analId,
      JSON.stringify([['e', '2026-09-24T10:00:00.000Z', '{"label":"[1,2]"}', '1.2.3', null]]),
    )
    const row = db.prepare(`SELECT properties_json FROM analytics_event`).get() as { properties_json: string }
    expect(JSON.parse(row.properties_json)).toEqual({ label: '[1,2]' })
    db.close()
  })
})

describe('the install-summary upsert against real SQLite', () => {
  it("records an install's first beat with what that beat knew", () => {
    const db = openDb()
    beat(db, analId, { at: '2026-11-08 09:00:00', country: 'SE', channel: 'direct' })
    expect(summary(db, analId)).toEqual({
      first_beat_at: '2026-11-08 09:00:00',
      country: 'SE',
      install_channel: 'direct',
    })
    db.close()
  })

  it('keeps the first beat and the first non-NULL country and channel', () => {
    const db = openDb()
    beat(db, analId, { at: '2026-11-08 09:00:00' })
    beat(db, analId, { at: '2026-11-08 12:00:00', country: 'HU' })
    beat(db, analId, { at: '2026-11-08 15:00:00', country: 'SE', channel: 'homebrew' })
    beat(db, analId, { at: '2026-11-09 09:00:00', country: 'DE', channel: 'direct' })
    expect(summary(db, analId)).toEqual({
      first_beat_at: '2026-11-08 09:00:00',
      country: 'HU',
      install_channel: 'homebrew',
    })
    db.close()
  })

  it('writes nothing for a beat that changes nothing, and leaves other installs alone', () => {
    const db = openDb()
    const other = 'anal_ffffffffffffffffffffffffffffffffffff'
    beat(db, analId, { at: '2026-11-08 09:00:00', country: 'SE', channel: 'direct' })
    beat(db, other, { at: '2026-11-08 10:00:00' })
    const result = db.prepare(upsertInstallSummarySql).run(analId, '2026-11-09 09:00:00', 'DE', 'homebrew')
    expect(Number(result.changes)).toBe(0)
    expect(summary(db, other)).toMatchObject({ first_beat_at: '2026-11-08 10:00:00', country: null })
    db.close()
  })
})

describe('migration 0031 backfills the install summary', () => {
  it('gives every existing install its first beat, with no country or channel', () => {
    const { sqlite } = createSqliteD1({ before: '0031' })
    const insert = sqlite.prepare(
      `INSERT INTO heartbeat (anal_id, created_at, app_version, os_version, arch) VALUES (?, ?, '1.0.0', '26.0', 'aarch64')`,
    )
    insert.run('anal_a', '2026-10-02 08:00:00')
    insert.run('anal_a', '2026-10-01 08:00:00')
    insert.run('anal_b', '2026-10-05 08:00:00')
    applyMigrationsFrom(sqlite, '0031')
    const rows = sqlite
      .prepare(`SELECT * FROM install_summary ORDER BY anal_id`)
      .all()
      .map((r) => ({ ...r }))
    expect(rows).toEqual([
      { anal_id: 'anal_a', first_beat_at: '2026-10-01 08:00:00', country: null, install_channel: null },
      { anal_id: 'anal_b', first_beat_at: '2026-10-05 08:00:00', country: null, install_channel: null },
    ])
    sqlite.close()
  })
})
