import { describe, expect, it, vi, afterEach } from 'vitest'
import { pingCronHealth } from './cron-health'

afterEach(() => {
  vi.unstubAllGlobals()
})

/** Capture every ping so a test can assert on the URL and body without a network call. */
function stubFetch(impl?: () => Promise<Response>) {
  const calls: { url: string; body: string }[] = []
  const fetchMock = vi.fn(async (url: string | URL, init?: RequestInit) => {
    calls.push({ url: String(url), body: typeof init?.body === 'string' ? init.body : '' })
    return impl ? await impl() : new Response('OK')
  })
  vi.stubGlobal('fetch', fetchMock)
  return calls
}

describe('pingCronHealth', () => {
  it('pings the bare URL when every job succeeded', async () => {
    const calls = stubFetch()

    await pingCronHealth('https://hc-ping.com/abc', [])

    expect(calls).toHaveLength(1)
    expect(calls[0]?.url).toBe('https://hc-ping.com/abc')
    expect(calls[0]?.body).toBe('All cron jobs finished.')
  })

  it('pings the /fail endpoint and names the jobs when one threw', async () => {
    const calls = stubFetch()

    await pingCronHealth('https://hc-ping.com/abc', ['Crash notifications', 'Retention sweep'])

    expect(calls).toHaveLength(1)
    expect(calls[0]?.url).toBe('https://hc-ping.com/abc/fail')
    expect(calls[0]?.body).toBe('These cron jobs threw: Crash notifications, Retention sweep.')
  })

  it('tolerates a trailing slash on the configured URL', async () => {
    const calls = stubFetch()

    await pingCronHealth('https://hc-ping.com/abc/', ['Daily aggregation'])

    expect(calls[0]?.url).toBe('https://hc-ping.com/abc/fail')
  })

  it('stays quiet when the ping itself throws, so it can never take the cron down', async () => {
    stubFetch(() => Promise.reject(new Error('network is down')))
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    await expect(pingCronHealth('https://hc-ping.com/abc', [])).resolves.toBeUndefined()

    expect(consoleError).toHaveBeenCalled()
    consoleError.mockRestore()
  })

  it('logs a ping the service rejected, rather than throwing', async () => {
    stubFetch(() => Promise.resolve(new Response('not found', { status: 404 })))
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    await pingCronHealth('https://hc-ping.com/abc', [])

    expect(consoleError).toHaveBeenCalledWith(expect.stringContaining('404'))
    consoleError.mockRestore()
  })
})
