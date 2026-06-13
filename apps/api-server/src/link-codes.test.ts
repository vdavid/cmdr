import { describe, expect, it, vi } from 'vitest'
import { app } from './index'
import { sanitizeUtmValue, isValidCode, type LinkCodeMap } from './link-codes'

/** Minimal KV mock backed by an in-memory store, supporting get/put/delete. */
function createMockKv(store: Record<string, string> = {}): KVNamespace {
  return {
    get: vi.fn((key: string, format?: string) => {
      if (!(key in store)) return Promise.resolve(null)
      const value = store[key]
      if (format === 'json') return Promise.resolve(JSON.parse(value))
      return Promise.resolve(value)
    }),
    put: vi.fn((key: string, value: string) => {
      store[key] = value
      return Promise.resolve()
    }),
    delete: vi.fn((key: string) => {
      // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test mock, key is a local var
      delete store[key]
      return Promise.resolve()
    }),
  } as unknown as KVNamespace
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() }
}

function createMockD1(): D1Database {
  const run = vi.fn(() => Promise.resolve({ success: true }))
  const bind = vi.fn(() => ({ run }))
  const prepare = vi.fn(() => ({ bind }))
  return { prepare } as unknown as D1Database
}

const adminToken = 'test-admin-token-secret'

function makeBindings(linkCodesStore: Record<string, string> = {}) {
  return {
    LICENSE_CODES: createMockKv(),
    LINK_CODES: createMockKv(linkCodesStore),
    DEVICE_COUNTS: createMockAnalyticsEngine(),
    TELEMETRY_DB: createMockD1(),
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: adminToken,
  }
}

const authHeader = { Authorization: `Bearer ${adminToken}` }

describe('sanitizeUtmValue', () => {
  it('lowercases and strips disallowed chars', () => {
    expect(sanitizeUtmValue('Hacker News!')).toBe('hackernews')
  })
  it('keeps allowed punctuation', () => {
    expect(sanitizeUtmValue('rust-users_forum.v2')).toBe('rust-users_forum.v2')
  })
  it('caps length', () => {
    expect(sanitizeUtmValue('a'.repeat(200)).length).toBe(120)
  })
  it('returns empty for nullish', () => {
    expect(sanitizeUtmValue(undefined)).toBe('')
    expect(sanitizeUtmValue('')).toBe('')
  })
})

describe('isValidCode', () => {
  it('accepts lowercase alnum and . _ -', () => {
    expect(isValidCode('rmc')).toBe(true)
    expect(isValidCode('hn.2026')).toBe(true)
    expect(isValidCode('rust-users_forum')).toBe(true)
  })
  it('rejects uppercase, spaces, and other chars', () => {
    expect(isValidCode('RMC')).toBe(false)
    expect(isValidCode('a b')).toBe(false)
    expect(isValidCode('a/b')).toBe(false)
    expect(isValidCode('a:b')).toBe(false)
  })
  it('rejects empty and over-long codes', () => {
    expect(isValidCode('')).toBe(false)
    expect(isValidCode('a'.repeat(65))).toBe(false)
  })
})

describe('GET /r-codes.json (public)', () => {
  it('returns the public map (source/medium only, no note) with CORS + cache headers', async () => {
    const store = {
      codes: JSON.stringify({
        rmc: { utm_source: 'reddit', utm_medium: 'social', note: 'r/macapps comment' },
        hn: { utm_source: 'hackernews' },
      } satisfies LinkCodeMap),
    }
    const res = await app.request('/r-codes.json', {}, makeBindings(store))
    expect(res.status).toBe(200)
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('*')
    expect(res.headers.get('Cache-Control')).toMatch(/max-age=\d+/)
    const body = await res.json()
    expect(body).toEqual({
      rmc: { utm_source: 'reddit', utm_medium: 'social' },
      hn: { utm_source: 'hackernews' },
    })
    // note must not leak into the public payload
    expect(JSON.stringify(body)).not.toContain('note')
  })

  it('returns an empty object when no codes are stored', async () => {
    const res = await app.request('/r-codes.json', {}, makeBindings())
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual({})
  })

  it('answers OPTIONS preflight with CORS headers', async () => {
    const res = await app.request('/r-codes.json', { method: 'OPTIONS' }, makeBindings())
    expect(res.status).toBe(204)
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('*')
  })
})

describe('GET /admin/r-codes (list)', () => {
  it('rejects without auth', async () => {
    const res = await app.request('/admin/r-codes', {}, makeBindings())
    expect(res.status).toBe(401)
  })

  it('returns the full map including notes with auth', async () => {
    const store = {
      codes: JSON.stringify({ rmc: { utm_source: 'reddit', utm_medium: 'social', note: 'a note' } }),
    }
    const res = await app.request('/admin/r-codes', { headers: authHeader }, makeBindings(store))
    expect(res.status).toBe(200)
    const body: LinkCodeMap = await res.json()
    expect(body.rmc.note).toBe('a note')
  })
})

describe('PUT /admin/r-codes/:code (upsert)', () => {
  it('rejects without auth', async () => {
    const res = await app.request(
      '/admin/r-codes/rmc',
      {
        method: 'PUT',
        body: JSON.stringify({ utm_source: 'reddit' }),
        headers: { 'Content-Type': 'application/json' },
      },
      makeBindings(),
    )
    expect(res.status).toBe(401)
  })

  it('creates a new code, sanitizing utm values', async () => {
    const store: Record<string, string> = {}
    const res = await app.request(
      '/admin/r-codes/rmc',
      {
        method: 'PUT',
        body: JSON.stringify({ utm_source: 'Reddit!', utm_medium: 'Social', note: 'r/macapps' }),
        headers: { ...authHeader, 'Content-Type': 'application/json' },
      },
      makeBindings(store),
    )
    expect(res.status).toBe(200)
    const saved = JSON.parse(store.codes) as LinkCodeMap
    expect(saved.rmc).toEqual({ utm_source: 'reddit', utm_medium: 'social', note: 'r/macapps' })
  })

  it('updates an existing code', async () => {
    const store = { codes: JSON.stringify({ rmc: { utm_source: 'old' } }) }
    const res = await app.request(
      '/admin/r-codes/rmc',
      {
        method: 'PUT',
        body: JSON.stringify({ utm_source: 'reddit' }),
        headers: { ...authHeader, 'Content-Type': 'application/json' },
      },
      makeBindings(store),
    )
    expect(res.status).toBe(200)
    const saved = JSON.parse(store.codes) as LinkCodeMap
    expect(saved.rmc.utm_source).toBe('reddit')
  })

  it('rejects an invalid code in the path', async () => {
    const res = await app.request(
      '/admin/r-codes/Bad%20Code',
      {
        method: 'PUT',
        body: JSON.stringify({ utm_source: 'reddit' }),
        headers: { ...authHeader, 'Content-Type': 'application/json' },
      },
      makeBindings(),
    )
    expect(res.status).toBe(400)
  })

  it('rejects when utm_source is missing or sanitizes to empty', async () => {
    const res = await app.request(
      '/admin/r-codes/rmc',
      {
        method: 'PUT',
        body: JSON.stringify({ utm_source: '!!!' }),
        headers: { ...authHeader, 'Content-Type': 'application/json' },
      },
      makeBindings(),
    )
    expect(res.status).toBe(400)
  })
})

describe('DELETE /admin/r-codes/:code', () => {
  it('rejects without auth', async () => {
    const res = await app.request('/admin/r-codes/rmc', { method: 'DELETE' }, makeBindings())
    expect(res.status).toBe(401)
  })

  it('removes the code from the map', async () => {
    const store = {
      codes: JSON.stringify({ rmc: { utm_source: 'reddit' }, hn: { utm_source: 'hackernews' } }),
    }
    const res = await app.request('/admin/r-codes/rmc', { method: 'DELETE', headers: authHeader }, makeBindings(store))
    expect(res.status).toBe(200)
    const saved = JSON.parse(store.codes) as LinkCodeMap
    expect(saved.rmc).toBeUndefined()
    expect(saved.hn).toBeDefined()
  })
})
