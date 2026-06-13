import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchLinkCodes, upsertLinkCode, deleteLinkCode } from './link-codes.js'

const env = { LICENSE_SERVER_ADMIN_TOKEN: 'test-admin-token' }

describe('fetchLinkCodes', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('returns the map and sends the bearer token to the admin endpoint', async () => {
    const map = { hn: { utm_source: 'hackernews', utm_medium: 'social' } }
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => map })
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchLinkCodes(env)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.data).toEqual(map)
    expect(fetchMock.mock.calls[0][0]).toBe('https://api.getcmdr.com/admin/r-codes')
    expect(fetchMock.mock.calls[0][1]?.headers).toEqual({ Authorization: 'Bearer test-admin-token' })
  })

  it('returns an error on 401', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401 }))
    const result = await fetchLinkCodes(env)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('401')
  })

  it('honors WORKER_BASE_URL override', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) })
    vi.stubGlobal('fetch', fetchMock)
    await fetchLinkCodes({ ...env, WORKER_BASE_URL: 'http://127.0.0.1:18900' })
    expect(fetchMock.mock.calls[0][0]).toBe('http://127.0.0.1:18900/admin/r-codes')
  })
})

describe('upsertLinkCode', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('PUTs the code with only the non-empty fields and auth + JSON headers', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, text: async () => '' })
    vi.stubGlobal('fetch', fetchMock)

    const result = await upsertLinkCode(env, { code: 'hn', utm_source: 'hackernews', utm_medium: 'social' })
    expect(result.ok).toBe(true)
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('https://api.getcmdr.com/admin/r-codes/hn')
    expect(init.method).toBe('PUT')
    expect(init.headers).toEqual({ Authorization: 'Bearer test-admin-token', 'Content-Type': 'application/json' })
    expect(JSON.parse(init.body)).toEqual({ utm_source: 'hackernews', utm_medium: 'social' })
  })

  it('omits empty medium and note from the body', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, text: async () => '' })
    vi.stubGlobal('fetch', fetchMock)
    await upsertLinkCode(env, { code: 'nl', utm_source: 'newsletter' })
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ utm_source: 'newsletter' })
  })

  it('url-encodes the code in the path', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, text: async () => '' })
    vi.stubGlobal('fetch', fetchMock)
    await upsertLinkCode(env, { code: 'a.b_c', utm_source: 'x' })
    expect(fetchMock.mock.calls[0][0]).toBe('https://api.getcmdr.com/admin/r-codes/a.b_c')
  })

  it('surfaces the worker error text on a 400', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 400, text: async () => 'Invalid code' }))
    const result = await upsertLinkCode(env, { code: 'hn', utm_source: 'x' })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Invalid code')
  })
})

describe('deleteLinkCode', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('DELETEs the code with the bearer token', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, text: async () => '' })
    vi.stubGlobal('fetch', fetchMock)
    const result = await deleteLinkCode(env, 'hn')
    expect(result.ok).toBe(true)
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('https://api.getcmdr.com/admin/r-codes/hn')
    expect(init.method).toBe('DELETE')
    expect(init.headers).toEqual({ Authorization: 'Bearer test-admin-token' })
  })

  it('returns an error on 401', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401, text: async () => 'Unauthorized' }))
    const result = await deleteLinkCode(env, 'hn')
    expect(result.ok).toBe(false)
  })
})
