import { Hono } from 'hono'
import { callerIp, enforceIpRateLimit, hashCallerIp, type Bindings } from './types'

const likes = new Hono<{ Bindings: Bindings }>()

type LikesData = { count: number; hashes: string[] }

const likesAllowedOrigins = new Set(['https://getcmdr.com', 'https://www.getcmdr.com'])

/**
 * The slug shape the blog itself produces (`src/content/blog/<slug>/index.md`, minted by the dev
 * editor's `slugPattern`). Anything else is refused BEFORE a KV key is touched: `POST /likes/:slug`
 * is unauthenticated and creates the key it writes, so without this an arbitrary slug is an
 * unbounded KV-growth primitive.
 */
const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const maxSlugLength = 80

function likesCors(c: {
  req: { header: (name: string) => string | undefined }
  header: (name: string, value: string) => void
}) {
  const origin = c.req.header('origin')
  if (origin && likesAllowedOrigins.has(origin)) {
    c.header('Access-Control-Allow-Origin', origin)
    c.header('Access-Control-Allow-Methods', 'GET, POST, DELETE, OPTIONS')
    c.header('Access-Control-Allow-Headers', 'Content-Type')
    c.header('Vary', 'Origin')
  }
}

function isValidSlug(slug: string): boolean {
  return slug.length <= maxSlugLength && slugPattern.test(slug)
}

/**
 * Per-reader pseudonym for one post, from the shared `hashCallerIp` (so blog likes and telemetry
 * anonymize an IP exactly one way), salted with the SLUG rather than the day: a like has to stay
 * recognizable for years, and it makes one reader unlinkable across posts.
 *
 * Truncated to the first 16 hex chars because this one is stored per liker per post; the full digest
 * would quadruple every KV value for no gain at these counts.
 */
async function likePseudonym(slug: string, ip: string, pepper: string | undefined): Promise<string> {
  return (await hashCallerIp(ip, slug, pepper)).slice(0, 16)
}

async function getLikesData(kv: KVNamespace, slug: string): Promise<LikesData> {
  const raw = await kv.get(`likes:${slug}`)
  if (!raw) return { count: 0, hashes: [] }
  return JSON.parse(raw) as LikesData
}

/**
 * Shared entry gate for every likes route: slug shape first, then (for writes) the IP rate limit.
 * Returns either the caller's per-post pseudonym or the response to send instead.
 */
async function resolveCaller(
  c: {
    env: Bindings
    req: { header: (name: string) => string | undefined; param: (name: string) => string }
    json: (body: unknown, status?: number) => Response
  },
  options: { rateLimited: boolean },
): Promise<{ slug: string; ipHash: string } | { response: Response }> {
  const slug = c.req.param('slug')
  if (!isValidSlug(slug)) {
    return { response: c.json({ error: 'Unknown post' }, 400) }
  }

  if (options.rateLimited) {
    const limited = await enforceIpRateLimit(c.env.LIKES_LIMITER, c.req)
    if (limited) return { response: limited }
  }

  return { slug, ipHash: await likePseudonym(slug, callerIp(c.req), c.env.IP_HASH_PEPPER) }
}

likes.options('/likes/:slug', (c) => {
  likesCors(c)
  return c.body(null, 204)
})

likes.get('/likes/:slug', async (c) => {
  likesCors(c)
  const caller = await resolveCaller(c, { rateLimited: false })
  if ('response' in caller) return caller.response

  const data = await getLikesData(c.env.BLOG_LIKES, caller.slug)
  return c.json({ count: data.count, liked: data.hashes.includes(caller.ipHash) })
})

likes.post('/likes/:slug', async (c) => {
  likesCors(c)
  const caller = await resolveCaller(c, { rateLimited: true })
  if ('response' in caller) return caller.response

  const data = await getLikesData(c.env.BLOG_LIKES, caller.slug)

  if (!data.hashes.includes(caller.ipHash)) {
    data.hashes.push(caller.ipHash)
    data.count = data.hashes.length
    await c.env.BLOG_LIKES.put(`likes:${caller.slug}`, JSON.stringify(data))
  }

  return c.json({ count: data.count, liked: true })
})

likes.delete('/likes/:slug', async (c) => {
  likesCors(c)
  const caller = await resolveCaller(c, { rateLimited: true })
  if ('response' in caller) return caller.response

  const data = await getLikesData(c.env.BLOG_LIKES, caller.slug)

  const idx = data.hashes.indexOf(caller.ipHash)
  if (idx !== -1) {
    data.hashes.splice(idx, 1)
    data.count = data.hashes.length
    await c.env.BLOG_LIKES.put(`likes:${caller.slug}`, JSON.stringify(data))
  }

  return c.json({ count: data.count, liked: false })
})

export { likes }
