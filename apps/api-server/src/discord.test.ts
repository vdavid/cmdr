import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import {
  buildListSignupPayload,
  buildCronFailurePayload,
  buildErrorReportPayload,
  buildEvictionPayload,
  buildFeedbackPayload,
  postListSignupNotification,
  postErrorReportNotification,
  postEvictionNotification,
  type ListSignupNotification,
  type ErrorReportNotification,
  type FeedbackNotification,
} from './discord'

const baseNotification: ErrorReportNotification = {
  id: 'ERR-A2345',
  kind: 'user',
  buildMode: 'release',
  appVersion: '0.13.0',
  osVersion: '15.3.1',
  arch: 'aarch64',
  sizeBytes: 1_234_567,
  uploadedUnixSeconds: 1_745_000_000,
  downloadUrl: 'https://example.com/bundle.zip?sig=abc',
  linkTtlHours: 24,
}

describe('buildErrorReportPayload', () => {
  it('produces a stable embed shape', () => {
    expect(buildErrorReportPayload(baseNotification)).toMatchInlineSnapshot(`
      {
        "embeds": [
          {
            "color": 16739179,
            "fields": [
              {
                "inline": true,
                "name": "Kind",
                "value": "user",
              },
              {
                "inline": true,
                "name": "App version",
                "value": "0.13.0",
              },
              {
                "inline": true,
                "name": "OS",
                "value": "15.3.1",
              },
              {
                "inline": true,
                "name": "Arch",
                "value": "aarch64",
              },
              {
                "inline": true,
                "name": "Size",
                "value": "1.18 MB",
              },
              {
                "inline": true,
                "name": "Uploaded",
                "value": "<t:1745000000:R>",
              },
              {
                "name": "Download",
                "value": "[Download bundle](https://example.com/bundle.zip?sig=abc) (link valid 24 hours)",
              },
            ],
            "title": "[PROD] Error report ERR-A2345",
          },
        ],
      }
    `)
  })

  it('includes the user note and truncates past 500 chars', () => {
    const longNote = 'x'.repeat(600)
    const payload = buildErrorReportPayload({ ...baseNotification, userNote: longNote }) as {
      embeds: { fields: { name: string; value: string }[] }[]
    }
    const noteField = payload.embeds[0].fields.find((f) => f.name === 'User note')
    expect(noteField).toBeDefined()
    expect(noteField?.value.length).toBeLessThanOrEqual(600)
    expect(noteField?.value.startsWith('x'.repeat(500))).toBe(true)
    expect(noteField?.value).toContain('full note in bundle')
  })

  it('omits the user note field when absent', () => {
    const payload = buildErrorReportPayload(baseNotification) as {
      embeds: { fields: { name: string; value: string }[] }[]
    }
    expect(payload.embeds[0].fields.find((f) => f.name === 'User note')).toBeUndefined()
  })

  it('prefixes the title with [DEV] when buildMode is debug', () => {
    const payload = buildErrorReportPayload({ ...baseNotification, buildMode: 'debug' }) as {
      embeds: { title: string }[]
    }
    expect(payload.embeds[0].title).toBe('[DEV] Error report ERR-A2345')
  })

  it('prefixes the title with [PROD] when buildMode is release', () => {
    const payload = buildErrorReportPayload(baseNotification) as { embeds: { title: string }[] }
    expect(payload.embeds[0].title).toBe('[PROD] Error report ERR-A2345')
  })
})

describe('buildFeedbackPayload', () => {
  const baseFeedback: FeedbackNotification = {
    buildMode: 'release',
    appVersion: '0.14.0',
    osVersion: 'macOS 26.0',
    hasReplyTo: false,
    feedback: 'Love the app! The Brief mode columns are perfect.',
  }

  it('puts the feedback text in the embed description with a [PROD] title prefix', () => {
    const payload = buildFeedbackPayload(baseFeedback) as {
      embeds: { title: string; description: string; fields: { name: string; value: string }[] }[]
    }
    expect(payload.embeds[0].title).toBe('[PROD] Feedback')
    expect(payload.embeds[0].description).toBe(baseFeedback.feedback)
    expect(payload.embeds[0].fields.map((f) => f.name)).toEqual(['App version', 'OS'])
  })

  it('prefixes [DEV] for debug builds and flags an attached reply-to without the address', () => {
    const payload = buildFeedbackPayload({
      ...baseFeedback,
      buildMode: 'debug',
      hasReplyTo: true,
    }) as { embeds: { title: string; fields: { name: string; value: string }[] }[] }
    expect(payload.embeds[0].title).toBe('[DEV] Feedback')
    expect(payload.embeds[0].fields).toContainEqual({
      name: 'Reply-to attached',
      value: 'Yes (address in the feedback table)',
      inline: true,
    })
  })

  it('truncates very long feedback below the Discord description cap', () => {
    const payload = buildFeedbackPayload({ ...baseFeedback, feedback: 'x'.repeat(10_000) }) as {
      embeds: { description: string }[]
    }
    expect(payload.embeds[0].description.length).toBeLessThanOrEqual(4096)
    expect(payload.embeds[0].description).toContain('full text in the feedback table')
  })
})

describe('buildListSignupPayload', () => {
  const baseSignup: ListSignupNotification = {
    list: 'beta',
    signupUnixSeconds: 1_745_000_000,
    listAdminUrl: 'https://mail.getcmdr.com/admin/subscribers?lists=4',
  }

  it('produces a stable embed shape for a beta signup', () => {
    expect(buildListSignupPayload(baseSignup)).toMatchInlineSnapshot(`
      {
        "embeds": [
          {
            "color": 5763719,
            "description": "Status: unconfirmed — Listmonk sent them the confirmation email.",
            "fields": [
              {
                "inline": true,
                "name": "When",
                "value": "<t:1745000000:R>",
              },
              {
                "name": "Listmonk",
                "value": "[Beta list subscribers](https://mail.getcmdr.com/admin/subscribers?lists=4)",
              },
            ],
            "title": "New beta-tester signup",
          },
        ],
      }
    `)
  })

  it('names the newsletter list for a newsletter signup', () => {
    const payload = buildListSignupPayload({ ...baseSignup, list: 'newsletter' }) as {
      embeds: { title: string; fields: { value: string }[] }[]
    }
    expect(payload.embeds[0].title).toBe('New newsletter signup')
    expect(payload.embeds[0].fields[1].value).toContain('[Newsletter subscribers]')
  })
})

describe('postListSignupNotification', () => {
  let originalFetch: typeof fetch
  beforeEach(() => {
    originalFetch = globalThis.fetch
  })
  afterEach(() => {
    globalThis.fetch = originalFetch
    vi.restoreAllMocks()
  })

  it('POSTs the beta-signup embed to the webhook', async () => {
    const mock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    globalThis.fetch = mock

    await postListSignupNotification('https://discord/webhook', {
      list: 'beta',
      signupUnixSeconds: 1_745_000_000,
      listAdminUrl: 'https://mail.getcmdr.com/admin/subscribers?lists=4',
    })

    expect(mock).toHaveBeenCalledOnce()
    const [url, init] = mock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('https://discord/webhook')
    const body = JSON.parse(init.body as string) as { embeds: { title: string }[] }
    expect(body.embeds[0].title).toBe('New beta-tester signup')
  })
})

describe('buildEvictionPayload', () => {
  it('formats a plain content message', () => {
    const payload = buildEvictionPayload({
      evictedCount: 7,
      freedBytes: 2 * 1024 ** 3,
      newTotalBytes: 6 * 1024 ** 3,
    })
    expect(payload).toEqual({
      content: 'Eviction sweep: removed 7 oldest bundle(s), freed 2.00 GB. New total: 6.00 GB.',
    })
  })
})

describe('postErrorReportNotification', () => {
  let originalFetch: typeof fetch
  beforeEach(() => {
    originalFetch = globalThis.fetch
  })
  afterEach(() => {
    globalThis.fetch = originalFetch
    vi.restoreAllMocks()
  })

  it('POSTs JSON to the webhook on happy path', async () => {
    const mock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    globalThis.fetch = mock

    await postErrorReportNotification('https://discord/webhook', baseNotification)

    expect(mock).toHaveBeenCalledOnce()
    const [url, init] = mock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('https://discord/webhook')
    expect(init.method).toBe('POST')
    expect((init.headers as Record<string, string>)['Content-Type']).toBe('application/json')
    const body = JSON.parse(init.body as string) as { embeds: unknown[] }
    expect(body.embeds).toHaveLength(1)
  })

  it('retries once after 429, honoring Retry-After', async () => {
    vi.useFakeTimers()
    const headers = new Headers({ 'Retry-After': '0.01' })
    const mock = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 429, headers }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
    globalThis.fetch = mock

    const promise = postErrorReportNotification('https://discord/webhook', baseNotification)
    await vi.advanceTimersByTimeAsync(10)
    await promise

    expect(mock).toHaveBeenCalledTimes(2)
    vi.useRealTimers()
  })

  it('logs and drops silently on second failure', async () => {
    vi.useFakeTimers()
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    const headers = new Headers({ 'Retry-After': '0.01' })
    const mock = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 429, headers }))
      .mockResolvedValueOnce(new Response(null, { status: 500 }))
    globalThis.fetch = mock

    const promise = postErrorReportNotification('https://discord/webhook', baseNotification)
    await vi.advanceTimersByTimeAsync(50)
    await expect(promise).resolves.toBeUndefined()

    expect(errSpy).toHaveBeenCalled()
    expect(errSpy.mock.calls[0][0]).toContain('error-report')
    vi.useRealTimers()
  })

  it('logs and drops when fetch throws', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    globalThis.fetch = () => Promise.reject(new Error('network down'))

    await expect(postErrorReportNotification('https://discord/webhook', baseNotification)).resolves.toBeUndefined()

    expect(errSpy).toHaveBeenCalled()
  })
})

describe('postEvictionNotification', () => {
  let originalFetch: typeof fetch
  beforeEach(() => {
    originalFetch = globalThis.fetch
  })
  afterEach(() => {
    globalThis.fetch = originalFetch
    vi.restoreAllMocks()
  })

  it('POSTs a plain-content message (no embed)', async () => {
    const mock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    globalThis.fetch = mock

    await postEvictionNotification('https://discord/webhook', {
      evictedCount: 3,
      freedBytes: 1024 ** 3,
      newTotalBytes: 6 * 1024 ** 3,
    })

    const [, init] = mock.mock.calls[0] as unknown as [string, RequestInit]
    const body = JSON.parse(init.body as string) as { content?: string; embeds?: unknown[] }
    expect(body.content).toContain('Eviction sweep')
    expect(body.embeds).toBeUndefined()
  })
})

describe('buildCronFailurePayload', () => {
  const failure = {
    job: 'Crash notifications',
    when: '2026-09-02T03:00:00.000Z',
    detail: 'Error: Resend rejected the crash notification email: API key is invalid',
  }

  it('names the job, the tick, and the cause', () => {
    const payload = buildCronFailurePayload(failure) as { content: string }

    expect(payload.content).toContain('Crash notifications')
    expect(payload.content).toContain('2026-09-02T03:00:00.000Z')
    expect(payload.content).toContain('API key is invalid')
  })

  it('sends plain content, so a malformed embed can never swallow the alert', () => {
    const payload = buildCronFailurePayload(failure) as { content: string; embeds?: unknown[] }

    expect(payload.embeds).toBeUndefined()
  })

  it('caps a runaway error so Discord accepts the message', () => {
    const payload = buildCronFailurePayload({ ...failure, detail: 'x'.repeat(9000) }) as { content: string }

    expect(payload.content.length).toBeLessThan(2000)
    expect(payload.content).toContain('truncated')
  })
})
