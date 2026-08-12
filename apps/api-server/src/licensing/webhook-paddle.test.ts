import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { app } from '../index'
import { issuanceStaleAfterMs } from './license-issuance'

// Mock Resend so no email leaves the test. The result shape matters here: the SDK reports a
// rejected send in the response rather than throwing, and the route has to notice.
interface SendResult {
  data: { id: string } | null
  error: { message: string } | null
}
const emailAccepted: SendResult = { data: { id: 'test-email-id' }, error: null }
const mockSend = vi.fn<(payload: { to: string; subject: string; html: string }) => Promise<SendResult>>(() =>
  Promise.resolve(emailAccepted),
)
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

const webhookSecret = 'test-webhook-secret'
const transactionId = 'txn_01hv8x'
const customerId = 'ctm_01hv8x'

/** In-memory KV holding license short codes, so a test can count what was minted. */
function createKv(): KVNamespace & { store: Map<string, string> } {
  const store = new Map<string, string>()
  const kv = {
    store,
    get: (key: string, type?: string) => {
      const raw = store.get(key)
      if (raw === undefined) return Promise.resolve(null)
      return Promise.resolve(type === 'json' ? (JSON.parse(raw) as unknown) : raw)
    },
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  }
  return kv as unknown as KVNamespace & { store: Map<string, string> }
}

interface IssuanceRow {
  transaction_id: string
  event_id: string | null
  short_codes: string
  quantity: number | null
  license_type: string | null
  customer_email: string | null
  claimed_at: string
  issued_at: string | null
  emailed_at: string | null
}

/**
 * In-memory stand-in for the `license_issuance` table. It models the four statements the
 * issuance module runs, including the conditional claim and take-over, so the tests exercise
 * the real concurrency rules rather than a mock that always says yes.
 */
function createIssuanceD1(): D1Database & { rows: Map<string, IssuanceRow> } {
  const rows = new Map<string, IssuanceRow>()

  function claim(args: unknown[]): { transaction_id: string } | null {
    const [id, eventId, claimedAt] = args as [string, string | null, string]
    if (rows.has(id)) return null
    rows.set(id, {
      transaction_id: id,
      event_id: eventId,
      short_codes: '[]',
      quantity: null,
      license_type: null,
      customer_email: null,
      claimed_at: claimedAt,
      issued_at: null,
      emailed_at: null,
    })
    return { transaction_id: id }
  }

  function takeOver(args: unknown[]): { transaction_id: string } | null {
    const [claimedAt, id, previousClaimedAt] = args as [string, string, string]
    const row = rows.get(id)
    if (!row || row.emailed_at !== null || row.claimed_at !== previousClaimedAt) return null
    row.claimed_at = claimedAt
    return { transaction_id: id }
  }

  function recordCodes(args: unknown[]): void {
    const [shortCodes, quantity, licenseType, customerEmail, issuedAt, id] = args as [
      string,
      number,
      string,
      string | null,
      string,
      string,
    ]
    const row = rows.get(id)
    if (!row) return
    Object.assign(row, {
      short_codes: shortCodes,
      quantity,
      license_type: licenseType,
      customer_email: customerEmail,
      issued_at: issuedAt,
    })
  }

  const db = {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: () => {
          if (sql.includes('INSERT INTO license_issuance')) return Promise.resolve(claim(args))
          if (sql.includes('SET claimed_at')) return Promise.resolve(takeOver(args))
          if (sql.includes('FROM license_issuance')) return Promise.resolve(rows.get(args[0] as string) ?? null)
          throw new Error(`Unexpected statement in first(): ${sql}`)
        },
        run: () => {
          if (sql.includes('SET short_codes')) recordCodes(args)
          else if (sql.includes('SET emailed_at')) {
            const row = rows.get(args[1] as string)
            if (row) row.emailed_at = args[0] as string
          } else throw new Error(`Unexpected statement in run(): ${sql}`)
          return Promise.resolve({ success: true })
        },
      }),
    }),
    rows,
  }
  return db as unknown as D1Database & { rows: Map<string, IssuanceRow> }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: createKv(),
    TELEMETRY_DB: createIssuanceD1(),
    DEVICE_COUNTS: { writeDataPoint: vi.fn() },
    ERROR_REPORTS_BUCKET: {} as R2Bucket,
    ERROR_REPORT_META: {} as KVNamespace,
    ED25519_PRIVATE_KEY: 'ab'.repeat(32),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'david@getcmdr.com',
    PADDLE_ENVIRONMENT: 'live',
    PADDLE_API_KEY_LIVE: 'test-paddle-key',
    PADDLE_WEBHOOK_SECRET_LIVE: webhookSecret,
    ...overrides,
  }
}

type Bindings = ReturnType<typeof createBindings>

function webhookBody(quantity = 1): string {
  return JSON.stringify({
    event_id: 'evt_01hv8x',
    event_type: 'transaction.completed',
    data: {
      id: transactionId,
      customer_id: customerId,
      items: [{ price: { id: 'pri_perpetual' }, quantity }],
    },
  })
}

async function sign(body: string, timestamp = '1704700000'): Promise<string> {
  const encoder = new TextEncoder()
  const key = await crypto.subtle.importKey(
    'raw',
    encoder.encode(webhookSecret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const bytes = await crypto.subtle.sign('HMAC', key, encoder.encode(`${timestamp}:${body}`))
  const signature = Array.from(new Uint8Array(bytes))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
  return `ts=${timestamp};h1=${signature}`
}

async function deliver(bindings: Bindings, quantity = 1): Promise<Response> {
  const body = webhookBody(quantity)
  return app.request(
    '/webhook/paddle',
    { method: 'POST', headers: { 'Paddle-Signature': await sign(body) }, body },
    bindings,
  )
}

/** Short codes currently in KV (the license entries, not the bookkeeping keys). */
function mintedCodes(bindings: Bindings): string[] {
  return [...bindings.LICENSE_CODES.store.keys()].filter((key) => key.startsWith('CMDR-'))
}

function emailedKeys(): string[] {
  const html = mockSend.mock.lastCall?.[0].html ?? ''
  return [...html.matchAll(/CMDR-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}/g)].map(
    (match) => match[0],
  )
}

beforeEach(() => {
  mockSend.mockClear()
  mockSend.mockImplementation(() => Promise.resolve(emailAccepted))
  globalThis.fetch = vi.fn(() =>
    Promise.resolve(Response.json({ data: { email: 'buyer@example.com', name: 'Robin', business: { name: 'Acme' } } })),
  )
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.useRealTimers()
})

describe('POST /webhook/paddle idempotency', () => {
  it('issues one code per seat and emails them on the first delivery', async () => {
    const bindings = createBindings()

    const res = await deliver(bindings, 3)

    expect(res.status).toBe(200)
    expect(mintedCodes(bindings)).toHaveLength(3)
    expect(mockSend).toHaveBeenCalledTimes(1)
    expect(emailedKeys()).toEqual(mintedCodes(bindings))
    const row = bindings.TELEMETRY_DB.rows.get(transactionId)
    expect(row?.emailed_at).toBeTruthy()
    expect(JSON.parse(row?.short_codes ?? '[]')).toEqual(mintedCodes(bindings))
  })

  it('mints nothing and sends nothing on a duplicate delivery of a fulfilled transaction', async () => {
    const bindings = createBindings()
    await deliver(bindings)
    const codesAfterFirst = mintedCodes(bindings)
    mockSend.mockClear()

    const res = await deliver(bindings)

    expect(res.status).toBe(200)
    expect(await res.json()).toMatchObject({ status: 'already_processed' })
    expect(mintedCodes(bindings)).toEqual(codesAfterFirst)
    expect(mockSend).not.toHaveBeenCalled()
  })

  it('re-sends the same codes when a retry follows a failed email, instead of minting new ones', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-08-12T10:00:00Z'))
    const bindings = createBindings()
    mockSend.mockImplementation(() => Promise.reject(new Error('Resend is down')))

    const failed = await deliver(bindings)
    expect(failed.status).toBe(500)
    const codesAfterFailure = mintedCodes(bindings)
    expect(codesAfterFailure).toHaveLength(1)
    expect(bindings.TELEMETRY_DB.rows.get(transactionId)?.emailed_at).toBeNull()

    mockSend.mockImplementation(() => Promise.resolve(emailAccepted))
    vi.setSystemTime(new Date(Date.now() + issuanceStaleAfterMs + 1000))

    const retried = await deliver(bindings)

    expect(retried.status).toBe(200)
    expect(mintedCodes(bindings)).toEqual(codesAfterFailure)
    expect(emailedKeys()).toEqual(codesAfterFailure)
    expect(bindings.TELEMETRY_DB.rows.get(transactionId)?.emailed_at).toBeTruthy()
  })

  it('leaves the purchase unfulfilled when Resend rejects the email', async () => {
    const bindings = createBindings()
    // Resend reports a rejected send in the response, it doesn't throw. Treating that as success
    // would mark the purchase delivered and stop Paddle retrying, leaving the buyer with nothing.
    mockSend.mockImplementation(() => Promise.resolve({ data: null, error: { message: 'rate_limit_exceeded' } }))

    const res = await deliver(bindings)

    expect(res.status).toBe(500)
    expect(bindings.TELEMETRY_DB.rows.get(transactionId)?.emailed_at).toBeNull()
  })

  it('asks a concurrent delivery to retry rather than issuing a second set of codes', async () => {
    const bindings = createBindings()
    // Hold the first delivery inside the email step, the widest window a redelivery can land in.
    let releaseEmail: () => void = () => undefined
    mockSend.mockImplementation(
      () =>
        new Promise((resolve) => {
          releaseEmail = () => {
            resolve(emailAccepted)
          }
        }),
    )

    const first = deliver(bindings)
    await vi.waitFor(() => {
      expect(mockSend).toHaveBeenCalledTimes(1)
    })

    const concurrent = await deliver(bindings)

    expect(concurrent.status).toBe(503)
    expect(mintedCodes(bindings)).toHaveLength(1)
    expect(mockSend).toHaveBeenCalledTimes(1)

    releaseEmail()
    expect((await first).status).toBe(200)
  })

  it('ignores events that are not transaction.completed', async () => {
    const bindings = createBindings()
    const body = JSON.stringify({ event_type: 'transaction.updated', data: { id: transactionId } })

    const res = await app.request(
      '/webhook/paddle',
      { method: 'POST', headers: { 'Paddle-Signature': await sign(body) }, body },
      bindings,
    )

    expect(res.status).toBe(200)
    expect(await res.json()).toMatchObject({ status: 'ignored' })
    expect(bindings.TELEMETRY_DB.rows.size).toBe(0)
    expect(mockSend).not.toHaveBeenCalled()
  })

  it('rejects an unsigned delivery before touching any state', async () => {
    const bindings = createBindings()
    const body = webhookBody()

    const res = await app.request(
      '/webhook/paddle',
      { method: 'POST', headers: { 'Paddle-Signature': 'ts=1704700000;h1=deadbeef' }, body },
      bindings,
    )

    expect(res.status).toBe(401)
    expect(bindings.TELEMETRY_DB.rows.size).toBe(0)
    expect(mintedCodes(bindings)).toHaveLength(0)
  })
})
