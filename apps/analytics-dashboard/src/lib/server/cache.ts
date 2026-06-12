/**
 * Cache key for a source's data. A plain `TimeRange` for relative windows, or `'day:YYYY-MM-DD'` for a
 * single specific day, so two different picked days don't collide on one cache entry. The funnel source
 * passes the literal `'30d'`.
 */
export type CacheRange = string

const cacheName = 'analytics-dashboard'

/** TTL in seconds: long/historical ranges get 1 hour, short live ranges get 5 min. */
function getTtl(range: CacheRange): number {
  // A specific past day is immutable history, and 30d is heavy; both cache for an hour. `today` and the
  // short rolling windows stay fresh at 5 min.
  return range === '30d' || range.startsWith('day:') ? 3600 : 300
}

// In-memory fallback for local dev (CF Cache API isn't available in wrangler pages dev)
const memoryCache = new Map<string, { value: string; expiresAt: number }>()

function buildCacheUrl(source: string, range: CacheRange, extra?: string): string {
  const suffix = extra ? `&${extra}` : ''
  return `https://cache/${source}?range=${encodeURIComponent(range)}${suffix}`
}

/**
 * Reads from cache. Returns null on miss.
 * Uses CF Cache API in production, in-memory Map locally.
 */
export async function cacheGet<T>(source: string, range: CacheRange, extra?: string): Promise<T | null> {
  const url = buildCacheUrl(source, range, extra)

  if (typeof caches !== 'undefined') {
    try {
      const cache = await caches.open(cacheName)
      const response = await cache.match(new Request(url))
      if (response) return (await response.json()) as T
    } catch {
      // Cache API not available (local dev); fall through to memory cache
    }
  }

  const entry = memoryCache.get(url)
  if (entry && entry.expiresAt > Date.now()) {
    return JSON.parse(entry.value) as T
  }
  memoryCache.delete(url)
  return null
}

/** Clears the in-memory cache. For testing only. */
export function clearMemoryCache(): void {
  memoryCache.clear()
}

/** Writes to cache with TTL based on time range. */
export async function cacheSet<T>(source: string, range: CacheRange, data: T, extra?: string): Promise<void> {
  const url = buildCacheUrl(source, range, extra)
  const ttl = getTtl(range)
  const body = JSON.stringify(data)

  if (typeof caches !== 'undefined') {
    try {
      const cache = await caches.open(cacheName)
      await cache.put(
        new Request(url),
        new Response(body, {
          headers: { 'Content-Type': 'application/json', 'Cache-Control': `max-age=${ttl}` },
        }),
      )
      return
    } catch {
      // Fall through to memory cache
    }
  }

  memoryCache.set(url, { value: body, expiresAt: Date.now() + ttl * 1000 })
}
