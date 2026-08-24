/**
 * The synthetic-heartbeat classifier, run against a REAL SQLite database rather than the
 * statement-shape mock the other cron tests use. The mock can prove which statements ran; only a
 * real engine can prove the predicate keeps a person's rows and drops a robot's, and that is the
 * whole risk of this sweep. `node:sqlite` ships with Node, so this costs no dependency.
 */
import { DatabaseSync } from 'node:sqlite'
import { describe, expect, it } from 'vitest'
import { deleteSyntheticHeartbeatsSql, syntheticHeartbeatGraceDays } from './scheduled'

/** The production `heartbeat` schema, per `migrations/0005_heartbeat.sql`. */
const heartbeatSchema = `
  CREATE TABLE heartbeat (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anal_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    app_version TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    build_mode TEXT,
    config_json TEXT
  );`

/** `YYYY-MM-DD HH:MM:SS`, the format `datetime('now')` writes. */
function hoursAgo(hours: number): string {
  return new Date(Date.now() - hours * 3_600_000).toISOString().slice(0, 19).replace('T', ' ')
}

/** The cutoff the cron binds: midnight UTC, `syntheticHeartbeatGraceDays` back. */
function cutoff(): string {
  const boundary = new Date(Date.now() - syntheticHeartbeatGraceDays * 86_400_000)
  return `${boundary.toISOString().slice(0, 10)} 00:00:00`
}

interface Beat {
  analId: string
  hoursAgo: number
  config: string | null
}

/** Seeds the beats, runs the sweep, and returns the install ids that still have rows. */
function sweep(beats: Beat[]): string[] {
  const db = new DatabaseSync(':memory:')
  db.exec(heartbeatSchema)
  const insert = db.prepare(
    `INSERT INTO heartbeat (anal_id, created_at, app_version, os_version, arch, build_mode, config_json)
       VALUES (?, ?, '0.39.0', 'macOS 26.6.2', 'aarch64', 'release', ?)`,
  )
  for (const beat of beats) insert.run(beat.analId, hoursAgo(beat.hoursAgo), beat.config)

  db.prepare(deleteSyntheticHeartbeatsSql).run(cutoff())

  const rows = db.prepare(`SELECT DISTINCT anal_id FROM heartbeat ORDER BY anal_id`).all() as { anal_id: string }[]
  db.close()
  return rows.map((r) => r.anal_id)
}

/** What a settings-saving install ships: `_schemaVersion` rides along as a number-valued key. */
const withSettings = '{"_schemaVersion":4,"fdaGranted":true,"theme.mode":"dark"}'
/** What an E2E shard ships: FDA mocked open, and no `settings.json` was ever written. */
const noSettings = '{"fdaGranted":true}'

describe('the synthetic-heartbeat classifier', () => {
  it('drops an install that never persisted a setting and has gone quiet', () => {
    expect(sweep([{ analId: 'robot', hoursAgo: 24 * 30, config: noSettings }])).toEqual([])
  })

  it('keeps an install that persisted a setting, however long ago it went quiet', () => {
    expect(sweep([{ analId: 'person', hoursAgo: 24 * 400, config: withSettings }])).toEqual(['person'])
  })

  it('keeps a brand-new install that has not saved a setting YET', () => {
    // The load-bearing case: a real new user looks exactly like a robot for their first minutes.
    // The grace period is the only thing standing between them and deletion.
    expect(sweep([{ analId: 'newcomer', hoursAgo: 1, config: noSettings }])).toEqual(['newcomer'])
  })

  it('keeps the EARLY pre-settings beats of an install that saved a setting later', () => {
    // One `_schemaVersion` beat vouches for the whole install, so the launch beats that preceded
    // the first settings save survive. Without that, every real install would lose its first day.
    const kept = sweep([
      { analId: 'person', hoursAgo: 24 * 60, config: noSettings },
      { analId: 'person', hoursAgo: 24 * 59, config: withSettings },
    ])
    expect(kept).toEqual(['person'])
  })

  it('is decided per install, so a quiet robot never takes a person down with it', () => {
    expect(
      sweep([
        { analId: 'person', hoursAgo: 24 * 30, config: withSettings },
        { analId: 'robot', hoursAgo: 24 * 30, config: noSettings },
      ]),
    ).toEqual(['person'])
  })

  it('keeps an install still beating today even though it never saved a setting', () => {
    // Silence is measured from the LAST beat, not the first: someone using Cmdr daily without ever
    // opening settings is safe for as long as they keep launching it.
    expect(
      sweep([
        { analId: 'lightuser', hoursAgo: 24 * 30, config: noSettings },
        { analId: 'lightuser', hoursAgo: 1, config: noSettings },
      ]),
    ).toEqual(['lightuser'])
  })

  it('treats a missing config blob as "never saved a setting"', () => {
    // `config_json` is nullable, so the predicate must not go NULL and match nothing.
    expect(sweep([{ analId: 'robot', hoursAgo: 24 * 30, config: null }])).toEqual([])
  })

  it('matches the key exactly, so SQLite LIKE wildcards cannot vouch for an install', () => {
    // `LIKE '%"_schemaVersion"%'` would match this, because `_` is a single-character wildcard.
    // `instr` does not, which is why the classifier uses it.
    expect(sweep([{ analId: 'robot', hoursAgo: 24 * 30, config: '{"xschemaVersion":4}' }])).toEqual([])
  })

  it('is idempotent: a second run deletes nothing more', () => {
    const db = new DatabaseSync(':memory:')
    db.exec(heartbeatSchema)
    db.prepare(
      `INSERT INTO heartbeat (anal_id, created_at, app_version, os_version, arch, config_json)
         VALUES ('robot', ?, '0.39.0', 'macOS 26.6.2', 'aarch64', ?)`,
    ).run(hoursAgo(24 * 30), noSettings)

    const statement = db.prepare(deleteSyntheticHeartbeatsSql)
    expect(statement.run(cutoff()).changes).toBe(1)
    expect(statement.run(cutoff()).changes).toBe(0)
    db.close()
  })
})
