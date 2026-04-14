import { Hono } from 'hono'
import type { Bindings } from './types'

const likes = new Hono<{ Bindings: Bindings }>()

type LikesData = { count: number; hashes: string[] }

const likesAllowedOrigins = new Set(['https://getcmdr.com', 'https://www.getcmdr.com'])

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

async function hashIpForLikes(ip: string): Promise<string> {
  const buffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode('cmdr-likes:' + ip))
  return [...new Uint8Array(buffer)]
    .slice(0, 8)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

async function getLikesData(kv: KVNamespace, slug: string): Promise<LikesData> {
  const raw = await kv.get(`likes:${slug}`)
  if (!raw) return { count: 0, hashes: [] }
  return JSON.parse(raw) as LikesData
}

likes.options('/likes/:slug', (c) => {
  likesCors(c)
  return c.body(null, 204)
})

likes.get('/likes/:slug', async (c) => {
  likesCors(c)
  const slug = c.req.param('slug')
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  const ipHash = await hashIpForLikes(ip)
  const data = await getLikesData(c.env.BLOG_LIKES, slug)
  return c.json({ count: data.count, liked: data.hashes.includes(ipHash) })
})

likes.post('/likes/:slug', async (c) => {
  likesCors(c)
  const slug = c.req.param('slug')
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  const ipHash = await hashIpForLikes(ip)
  const data = await getLikesData(c.env.BLOG_LIKES, slug)

  if (!data.hashes.includes(ipHash)) {
    data.hashes.push(ipHash)
    data.count = data.hashes.length
    await c.env.BLOG_LIKES.put(`likes:${slug}`, JSON.stringify(data))
  }

  return c.json({ count: data.count, liked: true })
})

likes.delete('/likes/:slug', async (c) => {
  likesCors(c)
  const slug = c.req.param('slug')
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  const ipHash = await hashIpForLikes(ip)
  const data = await getLikesData(c.env.BLOG_LIKES, slug)

  const idx = data.hashes.indexOf(ipHash)
  if (idx !== -1) {
    data.hashes.splice(idx, 1)
    data.count = data.hashes.length
    await c.env.BLOG_LIKES.put(`likes:${slug}`, JSON.stringify(data))
  }

  return c.json({ count: data.count, liked: false })
})

export { likes }
