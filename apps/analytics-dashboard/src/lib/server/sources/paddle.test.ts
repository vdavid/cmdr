import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchPaddleData, parseTransaction, parseSubscription, countSubscriptionsByStatus } from './paddle.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = { PADDLE_API_KEY_LIVE: 'test-paddle-key' }

const sampleRawTransaction = {
    id: 'txn_01abc123',
    status: 'completed',
    created_at: '2026-03-15T10:30:00Z',
    details: { totals: { total: '5900', currency_code: 'USD' } },
}

const sampleRawSubscription = {
    id: 'sub_01xyz789',
    status: 'active',
    customer_id: 'ctm_01def456',
    current_billing_period: {
        starts_at: '2026-03-01T00:00:00Z',
        ends_at: '2026-04-01T00:00:00Z',
    },
}

describe('parseTransaction', () => {
    it('parses a complete transaction', () => {
        const result = parseTransaction(sampleRawTransaction)
        expect(result).toEqual({
            id: 'txn_01abc123',
            status: 'completed',
            createdAt: '2026-03-15T10:30:00Z',
            total: '5900',
            currencyCode: 'USD',
        })
    })

    it('handles missing totals', () => {
        const result = parseTransaction({ id: 'txn_02', status: 'completed', created_at: '2026-03-15T10:30:00Z' })
        expect(result.total).toBe('0')
        expect(result.currencyCode).toBe('USD')
    })
})

describe('parseSubscription', () => {
    it('parses a subscription with billing period', () => {
        const result = parseSubscription(sampleRawSubscription)
        expect(result).toEqual({
            id: 'sub_01xyz789',
            status: 'active',
            customerId: 'ctm_01def456',
            currentBillingPeriod: {
                startsAt: '2026-03-01T00:00:00Z',
                endsAt: '2026-04-01T00:00:00Z',
            },
        })
    })

    it('handles null billing period', () => {
        const result = parseSubscription({
            id: 'sub_02',
            status: 'canceled',
            customer_id: 'ctm_02',
            current_billing_period: null,
        })
        expect(result.currentBillingPeriod).toBeNull()
    })

    it('handles missing billing period', () => {
        const result = parseSubscription({ id: 'sub_03', status: 'past_due', customer_id: 'ctm_03' })
        expect(result.currentBillingPeriod).toBeNull()
    })
})

describe('countSubscriptionsByStatus', () => {
    it('counts subscriptions by status', () => {
        const subs = [
            { id: '1', status: 'active', customerId: 'c1', currentBillingPeriod: null },
            { id: '2', status: 'active', customerId: 'c2', currentBillingPeriod: null },
            { id: '3', status: 'canceled', customerId: 'c3', currentBillingPeriod: null },
            { id: '4', status: 'past_due', customerId: 'c4', currentBillingPeriod: null },
        ]
        const counts = countSubscriptionsByStatus(subs)
        expect(counts).toEqual({ active: 2, canceled: 1, past_due: 1 })
    })

    it('handles empty array', () => {
        expect(countSubscriptionsByStatus([])).toEqual({})
    })
})

describe('fetchPaddleData', () => {
    beforeEach(() => {
        vi.restoreAllMocks()
        clearMemoryCache()
    })

    it('returns parsed data on success', async () => {
        const fetchMock = vi.fn()

        // Transactions (single page)
        fetchMock.mockResolvedValueOnce({
            ok: true,
            json: async () => ({
                data: [sampleRawTransaction],
                meta: { pagination: { has_more: false, estimated_total: 1 } },
            }),
        })
        // Subscriptions (single page)
        fetchMock.mockResolvedValueOnce({
            ok: true,
            json: async () => ({
                data: [sampleRawSubscription],
                meta: { pagination: { has_more: false, estimated_total: 1 } },
            }),
        })

        vi.stubGlobal('fetch', fetchMock)

        const result = await fetchPaddleData(mockEnv, '7d')
        expect(result.ok).toBe(true)
        if (!result.ok) return

        expect(result.data.transactions).toHaveLength(1)
        expect(result.data.transactions[0].id).toBe('txn_01abc123')
        expect(result.data.activeSubscriptions).toHaveLength(1)
        expect(result.data.subscriptionsByStatus).toEqual({ active: 1 })
    })

    it('paginates through multiple pages', async () => {
        // Use URL-based routing since Promise.all runs both fetchers in parallel
        const fetchMock = vi.fn().mockImplementation((url: string) => {
            if (url.includes('/transactions') && !url.includes('after=')) {
                return Promise.resolve({
                    ok: true,
                    json: async () => ({
                        data: [sampleRawTransaction],
                        meta: { pagination: { has_more: true, next: 'cursor_abc', estimated_total: 2 } },
                    }),
                })
            }
            if (url.includes('/transactions') && url.includes('after=cursor_abc')) {
                return Promise.resolve({
                    ok: true,
                    json: async () => ({
                        data: [{ ...sampleRawTransaction, id: 'txn_02' }],
                        meta: { pagination: { has_more: false, estimated_total: 2 } },
                    }),
                })
            }
            if (url.includes('/subscriptions')) {
                return Promise.resolve({
                    ok: true,
                    json: async () => ({
                        data: [],
                        meta: { pagination: { has_more: false, estimated_total: 0 } },
                    }),
                })
            }
            return Promise.resolve({ ok: false, status: 404 })
        })
        vi.stubGlobal('fetch', fetchMock)

        const result = await fetchPaddleData(mockEnv, '30d')
        expect(result.ok).toBe(true)
        if (!result.ok) return
        expect(result.data.transactions).toHaveLength(2)

        // Verify pagination cursor was used
        const paginatedCall = fetchMock.mock.calls.find(
            (call: unknown[]) => typeof call[0] === 'string' && call[0].includes('after=cursor_abc')
        )
        expect(paginatedCall).toBeDefined()
    })

    it('returns error when API fails', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401 }))

        const result = await fetchPaddleData(mockEnv, '7d')
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('Paddle')
        expect(result.error).toContain('401')
    })
})
