import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { createTestHarness } from 'wrangler'
import * as ed from '@noble/ed25519'
import { isValidShortCode, validationAnswerSignaturePrefix, type LicenseData } from './license'
import type { LicenseListing } from './admin-licenses'
import type { ValidationResponse } from './paddle-api'
import { issuanceStaleAfterMs } from './license-issuance'

/**
 * The purchase path in the REAL Worker: wrangler builds `src/index.ts` and runs it in workerd under
 * the production `wrangler.toml`, so a Node global anywhere between the webhook and the license
 * email throws here exactly as it throws for a buyer. `production-runtime.test.ts` does the same for
 * the hand-issued routes; both exist because no other lane can prove that (`../../DETAILS.md` §
 * Test runtimes).
 *
 * ❌ Never give this harness its own compatibility settings, and never reach for `nodejs_compat`.
 *
 * **Paddle and Resend are stubbed at the socket, not in the source.** The harness points the
 * Worker's global `fetch` at this process, so replacing `globalThis.fetch` here intercepts every
 * outbound request the Worker makes, with the route, the SDK, and the runtime all untouched. A
 * request to any other host fails loudly rather than leaving the machine.
 */

const webhookSecret = 'test-webhook-secret'
const adminToken = 'test-admin-token'
const perpetualPriceId = 'pri_perpetual'
const privateKey = ed.utils.randomSecretKey()
const privateKeyHex = Array.from(privateKey, (byte) => byte.toString(16).padStart(2, '0')).join('')

const server = createTestHarness({
  workers: [
    {
      configPath: new URL('../../wrangler.toml', import.meta.url),
      // Test-only, and deliberately not the production signer: this key only has to verify here.
      secrets: {
        ED25519_PRIVATE_KEY: privateKeyHex,
        PADDLE_WEBHOOK_SECRET_LIVE: webhookSecret,
        PADDLE_API_KEY_LIVE: 'test-paddle-key',
        ADMIN_API_TOKEN: adminToken,
        PRICE_ID_COMMERCIAL_PERPETUAL: perpetualPriceId,
        RESEND_API_KEY: 'test-resend-key',
      },
    },
  ],
})

interface HarnessEnv {
  LICENSE_CODES: KVNamespace
  TELEMETRY_DB: D1Database
}

/** The license email as it left the Worker, read off the wire rather than off a mocked SDK. */
interface SentEmail {
  to: string
  subject: string
  html: string
  text: string
}

/** What Paddle and Resend answer, per test. */
const paddleCustomer = { email: 'buyer@example.com', name: 'Robin', business: { name: 'Acme Inc' } }
const resend = {
  sent: [] as SentEmail[],
  /** `reject` makes Resend answer the way a rate limit does: an error in the response, not a throw. */
  mode: 'accept' as 'accept' | 'reject',
  /** Set to hold a send open, which is the widest window a redelivery can land in. */
  held: null as { promise: Promise<void>; release: () => void } | null,
}
/** Any host we didn't stub. Asserted empty after every test: the Worker talks to two services. */
const unexpectedOutbound: string[] = []

const realFetch = globalThis.fetch

beforeAll(async () => {
  globalThis.fetch = stubbedFetch
  await server.listen()
  // The harness runs the real migration files, so a column these tests rely on can't exist only here.
  await server.getWorker<HarnessEnv>().applyD1Migrations('TELEMETRY_DB')
}, 60_000)

afterAll(async () => {
  globalThis.fetch = realFetch
  await server.close()
})

beforeEach(() => {
  resend.sent.length = 0
  resend.mode = 'accept'
  resend.held = null
})

afterEach(() => {
  expect(unexpectedOutbound).toEqual([])
})

async function stubbedFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
  if (url.startsWith('https://api.paddle.com/customers/')) {
    return Response.json({ data: paddleCustomer })
  }
  // Every transaction is a completed one-time purchase, which Paddle keeps answering for after a
  // refund: the refund is an adjustment beside the transaction, never a change to it.
  if (url.startsWith('https://api.paddle.com/transactions/')) {
    const id = url.slice('https://api.paddle.com/transactions/'.length)
    return Response.json({ data: { id, status: 'completed', customer_id: 'ctm_01hv8x', custom_data: null } })
  }
  if (url === 'https://api.resend.com/emails') {
    return await answerResend(await requestBody(input, init))
  }
  unexpectedOutbound.push(url)
  return Response.json({ error: 'This host is not stubbed' }, { status: 502 })
}

async function answerResend(body: string): Promise<Response> {
  resend.sent.push(JSON.parse(body) as SentEmail)
  if (resend.held) await resend.held.promise
  if (resend.mode === 'reject') {
    return Response.json({ name: 'rate_limit_exceeded', message: 'Too many requests' }, { status: 429 })
  }
  return Response.json({ id: `email_${String(resend.sent.length)}` })
}

/**
 * The body of an outbound request. The harness hands it over as `fetch(url, request)`, so the body
 * rides on the init rather than the input.
 *
 * ❌ No `instanceof Request` here: the init is undici's internal `Request` class, which is NOT the
 * `Request` this module's global resolves to, so the check silently reads false and the body comes
 * back empty. Duck-typing on `text()` is what survives that.
 */
async function requestBody(input: RequestInfo | URL, init?: RequestInit): Promise<string> {
  for (const candidate of [init, input]) {
    const body = candidate as { text?: () => Promise<string> } | undefined
    if (typeof body?.text === 'function') return await body.text()
  }
  return typeof init?.body === 'string' ? init.body : ''
}

/** Hold the license email open until the returned release is called. Always release in a `finally`. */
function holdEmail(): { release: () => void } {
  let release: () => void = () => undefined
  const promise = new Promise<void>((resolve) => {
    release = resolve
  })
  resend.held = { promise, release }
  return {
    release: () => {
      resend.held = null
      release()
    },
  }
}

async function sign(body: string, timestamp = String(Math.floor(Date.now() / 1000))): Promise<string> {
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
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
  return `ts=${timestamp};h1=${signature}`
}

interface Delivery {
  status: number
  text: string
  body: { status?: string; email?: string; licenseType?: string; quantity?: number }
}

/** Deliver a signed `transaction.completed`, the way Paddle does on a completed purchase. */
async function deliver(params: { transactionId: string; quantity?: number }): Promise<Delivery> {
  const body = JSON.stringify({
    event_id: `evt_${params.transactionId}`,
    event_type: 'transaction.completed',
    data: {
      id: params.transactionId,
      customer_id: 'ctm_01hv8x',
      items: [{ price: { id: perpetualPriceId }, quantity: params.quantity ?? 1 }],
    },
  })
  const response = await server.fetch('http://api.getcmdr.com/webhook/paddle', {
    method: 'POST',
    headers: { 'Paddle-Signature': await sign(body), 'Content-Type': 'application/json' },
    body,
  })
  const text = await response.text()
  return { status: response.status, text, body: parseDeliveryBody(text) }
}

/**
 * Parse, tolerating a non-JSON body. A route that throws inside workerd answers with bare 500 text,
 * and a parse error thrown here would bury the status assertion that explains what happened.
 */
function parseDeliveryBody(text: string): Delivery['body'] {
  try {
    return JSON.parse(text) as Delivery['body']
  } catch {
    return {}
  }
}

/** The runtime's own logs, for a failure message: a throw inside workerd leaves nothing in the body. */
function workerLogs(): string {
  return server
    .getLogs()
    .map((log) => JSON.stringify(log))
    .join('\n')
}

/** Every license short code in KV. Bookkeeping keys (`devices:`, `_meta:`) are not codes. */
async function storedCodes(): Promise<string[]> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  const listed = await env.LICENSE_CODES.list()
  return listed.keys
    .map((key) => key.name)
    .filter((name) => isValidShortCode(name))
    .sort()
}

interface LedgerRow {
  source: string
  short_codes: string
  quantity: number | null
  license_type: string | null
  customer_email: string | null
  claimed_at: string
  issued_at: string | null
  emailed_at: string | null
}

async function readLedgerRow(transactionId: string): Promise<LedgerRow | null> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  return await env.TELEMETRY_DB.prepare(
    `SELECT source, short_codes, quantity, license_type, customer_email, claimed_at, issued_at, emailed_at
     FROM license_issuance WHERE transaction_id = ?`,
  )
    .bind(transactionId)
    .first<LedgerRow>()
}

/**
 * Age the claim past `issuanceStaleAfterMs`, which is what a delivery that died mid-flight leaves
 * behind. The clock lives inside workerd, so this is the only way to reach the take-over path.
 */
async function ageClaim(transactionId: string): Promise<void> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  const staleAt = new Date(Date.now() - issuanceStaleAfterMs - 60_000).toISOString()
  await env.TELEMETRY_DB.prepare(`UPDATE license_issuance SET claimed_at = ? WHERE transaction_id = ?`)
    .bind(staleAt, transactionId)
    .run()
}

function base64ToBytes(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
}

/** The key a buyer would activate, verified against the signer the harness was given. */
async function verifyStoredLicense(code: string): Promise<LicenseData> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  const stored = await env.LICENSE_CODES.get<{ fullKey: string }>(code, 'json')
  const [payloadBase64, signatureBase64] = (stored?.fullKey ?? '').split('.')
  const payloadBytes = base64ToBytes(payloadBase64)
  const publicKey = await ed.getPublicKeyAsync(privateKey)
  expect(await ed.verifyAsync(base64ToBytes(signatureBase64), payloadBytes, publicKey)).toBe(true)
  return JSON.parse(new TextDecoder().decode(payloadBytes)) as LicenseData
}

const codePattern = /CMDR-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}/g

/** The codes the buyer actually received, read out of the delivered message. */
function emailedCodes(email: SentEmail): string[] {
  return [...new Set(email.html.match(codePattern) ?? [])].sort()
}

describe('fulfilling a purchase in the Worker runtime', () => {
  it('mints a signed license, records it, and mails it to the buyer', async () => {
    const transactionId = 'txn_runtime_single'

    const delivery = await deliver({ transactionId })

    expect(delivery.status, `Body: ${delivery.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    expect(delivery.body).toMatchObject({ status: 'ok', email: paddleCustomer.email, quantity: 1 })

    const codes = await storedCodes()
    expect(codes).toHaveLength(1)
    const payload = await verifyStoredLicense(codes[0])
    expect(payload).toMatchObject({
      email: paddleCustomer.email,
      transactionId,
      type: 'commercial_perpetual',
      organizationName: paddleCustomer.business.name,
      shortCode: codes[0],
    })

    const row = await readLedgerRow(transactionId)
    expect(row).toMatchObject({
      source: 'paddle',
      short_codes: JSON.stringify(codes),
      quantity: 1,
      license_type: 'commercial_perpetual',
      customer_email: paddleCustomer.email,
    })
    expect(row?.issued_at).toBeTruthy()
    expect(row?.emailed_at).toBeTruthy()

    expect(resend.sent).toHaveLength(1)
    expect(resend.sent[0].to).toBe(paddleCustomer.email)
    expect(emailedCodes(resend.sent[0])).toEqual(codes)
    expect(resend.sent[0].text).toContain(codes[0])
  })

  it('issues one code per seat, and mails all of them', async () => {
    const transactionId = 'txn_runtime_seats'
    const before = await storedCodes()

    const delivery = await deliver({ transactionId, quantity: 3 })

    expect(delivery.status, `Body: ${delivery.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    expect(delivery.body.quantity).toBe(3)

    const minted = (await storedCodes()).filter((code) => !before.includes(code))
    expect(minted).toHaveLength(3)
    const row = await readLedgerRow(transactionId)
    expect(JSON.parse(row?.short_codes ?? '[]')).toEqual(expect.arrayContaining(minted))
    expect(row?.quantity).toBe(3)
    expect(emailedCodes(resend.sent[0])).toEqual(minted)

    // Each seat carries its own id, so device tracking counts a seat rather than the whole purchase.
    const payloads = await Promise.all(minted.map((code) => verifyStoredLicense(code)))
    expect(payloads.map((payload) => payload.transactionId).sort()).toEqual([
      `${transactionId}-1`,
      `${transactionId}-2`,
      `${transactionId}-3`,
    ])
  })
})

describe('redelivering a purchase in the Worker runtime', () => {
  it('mints nothing and sends nothing once the purchase is fulfilled', async () => {
    const transactionId = 'txn_runtime_duplicate'
    await deliver({ transactionId })
    const codesAfterFirst = await storedCodes()
    resend.sent.length = 0

    const redelivery = await deliver({ transactionId })

    expect(redelivery.status).toBe(200)
    expect(redelivery.body.status).toBe('already_processed')
    expect(await storedCodes()).toEqual(codesAfterFirst)
    expect(resend.sent).toHaveLength(0)
  })

  it('re-sends the codes it already minted when the first email was rejected', async () => {
    const transactionId = 'txn_runtime_retry'
    const before = await storedCodes()
    resend.mode = 'reject'

    const failed = await deliver({ transactionId })

    expect(failed.status).toBe(500)
    const mintedCodes = (await storedCodes()).filter((code) => !before.includes(code))
    expect(mintedCodes).toHaveLength(1)
    // The codes are stored before the email goes out, precisely so this retry can reuse them.
    expect(JSON.parse((await readLedgerRow(transactionId))?.short_codes ?? '[]')).toEqual(mintedCodes)
    expect((await readLedgerRow(transactionId))?.emailed_at).toBeNull()

    resend.mode = 'accept'
    resend.sent.length = 0
    await ageClaim(transactionId)

    const retried = await deliver({ transactionId })

    expect(retried.status, `Body: ${retried.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    expect((await storedCodes()).filter((code) => !before.includes(code))).toEqual(mintedCodes)
    expect(emailedCodes(resend.sent[0])).toEqual(mintedCodes)
    expect((await readLedgerRow(transactionId))?.emailed_at).toBeTruthy()
  })

  it('asks a delivery that arrives mid-fulfillment to retry, rather than issuing a second set', async () => {
    const transactionId = 'txn_runtime_concurrent'
    const before = await storedCodes()
    const held = holdEmail()

    try {
      const first = deliver({ transactionId })
      // Wait until the first delivery is inside the email step, which is where the claim is held.
      await expect.poll(() => resend.sent.length).toBe(1)

      const concurrent = await deliver({ transactionId })

      expect(concurrent.status).toBe(503)
      expect(concurrent.body.status).toBe('in_progress')
      expect((await storedCodes()).filter((code) => !before.includes(code))).toHaveLength(1)
      expect(resend.sent).toHaveLength(1)

      held.release()
      expect((await first).status).toBe(200)
    } finally {
      held.release()
    }
  })
})

/** Deliver a signed adjustment event, the way Paddle does on a refund or a chargeback. */
async function deliverAdjustment(params: {
  transactionId: string
  adjustmentId: string
  eventType?: 'adjustment.created' | 'adjustment.updated'
  action?: string
  type?: 'full' | 'partial'
  status?: string
  updatedAt?: string
}): Promise<Delivery> {
  const body = JSON.stringify({
    event_id: `evt_${params.adjustmentId}_${params.status ?? 'approved'}`,
    event_type: params.eventType ?? 'adjustment.created',
    occurred_at: params.updatedAt ?? new Date().toISOString(),
    data: {
      id: params.adjustmentId,
      transaction_id: params.transactionId,
      action: params.action ?? 'refund',
      type: params.type ?? 'full',
      status: params.status ?? 'approved',
      reason: 'requested_by_customer',
      totals: { total: '6900', currency_code: 'USD' },
      updated_at: params.updatedAt ?? new Date().toISOString(),
    },
  })
  const response = await server.fetch('http://api.getcmdr.com/webhook/paddle', {
    method: 'POST',
    headers: { 'Paddle-Signature': await sign(body), 'Content-Type': 'application/json' },
    body,
  })
  const text = await response.text()
  return { status: response.status, text, body: parseDeliveryBody(text) }
}

type ValidateBody = Partial<ValidationResponse> & { signedAnswer?: { payload: string; signature: string } }

async function validate(transactionId: string, nonce?: string): Promise<{ status: number; body: ValidateBody }> {
  const response = await server.fetch('http://api.getcmdr.com/validate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ transactionId, nonce }),
  })
  return { status: response.status, body: JSON.parse(await response.text()) as ValidateBody }
}

async function listLicenses(): Promise<LicenseListing> {
  const response = await server.fetch('http://api.getcmdr.com/admin/licenses', {
    headers: { Authorization: `Bearer ${adminToken}` },
  })
  return JSON.parse(await response.text()) as LicenseListing
}

/** Buy a perpetual license and return its codes. */
async function buy(transactionId: string, quantity = 1): Promise<string[]> {
  const before = await storedCodes()
  const delivery = await deliver({ transactionId, quantity })
  expect(delivery.status, `Body: ${delivery.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
  return (await storedCodes()).filter((code) => !before.includes(code))
}

/**
 * A refund in Paddle is an adjustment that leaves the transaction completed, so `/validate` asking
 * Paddle would keep answering `active` for a refunded perpetual license forever. The adjustment
 * webhook writes the refund into our ledger, and `/validate` honors it with a SIGNED `invalid`,
 * which is the only answer that takes a license off a Mac.
 */
describe('refunding a purchase in the Worker runtime', () => {
  it('revokes a fully refunded perpetual license with a signed answer, and stops its codes activating', async () => {
    const transactionId = 'txn_refund_full'
    const codes = await buy(transactionId, 2)
    expect((await validate(`${transactionId}-1`)).body.status).toBe('active')

    const refund = await deliverAdjustment({ transactionId, adjustmentId: 'adj_refund_full' })

    expect(refund.status, `Body: ${refund.text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    const nonce = 'aaaabbbbccccdddd0000111122223333'
    for (const seat of [`${transactionId}-1`, `${transactionId}-2`]) {
      const { status, body } = await validate(seat, nonce)
      expect(status).toBe(200)
      expect(body.status).toBe('invalid')
      const payloadBytes = base64ToBytes(body.signedAnswer?.payload ?? '')
      const prefixed = new Uint8Array([...new TextEncoder().encode(validationAnswerSignaturePrefix), ...payloadBytes])
      const publicKey = await ed.getPublicKeyAsync(privateKey)
      expect(await ed.verifyAsync(base64ToBytes(body.signedAnswer?.signature ?? ''), prefixed, publicKey)).toBe(true)
      expect(JSON.parse(new TextDecoder().decode(payloadBytes))).toMatchObject({
        transactionId: seat,
        status: 'invalid',
      })
    }
    const remaining = await storedCodes()
    expect(codes.filter((code) => remaining.includes(code))).toEqual([])

    const listed = (await listLicenses()).licenses.find((license) => license.transactionId === transactionId)
    expect(listed?.state).toBe('revoked')
    expect(listed?.adjustments).toMatchObject([
      { adjustmentId: 'adj_refund_full', action: 'refund', type: 'full', status: 'approved', revokes: true },
    ])
  })

  it('takes a redelivered refund as already done', async () => {
    const transactionId = 'txn_refund_redelivered'
    await buy(transactionId)
    await deliverAdjustment({ transactionId, adjustmentId: 'adj_refund_twice' })
    const revokedAt = (await listLicenses()).licenses.find(
      (license) => license.transactionId === transactionId,
    )?.revokedAt

    const again = await deliverAdjustment({ transactionId, adjustmentId: 'adj_refund_twice' })

    expect(again.status).toBe(200)
    const listed = (await listLicenses()).licenses.find((license) => license.transactionId === transactionId)
    expect(listed?.revokedAt).toBe(revokedAt)
    expect(listed?.adjustments).toHaveLength(1)
  })

  it('waits for Paddle to approve a refund, then revokes on the update', async () => {
    const transactionId = 'txn_refund_pending'
    await buy(transactionId)
    const createdAt = new Date(Date.now() - 60_000).toISOString()

    await deliverAdjustment({
      transactionId,
      adjustmentId: 'adj_pending',
      status: 'pending_approval',
      updatedAt: createdAt,
    })
    expect((await validate(transactionId)).body.status).toBe('active')

    await deliverAdjustment({ transactionId, adjustmentId: 'adj_pending', eventType: 'adjustment.updated' })
    expect((await validate(transactionId)).body.status).toBe('invalid')

    // A late redelivery of the older `pending_approval` event can't walk the record backwards.
    await deliverAdjustment({
      transactionId,
      adjustmentId: 'adj_pending',
      status: 'pending_approval',
      updatedAt: createdAt,
    })
    const listed = (await listLicenses()).licenses.find((license) => license.transactionId === transactionId)
    expect(listed?.adjustments).toMatchObject([{ adjustmentId: 'adj_pending', status: 'approved' }])
    expect((await validate(transactionId)).body.status).toBe('invalid')
  })

  it('keeps the license on a partial refund, and records it for a human', async () => {
    const transactionId = 'txn_refund_partial'
    await buy(transactionId)

    const refund = await deliverAdjustment({ transactionId, adjustmentId: 'adj_partial', type: 'partial' })

    expect(refund.status).toBe(200)
    expect((await validate(transactionId)).body.status).toBe('active')
    const listed = (await listLicenses()).licenses.find((license) => license.transactionId === transactionId)
    expect(listed?.state).toBe('active')
    expect(listed?.adjustments).toMatchObject([{ adjustmentId: 'adj_partial', type: 'partial', revokes: false }])
  })

  it('revokes on a chargeback', async () => {
    const transactionId = 'txn_refund_chargeback'
    await buy(transactionId)

    await deliverAdjustment({ transactionId, adjustmentId: 'adj_chargeback', action: 'chargeback' })

    expect((await validate(transactionId)).body.status).toBe('invalid')
  })

  it('never fulfills a purchase whose refund arrived first', async () => {
    const transactionId = 'txn_refund_before_fulfillment'
    const before = await storedCodes()

    const refund = await deliverAdjustment({ transactionId, adjustmentId: 'adj_early' })
    expect(refund.status, `Body: ${refund.text}\nWorker logs:\n${workerLogs()}`).toBe(200)

    const late = await deliver({ transactionId })

    expect(late.status).toBe(200)
    expect(late.body.status).toBe('revoked')
    expect(await storedCodes()).toEqual(before)
    expect(resend.sent).toHaveLength(0)
    expect((await validate(transactionId)).body.status).toBe('invalid')
  })

  it('records an adjustment for a transaction it never issued, without inventing a license', async () => {
    const refund = await deliverAdjustment({
      transactionId: 'txn_refund_unknown',
      adjustmentId: 'adj_unknown_partial',
      type: 'partial',
    })

    expect(refund.status).toBe(200)
    const listing = await listLicenses()
    expect(listing.licenses.find((license) => license.transactionId === 'txn_refund_unknown')).toBeUndefined()
    expect(listing.unmatchedAdjustments).toMatchObject([
      { adjustmentId: 'adj_unknown_partial', transactionId: 'txn_refund_unknown' },
    ])
  })
})

/**
 * `GET /admin/licenses` reconciles the ledger with the KV namespace, so a mock of either proves
 * nothing about the real pair. Here the rows are ones the webhook actually wrote, and the query runs
 * against a real SQLite through the D1 binding.
 */
describe('listing licenses in the Worker runtime', () => {
  it('shows what the webhook issued, and flags a code no row explains', async () => {
    const env = await server.getWorker<HarnessEnv>().getEnv()
    const orphan = 'CMDR-2345-6789-ABCD'
    await env.LICENSE_CODES.put(orphan, JSON.stringify({ fullKey: 'from.before' }))

    const response = await server.fetch('http://api.getcmdr.com/admin/licenses', {
      headers: { Authorization: `Bearer ${adminToken}` },
    })
    const text = await response.text()
    expect(response.status, `Body: ${text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    const listing = JSON.parse(text) as LicenseListing

    const fulfilled = listing.licenses.find((license) => license.transactionId === 'txn_runtime_single')
    expect(fulfilled).toMatchObject({
      source: 'paddle',
      state: 'active',
      licenseType: 'commercial_perpetual',
      customerEmail: paddleCustomer.email,
      quantity: 1,
    })
    expect(fulfilled?.shortCodes).toHaveLength(1)

    expect(listing.orphanCodes).toEqual([orphan])
    expect(listing.missingCodes).toEqual([])
  })

  it('refuses a caller without the admin token', async () => {
    const response = await server.fetch('http://api.getcmdr.com/admin/licenses')

    expect(response.status).toBe(401)
  })
})
