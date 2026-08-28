/**
 * The email a hand-written error report triggers at intake: who gets one, who never does, and what
 * the message says. Split from `error-report.test.ts` for the same reason the crash and feedback
 * emails are split from `scheduled.test.ts`: the output is a document, so it grows on its own axis.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { app } from '../index'
import { DAILY_ERROR_REPORT_EMAIL_CAP, errorReportEmailCountKey } from './error-report-intake'
import { buildMultipart, createBindings, createKv, createR2, todayUtc, validMeta } from './error-report-test-helpers'

/** The fields of the Resend payload these tests read back. */
interface SentEmail {
  from: string
  to: string
  subject: string
  html: string
  replyTo?: string
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

/** Bindings with a recipient configured, so the email path is live. */
function mailableBindings(overrides: Record<string, unknown> = {}) {
  return createBindings({ CRASH_NOTIFICATION_EMAIL: 'david@example.com', ...overrides })
}

/** Post one report and assert the upload itself succeeded. */
async function upload(bindings: Record<string, unknown>, meta: unknown = validMeta): Promise<void> {
  const fd = buildMultipart(new Uint8Array([1, 2, 3]), meta)
  const res = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
  expect(res.status).toBe(200)
}

beforeEach(() => {
  mockSend.mockClear()
  // Swallow Discord webhook calls unless a test watches them.
  globalThis.fetch = () => Promise.resolve(new Response(null, { status: 204 }))
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('error-report notification email', () => {
  it('mails a hand-written report to the configured recipient', async () => {
    await upload(mailableBindings())

    expect(mockSend).toHaveBeenCalledOnce()
    const { to, from } = lastEmailCall()
    expect(to).toBe('david@example.com')
    expect(from).toBe('Cmdr Error Reports <noreply@getcmdr.com>')
  })

  it('never mails an auto-sent report', async () => {
    // One misbehaving install produced 50+ auto bundles in three days. Discord absorbs that; an
    // inbox does not.
    await upload(mailableBindings(), { ...validMeta, kind: 'auto' })

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('prefers FEEDBACK_NOTIFICATION_EMAIL over the crash recipient', async () => {
    await upload(mailableBindings({ FEEDBACK_NOTIFICATION_EMAIL: 'feedback@example.com' }))

    expect(lastEmailCall().to).toBe('feedback@example.com')
  })

  it('stays a no-op when no recipient is configured', async () => {
    await upload(createBindings())

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('stays a no-op when RESEND_API_KEY is not set', async () => {
    await upload(mailableBindings({ RESEND_API_KEY: undefined }))

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('still pings Discord for both kinds, exactly once each', async () => {
    const posts: string[] = []
    globalThis.fetch = ((_url: string, init: { body: string }) => {
      posts.push(init.body)
      return Promise.resolve(new Response(null, { status: 204 }))
    }) as unknown as typeof fetch
    const bindings = mailableBindings({ DISCORD_WEBHOOK_URL: 'https://discord.example/webhook' })

    await upload(bindings)
    await upload(bindings, { ...validMeta, kind: 'auto' })

    expect(posts).toHaveLength(2)
    expect(posts[0]).toContain('ERR-A2345')
    expect(posts[1]).toContain('ERR-A2345')
    // The email is the extra channel, not a replacement: only the user report got one.
    expect(mockSend).toHaveBeenCalledOnce()
  })

  it('carries the short id, versions, arch, and bundle size', async () => {
    await upload(mailableBindings())

    const { subject, html } = lastEmailCall()
    expect(subject).toContain('ERR-A2345')
    expect(html).toContain('ERR-A2345')
    expect(html).toContain('0.13.0')
    expect(html).toContain('15.3.1')
    expect(html).toContain('aarch64')
    expect(html).toContain('3 B')
  })

  it('shows the note the person wrote, with their line breaks intact', async () => {
    // The note is the whole reason this email exists. Collapsing its paragraphs into one run is
    // how a carefully written report becomes unreadable in the one place it gets read.
    const note = 'Copying stalled at 90%.\n\nSecond try worked.'
    await upload(mailableBindings(), { ...validMeta, userNote: note })

    const { html } = lastEmailCall()
    expect(html).toContain('white-space: pre-wrap')
    expect(html).toContain(note)
  })

  it('escapes HTML in the note', async () => {
    await upload(mailableBindings(), {
      ...validMeta,
      userNote: '<script>alert(1)</script> A & B <img src=x onerror=alert(2)>',
    })

    const { html } = lastEmailCall()
    expect(html).not.toContain('<script>')
    expect(html).not.toContain('<img src=x')
    expect(html).toContain('&lt;script&gt;')
    expect(html).toContain('A &amp; B')
  })

  it('escapes HTML in the id and the machine fields', async () => {
    await upload(mailableBindings(), { ...validMeta, appVersion: '<b>0.13.0</b>', arch: '"><i>x</i>' })

    const { html } = lastEmailCall()
    expect(html).not.toContain('<b>0.13.0</b>')
    expect(html).not.toContain('<i>x</i>')
  })

  it('says so plainly when the person attached no note', async () => {
    await upload(mailableBindings(), { ...validMeta, userNote: null })

    expect(lastEmailCall().html).toContain('No note')
  })

  it('marks dev builds in the subject and the card', async () => {
    await upload(mailableBindings(), { ...validMeta, buildMode: 'debug' })

    const { subject, html } = lastEmailCall()
    expect(subject).toContain('[DEV]')
    expect(html).toContain('>dev<')
  })

  it('leaves a release-build subject unmarked', async () => {
    await upload(mailableBindings())

    const { subject, html } = lastEmailCall()
    expect(subject).not.toContain('[DEV]')
    expect(html).toContain('>prod<')
  })

  it('carries the presigned download link and says when it stops working', async () => {
    await upload(
      mailableBindings({
        R2_ACCOUNT_ID: 'test-account',
        R2_ACCESS_KEY_ID: 'test-key-id',
        R2_SECRET_ACCESS_KEY: 'test-secret',
      }),
    )

    const { html } = lastEmailCall()
    expect(html).toContain('test-account.r2.cloudflarestorage.com')
    expect(html).toContain('X-Amz-Signature')
    // A stale click a week later should not be a mystery.
    expect(html).toContain('7 days')
  })

  it('still mails when the presigned link cannot be minted', async () => {
    // No R2 credentials configured: the note is worth reading even without a one-click download.
    await upload(mailableBindings())

    const { html } = lastEmailCall()
    expect(html).toContain('ERR-A2345')
    expect(html).toContain('admin')
  })

  it('makes the attached address the reply-to, so answering is a plain reply', async () => {
    await upload(mailableBindings(), { ...validMeta, email: 'reporter@example.com' })

    const { replyTo, html } = lastEmailCall()
    expect(replyTo).toBe('reporter@example.com')
    expect(html).toContain('reporter@example.com')
  })

  it('says there is no reply channel when the reporter attached no address', async () => {
    await upload(mailableBindings())

    const { replyTo, html } = lastEmailCall()
    expect(replyTo).toBeUndefined()
    expect(html).toContain('No reply-to address')
  })

  it('escapes HTML in the attached address', async () => {
    await upload(mailableBindings(), { ...validMeta, email: '"><i>x</i>@example.com' })

    expect(lastEmailCall().html).not.toContain('<i>x</i>')
  })

  it('stops mailing past the daily cap, after one notice saying so', async () => {
    const kv = createKv()
    // Start one slot below the cap so the run is short.
    await kv.put(errorReportEmailCountKey(todayUtc()), String(DAILY_ERROR_REPORT_EMAIL_CAP - 1))
    const bindings = mailableBindings({ ERROR_REPORT_META: kv })

    for (let i = 0; i < 4; i++) await upload(bindings)

    // Upload 1 mails the report, upload 2 mails the notice, uploads 3 and 4 mail nothing.
    expect(mockSend).toHaveBeenCalledTimes(2)
    const subjects = mockSend.mock.calls.map(([payload]) => payload.subject)
    expect(subjects[0]).toContain('ERR-A2345')
    expect(subjects[1]).toContain('suppressed')
  })

  it('counts the email cap on its own key, leaving the Discord allowance alone', async () => {
    const kv = createKv()
    await kv.put(errorReportEmailCountKey(todayUtc()), String(DAILY_ERROR_REPORT_EMAIL_CAP + 5))
    const posts: string[] = []
    globalThis.fetch = ((_url: string, init: { body: string }) => {
      posts.push(init.body)
      return Promise.resolve(new Response(null, { status: 204 }))
    }) as unknown as typeof fetch
    const bindings = mailableBindings({ ERROR_REPORT_META: kv, DISCORD_WEBHOOK_URL: 'https://discord.example/webhook' })

    await upload(bindings)

    expect(mockSend).not.toHaveBeenCalled()
    expect(posts).toHaveLength(1)
    expect(posts[0]).toContain('ERR-A2345')
  })

  it('still mails when the Discord side of the fan-out throws', async () => {
    // The two channels are independent. A KV or webhook problem on one must not silence the other.
    const kv = createKv()
    const failing = {
      get: (key: string) => (key.startsWith('notify_count:') ? Promise.reject(new Error('KV is down')) : kv.get(key)),
      put: (key: string, value: string) => kv.put(key, value),
      delete: (key: string) => kv.delete(key),
    } as unknown as KVNamespace
    const bindings = mailableBindings({
      ERROR_REPORT_META: failing,
      DISCORD_WEBHOOK_URL: 'https://discord.example/webhook',
    })

    await upload(bindings)

    expect(mockSend).toHaveBeenCalledOnce()
  })

  it('keeps the upload successful when Resend rejects the send', async () => {
    // The bundle is already in R2 and the client is waiting on a 200. A mail problem is ours.
    mockSend.mockImplementationOnce(() => Promise.resolve({ error: { message: 'Resend is unhappy' } }))
    const bucket = createR2()
    const bindings = mailableBindings({ ERROR_REPORTS_BUCKET: bucket })

    await upload(bindings)

    expect(bucket._store.size).toBe(1)
  })

  it('keeps the upload successful when the send throws outright', async () => {
    mockSend.mockImplementationOnce(() => Promise.reject(new Error('network is down')))
    const bucket = createR2()
    const bindings = mailableBindings({ ERROR_REPORTS_BUCKET: bucket })

    await upload(bindings)

    expect(bucket._store.size).toBe(1)
  })
})

describe('error-report amendment email', () => {
  /** Upload, then amend, returning the amend response. */
  async function uploadThenAmend(
    bindings: Record<string, unknown>,
    body: (amendKey: string | null) => unknown,
  ): Promise<Response> {
    const fd = buildMultipart(new Uint8Array([1, 2, 3]), validMeta)
    const uploaded = await app.request('/error-report', { method: 'POST', body: fd }, bindings)
    const { id, amendKey } = await uploaded.json<{ id: string; amendKey: string | null }>()
    mockSend.mockClear()
    return app.request(
      `/error-report/${id}/amend`,
      {
        method: 'POST',
        body: JSON.stringify(body(amendKey)),
        headers: { 'content-type': 'application/json' },
      },
      bindings,
    )
  }

  it('mails the amendment so it reaches a human like the report did', async () => {
    const res = await uploadThenAmend(mailableBindings(), (amendKey) => ({
      amendKey,
      note: 'One more thing: it only happens on Wi-Fi.',
    }))

    expect(res.status).toBe(200)
    expect(mockSend).toHaveBeenCalledOnce()
    const { to, subject, html } = lastEmailCall()
    expect(to).toBe('david@example.com')
    expect(subject).toContain('ERR-A2345')
    expect(html).toContain('One more thing: it only happens on Wi-Fi.')
  })

  it('sets the reply-to when the amendment carries an address', async () => {
    await uploadThenAmend(mailableBindings(), (amendKey) => ({ amendKey, email: 'later@example.com' }))

    expect(lastEmailCall().replyTo).toBe('later@example.com')
  })

  it('escapes HTML in the amendment note', async () => {
    await uploadThenAmend(mailableBindings(), (amendKey) => ({
      amendKey,
      note: '<script>alert(1)</script> A & B',
    }))

    const { html } = lastEmailCall()
    expect(html).not.toContain('<script>')
    expect(html).toContain('&lt;script&gt;')
  })

  it('still amends when no recipient is configured', async () => {
    const res = await uploadThenAmend(createBindings(), (amendKey) => ({ amendKey, note: 'no inbox here' }))

    expect(res.status).toBe(200)
    expect(mockSend).not.toHaveBeenCalled()
  })

  it('keeps the amendment successful when the send throws outright', async () => {
    // The sidecar is already in R2 and the client is waiting. A mail problem is ours.
    mockSend.mockImplementationOnce(() => Promise.reject(new Error('network is down')))

    const res = await uploadThenAmend(mailableBindings(), (amendKey) => ({ amendKey, note: 'still lands' }))

    expect(res.status).toBe(200)
  })
})
