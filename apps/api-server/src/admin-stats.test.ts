import { describe, expect, it, vi } from 'vitest'
import { app } from './index'

/** Minimal KV mock that supports get/put. Handles the `'json'` format used by Hono. */
function createMockKv(store: Record<string, string> = {}): KVNamespace {
    return {
        get: vi.fn((key: string, format?: string) => {
            if (!(key in store)) return null
            const value = store[key]
            // eslint-disable-next-line @typescript-eslint/no-unsafe-return
            if (format === 'json') return JSON.parse(value)
            return value
        }),
        put: vi.fn((key: string, value: string) => {
            store[key] = value
        }),
    } as unknown as KVNamespace
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
    return { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset
}

function createMockD1(): D1Database {
    const run = vi.fn(() => Promise.resolve({ success: true }))
    const bind = vi.fn(() => ({ run }))
    const prepare = vi.fn(() => ({ bind }))
    return { prepare } as unknown as D1Database
}

const baseBindings = {
    LICENSE_CODES: createMockKv(),
    DEVICE_COUNTS: createMockAnalyticsEngine(),
    TELEMETRY_DB: createMockD1(),
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: 'test-admin-token-secret',
}

describe('GET /admin/stats', () => {
    it('returns 401 without auth header', async () => {
        const res = await app.request('/admin/stats', {}, baseBindings)

        expect(res.status).toBe(401)
        const body = await res.json()
        expect(body).toEqual({ error: 'Unauthorized' })
    })

    it('returns 401 with wrong token', async () => {
        const res = await app.request(
            '/admin/stats',
            { headers: { Authorization: 'Bearer wrong-token' } },
            baseBindings,
        )

        expect(res.status).toBe(401)
    })

    it('returns 401 with malformed auth header', async () => {
        const res = await app.request(
            '/admin/stats',
            { headers: { Authorization: 'Basic dGVzdDp0ZXN0' } },
            baseBindings,
        )

        expect(res.status).toBe(401)
    })

    it('returns stats with valid token', async () => {
        const res = await app.request(
            '/admin/stats',
            { headers: { Authorization: 'Bearer test-admin-token-secret' } },
            baseBindings,
        )

        expect(res.status).toBe(200)
        const body = await res.json()
        expect(body).toEqual({ totalActivations: 0, activeDevices: null })
    })

    it('returns activation count from KV', async () => {
        const kv = createMockKv({ '_meta:activation_count': '42' })
        const bindings = { ...baseBindings, LICENSE_CODES: kv }

        const res = await app.request(
            '/admin/stats',
            { headers: { Authorization: 'Bearer test-admin-token-secret' } },
            bindings,
        )

        expect(res.status).toBe(200)
        const body = await res.json()
        expect(body).toEqual({ totalActivations: 42, activeDevices: null })
    })

    it('returns 500 when ADMIN_API_TOKEN is not configured', async () => {
        const bindings = { ...baseBindings, ADMIN_API_TOKEN: undefined }

        const res = await app.request('/admin/stats', {}, bindings)

        expect(res.status).toBe(500)
        const body = await res.json()
        expect(body).toEqual({ error: 'Admin API not configured' })
    })
})

describe('POST /activate (activation counter)', () => {
    it('increments activation count on successful activation', async () => {
        const store: Record<string, string> = {
            'CMDR-ABCD-EFGH-2345': JSON.stringify({ fullKey: 'test-key', organizationName: 'Test' }),
            '_meta:activation_count': '5',
        }
        const kv = createMockKv(store)
        const bindings = { ...baseBindings, LICENSE_CODES: kv }

        const res = await app.request(
            '/activate',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ code: 'CMDR-ABCD-EFGH-2345' }),
            },
            bindings,
        )

        expect(res.status).toBe(200)
        const body = await res.json<{ licenseKey: string }>()
        expect(body.licenseKey).toBe('test-key')

        // The counter increment runs inline (waitUntil fallback in test mode)
        expect(store['_meta:activation_count']).toBe('6')
    })

    it('does not increment counter on invalid code', async () => {
        const store: Record<string, string> = { '_meta:activation_count': '5' }
        const kv = createMockKv(store)
        const bindings = { ...baseBindings, LICENSE_CODES: kv }

        const res = await app.request(
            '/activate',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ code: 'CMDR-ZZZZ-ZZZZ-ZZZZ' }),
            },
            bindings,
        )

        expect(res.status).toBe(404)
        expect(store['_meta:activation_count']).toBe('5')
    })
})
