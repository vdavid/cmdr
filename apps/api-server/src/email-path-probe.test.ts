/**
 * The daily liveness probe for the SMTP2Go path. Sends are sporadic (a handful a month), so
 * without this a revoked `SMTP2GO_API_KEY` or a spent quota stays invisible until the moment it
 * costs us something: a report digest or a buyer's license key. The threshold and error shapes are
 * pinned in `email/send.test.ts`; this covers the cron wiring.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { handleEmailPathProbe } from './index'
import { createBaseEnv } from './cron-test-helpers'

let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>

function cycle(remaining: number, max = 1000): Response {
  return Response.json({ data: { cycle_used: max - remaining, cycle_remaining: remaining, cycle_max: max } })
}

beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>(() => Promise.resolve(cycle(900)))
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('handleEmailPathProbe', () => {
  it('reads the email cycle with the configured key and sends no email', async () => {
    await handleEmailPathProbe(createBaseEnv() as never)

    expect(fetchMock).toHaveBeenCalledOnce()
    const [url, init] = fetchMock.mock.lastCall ?? []
    expect(url).toBe('https://eu-api.smtp2go.com/v3/stats/email_cycle')
    expect(new Headers(init?.headers).get('X-Smtp2go-Api-Key')).toBe('test-smtp2go-key')
  })

  it('throws when SMTP2Go refuses the key, which is what raises the cron alarm', async () => {
    fetchMock.mockImplementation(() =>
      Promise.resolve(Response.json({ data: { error: 'The API key is invalid' } }, { status: 401 })),
    )

    await expect(handleEmailPathProbe(createBaseEnv() as never)).rejects.toThrow('The API key is invalid')
  })

  it('throws when the quota runs low, so a newsletter can never quietly starve license emails', async () => {
    fetchMock.mockImplementation(() => Promise.resolve(cycle(40)))

    await expect(handleEmailPathProbe(createBaseEnv() as never)).rejects.toThrow(/40 of 1,000/)
  })

  it('stays quiet when no key is configured, so local dev and tests need no setup', async () => {
    await handleEmailPathProbe(createBaseEnv({ SMTP2GO_API_KEY: undefined }) as never)

    expect(fetchMock).not.toHaveBeenCalled()
  })
})
