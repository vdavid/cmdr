/**
 * The daily liveness probe for the Resend send path. Sends are sporadic (a handful a month), so
 * without this a revoked or rotated `RESEND_API_KEY` stays invisible until the moment it costs us
 * something: a crash alert, a feedback digest, or a buyer's license key.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest'
import { handleEmailPathProbe } from './index'
import { EMAIL_PROBE_RECIPIENT } from './email'
import { createBaseEnv } from './cron-test-helpers'

/** The fields of the Resend payload these tests read back. */
interface SentEmail {
  from: string
  to: string
  subject: string
  text: string
}

/** Resend reports a rejected send in the response rather than throwing, so both shapes are valid. */
type SendOutcome = { id: string; error?: never } | { id?: never; error: { message: string } }

const mockSend = vi.fn<(payload: SentEmail) => Promise<SendOutcome>>(() => Promise.resolve({ id: 'test-email-id' }))
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

function lastEmailCall(): SentEmail {
  const payload = mockSend.mock.lastCall?.[0]
  if (!payload) throw new Error('No email was sent')
  return payload
}

beforeEach(() => {
  mockSend.mockClear()
})

describe('handleEmailPathProbe', () => {
  it("sends to Resend's simulator address, so no person ever receives the probe", async () => {
    await handleEmailPathProbe(createBaseEnv() as never)

    expect(mockSend).toHaveBeenCalledOnce()
    expect(lastEmailCall().to).toBe(EMAIL_PROBE_RECIPIENT)
    expect(EMAIL_PROBE_RECIPIENT).toBe('delivered@resend.dev')
  })

  it('sends from our own verified domain, so it exercises the real sending identity', async () => {
    await handleEmailPathProbe(createBaseEnv() as never)

    expect(lastEmailCall().from).toContain('@getcmdr.com')
  })

  it('says what it is, so a stray copy in a log or dashboard explains itself', async () => {
    await handleEmailPathProbe(createBaseEnv() as never)

    expect(lastEmailCall().subject.toLowerCase()).toContain('probe')
  })

  it('throws when Resend rejects the key, which is what raises the Discord alert', async () => {
    mockSend.mockImplementationOnce(() => Promise.resolve({ error: { message: 'API key is invalid' } }))

    await expect(handleEmailPathProbe(createBaseEnv() as never)).rejects.toThrow('API key is invalid')
  })

  it('stays quiet when no key is configured, so local dev and tests need no setup', async () => {
    await handleEmailPathProbe(createBaseEnv({ RESEND_API_KEY: undefined }) as never)

    expect(mockSend).not.toHaveBeenCalled()
  })
})
