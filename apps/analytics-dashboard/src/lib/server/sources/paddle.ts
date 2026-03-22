import type { TimeRange, SourceResult } from '../types.js'
import { toTimeWindow } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

export interface PaddleTransaction {
    id: string
    status: string
    createdAt: string
    total: string
    currencyCode: string
}

export interface PaddleSubscription {
    id: string
    status: string
    customerId: string
    currentBillingPeriod: { startsAt: string; endsAt: string } | null
}

export interface PaddleData {
    transactions: PaddleTransaction[]
    activeSubscriptions: PaddleSubscription[]
    /** Count of subscriptions by status (for retention analysis) */
    subscriptionsByStatus: Record<string, number>
}

interface PaddleEnv {
    PADDLE_API_KEY_LIVE: string
}

const paddleApiBase = 'https://api.paddle.com'

interface PaddleListResponse<T> {
    data: T[]
    meta: { pagination: { next?: string; has_more: boolean; estimated_total: number } }
}

interface PaddleRawTransaction {
    id: string
    status: string
    created_at: string
    details?: { totals?: { total?: string; currency_code?: string } }
}

interface PaddleRawSubscription {
    id: string
    status: string
    customer_id: string
    current_billing_period?: { starts_at: string; ends_at: string } | null
}

async function paddleFetch<T>(apiKey: string, path: string): Promise<PaddleListResponse<T>> {
    const response = await fetch(`${paddleApiBase}${path}`, {
        headers: { Authorization: `Bearer ${apiKey}` },
    })
    if (!response.ok) {
        throw new Error(`Paddle ${path} returned ${response.status}`)
    }
    return (await response.json()) as PaddleListResponse<T>
}

/** Paginates through all results using Paddle's `after` cursor. */
async function paddleFetchAll<T>(apiKey: string, basePath: string): Promise<T[]> {
    const all: T[] = []
    let path: string | null = basePath
    while (path) {
        const result: PaddleListResponse<T> = await paddleFetch<T>(apiKey, path)
        all.push(...result.data)
        if (result.meta.pagination.has_more && result.meta.pagination.next) {
            const separator = basePath.includes('?') ? '&' : '?'
            path = `${basePath}${separator}after=${result.meta.pagination.next}`
        } else {
            path = null
        }
    }
    return all
}

export function parseTransaction(raw: PaddleRawTransaction): PaddleTransaction {
    return {
        id: raw.id,
        status: raw.status,
        createdAt: raw.created_at,
        total: raw.details?.totals?.total ?? '0',
        currencyCode: raw.details?.totals?.currency_code ?? 'USD',
    }
}

export function parseSubscription(raw: PaddleRawSubscription): PaddleSubscription {
    return {
        id: raw.id,
        status: raw.status,
        customerId: raw.customer_id,
        currentBillingPeriod: raw.current_billing_period
            ? { startsAt: raw.current_billing_period.starts_at, endsAt: raw.current_billing_period.ends_at }
            : null,
    }
}

export function countSubscriptionsByStatus(subscriptions: PaddleSubscription[]): Record<string, number> {
    const counts: Record<string, number> = {}
    for (const sub of subscriptions) {
        counts[sub.status] = (counts[sub.status] ?? 0) + 1
    }
    return counts
}

export async function fetchPaddleData(env: PaddleEnv, range: TimeRange): Promise<SourceResult<PaddleData>> {
    const cached = await cacheGet<PaddleData>('paddle', range)
    if (cached) return { ok: true, data: cached }

    try {
        const { startAt } = toTimeWindow(range)
        const startIso = new Date(startAt).toISOString()

        const [rawTransactions, rawSubscriptions] = await Promise.all([
            paddleFetchAll<PaddleRawTransaction>(
                env.PADDLE_API_KEY_LIVE,
                `/transactions?status=completed&created_at[gte]=${startIso}`
            ),
            paddleFetchAll<PaddleRawSubscription>(env.PADDLE_API_KEY_LIVE, '/subscriptions'),
        ])

        const transactions = rawTransactions.map(parseTransaction)
        const allSubscriptions = rawSubscriptions.map(parseSubscription)
        const activeSubscriptions = allSubscriptions.filter((s) => s.status === 'active')
        const subscriptionsByStatus = countSubscriptionsByStatus(allSubscriptions)

        const data: PaddleData = { transactions, activeSubscriptions, subscriptionsByStatus }
        await cacheSet('paddle', range, data)
        return { ok: true, data }
    } catch (e) {
        return { ok: false, error: `Paddle: ${e instanceof Error ? e.message : String(e)}` }
    }
}
