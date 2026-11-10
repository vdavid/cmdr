import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { githubFetch } from './github-fetch'
import { pause } from './retry-pause'

beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => undefined)
  vi.mocked(pause).mockClear()
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

const url = 'https://api.github.com/repos/getcmdr/cmdr-reports/issues/comments'

/** A fetch that answers with each status in turn (a thrown `Error` for `'throw'`), then 200. */
function answers(...sequence: (number | 'throw')[]) {
  const fetchMock = vi.fn(() => {
    const next = sequence.shift() ?? 200
    if (next === 'throw') return Promise.reject(new Error('network connection lost'))
    return Promise.resolve(new Response('{}', { status: next }))
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

describe('githubFetch', () => {
  it('rides out a brief outage on a read: a 522, then a 502, then the answer', async () => {
    const fetchMock = answers(522, 502)

    const response = await githubFetch(url, {})

    expect(response.status).toBe(200)
    expect(fetchMock).toHaveBeenCalledTimes(3)
  })

  it('retries a read that threw on the network', async () => {
    const fetchMock = answers('throw')

    await expect(githubFetch(url, {})).resolves.toHaveProperty('status', 200)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('gives up after three tries and hands back the last answer, so the caller reports it', async () => {
    const fetchMock = answers(503, 503, 522, 200)

    const response = await githubFetch(url, {})

    expect(response.status).toBe(522)
    expect(fetchMock).toHaveBeenCalledTimes(3)
    // A moment apart, then a longer moment: enough for a blip to pass, short enough for a tick.
    expect(vi.mocked(pause).mock.calls).toEqual([[1_000], [3_000]])
  })

  it('rethrows when every try threw', async () => {
    answers('throw', 'throw', 'throw')

    await expect(githubFetch(url, {})).rejects.toThrow('network connection lost')
  })

  it('never retries a 4xx: GitHub meant it', async () => {
    const fetchMock = answers(404)

    await expect(githubFetch(url, {})).resolves.toHaveProperty('status', 404)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('retries a DELETE, since deleting twice leaves the same state', async () => {
    const fetchMock = answers(500)

    await expect(githubFetch(url, { method: 'DELETE' })).resolves.toHaveProperty('status', 200)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('never retries a POST, which may have landed before the 5xx and would land twice', async () => {
    const fetchMock = answers(502)

    await expect(githubFetch(url, { method: 'POST', body: '{}' })).resolves.toHaveProperty('status', 502)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('retries a POST its caller vouches for as a read, like a GraphQL query', async () => {
    const fetchMock = answers(502)

    const response = await githubFetch(url, { method: 'POST', body: '{}' }, { idempotent: true })

    expect(response.status).toBe(200)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})
