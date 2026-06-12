import type { DashboardSelection, SourceResult } from '../types.js'
import { toTimeWindow, selectionCacheKey } from '../types.js'
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

/**
 * Buckets completed Paddle transactions created on/after `sinceDate` (a `YYYY-MM-DD`) into per-UTC-day
 * counts, for the daily funnel's "purchases" column. Paddle is the source of truth for revenue; this
 * just counts completed transactions by the UTC day of `created_at`. Returns a `Map<YYYY-MM-DD, count>`;
 * throws on any Paddle error (the funnel source catches and shows that column as dashes).
 */
export async function fetchPaddlePurchasesByDay(env: PaddleEnv, sinceDate: string): Promise<Map<string, number>> {
  const startIso = new Date(`${sinceDate}T00:00:00Z`).toISOString()
  const raw = await paddleFetchAll<PaddleRawTransaction>(
    env.PADDLE_API_KEY_LIVE,
    `/transactions?status=completed&created_at[gte]=${startIso}`,
  )
  const byDay = new Map<string, number>()
  for (const txn of raw) {
    // `created_at` is ISO8601 with a `Z`, so Date parses it as UTC; slice to the UTC day.
    const day = new Date(txn.created_at).toISOString().slice(0, 10)
    byDay.set(day, (byDay.get(day) ?? 0) + 1)
  }
  return byDay
}

export async function fetchPaddleData(
  env: PaddleEnv,
  selection: DashboardSelection,
): Promise<SourceResult<PaddleData>> {
  const cacheKey = selectionCacheKey(selection)
  const cached = await cacheGet<PaddleData>('paddle', cacheKey)
  if (cached) return { ok: true, data: cached }

  try {
    const { startAt, endAt } = toTimeWindow(selection)
    const startIso = new Date(startAt).toISOString()
    // A specific single day needs an upper bound too, so transactions after that day don't leak in.
    // The rolling ranges and `today` end at now, so the gte filter alone already covers them.
    const upperBound = selection.range === 'day' ? `&created_at[lt]=${new Date(endAt).toISOString()}` : ''

    const [rawTransactions, rawSubscriptions] = await Promise.all([
      paddleFetchAll<PaddleRawTransaction>(
        env.PADDLE_API_KEY_LIVE,
        `/transactions?status=completed&created_at[gte]=${startIso}${upperBound}`,
      ),
      paddleFetchAll<PaddleRawSubscription>(env.PADDLE_API_KEY_LIVE, '/subscriptions'),
    ])

    const transactions = rawTransactions.map(parseTransaction)
    const allSubscriptions = rawSubscriptions.map(parseSubscription)
    const activeSubscriptions = allSubscriptions.filter((s) => s.status === 'active')
    const subscriptionsByStatus = countSubscriptionsByStatus(allSubscriptions)

    const data: PaddleData = { transactions, activeSubscriptions, subscriptionsByStatus }
    await cacheSet('paddle', cacheKey, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Paddle: ${e instanceof Error ? e.message : String(e)}` }
  }
}
