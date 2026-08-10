import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { app } from './index'
import { DAILY_INTAKE_BUDGET_BYTES, dailyBytesKey, pauseIntake } from './error-report-intake'

/** The UTC day the route charges an upload against. */
function todayUtc(): string {
  return new Date().toISOString().slice(0, 10)
}

interface StoredObj {
  body: Uint8Array
  size: number
  customMetadata?: Record<string, string>
  uploaded: Date
}

/** In-memory R2 stub matching the subset we use. */
function createR2(): R2Bucket & { _store: Map<string, StoredObj> } {
  const store = new Map<string, StoredObj>()
  return {
    _store: store,
    head: (key: string) => Promise.resolve(store.has(key) ? ({ key } as unknown as R2Object) : null),
    put: async (
      key: string,
      value: ReadableStream | ArrayBuffer | Uint8Array | string,
      opts?: { httpMetadata?: unknown; customMetadata?: Record<string, string> },
    ) => {
      let bytes: Uint8Array
      if (value instanceof Uint8Array) bytes = value
      else if (value instanceof ArrayBuffer) bytes = new Uint8Array(value)
      else if (typeof value === 'string') bytes = new TextEncoder().encode(value)
      else {
        // ReadableStream
        const reader = (value as ReadableStream<Uint8Array>).getReader()
        const chunks: Uint8Array[] = []
        let total = 0
        let done = false
        while (!done) {
          const readResult = await reader.read()
          done = readResult.done
          const chunk = readResult.value
          if (chunk) {
            chunks.push(chunk)
            total += chunk.length
          }
        }
        bytes = new Uint8Array(total)
        let offset = 0
        for (const c of chunks) {
          bytes.set(c, offset)
          offset += c.length
        }
      }
      store.set(key, {
        body: bytes,
        size: bytes.length,
        customMetadata: opts?.customMetadata,
        uploaded: new Date(),
      })
      return { key, size: bytes.length } as unknown
    },
    list: ({ prefix, cursor, limit }: { prefix?: string; cursor?: string; limit?: number } = {}) => {
      const all = [...store.entries()]
        .filter(([k]) => !prefix || k.startsWith(prefix))
        .sort(([a], [b]) => (a < b ? -1 : 1))
      const pageSize = limit ?? 1000
      const startIdx = cursor ? parseInt(cursor, 10) : 0
      const slice = all.slice(startIdx, startIdx + pageSize)
      return Promise.resolve({
        objects: slice.map(([k, v]) => ({ key: k, size: v.size, uploaded: v.uploaded })),
        truncated: startIdx + pageSize < all.length,
        cursor: startIdx + pageSize < all.length ? String(startIdx + pageSize) : undefined,
      })
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
  } as unknown as R2Bucket & { _store: Map<string, StoredObj> }
}

/** In-memory KV stub. */
function createKv(): KVNamespace {
  const store = new Map<string, string>()
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
}

function createMockD1(): D1Database {
  const run = vi.fn(() => Promise.resolve({ success: true }))
  const bind = vi.fn(() => ({ run }))
  const prepare = vi.fn(() => ({ bind }))
  return { prepare } as unknown as D1Database
}

/** Mock the Workers rate-limit binding. Defaults to allowing every request. */
function createMockRateLimiter(success = true): { limiter: RateLimit; limitMock: Mock } {
  const limitMock = vi.fn(() => Promise.resolve({ success }))
  return { limiter: { limit: limitMock }, limitMock }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: createKv(),
    DEVICE_COUNTS: { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset,
    TELEMETRY_DB: createMockD1(),
    ERROR_REPORTS_BUCKET: createR2(),
    ERROR_REPORT_META: createKv(),
    ERROR_REPORT_LIMITER: createMockRateLimiter().limiter,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: 'test-admin-token-secret',
    // Discord webhook intentionally unset → no network calls
    ...overrides,
  }
}

/** Build a multipart/form-data body for the error-report endpoint. */
function buildMultipart(bundleBytes: Uint8Array, meta: unknown, bundleName = 'bundle.zip'): FormData {
  const fd = new FormData()
  fd.append('bundle', new Blob([new Uint8Array(bundleBytes)], { type: 'application/zip' }), bundleName)
  fd.append('meta', JSON.stringify(meta))
  return fd
}

const validMeta = {
  id: 'ERR-A2345',
  kind: 'user' as const,
  appVersion: '0.13.0',
  osVersion: '15.3.1',
  arch: 'aarch64',
  generatedAt: '2026-04-23T10:00:00Z',
}

beforeEach(() => {
  // Swallow Discord webhook calls if one somehow gets through
  globalThis.fetch = () => Promise.resolve(new Response(null, { status: 204 }))
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('POST /error-report', () => {
  it('returns 200 echoing the client-supplied id on a valid upload', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1, 2, 3, 4]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(200)
    const body = await res.json<{ id: string }>()
    expect(body.id).toBe(validMeta.id)
  })

  it('accepts a note-less report where the Rust client sends userNote: null', async () => {
    // serde `Option::None` serializes as JSON `null`, not omitted. A `!== undefined`-only
    // validator rejected every note-less report with a 400 (the bug that broke sending).
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1, 2, 3, 4]), { ...validMeta, userNote: null, buildMode: null })

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(200)
    const body = await res.json<{ id: string }>()
    expect(body.id).toBe(validMeta.id)
  })

  it('writes the bundle to R2 with the new env/date key shape and metadata', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const fd = buildMultipart(new Uint8Array([9, 9, 9]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
    const { id } = await res.json<{ id: string }>()

    const [[key, obj]] = [...bucket._store.entries()]
    // Default `validMeta` has no `buildMode`, which is treated as `'release'` → `prod`.
    expect(key).toMatch(new RegExp(`^error-reports/prod/\\d{4}-\\d{2}-\\d{2}/${id}-[0-9a-f-]{36}\\.zip$`))
    expect(obj.customMetadata).toMatchObject({
      id,
      kind: 'user',
      appVersion: '0.13.0',
      osVersion: '15.3.1',
      arch: 'aarch64',
      generatedAt: '2026-04-23T10:00:00Z',
    })
    expect(obj.size).toBe(3)
  })

  it('places debug-build uploads under the `dev/` env prefix', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const fd = buildMultipart(new Uint8Array([1, 2, 3]), { ...validMeta, buildMode: 'debug' })

    await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    const [[key]] = [...bucket._store.entries()]
    expect(key).toMatch(new RegExp(`^error-reports/dev/\\d{4}-\\d{2}-\\d{2}/${validMeta.id}-[0-9a-f-]{36}\\.zip$`))
  })

  it('returns 400 when the meta id is missing', async () => {
    const bindings = createBindings()
    const { id: _id, ...rest } = validMeta
    void _id
    const fd = buildMultipart(new Uint8Array([1]), rest)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid meta shape')
  })

  it('returns 400 when the meta id is malformed', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1]), { ...validMeta, id: 'ERR-LOWER' })

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid meta shape')
  })

  it('returns 413 for a bundle over 10 MB', async () => {
    const bindings = createBindings()
    // 11 MB of 0s
    const big = new Uint8Array(11 * 1024 * 1024)
    const fd = buildMultipart(big, validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(413)
    const body = await res.json<{ error: string }>()
    expect(body.error).toContain('too large')
  })

  it('returns 400 when "meta" is missing', async () => {
    const bindings = createBindings()
    const fd = new FormData()
    fd.append('bundle', new Blob([new Uint8Array([1])], { type: 'application/zip' }), 'b.zip')

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toMatch(/meta/)
  })

  it('returns 400 when "bundle" is missing', async () => {
    const bindings = createBindings()
    const fd = new FormData()
    fd.append('meta', JSON.stringify(validMeta))

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toMatch(/bundle/)
  })

  it('returns 400 for malformed meta JSON', async () => {
    const bindings = createBindings()
    const fd = new FormData()
    fd.append('bundle', new Blob([new Uint8Array([1])], { type: 'application/zip' }), 'b.zip')
    fd.append('meta', 'not-json{{')

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toContain('Malformed')
  })

  it('returns 400 for meta with invalid kind', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1]), { ...validMeta, kind: 'oops' })

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid meta shape')
  })

  it('returns 400 for meta missing a required field', async () => {
    const bindings = createBindings()
    const { arch, ...rest } = validMeta
    void arch
    const fd = buildMultipart(new Uint8Array([1]), rest)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
  })

  it('returns 429 when the caller is over the IP rate limit', async () => {
    const { limiter } = createMockRateLimiter(false)
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORT_LIMITER: limiter, ERROR_REPORTS_BUCKET: bucket })
    const fd = buildMultipart(new Uint8Array([1, 2, 3, 4]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(429)
    // The gate runs before the body is parsed or stored, so a flood costs no R2 writes.
    expect(bucket._store.size).toBe(0)
  })

  it('keys the rate limiter by the caller IP', async () => {
    const { limiter, limitMock } = createMockRateLimiter(true)
    const bindings = createBindings({ ERROR_REPORT_LIMITER: limiter })
    const fd = buildMultipart(new Uint8Array([1]), validMeta)

    await app.request(
      '/error-report',
      { method: 'POST', body: fd, headers: { 'cf-connecting-ip': '203.0.113.7' } },
      bindings,
    )

    expect(limitMock).toHaveBeenCalledWith({ key: '203.0.113.7' })
  })

  it('increments the total_bytes counter by the upload size', async () => {
    const kv = createKv()
    const bindings = createBindings({ ERROR_REPORT_META: kv })
    const payload = new Uint8Array(1234)
    const fd = buildMultipart(payload, validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
    expect(res.status).toBe(200)

    // Background work (waitUntil fallback awaits inline in tests)
    const total = await kv.get('total_bytes')
    expect(total).toBe('1234')
  })

  it('charges the accepted upload against the daily intake budget', async () => {
    const kv = createKv()
    const bindings = createBindings({ ERROR_REPORT_META: kv })
    const fd = buildMultipart(new Uint8Array(4321), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
    expect(res.status).toBe(200)

    expect(await kv.get(dailyBytesKey(todayUtc()))).toBe('4321')
  })

  it('returns 503 once the global daily byte budget is spent', async () => {
    const kv = createKv()
    await kv.put(dailyBytesKey(todayUtc()), String(DAILY_INTAKE_BUDGET_BYTES))
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORT_META: kv, ERROR_REPORTS_BUCKET: bucket })
    const fd = buildMultipart(new Uint8Array([1, 2, 3]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(503)
    expect(res.headers.get('Retry-After')).toBeTruthy()
    // A rejected upload costs no storage: the whole point of the ceiling.
    expect(bucket._store.size).toBe(0)
  })

  it('returns 503 while intake is paused', async () => {
    const kv = createKv()
    await pauseIntake(kv)
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORT_META: kv, ERROR_REPORTS_BUCKET: bucket })
    const fd = buildMultipart(new Uint8Array([1, 2, 3]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(503)
    expect(bucket._store.size).toBe(0)
  })

  it('pings Discord once when the budget runs out, not once per rejected upload', async () => {
    const kv = createKv()
    await kv.put(dailyBytesKey(todayUtc()), String(DAILY_INTAKE_BUDGET_BYTES))
    const posts: string[] = []
    globalThis.fetch = ((url: string) => {
      posts.push(url)
      return Promise.resolve(new Response(null, { status: 204 }))
    }) as unknown as typeof fetch
    const bindings = createBindings({
      ERROR_REPORT_META: kv,
      DISCORD_WEBHOOK_URL: 'https://discord.example/webhook',
    })

    for (let i = 0; i < 3; i++) {
      const fd = buildMultipart(new Uint8Array([1]), validMeta)
      const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
      expect(res.status).toBe(503)
    }

    expect(posts).toHaveLength(1)
  })
})
