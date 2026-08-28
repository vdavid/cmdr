import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { app } from '../index'
import { DAILY_INTAKE_BUDGET_BYTES, DAILY_NOTIFICATION_CAP, dailyBytesKey, pauseIntake } from './error-report-intake'
import {
  buildMultipart,
  createBindings,
  createKv,
  createMockRateLimiter,
  createR2,
  todayUtc,
  validMeta,
} from './error-report-test-helpers'

const MULTIPART_BOUNDARY = '----CmdrTestBoundary7hK2'

/** The framing around a `bundle` part of `bundleBytes`, plus the `meta` part. */
function multipartFraming(): { head: Uint8Array; tail: Uint8Array } {
  const encoder = new TextEncoder()
  return {
    head: encoder.encode(
      `--${MULTIPART_BOUNDARY}\r\n` +
        'Content-Disposition: form-data; name="bundle"; filename="bundle.zip"\r\n' +
        'Content-Type: application/zip\r\n\r\n',
    ),
    tail: encoder.encode(
      `\r\n--${MULTIPART_BOUNDARY}\r\n` +
        'Content-Disposition: form-data; name="meta"\r\n\r\n' +
        `${JSON.stringify(validMeta)}\r\n` +
        `--${MULTIPART_BOUNDARY}--\r\n`,
    ),
  }
}

/**
 * Hand-rolled multipart as one buffer, so the request carries a real `content-length` and undici
 * has no async body pump to leave dangling when the route rejects without reading.
 */
function multipartBytes(bundleBytes: number): { body: ArrayBuffer; contentType: string } {
  const { head, tail } = multipartFraming()
  const buffer = new ArrayBuffer(head.byteLength + bundleBytes + tail.byteLength)
  const body = new Uint8Array(buffer)
  body.set(head, 0)
  body.set(tail, head.byteLength + bundleBytes)
  return { body: buffer, contentType: `multipart/form-data; boundary=${MULTIPART_BOUNDARY}` }
}

/**
 * Hand-rolled multipart delivered as a stream, so the request carries no `content-length` and the
 * bundle bytes are produced lazily. `FormData` can't express this: it always yields a known-length
 * body. `bundleBytes` is emitted in 1 MB chunks, so a 30 MB case never materializes as one buffer.
 */
function streamingMultipart(bundleBytes: number): {
  body: ReadableStream<Uint8Array>
  contentType: string
  /** Bytes the server actually pulled off the stream. The point of the cap is that this stays small. */
  bytesProduced: () => number
} {
  const { head, tail } = multipartFraming()

  const chunkSize = 1024 * 1024
  let sent = 0
  let produced = 0
  // The route cancels the read as soon as the cap trips, so `pull` can still be in flight against
  // a closed controller. Enqueuing then would throw an unhandled rejection outliving the test.
  let cancelled = false
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      produced += head.byteLength
      controller.enqueue(head)
    },
    pull(controller) {
      if (cancelled) return
      if (sent >= bundleBytes) {
        produced += tail.byteLength
        controller.enqueue(tail)
        controller.close()
        return
      }
      const size = Math.min(chunkSize, bundleBytes - sent)
      controller.enqueue(new Uint8Array(size))
      sent += size
      produced += size
    },
    cancel() {
      cancelled = true
    },
  })

  return { body, contentType: `multipart/form-data; boundary=${MULTIPART_BOUNDARY}`, bytesProduced: () => produced }
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
    // A pre-serialized body, not `FormData`: the route rejects this on the declared length without
    // reading it, and undici's FormData pump throws an unhandled rejection when its stream is left
    // undrained. (workerd discards an undrained body without complaint, so this is a test-host
    // artifact, but an unhandled rejection is never worth tolerating in the suite.)
    const { body, contentType } = multipartBytes(12 * 1024 * 1024)

    const res = await app.request(
      '/error-report',
      { method: 'POST', body, headers: { 'content-type': contentType } },
      bindings,
    )

    expect(res.status).toBe(413)
    const parsed = await res.json<{ error: string }>()
    expect(parsed.error).toContain('too large')
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

  it('accepts the reply-to email the send dialog attaches', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1]), { ...validMeta, email: 'someone@example.com' })

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(200)
  })

  it('accepts a report whose email the Rust client sends as null', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1]), { ...validMeta, email: null })

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(200)
  })

  it('returns 400 for an email that is not shaped like an address', async () => {
    const bindings = createBindings()
    const fd = buildMultipart(new Uint8Array([1]), { ...validMeta, email: 'not an address' })

    const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid meta shape')
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

  it('rejects an oversized body that never declares a content-length', async () => {
    // The pre-parse `content-length` check is advisory: a chunked upload declares no length at
    // all, and a lying one declares whatever it likes. Without a cap on the bytes actually read,
    // the multipart parser would buffer up to Cloudflare's 100 MB request limit inside a 128 MB
    // isolate.
    const bucket = createR2()
    const bindings = createBindings({ ERROR_REPORTS_BUCKET: bucket })
    const total = 30 * 1024 * 1024
    const { body, contentType, bytesProduced } = streamingMultipart(total)

    const res = await app.request(
      '/error-report',
      { method: 'POST', body, headers: { 'content-type': contentType }, duplex: 'half' } as RequestInit,
      bindings,
    )

    expect(res.status).toBe(413)
    expect(bucket._store.size).toBe(0)
    // The status alone proves nothing: buffering all 30 MB and then measuring would also 413.
    // What matters is that the server stopped reading near the cap instead of taking the body.
    expect(bytesProduced()).toBeLessThan(total / 2)
  })

  it('accepts a streamed body that stays under the cap', async () => {
    // Same streaming shape, legitimate size: the cap must not reject uploads that simply arrive
    // without a content-length.
    const bindings = createBindings()
    const { body, contentType } = streamingMultipart(64 * 1024)

    const res = await app.request(
      '/error-report',
      { method: 'POST', body, headers: { 'content-type': contentType }, duplex: 'half' } as RequestInit,
      bindings,
    )

    expect(res.status).toBe(200)
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

  it('stops pinging Discord per upload once the daily notification cap is passed', async () => {
    const kv = createKv()
    // Start one slot below the cap so the run is short: one more ping, then the single suppression
    // notice, then silence however many uploads follow.
    await kv.put(`notify_count:${todayUtc()}`, String(DAILY_NOTIFICATION_CAP - 1))
    const posts: string[] = []
    globalThis.fetch = ((_url: string, init: { body: string }) => {
      posts.push(init.body)
      return Promise.resolve(new Response(null, { status: 204 }))
    }) as unknown as typeof fetch
    const bindings = createBindings({
      ERROR_REPORT_META: kv,
      DISCORD_WEBHOOK_URL: 'https://discord.example/webhook',
    })

    for (let i = 0; i < 4; i++) {
      const fd = buildMultipart(new Uint8Array([1]), validMeta)
      const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
      expect(res.status).toBe(200)
    }

    // Upload 1 pings, upload 2 posts the suppression notice, uploads 3 and 4 post nothing.
    expect(posts).toHaveLength(2)
    expect(posts[0]).toContain('ERR-A2345')
    expect(posts[1]).toContain('suppressed')
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
