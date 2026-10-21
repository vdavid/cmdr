/**
 * The admin feedback list and marking feedback answered, end to end through the Hono app against a
 * REAL SQLite with every migration applied, so the columns and the SQL are what's under test.
 */
import { describe, expect, it } from 'vitest'
import { app } from '../index'
import { createSqliteD1 } from '../sqlite-d1-test-helper'

const authHeaders = { Authorization: 'Bearer test-admin-token-secret', 'Content-Type': 'application/json' }

function setup() {
  const { d1, sqlite } = createSqliteD1()
  sqlite
    .prepare(
      `INSERT INTO feedback (id, created_at, feedback, email, app_version, os_version, build_mode)
         VALUES (?, datetime('now'), ?, ?, '0.52.0', 'macOS 26.0', 'release')`,
    )
    .run(1, 'Cyrillic quick filter', 'someone@example.com')
  const bindings = { TELEMETRY_DB: d1, ADMIN_API_TOKEN: 'test-admin-token-secret' }
  return { sqlite, bindings }
}

function mark(bindings: object, id: number | string, body: unknown, headers: Record<string, string> = authHeaders) {
  return app.request(
    `/admin/feedback/${String(id)}/answered`,
    { method: 'PUT', headers, body: JSON.stringify(body) },
    bindings,
  )
}

describe('PUT /admin/feedback/:id/answered', () => {
  it('returns 401 without auth', async () => {
    const { bindings } = setup()
    const res = await mark(bindings, 1, { answered: true }, { 'Content-Type': 'application/json' })
    expect(res.status).toBe(401)
  })

  it('stamps answered_at, and GET /admin/feedback returns it', async () => {
    const { bindings } = setup()
    const res = await mark(bindings, 1, { answered: true })
    expect(res.status).toBe(200)
    const body = await res.json<{ id: number; answeredAt: string | null }>()
    expect(body.id).toBe(1)
    expect(body.answeredAt).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)

    const listed = await app.request('/admin/feedback?range=7d', { headers: authHeaders }, bindings)
    const rows = await listed.json<Array<{ id: number; answeredAt: string | null }>>()
    expect(rows[0].answeredAt).toBe(body.answeredAt)
  })

  it('keeps the first answered time when marked answered twice', async () => {
    const { sqlite, bindings } = setup()
    sqlite.prepare(`UPDATE feedback SET answered_at = '2026-10-14 21:12:00' WHERE id = 1`).run()
    const res = await mark(bindings, 1, { answered: true })
    expect((await res.json<{ answeredAt: string }>()).answeredAt).toBe('2026-10-14 21:12:00')
  })

  it('clears answered_at when unmarked', async () => {
    const { sqlite, bindings } = setup()
    sqlite.prepare(`UPDATE feedback SET answered_at = '2026-10-14 21:12:00' WHERE id = 1`).run()
    const res = await mark(bindings, 1, { answered: false })
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual({ id: 1, answeredAt: null })
    expect(sqlite.prepare('SELECT answered_at FROM feedback WHERE id = 1').get()).toEqual({ answered_at: null })
  })

  it('returns 404 for an unknown id', async () => {
    const { bindings } = setup()
    const res = await mark(bindings, 99, { answered: true })
    expect(res.status).toBe(404)
  })

  it('returns 400 for a non-numeric id or a non-boolean flag', async () => {
    const { bindings } = setup()
    expect((await mark(bindings, 'abc', { answered: true })).status).toBe(400)
    expect((await mark(bindings, 1, { answered: 'yes' })).status).toBe(400)
    expect((await mark(bindings, 1, {})).status).toBe(400)
  })
})

describe('GET /admin/feedback', () => {
  it('returns the linked error report id, and null when the message came without logs', async () => {
    const { sqlite, bindings } = setup()
    sqlite
      .prepare(
        `INSERT INTO feedback (id, created_at, feedback, app_version, os_version, error_report_id)
           VALUES (2, datetime('now', '+1 minute'), 'Copy stalls', '0.52.0', 'macOS 26.0', 'ERR-AB7PV')`,
      )
      .run()
    const res = await app.request('/admin/feedback?range=7d', { headers: authHeaders }, bindings)
    const rows = await res.json<Array<{ id: number; errorReportId: string | null }>>()
    expect(rows.map((row) => [row.id, row.errorReportId])).toEqual([
      [2, 'ERR-AB7PV'],
      [1, null],
    ])
  })
})
