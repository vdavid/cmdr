/**
 * R2, KV, D1, and binding fakes shared by the `POST /error-report` route tests
 * (`error-report.test.ts` and `error-report-email.test.ts`). The Resend mock stays per-file:
 * `vi.mock` is hoisted into the file it appears in, so it can't be shared from here.
 */
import { vi, type Mock } from 'vitest'

/** The UTC day the route charges an upload against. */
export function todayUtc(): string {
  return new Date().toISOString().slice(0, 10)
}

export interface StoredObj {
  body: Uint8Array
  size: number
  customMetadata?: Record<string, string>
  uploaded: Date
}

/** In-memory R2 stub matching the subset we use. */
export function createR2(): R2Bucket & { _store: Map<string, StoredObj> } {
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
export function createKv(): KVNamespace {
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

export function createMockD1(): D1Database {
  const run = vi.fn(() => Promise.resolve({ success: true }))
  const bind = vi.fn(() => ({ run }))
  const prepare = vi.fn(() => ({ bind }))
  return { prepare } as unknown as D1Database
}

/** Mock the Workers rate-limit binding. Defaults to allowing every request. */
export function createMockRateLimiter(success = true): { limiter: RateLimit; limitMock: Mock } {
  const limitMock = vi.fn(() => Promise.resolve({ success }))
  return { limiter: { limit: limitMock }, limitMock }
}

export function createBindings(overrides: Record<string, unknown> = {}) {
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
    // Discord webhook and notification recipient intentionally unset → no network calls, no email
    ...overrides,
  }
}

/** The manifest a well-formed hand-written report carries. */
export const validMeta = {
  id: 'ERR-A2345',
  kind: 'user' as const,
  appVersion: '0.13.0',
  osVersion: '15.3.1',
  arch: 'aarch64',
  generatedAt: '2026-04-23T10:00:00Z',
}

/** Build a multipart/form-data body for the error-report endpoint. */
export function buildMultipart(bundleBytes: Uint8Array, meta: unknown, bundleName = 'bundle.zip'): FormData {
  const fd = new FormData()
  fd.append('bundle', new Blob([new Uint8Array(bundleBytes)], { type: 'application/zip' }), bundleName)
  fd.append('meta', JSON.stringify(meta))
  return fd
}
