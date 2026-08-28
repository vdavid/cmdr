/**
 * `POST /error-report/:id/amend`: the second thought a reporter has after their bundle is already
 * in R2. Covers the credential check (the part that decides whether a stranger can write into
 * someone else's report), the sidecar's read-modify-write, and the validation surface.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { app } from '../index'
import { buildMultipart, createBindings, createKv, createR2, todayUtc, validMeta } from './error-report-test-helpers'
import { amendSidecarKey } from './error-report-eviction'
import { reportIndexKey, type ReportIndexEntry } from './error-report-amend'
import { dailyBytesKey } from './error-report-intake'

/** One amendment as the sidecar records it. */
interface StoredAmendment {
  note: string | null
  email: string | null
  amendedAt: string
}

/** Upload one report and hand back what the amend route needs. */
async function upload(
  bindings: Record<string, unknown>,
  meta: unknown = validMeta,
): Promise<{ id: string; amendKey: string | null }> {
  const fd = buildMultipart(new Uint8Array([1, 2, 3]), meta)
  const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
  expect(res.status).toBe(200)
  return await res.json<{ id: string; amendKey: string | null }>()
}

/** POST an amendment body the way the desktop client does. */
async function amend(bindings: Record<string, unknown>, id: string, body: unknown): Promise<Response> {
  return await app.request(
    `/error-report/${id}/amend`,
    { method: 'POST', body: JSON.stringify(body), headers: { 'content-type': 'application/json' } },
    bindings,
  )
}

/** The index entry the upload wrote, parsed. */
async function readIndex(kv: KVNamespace, id: string): Promise<ReportIndexEntry | null> {
  const raw = await kv.get(reportIndexKey(id))
  return raw === null ? null : (JSON.parse(raw) as ReportIndexEntry)
}

/** The sidecar's parsed contents, or null when no sidecar was written. */
function readSidecar(
  bucket: ReturnType<typeof createR2>,
  bundleKey: string,
): { id: string; amendments: StoredAmendment[] } | null {
  const stored = bucket._store.get(amendSidecarKey(bundleKey))
  if (!stored) return null
  return JSON.parse(new TextDecoder().decode(stored.body)) as { id: string; amendments: StoredAmendment[] }
}

/**
 * A JSON body delivered as a stream, so the request carries no `content-length` and the bytes are
 * produced lazily. `padBytes` of filler go into a `note`, emitted in 64 KB chunks, so a 4 MB case
 * never materializes as one buffer here either.
 */
function streamingAmendBody(padBytes: number): {
  body: ReadableStream<Uint8Array>
  /** Bytes the server actually pulled off the stream. The point of the cap is that this stays small. */
  bytesProduced: () => number
} {
  const encoder = new TextEncoder()
  const head = encoder.encode('{"amendKey":"' + 'x'.repeat(43) + '","note":"')
  const tail = encoder.encode('"}')
  const chunkSize = 64 * 1024

  let sent = 0
  let produced = 0
  // The route cancels the read as soon as the cap trips, so `pull` can still be in flight against a
  // closed controller. Enqueuing then would throw an unhandled rejection outliving the test.
  let cancelled = false
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      produced += head.byteLength
      controller.enqueue(head)
    },
    pull(controller) {
      if (cancelled) return
      if (sent >= padBytes) {
        produced += tail.byteLength
        controller.enqueue(tail)
        controller.close()
        return
      }
      const size = Math.min(chunkSize, padBytes - sent)
      controller.enqueue(encoder.encode('a'.repeat(size)))
      sent += size
      produced += size
    },
    cancel() {
      cancelled = true
    },
  })

  return { body, bytesProduced: () => produced }
}

beforeEach(() => {
  // Swallow Discord webhook calls if one somehow gets through.
  globalThis.fetch = () => Promise.resolve(new Response(null, { status: 204 }))
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('POST /error-report indexing', () => {
  it('hands back an amend key and indexes the report before the 200 ships', async () => {
    const kv = createKv()
    const bindings = createBindings({ ERROR_REPORT_META: kv })

    const { id, amendKey } = await upload(bindings)

    expect(amendKey).toMatch(/^[A-Za-z0-9_-]{43}$/)
    const entry = await readIndex(kv, id)
    expect(entry?.env).toBe('prod')
    expect(entry?.date).toBe(todayUtc())
    expect(entry?.key).toMatch(new RegExp(`^error-reports/prod/${todayUtc()}/${id}-[0-9a-f-]{36}\\.zip$`))
    expect(entry?.amendKeyHash).toMatch(/^[0-9a-f]{64}$/)
  })

  it('never stores the amend key itself, only its hash', async () => {
    // A KV dump must not be a pile of live credentials.
    const kv = createKv()
    const bindings = createBindings({ ERROR_REPORT_META: kv })

    const { id, amendKey } = await upload(bindings)

    const raw = (await kv.get(reportIndexKey(id))) ?? ''
    expect(amendKey).not.toBeNull()
    expect(raw).not.toContain(amendKey)
  })

  it('mints a fresh amend key per upload', async () => {
    const bindings = createBindings()

    const first = await upload(bindings)
    const second = await upload(bindings)

    expect(first.amendKey).not.toBe(second.amendKey)
  })

  it('indexes a debug-build report under the dev env', async () => {
    const kv = createKv()
    const bindings = createBindings({ ERROR_REPORT_META: kv })

    const { id } = await upload(bindings, { ...validMeta, buildMode: 'debug' })

    expect((await readIndex(kv, id))?.env).toBe('dev')
  })

  it('still returns 200 with a null amend key when the index write fails', async () => {
    // The bundle is safely stored; only the amend flow is lost, and the client treats the key as
    // optional exactly for this.
    const kv = createKv()
    const failing = {
      get: (key: string) => kv.get(key),
      put: (key: string, value: string) =>
        key.startsWith('report:') ? Promise.reject(new Error('KV is down')) : kv.put(key, value),
      delete: (key: string) => kv.delete(key),
    } as unknown as KVNamespace
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORT_META: failing, ERROR_REPORTS_BUCKET: bucket })

    const { amendKey } = await upload(bindings)

    expect(amendKey).toBeNull()
    expect(bucket._store.size).toBe(1)
  })
})

describe('POST /error-report/:id/amend', () => {
  it('writes a note into a sidecar next to the bundle', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const { id, amendKey } = await upload(bindings)
    const [bundleKey] = [...bucket._store.keys()]

    const res = await amend(bindings, id, { amendKey, note: 'It also happens on an external drive.' })

    expect(res.status).toBe(200)
    const sidecar = readSidecar(bucket, bundleKey)
    expect(sidecar?.id).toBe(id)
    expect(sidecar?.amendments).toHaveLength(1)
    expect(sidecar?.amendments[0]?.note).toBe('It also happens on an external drive.')
    expect(sidecar?.amendments[0]?.email).toBeNull()
    expect(sidecar?.amendments[0]?.amendedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/)
  })

  it('appends a second amendment rather than clobbering the first', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const { id, amendKey } = await upload(bindings)
    const [bundleKey] = [...bucket._store.keys()]

    await amend(bindings, id, { amendKey, note: 'first' })
    await amend(bindings, id, { amendKey, note: 'second' })

    const sidecar = readSidecar(bucket, bundleKey)
    expect(sidecar?.amendments.map((a) => a.note)).toEqual(['first', 'second'])
  })

  it('accepts an email on its own, with no note', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const { id, amendKey } = await upload(bindings)
    const [bundleKey] = [...bucket._store.keys()]

    const res = await amend(bindings, id, { amendKey, email: 'someone@example.com' })

    expect(res.status).toBe(200)
    const sidecar = readSidecar(bucket, bundleKey)
    expect(sidecar?.amendments[0]?.email).toBe('someone@example.com')
    expect(sidecar?.amendments[0]?.note).toBeNull()
  })

  it('rejects a wrong amend key with 401 and writes nothing', async () => {
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const { id } = await upload(bindings)
    const [bundleKey] = [...bucket._store.keys()]

    const res = await amend(bindings, id, { amendKey: 'a'.repeat(43), note: 'let me in' })

    expect(res.status).toBe(401)
    expect(readSidecar(bucket, bundleKey)).toBeNull()
  })

  it('rejects a known id presented as its own credential', async () => {
    // `ERR-XXXXX` is 31^5 values and is shown to the user, so it can never be proof of ownership.
    const bindings = createBindings()
    const { id } = await upload(bindings)

    const res = await amend(bindings, id, { amendKey: id, note: 'let me in' })

    expect(res.status).toBe(401)
  })

  it('returns 404 for an id that was never indexed', async () => {
    const bindings = createBindings()

    const res = await amend(bindings, 'ERR-B2345', { amendKey: 'x'.repeat(43), note: 'hello' })

    expect(res.status).toBe(404)
  })

  it('returns 400 for a malformed id', async () => {
    const bindings = createBindings()

    const res = await amend(bindings, 'ERR-lower', { amendKey: 'x'.repeat(43), note: 'hello' })

    expect(res.status).toBe(400)
  })

  it('returns 400 when neither a note nor an email came along', async () => {
    const bindings = createBindings()
    const { id, amendKey } = await upload(bindings)

    const res = await amend(bindings, id, { amendKey })

    expect(res.status).toBe(400)
  })

  it('returns 400 for an email that is not shaped like an address', async () => {
    const bindings = createBindings()
    const { id, amendKey } = await upload(bindings)

    const res = await amend(bindings, id, { amendKey, email: 'not an address' })

    expect(res.status).toBe(400)
  })

  it('returns 400 when the amend key is missing', async () => {
    const bindings = createBindings()
    const { id } = await upload(bindings)

    const res = await amend(bindings, id, { note: 'hello' })

    expect(res.status).toBe(400)
  })

  it('stops reading a streamed body at the cap, whatever content-length claims', async () => {
    // `content-length` is advisory: absent here, and a hostile client could equally understate it.
    // Buffering the body and measuring afterwards would 413 too, so the status alone proves
    // nothing; what matters is that the server stopped pulling bytes near the cap.
    const bindings = createBindings()
    const total = 4 * 1024 * 1024
    const { body, bytesProduced } = streamingAmendBody(total)

    const res = await app.request(
      '/error-report/ERR-B2345/amend',
      { method: 'POST', body, headers: { 'content-type': 'application/json' }, duplex: 'half' } as RequestInit,
      bindings,
    )

    expect(res.status).toBe(413)
    expect(bytesProduced()).toBeLessThan(total / 2)
  })

  it('accepts a streamed body that stays under the cap', async () => {
    // The cap must not turn away an ordinary amendment that simply arrives without a
    // `content-length`.
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const { id, amendKey } = await upload(bindings)
    const [bundleKey] = [...bucket._store.keys()]
    const encoder = new TextEncoder()
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode(JSON.stringify({ amendKey, note: 'streamed in' })))
        controller.close()
      },
    })

    const res = await app.request(
      `/error-report/${id}/amend`,
      { method: 'POST', body, headers: { 'content-type': 'application/json' }, duplex: 'half' } as RequestInit,
      bindings,
    )

    expect(res.status).toBe(200)
    expect(readSidecar(bucket, bundleKey)?.amendments[0]?.note).toBe('streamed in')
  })

  it('returns 429 when the amend limiter turns the caller away', async () => {
    const limitMock = vi.fn(() => Promise.resolve({ success: false }))
    const bindings = createBindings({ ERROR_REPORT_AMEND_LIMITER: { limit: limitMock } })

    const res = await amend(bindings, 'ERR-B2345', { amendKey: 'x'.repeat(43), note: 'hello' })

    expect(res.status).toBe(429)
  })

  it('leaves the daily byte budget alone', async () => {
    // A note is bytes-tiny; charging it against the bundle budget would be nonsense.
    const kv = createKv()
    const bindings = createBindings({ ERROR_REPORT_META: kv })
    const { id, amendKey } = await upload(bindings)
    const before = await kv.get(dailyBytesKey(todayUtc()))

    await amend(bindings, id, { amendKey, note: 'a note' })

    expect(await kv.get(dailyBytesKey(todayUtc()))).toBe(before)
  })
})
