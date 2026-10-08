import { describe, expect, it, vi } from 'vitest'
import { app } from '../index'

/** A KV that records every read, so a cell can prove a refused request never looked a code up. */
function createKv() {
  const get = vi.fn(() => Promise.resolve(null))
  return { kv: { get, put: vi.fn(() => Promise.resolve()) } as unknown as KVNamespace, get }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: createKv().kv,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ...overrides,
  }
}

function activate(bindings: ReturnType<typeof createBindings>, ip = '203.0.113.7') {
  return app.request(
    '/activate',
    {
      method: 'POST',
      headers: { 'cf-connecting-ip': ip, 'content-type': 'application/json' },
      body: JSON.stringify({ code: 'ABCD-EFGH-JKLM' }),
    },
    bindings,
  )
}

// A short code is all it takes to fetch a full license key, so guessing them must be slow.
describe('/activate rate limiting', () => {
  it('returns 429 and never looks the code up when the limiter rejects the caller', async () => {
    const { kv, get } = createKv()
    const bindings = createBindings({
      LICENSE_CODES: kv,
      ACTIVATE_LIMITER: { limit: vi.fn(() => Promise.resolve({ success: false })) },
    })

    const res = await activate(bindings)

    expect(res.status).toBe(429)
    expect(get).not.toHaveBeenCalled()
  })

  it('keys the limit on the caller IP and lets an allowed request through', async () => {
    const limit = vi.fn(() => Promise.resolve({ success: true }))
    const bindings = createBindings({ ACTIVATE_LIMITER: { limit } })

    const res = await activate(bindings)

    expect(res.status).not.toBe(429)
    expect(limit).toHaveBeenCalledWith({ key: '203.0.113.7' })
  })
})

function validate(bindings: ReturnType<typeof createBindings>, ip = '203.0.113.7') {
  return app.request(
    '/validate',
    {
      method: 'POST',
      headers: { 'cf-connecting-ip': ip, 'content-type': 'application/json' },
      body: JSON.stringify({ transactionId: 'txn_01abc', deviceId: 'device-1' }),
    },
    bindings,
  )
}

// Each request costs a Paddle API call. A 429 is safe for a real Mac: the app reads any non-502
// failure as a network error and keeps its cached status (`validation_client.rs`).
describe('/validate rate limiting', () => {
  it('returns 429 and never asks Paddle when the limiter rejects the caller', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    const bindings = createBindings({
      VALIDATE_LIMITER: { limit: vi.fn(() => Promise.resolve({ success: false })) },
    })

    const res = await validate(bindings)

    expect(res.status).toBe(429)
    expect(fetchSpy).not.toHaveBeenCalled()
    fetchSpy.mockRestore()
  })

  it('keys the limit on the caller IP', async () => {
    const limit = vi.fn(() => Promise.resolve({ success: false }))
    await validate(createBindings({ VALIDATE_LIMITER: { limit } }))
    expect(limit).toHaveBeenCalledWith({ key: '203.0.113.7' })
  })
})
