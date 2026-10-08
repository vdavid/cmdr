import { describe, expect, it } from 'vitest'
import { PADDLE_TIMESTAMP_TOLERANCE_SECONDS, verifyPaddleWebhook } from './paddle'

describe('verifyPaddleWebhook', () => {
  const secret = 'test-webhook-secret'
  // The tests' clock: the moment Paddle "sent" their webhooks.
  const sentAtMs = 1704700000 * 1000

  async function createValidSignature(body: string, timestamp: string): Promise<string> {
    const signedPayload = `${timestamp}:${body}`
    const encoder = new TextEncoder()
    const key = await crypto.subtle.importKey('raw', encoder.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, [
      'sign',
    ])
    const signatureBytes = await crypto.subtle.sign('HMAC', key, encoder.encode(signedPayload))
    const signature = Array.from(new Uint8Array(signatureBytes))
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('')
    return `ts=${timestamp};h1=${signature}`
  }

  it('returns false for empty signature header', async () => {
    const result = await verifyPaddleWebhook('{"test": true}', '', secret, sentAtMs)
    expect(result).toBe(false)
  })

  it('returns false for missing timestamp', async () => {
    const result = await verifyPaddleWebhook('{"test": true}', 'h1=abc123', secret, sentAtMs)
    expect(result).toBe(false)
  })

  it('returns false for missing signature', async () => {
    const result = await verifyPaddleWebhook('{"test": true}', 'ts=1234567890', secret, sentAtMs)
    expect(result).toBe(false)
  })

  it('verifies a valid signature', async () => {
    const body = '{"event_type":"transaction.completed","data":{}}'
    const timestamp = '1704700000'
    const signatureHeader = await createValidSignature(body, timestamp)

    const result = await verifyPaddleWebhook(body, signatureHeader, secret, sentAtMs)
    expect(result).toBe(true)
  })

  it('rejects an invalid signature', async () => {
    const body = '{"event_type":"transaction.completed","data":{}}'
    const timestamp = '1704700000'
    const signatureHeader = `ts=${timestamp};h1=invalid_signature_here`

    const result = await verifyPaddleWebhook(body, signatureHeader, secret, sentAtMs)
    expect(result).toBe(false)
  })

  it('rejects a tampered body', async () => {
    const originalBody = '{"event_type":"transaction.completed","data":{}}'
    const timestamp = '1704700000'
    const signatureHeader = await createValidSignature(originalBody, timestamp)

    // Tamper with the body
    const tamperedBody = '{"event_type":"transaction.completed","data":{"hacked":true}}'

    const result = await verifyPaddleWebhook(tamperedBody, signatureHeader, secret, sentAtMs)
    expect(result).toBe(false)
  })

  it('rejects a wrong secret', async () => {
    const body = '{"event_type":"transaction.completed"}'
    const timestamp = '1704700000'
    const signatureHeader = await createValidSignature(body, timestamp)

    // Use different secret for verification
    const result = await verifyPaddleWebhook(body, signatureHeader, 'wrong-secret', sentAtMs)
    expect(result).toBe(false)
  })

  // A captured webhook must not stay valid forever: Paddle's own SDKs refuse a `ts` outside a
  // tolerance (https://developer.paddle.com/webhooks/about/signature-verification).
  it('rejects a validly signed webhook replayed past the tolerance', async () => {
    const body = '{"event_type":"transaction.completed","data":{}}'
    const signatureHeader = await createValidSignature(body, '1704700000')
    const late = sentAtMs + (PADDLE_TIMESTAMP_TOLERANCE_SECONDS + 1) * 1000
    expect(await verifyPaddleWebhook(body, signatureHeader, secret, late)).toBe(false)
  })

  it('rejects a timestamp from the future past the tolerance', async () => {
    const body = '{"event_type":"transaction.completed","data":{}}'
    const signatureHeader = await createValidSignature(body, '1704700000')
    const early = sentAtMs - (PADDLE_TIMESTAMP_TOLERANCE_SECONDS + 1) * 1000
    expect(await verifyPaddleWebhook(body, signatureHeader, secret, early)).toBe(false)
  })

  it('accepts a webhook that arrives within the tolerance', async () => {
    const body = '{"event_type":"transaction.completed","data":{}}'
    const signatureHeader = await createValidSignature(body, '1704700000')
    const inTime = sentAtMs + (PADDLE_TIMESTAMP_TOLERANCE_SECONDS - 1) * 1000
    expect(await verifyPaddleWebhook(body, signatureHeader, secret, inTime)).toBe(true)
  })
})
