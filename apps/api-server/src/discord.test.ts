import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import {
  buildErrorReportPayload,
  buildEvictionPayload,
  formatBytes,
  postErrorReportNotification,
  postEvictionNotification,
  type ErrorReportNotification,
} from './discord'

const baseNotification: ErrorReportNotification = {
  id: 'ERR-A2345',
  kind: 'user',
  appVersion: '0.13.0',
  osVersion: '15.3.1',
  arch: 'aarch64',
  sizeBytes: 1_234_567,
  uploadedUnixSeconds: 1_745_000_000,
  downloadUrl: 'https://example.com/bundle.zip?sig=abc',
}

describe('formatBytes', () => {
  it('formats bytes across units', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(1536)).toBe('1.50 KB')
    expect(formatBytes(10 * 1024)).toBe('10.0 KB')
    expect(formatBytes(1_048_576)).toBe('1.00 MB')
    expect(formatBytes(2 * 1024 ** 3)).toBe('2.00 GB')
  })
})

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
                "value": "[Download bundle](https://example.com/bundle.zip?sig=abc) (link valid 7 days)",
              },
            ],
            "title": "Error report ERR-A2345",
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
