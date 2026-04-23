import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { app } from './index'

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

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: createKv(),
    DEVICE_COUNTS: { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset,
    TELEMETRY_DB: createMockD1(),
    ERROR_REPORTS_BUCKET: createR2(),
    ERROR_REPORT_META: createKv(),
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
  kind: 'user' as const,
  appVersion: '0.13.0',
  osVersion: '15.3.1',
  arch: 'aarch64',
  generatedAt: '2026-04-23T10:00:00Z',
}

beforeEach(() => {
  // Swallow Discord webhook calls if one somehow gets through
  globalThis.fetch = (() => Promise.resolve(new Response(null, { status: 204 }))) as unknown as typeof fetch
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('POST /error-report', () => {
  it('returns 200 with an ERR-XXXXX id on a valid upload', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1, 2, 3, 4]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(200)
    const body = await res.json<{ id: string }>()
    expect(body.id).toMatch(/^ERR-[23456789A-HJ-NP-Z]{5}$/)
  })

  it('writes the bundle to R2 with the expected key shape and metadata', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const fd = buildMultipart(new Uint8Array([9, 9, 9]), validMeta)

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
    const { id } = await res.json<{ id: string }>()

    const [[key, obj]] = [...bucket._store.entries()]
    expect(key).toMatch(new RegExp(`^error-reports/\\d{4}-\\d{2}-\\d{2}/${id}-[0-9a-f-]{36}\\.zip$`))
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
})
