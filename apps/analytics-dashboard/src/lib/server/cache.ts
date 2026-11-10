/**
 * Cache key for a source's data: a `TimeRange` for the picker-driven sources, or a source's own fixed key
 * (the funnel passes `days:181`).
 */
export type CacheRange = string

const cacheName = 'analytics-dashboard'

/** TTL in seconds: long/historical ranges get 1 hour, short live ranges get 5 min. */
function getTtl(range: CacheRange): number {
  // 30d and 90d are heavy and barely move within an hour; 7d and everything else stay fresh at 5 min.
  return range === '30d' || range === '90d' ? 3600 : 300
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
export async function cacheSet(source: string, range: CacheRange, data: unknown, extra?: string): Promise<void> {
  const url = buildCacheUrl(source, range, extra)
  const ttl = getTtl(range)
  const body = JSON.stringify(data)

  if (typeof caches !== 'undefined') {
    try {
      const cache = await caches.open(cacheName)
      await cache.put(
        new Request(url),
        new Response(body, {
          headers: { 'Content-Type': 'application/json', 'Cache-Control': `max-age=${String(ttl)}` },
        }),
      )
      return
    } catch {
      // Fall through to memory cache
    }
  }

  memoryCache.set(url, { value: body, expiresAt: Date.now() + ttl * 1000 })
}
