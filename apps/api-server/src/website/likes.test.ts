import { describe, expect, it, vi } from 'vitest'
import { app } from '../index'

/** In-memory KV stub over a plain Map, enough for the read-modify-write the likes routes do. */
function createKv(seed: Record<string, string> = {}) {
  const store = new Map(Object.entries(seed))
  const kv = {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
  return { kv, store }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    BLOG_LIKES: createKv().kv,
    IP_HASH_PEPPER: 'test-pepper',
    LICENSE_CODES: {} as KVNamespace,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ...overrides,
  }
}

function request(path: string, method: string, bindings: ReturnType<typeof createBindings>, ip = '203.0.113.7') {
  return app.request(path, { method, headers: { 'cf-connecting-ip': ip } }, bindings)
}

/** Plain, unpeppered SHA-256, standing in for what an attacker can compute over the IPv4 space. */
async function sha256Hex(input: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input))
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** The stored hash for a slug, or undefined when nothing was written. */
function storedHashes(store: Map<string, string>, slug: string): string[] | undefined {
  const raw = store.get(`likes:${slug}`)
  return raw ? (JSON.parse(raw) as { hashes: string[] }).hashes : undefined
}

describe('likes slug validation', () => {
  it.each([
    ['a slug with a slash', 'foo/bar'],
    ['an uppercase slug', 'Foo-Bar'],
    ['a slug with a double hyphen', 'foo--bar'],
    ['a slug with a dot', 'foo.bar'],
    ['an empty-ish slug', '-'],
  ])('rejects %s without writing to KV', async (_label, slug) => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    const res = await request(`/likes/${encodeURIComponent(slug)}`, 'POST', bindings)

    expect(res.status).toBe(400)
    expect(store.size).toBe(0)
  })

  it('rejects an over-long slug without writing to KV', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    const res = await request(`/likes/${'a'.repeat(81)}`, 'POST', bindings)

    expect(res.status).toBe(400)
    expect(store.size).toBe(0)
  })

  it('rejects an invalid slug on GET too, so a read cannot be amplified', async () => {
    const bindings = createBindings()

    const res = await request('/likes/Foo_Bar', 'GET', bindings)

    expect(res.status).toBe(400)
  })

  it('accepts a well-formed slug', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    const res = await request('/likes/why-cmdr-is-fast-2', 'POST', bindings)

    expect(res.status).toBe(200)
    expect(await res.json()).toEqual({ count: 1, liked: true })
    expect(storedHashes(store, 'why-cmdr-is-fast-2')).toHaveLength(1)
  })
})

describe('likes rate limiting', () => {
  it('returns 429 and writes nothing when the limiter rejects the caller', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({
      BLOG_LIKES: kv,
      LIKES_LIMITER: { limit: vi.fn(() => Promise.resolve({ success: false })) },
    })

    const res = await request('/likes/a-post', 'POST', bindings)

    expect(res.status).toBe(429)
    expect(store.size).toBe(0)
  })

  it('lets the request through when the limiter allows it', async () => {
    const limit = vi.fn(() => Promise.resolve({ success: true }))
    const bindings = createBindings({ LIKES_LIMITER: { limit } })

    const res = await request('/likes/a-post', 'POST', bindings)

    expect(res.status).toBe(200)
    expect(limit).toHaveBeenCalledWith({ key: '203.0.113.7' })
  })

  it('gates DELETE as well as POST', async () => {
    const bindings = createBindings({
      LIKES_LIMITER: { limit: vi.fn(() => Promise.resolve({ success: false })) },
    })

    const res = await request('/likes/a-post', 'DELETE', bindings)

    expect(res.status).toBe(429)
  })
})

describe('likes IP pseudonyms', () => {
  it('never stores the plain SHA-256 of the IP, which is what a missing pepper would let anyone reverse', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    await request('/likes/a-post', 'POST', bindings)

    const unpeppered = await sha256Hex('203.0.113.7')
    const unpepperedWithSlug = await sha256Hex('203.0.113.7a-post')
    expect(storedHashes(store, 'a-post')).not.toContainEqual(unpeppered.slice(0, 16))
    expect(storedHashes(store, 'a-post')).not.toContainEqual(unpepperedWithSlug.slice(0, 16))
  })

  it('derives a different pseudonym for the same IP under a different pepper', async () => {
    const first = createKv()
    const second = createKv()

    await request('/likes/a-post', 'POST', createBindings({ BLOG_LIKES: first.kv, IP_HASH_PEPPER: 'pepper-one' }))
    await request('/likes/a-post', 'POST', createBindings({ BLOG_LIKES: second.kv, IP_HASH_PEPPER: 'pepper-two' }))

    expect(storedHashes(first.store, 'a-post')).not.toEqual(storedHashes(second.store, 'a-post'))
  })

  it('derives a different pseudonym per slug, so one reader is not linkable across posts', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    await request('/likes/first-post', 'POST', bindings)
    await request('/likes/second-post', 'POST', bindings)

    expect(storedHashes(store, 'first-post')).not.toEqual(storedHashes(store, 'second-post'))
  })

  it('is stable for one IP on one slug, so a second like is a no-op', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    await request('/likes/a-post', 'POST', bindings)
    const res = await request('/likes/a-post', 'POST', bindings)

    expect(await res.json()).toEqual({ count: 1, liked: true })
    expect(storedHashes(store, 'a-post')).toHaveLength(1)
  })

  it('separates two different IPs', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    await request('/likes/a-post', 'POST', bindings, '203.0.113.7')
    await request('/likes/a-post', 'POST', bindings, '198.51.100.4')

    expect(storedHashes(store, 'a-post')).toHaveLength(2)
  })
})

describe('likes round trip', () => {
  it('reports the caller as having liked the post, then unliked after DELETE', async () => {
    const { kv } = createKv()
    const bindings = createBindings({ BLOG_LIKES: kv })

    await request('/likes/a-post', 'POST', bindings)
    const afterLike = await request('/likes/a-post', 'GET', bindings)
    expect(await afterLike.json()).toEqual({ count: 1, liked: true })

    await request('/likes/a-post', 'DELETE', bindings)
    const afterUnlike = await request('/likes/a-post', 'GET', bindings)
    expect(await afterUnlike.json()).toEqual({ count: 0, liked: false })
  })
})
