import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { createTestHarness } from 'wrangler'
import * as ed from '@noble/ed25519'

/**
 * The sale notice in the REAL Worker, beside the purchase it celebrates: wrangler builds
 * `src/index.ts` and runs it in workerd under the production `wrangler.toml`, with Paddle and Resend
 * stubbed at the socket, the same way `webhook-runtime.test.ts` does (read its header for the
 * mechanics). Its own harness because this one configures a notification recipient, which is what
 * turns the notice on; the purchase suite runs without one so its email counts stay exact.
 *
 * What it pins: the notice never costs the buyer their key or Paddle its 200, one sale is one email
 * however often Paddle redelivers, and the backfill route fills in exactly what's missing.
 */

const webhookSecret = 'test-webhook-secret'
const adminToken = 'test-admin-token'
const perpetualPriceId = 'pri_perpetual'
const david = 'david@example.com'
const privateKey = ed.utils.randomSecretKey()
const privateKeyHex = Array.from(privateKey, (byte) => byte.toString(16).padStart(2, '0')).join('')

const server = createTestHarness({
  workers: [
    {
      configPath: new URL('../../wrangler.toml', import.meta.url),
      secrets: {
        ED25519_PRIVATE_KEY: privateKeyHex,
        PADDLE_WEBHOOK_SECRET_LIVE: webhookSecret,
        PADDLE_API_KEY_LIVE: 'test-paddle-key',
        ADMIN_API_TOKEN: adminToken,
        PRICE_ID_COMMERCIAL_PERPETUAL: perpetualPriceId,
        RESEND_API_KEY: 'test-resend-key',
        CRASH_NOTIFICATION_EMAIL: david,
      },
    },
  ],
})

interface HarnessEnv {
  TELEMETRY_DB: D1Database
}

interface SentEmail {
  to: string
  subject: string
  html: string
  text: string
}

const buyer = { email: 'buyer@example.com', name: 'Robin' }
/** Per-transaction overrides of what Paddle answers: a $0 purchase, an unknown id. */
const paddleTransactions = new Map<string, { grandTotal?: string; missing?: boolean }>()
const resend = {
  sent: [] as SentEmail[],
  /** Recipients Resend refuses, the way a rate limit answers: an error in the response. */
  rejectTo: new Set<string>(),
}
const unexpectedOutbound: string[] = []
const realFetch = globalThis.fetch

beforeAll(async () => {
  globalThis.fetch = stubbedFetch
  await server.listen()
  await server.getWorker<HarnessEnv>().applyD1Migrations('TELEMETRY_DB')
}, 60_000)

afterAll(async () => {
  globalThis.fetch = realFetch
  await server.close()
})

beforeEach(() => {
  resend.sent.length = 0
  resend.rejectTo.clear()
})

afterEach(() => {
  expect(unexpectedOutbound).toEqual([])
})

async function stubbedFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
  if (url.startsWith('https://api.paddle.com/customers/')) {
    return Response.json({ data: { ...buyer, business: null } })
  }
  if (url.startsWith('https://api.paddle.com/transactions/')) {
    const id = decodeURIComponent(new URL(url).pathname.slice('/transactions/'.length))
    const override = paddleTransactions.get(id)
    if (override?.missing) return Response.json({ error: { code: 'not_found' } }, { status: 404 })
    return Response.json({ data: paddleTransaction(id, override?.grandTotal ?? '5900') })
  }
  if (url === 'https://api.resend.com/emails') {
    const email = JSON.parse(await requestBody(input, init)) as SentEmail
    resend.sent.push(email)
    if (resend.rejectTo.has(email.to)) {
      return Response.json({ name: 'rate_limit_exceeded', message: 'Too many requests' }, { status: 429 })
    }
    return Response.json({ id: `email_${String(resend.sent.length)}` })
  }
  unexpectedOutbound.push(url)
  return Response.json({ error: 'This host is not stubbed' }, { status: 502 })
}

/** A completed transaction the way Paddle answers `?include=customer,address,business`. */
function paddleTransaction(id: string, grandTotal: string): unknown {
  return {
    id,
    status: 'completed',
    origin: 'web',
    customer_id: 'ctm_01hv8x',
    custom_data: null,
    currency_code: 'USD',
    billed_at: '2026-10-21T13:12:03.620649Z',
    items: [{ price: { id: perpetualPriceId, name: 'Commercial license' }, quantity: 1 }],
    details: {
      totals: { grand_total: grandTotal, earnings: grandTotal === '0' ? '0' : '5555', currency_code: 'USD' },
      line_items: [{ product: { name: 'Cmdr' } }],
    },
    customer: buyer,
    address: { country_code: 'DE' },
    business: null,
  }
}

/** See `webhook-runtime.test.ts::requestBody` for why this duck-types. */
async function requestBody(input: RequestInfo | URL, init?: RequestInit): Promise<string> {
  for (const candidate of [init, input]) {
    const body = candidate as { text?: () => Promise<string> } | undefined
    if (typeof body?.text === 'function') return await body.text()
  }
  return typeof init?.body === 'string' ? init.body : ''
}

async function sign(body: string): Promise<string> {
  const timestamp = String(Math.floor(Date.now() / 1000))
  const encoder = new TextEncoder()
  const key = await crypto.subtle.importKey(
    'raw',
    encoder.encode(webhookSecret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const bytes = await crypto.subtle.sign('HMAC', key, encoder.encode(`${timestamp}:${body}`))
  const signature = Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `ts=${timestamp};h1=${signature}`
}

async function deliver(transactionId: string): Promise<{ status: number; text: string }> {
  const body = JSON.stringify({
    event_id: `evt_${transactionId}`,
    event_type: 'transaction.completed',
    data: { id: transactionId, customer_id: 'ctm_01hv8x', items: [{ price: { id: perpetualPriceId }, quantity: 1 }] },
  })
  const response = await server.fetch('http://api.getcmdr.com/webhook/paddle', {
    method: 'POST',
    headers: { 'Paddle-Signature': await sign(body), 'Content-Type': 'application/json' },
    body,
  })
  return { status: response.status, text: await response.text() }
}

async function backfill(
  transactionIds: string[],
  token = adminToken,
): Promise<{ status: number; body: { results?: Record<string, string> } }> {
  const response = await server.fetch('http://api.getcmdr.com/admin/sale-notices', {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ transactionIds }),
  })
  return { status: response.status, body: JSON.parse(await response.text()) as { results?: Record<string, string> } }
}

function workerLogs(): string {
  return server
    .getLogs()
    .map((log) => JSON.stringify(log))
    .join('\n')
}

function sentTo(to: string): SentEmail[] {
  return resend.sent.filter((email) => email.to === to)
}

async function db(): Promise<D1Database> {
  return (await server.getWorker<HarnessEnv>().getEnv()).TELEMETRY_DB
}

async function saleNotifiedAt(transactionId: string): Promise<string | null> {
  const row = await (
    await db()
  )
    .prepare(`SELECT sale_notified_at FROM license_issuance WHERE transaction_id = ?`)
    .bind(transactionId)
    .first<{ sale_notified_at: string | null }>()
  return row?.sale_notified_at ?? null
}

async function paddleRowCount(): Promise<number> {
  const row = await (
    await db()
  )
    .prepare(`SELECT COUNT(*) AS n FROM license_issuance WHERE source = 'paddle'`)
    .first<{ n: number }>()
  return row?.n ?? 0
}

/** Let the Worker's background work (the notice runs after the response) finish. */
async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 300))
}

describe('the sale notice in the Worker runtime', () => {
  it('mails David once per purchase, beside the buyer’s license', async () => {
    const transactionId = 'txn_sale_first'

    const delivery = await deliver(transactionId)

    expect(delivery.status, `Body: ${delivery.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    expect(sentTo(buyer.email)).toHaveLength(1)
    await expect.poll(() => sentTo(david).length).toBe(1)
    const notice = sentTo(david)[0]
    expect(notice.subject).toBe('A new Cmdr sale: $59 from Germany')
    expect(notice.text).toContain(`That's sale #${String(await paddleRowCount())}!`)
    expect(notice.text).toContain('buyer@example.com')
    expect(notice.html).toContain('https://vendors.paddle.com/transactions-v2/txn_sale_first')
    expect(await saleNotifiedAt(transactionId)).toBeTruthy()

    resend.sent.length = 0
    const redelivery = await deliver(transactionId)
    await settle()

    expect(redelivery.status).toBe(200)
    expect(sentTo(david)).toHaveLength(0)
  })

  it('keeps the purchase fulfilled when the notice fails, and lets the backfill send it later', async () => {
    const transactionId = 'txn_sale_notice_rejected'
    resend.rejectTo.add(david)

    const delivery = await deliver(transactionId)

    expect(delivery.status, `Body: ${delivery.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    expect(sentTo(buyer.email)).toHaveLength(1)
    await expect.poll(() => sentTo(david).length).toBe(1)
    await settle()
    expect(await saleNotifiedAt(transactionId)).toBeNull()
    const event = await (
      await db()
    )
      .prepare(`SELECT message FROM ops_events WHERE kind = 'sale-notice-failed'`)
      .first<{ message: string }>()
    expect(event?.message).toContain(transactionId)

    resend.rejectTo.clear()
    resend.sent.length = 0
    const first = await backfill([transactionId])

    expect(first.status, workerLogs()).toBe(200)
    expect(first.body.results).toEqual({ [transactionId]: 'sent' })
    expect(sentTo(david)).toHaveLength(1)
    expect(sentTo(buyer.email)).toHaveLength(0)

    const again = await backfill([transactionId])
    expect(again.body.results).toEqual({ [transactionId]: 'already_sent' })
    expect(sentTo(david)).toHaveLength(1)
  })

  it('stays quiet for a $0 purchase, which is a license handed out rather than a sale', async () => {
    const transactionId = 'txn_sale_free'
    paddleTransactions.set(transactionId, { grandTotal: '0' })

    const delivery = await deliver(transactionId)
    await settle()

    expect(delivery.status).toBe(200)
    expect(sentTo(buyer.email)).toHaveLength(1)
    expect(sentTo(david)).toHaveLength(0)
    expect((await backfill([transactionId])).body.results).toEqual({ [transactionId]: 'free' })
  })

  it('answers each backfilled id, and touches nothing it has no purchase for', async () => {
    paddleTransactions.set('txn_sale_unknown_to_paddle', { missing: true })

    const result = await backfill(['txn_sale_not_in_ledger', 'txn_sale_unknown_to_paddle'])

    expect(result.status).toBe(200)
    expect(result.body.results).toEqual({
      txn_sale_not_in_ledger: 'not_found',
      txn_sale_unknown_to_paddle: 'not_found',
    })
    expect(resend.sent).toHaveLength(0)
  })

  it('refuses a backfill without the admin token, or with ids that are not Paddle transactions', async () => {
    expect((await backfill(['txn_sale_first'], 'wrong-token')).status).toBe(401)
    expect((await backfill(['manual-ABCDEFGHJKMN'])).status).toBe(400)
    expect((await backfill([])).status).toBe(400)
  })
})
