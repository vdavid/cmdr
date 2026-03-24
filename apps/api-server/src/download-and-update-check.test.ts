import { describe, expect, it, vi, type Mock } from 'vitest'
import { app } from './index'

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
    return { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset
}

/** Mock D1Database that tracks prepare/bind/run calls. Returns mocks for assertions. */
function createMockD1(runImpl?: () => Promise<unknown>): {
    db: D1Database
    prepareMock: Mock
    bindMock: Mock
} {
    const run = vi.fn(runImpl ?? (() => Promise.resolve({ success: true })))
    const bindMock = vi.fn(() => ({ run }))
    const prepareMock = vi.fn(() => ({ bind: bindMock }))
    return { db: { prepare: prepareMock } as unknown as D1Database, prepareMock, bindMock }
}

function createBindings(overrides: Record<string, unknown> = {}) {
    return {
        LICENSE_CODES: { get: vi.fn(() => null), put: vi.fn() } as unknown as KVNamespace,
        DEVICE_COUNTS: createMockAnalyticsEngine(),
        TELEMETRY_DB: createMockD1().db,
        ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
        RESEND_API_KEY: 'test-resend-key',
        PRODUCT_NAME: 'Cmdr',
        SUPPORT_EMAIL: 'test@example.com',
        ADMIN_API_TOKEN: 'test-admin-token-secret',
        ...overrides,
    }
}

describe('GET /download/:version/:arch', () => {
    it('redirects to GitHub Releases', async () => {
        const bindings = createBindings()
        const res = await app.request('/download/1.2.3/aarch64', {}, bindings)

        expect(res.status).toBe(302)
        expect(res.headers.get('location')).toBe(
            'https://github.com/vdavid/cmdr/releases/download/v1.2.3/Cmdr_1.2.3_aarch64.dmg',
        )
    })

    it('inserts correct data into D1 downloads table', async () => {
        const { db, prepareMock, bindMock } = createMockD1()
        const bindings = createBindings({ TELEMETRY_DB: db })

        await app.request('/download/1.2.3/aarch64', {}, bindings)

        expect(prepareMock).toHaveBeenCalledOnce()
        const sql = prepareMock.mock.calls[0][0] as string
        expect(sql).toContain('INSERT INTO downloads')

        const bindArgs = bindMock.mock.calls[0]
        // bindArgs: [app_version, arch, country, continent]
        expect(bindArgs[0]).toBe('1.2.3')
        expect(bindArgs[1]).toBe('aarch64')
        expect(bindArgs[2]).toBe('unknown') // no cf object in test
        expect(bindArgs[3]).toBe('unknown')
    })

    it('returns 302 even when D1 write fails', async () => {
        const { db } = createMockD1(() => Promise.reject(new Error('D1 unavailable')))
        const bindings = createBindings({ TELEMETRY_DB: db })

        const res = await app.request('/download/1.2.3/aarch64', {}, bindings)
        expect(res.status).toBe(302)
    })

    it('returns 400 for invalid version', async () => {
        const bindings = createBindings()
        const res = await app.request('/download/not-a-version/aarch64', {}, bindings)
        expect(res.status).toBe(400)
    })

    it('returns 400 for invalid architecture', async () => {
        const bindings = createBindings()
        const res = await app.request('/download/1.2.3/windows', {}, bindings)
        expect(res.status).toBe(400)
    })
})

describe('GET /update-check/:version', () => {
    it('redirects to latest.json', async () => {
        const bindings = createBindings()
        const res = await app.request('/update-check/1.2.3', {}, bindings)

        expect(res.status).toBe(302)
        expect(res.headers.get('location')).toBe('https://getcmdr.com/latest.json')
    })

    it('inserts correct data into D1 update_checks table', async () => {
        const { db, prepareMock, bindMock } = createMockD1()
        const bindings = createBindings({ TELEMETRY_DB: db })

        await app.request('/update-check/1.2.3?arch=aarch64', {}, bindings)

        expect(prepareMock).toHaveBeenCalledOnce()
        const sql = prepareMock.mock.calls[0][0] as string
        expect(sql).toContain('INSERT OR IGNORE INTO update_checks')

        const bindArgs = bindMock.mock.calls[0]
        // bindArgs: [date, hashed_ip, app_version, arch]
        expect(bindArgs[0]).toMatch(/^\d{4}-\d{2}-\d{2}$/) // YYYY-MM-DD
        expect(bindArgs[1]).toMatch(/^[0-9a-f]{64}$/) // SHA-256 hex
        expect(bindArgs[2]).toBe('1.2.3')
        expect(bindArgs[3]).toBe('aarch64')
    })

    it('uses "unknown" arch when not provided', async () => {
        const { db, bindMock } = createMockD1()
        const bindings = createBindings({ TELEMETRY_DB: db })

        await app.request('/update-check/1.2.3', {}, bindings)

        expect(bindMock.mock.calls[0][3]).toBe('unknown')
    })

    it('silently ignores duplicate update checks (INSERT OR IGNORE)', async () => {
        // Simulate D1 returning success for INSERT OR IGNORE on a duplicate — the UNIQUE constraint
        // makes it a no-op. The route should still return 302 without errors.
        const { db } = createMockD1(() => Promise.resolve({ success: true, meta: { changes: 0 } }))
        const bindings = createBindings({ TELEMETRY_DB: db })

        const res = await app.request('/update-check/1.2.3?arch=aarch64', {}, bindings)
        expect(res.status).toBe(302)
    })

    it('returns 302 even when D1 write fails', async () => {
        const { db } = createMockD1(() => Promise.reject(new Error('D1 unavailable')))
        const bindings = createBindings({ TELEMETRY_DB: db })

        const res = await app.request('/update-check/1.2.3', {}, bindings)
        expect(res.status).toBe(302)
    })

    it('returns 400 for invalid version', async () => {
        const bindings = createBindings()
        const res = await app.request('/update-check/abc', {}, bindings)
        expect(res.status).toBe(400)
    })
})
