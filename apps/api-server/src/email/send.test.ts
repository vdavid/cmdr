/**
 * The one door out to SMTP2Go. SMTP2Go answers 200 OK even when a send failed (the failure sits in
 * `data.failed` / `data.failures`), so every shape of "it didn't go out" has to become a throw here,
 * or a license email can vanish while the purchase looks fulfilled.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { checkEmailPath, EMAIL_QUOTA_ALARM_FRACTION, sendEmail, SMTP2GO_API_BASE, type EmailMessage } from './send'

const apiKey = 'api-SECRET-KEY-VALUE'
const message: EmailMessage = {
  from: 'Cmdr <noreply@getcmdr.com>',
  to: 'buyer@example.com',
  subject: 'Your license',
  html: '<p>Hi</p>',
  text: 'Hi',
  replyTo: 'person@example.com',
}

const accepted = { request_id: 'r1', data: { succeeded: 1, failed: 0, failures: [], email_id: 'e1' } }

let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>

function answer(body: unknown, status = 200): void {
  fetchMock.mockImplementation(() => Promise.resolve(Response.json(body, { status })))
}

function lastRequest(): { url: string; headers: Headers; body: Record<string, unknown> } {
  const call = fetchMock.mock.lastCall
  if (!call) throw new Error('Nothing was sent')
  const [url, init] = call
  if (typeof url !== 'string' || typeof init?.body !== 'string') throw new Error('Expected a string URL and JSON body')
  return {
    url,
    headers: new Headers(init.headers),
    body: JSON.parse(init.body) as Record<string, unknown>,
  }
}

beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>()
  answer(accepted)
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('sendEmail', () => {
  it("posts to SMTP2Go's EU endpoint with the key in its header and waits for the verdict", async () => {
    await sendEmail(apiKey, message, 'license')

    const request = lastRequest()
    expect(request.url).toBe('https://eu-api.smtp2go.com/v3/email/send')
    expect(SMTP2GO_API_BASE).toBe('https://eu-api.smtp2go.com/v3')
    expect(request.headers.get('X-Smtp2go-Api-Key')).toBe(apiKey)
    // `fastaccept: true` would answer before the send is judged, so a failure would read as success.
    expect(request.body.fastaccept).toBe(false)
  })

  it("maps our message onto SMTP2Go's fields, with Reply-To as a custom header", async () => {
    await sendEmail(apiKey, message, 'license')

    expect(lastRequest().body).toEqual({
      sender: 'Cmdr <noreply@getcmdr.com>',
      to: ['buyer@example.com'],
      subject: 'Your license',
      html_body: '<p>Hi</p>',
      text_body: 'Hi',
      custom_headers: [{ header: 'Reply-To', value: 'person@example.com' }],
      fastaccept: false,
    })
  })

  it('leaves out the optional fields a message has no value for', async () => {
    await sendEmail(apiKey, { from: message.from, to: message.to, subject: 'S', html: '<p>x</p>' }, 'digest')

    const body = lastRequest().body
    expect(body).not.toHaveProperty('text_body')
    expect(body).not.toHaveProperty('custom_headers')
  })

  it('throws on a network failure, naming the email', async () => {
    fetchMock.mockImplementation(() => Promise.reject(new TypeError('fetch failed')))

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email.*fetch failed/)
  })

  it("throws on a non-2xx, carrying SMTP2Go's own error", async () => {
    answer({ data: { error_code: 'E_ApiResponseCodes.API_KEY_INVALID', error: 'The API key is invalid' } }, 401)

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email.*401.*The API key is invalid/)
  })

  it('throws on a non-2xx whose body is not JSON', async () => {
    fetchMock.mockImplementation(() => Promise.resolve(new Response('Bad gateway', { status: 502 })))

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email.*502/)
  })

  it('throws on a 200 whose body carries an error', async () => {
    answer({ data: { error_code: 'E_ApiResponseCodes.ENDPOINT_PERMISSION_DENIED', error: 'No permission' } })

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email.*No permission/)
  })

  it('throws on a 200 whose body carries only an error code', async () => {
    answer({ data: { error_code: 'E_ApiResponseCodes.SOMETHING' } })

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/E_ApiResponseCodes.SOMETHING/)
  })

  it('throws on a 200 that reports the send failed, naming the failures', async () => {
    answer({ data: { succeeded: 0, failed: 1, failures: ['Sender domain not verified'], email_id: 'e1' } })

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email.*Sender domain not verified/)
  })

  it('throws on a 200 that reports success AND failure', async () => {
    answer({ data: { succeeded: 1, failed: 1, failures: [], email_id: 'e1' } })

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email/)
  })

  it('throws on a 200 that never says it succeeded', async () => {
    answer({ data: { email_id: 'e1' } })

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email/)
  })

  it('throws on a 200 whose body is not JSON', async () => {
    fetchMock.mockImplementation(() => Promise.resolve(new Response('<html>ok</html>', { status: 200 })))

    await expect(sendEmail(apiKey, message, 'license')).rejects.toThrow(/license email/)
  })

  it('never puts the key in the error', async () => {
    answer({ data: { error: `bad key ${apiKey}` } }, 401)

    const error = await sendEmail(apiKey, message, 'license').catch((e: unknown) => e as Error)
    expect(error).toBeInstanceOf(Error)
    expect((error as Error).message).not.toContain(apiKey)
  })
})

describe('checkEmailPath', () => {
  function cycle(remaining: number, max = 1000): unknown {
    return {
      request_id: 'r1',
      data: {
        cycle_start: '2026-11-01 00:00:00+00:00',
        cycle_end: '2026-12-01 00:00:00+00:00',
        cycle_used: max - remaining,
        cycle_remaining: remaining,
        cycle_max: max,
      },
    }
  }

  it("asks SMTP2Go's EU stats endpoint with the same key, and sends no email", async () => {
    answer(cycle(900))

    await checkEmailPath(apiKey)

    expect(fetchMock).toHaveBeenCalledOnce()
    const request = lastRequest()
    expect(request.url).toBe('https://eu-api.smtp2go.com/v3/stats/email_cycle')
    expect(request.headers.get('X-Smtp2go-Api-Key')).toBe(apiKey)
  })

  it('throws when the key is dead, which is what raises the cron alarm', async () => {
    answer({ data: { error_code: 'E_ApiResponseCodes.API_KEY_INVALID', error: 'The API key is invalid' } }, 401)

    await expect(checkEmailPath(apiKey)).rejects.toThrow(/The API key is invalid/)
  })

  it('throws on a 200 that carries an error', async () => {
    answer({ data: { error: 'You do not have permission to access this API endpoint' } })

    await expect(checkEmailPath(apiKey)).rejects.toThrow(/permission/)
  })

  it('throws when the quota is below the alarm line, saying how many remain', async () => {
    answer(cycle(149))

    await expect(checkEmailPath(apiKey)).rejects.toThrow(/149 of 1,000.*license emails/)
  })

  it('stays quiet at exactly the alarm line', async () => {
    expect(EMAIL_QUOTA_ALARM_FRACTION).toBe(0.15)
    answer(cycle(150))

    await expect(checkEmailPath(apiKey)).resolves.toBeUndefined()
  })

  it('throws when the answer has no quota numbers to judge', async () => {
    answer({ data: {} })

    await expect(checkEmailPath(apiKey)).rejects.toThrow(/cycle/)
  })
})
