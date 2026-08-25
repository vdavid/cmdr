/**
 * The feedback digest email: what `handleFeedbackNotifications` reads out of `feedback` and how
 * `sendFeedbackNotificationEmail` renders it. Split from `scheduled.test.ts` for the same reason
 * the crash email is: its output is a document, so it grows on its own axis.
 */

import { describe, expect, it, vi, beforeEach } from 'vitest'
import { handleFeedbackNotifications } from './index'
import { createBaseEnv, createMockD1 } from './cron-test-helpers'

/** The fields of the Resend payload these tests read back. */
interface SentEmail {
  from: string
  to: string
  subject: string
  html: string
  replyTo?: string | string[]
}

/** Resend reports a rejected send in the response rather than throwing, so both shapes are valid. */
type SendOutcome = { id: string; error?: never } | { id?: never; error: { message: string } }

// Mock Resend: intercept email sends
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

interface FeedbackDbRow {
  id: number
  created_at: string
  feedback: string
  email: string | null
  app_version: string
  os_version: string
  build_mode: string | null
}

/** Build the mock D1 response for the un-notified-feedback SELECT, filling in defaults per row. */
function feedbackRows(rows: Partial<FeedbackDbRow>[]): Map<string, unknown> {
  const results = rows.map((row, index) => ({
    id: index + 1,
    created_at: '2026-08-20 10:00:00',
    feedback: 'Cmdr is lovely, thanks.',
    email: null,
    app_version: '1.2.3',
    os_version: '15.3',
    build_mode: 'release',
    ...row,
  }))
  return new Map<string, unknown>([['FROM feedback', { results }]])
}

beforeEach(() => {
  mockSend.mockClear()
})

describe('handleFeedbackNotifications', () => {
  it('does not send email when nothing is un-notified', async () => {
    const { db } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('skips when no recipient is configured', async () => {
    const { db, calls } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ CRASH_NOTIFICATION_EMAIL: undefined, TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
    // Should not even query D1
    expect(calls).toHaveLength(0)
  })

  it('skips when RESEND_API_KEY is not set', async () => {
    const { db, calls } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ RESEND_API_KEY: undefined, TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
    expect(calls).toHaveLength(0)
  })

  it('prefers FEEDBACK_NOTIFICATION_EMAIL over the crash recipient', async () => {
    const { db } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ TELEMETRY_DB: db, FEEDBACK_NOTIFICATION_EMAIL: 'feedback@example.com' })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().to).toBe('feedback@example.com')
  })

  it('falls back to the crash recipient so no new secret is needed', async () => {
    const { db } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().to).toBe('test@example.com')
  })

  it('sends one email for the whole batch, with a plural subject', async () => {
    const { db } = createMockD1(
      feedbackRows([
        { feedback: 'The dual pane is great.' },
        { feedback: 'Please add tabs.' },
        { feedback: 'Found a snag in search.' },
      ]),
    )
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(mockSend).toHaveBeenCalledOnce()
    const { subject, html, from } = lastEmailCall()
    expect(subject).toBe('Cmdr: 3 new feedback messages')
    expect(from).toBe('Cmdr Feedback <noreply@getcmdr.com>')
    expect(html).toContain('The dual pane is great.')
    expect(html).toContain('Please add tabs.')
    expect(html).toContain('Found a snag in search.')
  })

  it('sends a singular subject for one message', async () => {
    const { db } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().subject).toBe('Cmdr: 1 new feedback message')
  })

  it('shows the app version, the OS version, and the build-mode chip per card', async () => {
    const { db } = createMockD1(
      feedbackRows([
        { app_version: '1.2.3', os_version: '15.3', build_mode: 'release' },
        { app_version: '1.3.0', os_version: '14.5', build_mode: 'debug' },
        { app_version: '1.1.0', os_version: '13.2', build_mode: null },
      ]),
    )
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    const { html } = lastEmailCall()
    expect(html).toContain('1.2.3')
    expect(html).toContain('15.3')
    expect(html).toContain('>prod<')
    expect(html).toContain('>dev<')
    // A row with no build mode claims nothing rather than guessing `prod`.
    expect(html).toContain('>?<')
  })

  it('keeps the sender line-breaks readable', async () => {
    // Feedback is prose. Collapsing the writer's paragraphs into one run is how a long,
    // carefully structured message becomes unreadable in the one place it gets read.
    const { db } = createMockD1(feedbackRows([{ feedback: 'First thought.\n\nSecond thought.' }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    const { html } = lastEmailCall()
    expect(html).toContain('white-space: pre-wrap')
    expect(html).toContain('First thought.\n\nSecond thought.')
  })

  it('escapes HTML in the message', async () => {
    const { db } = createMockD1(
      feedbackRows([{ feedback: '<script>alert(1)</script> A & B <img src=x onerror=alert(2)>' }]),
    )
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    const { html } = lastEmailCall()
    expect(html).not.toContain('<script>')
    expect(html).not.toContain('<img src=x')
    expect(html).toContain('&lt;script&gt;')
    expect(html).toContain('A &amp; B')
  })

  it('escapes HTML in the reply-to address and the version columns', async () => {
    const { db } = createMockD1(
      feedbackRows([{ email: '"><script>x</script>@example.com', app_version: '<b>1.2.3</b>' }]),
    )
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    const { html } = lastEmailCall()
    expect(html).not.toContain('<script>')
    expect(html).not.toContain('<b>1.2.3</b>')
  })

  it('carries non-Latin text through intact', async () => {
    const hungarian = 'Nagyon jó a program, köszönöm! Az árvíztűrő tükörfúrógép is működik. 🙂'
    const { db } = createMockD1(feedbackRows([{ feedback: hungarian }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().html).toContain(hungarian)
  })

  it('links the reply-to address, and says so plainly when there is none', async () => {
    const { db } = createMockD1(feedbackRows([{ email: 'user@example.com' }, { email: null }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    const { html } = lastEmailCall()
    expect(html).toContain('mailto:user@example.com')
    // No em dash placeholder: the house style bans them, and a dash says nothing.
    expect(html).not.toContain('—')
  })

  it('sets replyTo when exactly one message carries an address', async () => {
    const { db } = createMockD1(feedbackRows([{ email: 'only@example.com' }, { email: null }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().replyTo).toBe('only@example.com')
  })

  it('omits replyTo when no message carries an address', async () => {
    const { db } = createMockD1(feedbackRows([{ email: null }, { email: null }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().replyTo).toBeUndefined()
  })

  it('omits replyTo when several messages carry an address', async () => {
    // Replying would silently answer one of them; the per-card `mailto:` links stay honest.
    const { db } = createMockD1(feedbackRows([{ email: 'a@example.com' }, { email: 'b@example.com' }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    expect(lastEmailCall().replyTo).toBeUndefined()
  })

  it('stamps notified_at only after the send succeeds', async () => {
    const { db, calls } = createMockD1(feedbackRows([{}, {}]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    let stampedBeforeSend: boolean | undefined
    mockSend.mockImplementationOnce(() => {
      stampedBeforeSend = calls.some((call) => call.sql.includes('UPDATE feedback'))
      return Promise.resolve({ id: 'test-email-id' })
    })

    await handleFeedbackNotifications(env as never)

    expect(stampedBeforeSend).toBe(false)
    const updateCall = calls.find((call) => call.sql.includes('UPDATE feedback'))
    expect(updateCall).toBeDefined()
    // Bindings: [now, ...ids]
    expect(updateCall?.bindings).toHaveLength(3)
    expect(updateCall?.bindings[1]).toBe(1)
    expect(updateCall?.bindings[2]).toBe(2)
  })

  it('leaves the rows un-notified when the send is rejected, so the next tick retries', async () => {
    const { db, calls } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    mockSend.mockImplementationOnce(() => Promise.resolve({ error: { message: 'Resend is unhappy' } }))

    await expect(handleFeedbackNotifications(env as never)).rejects.toThrow()

    expect(calls.find((call) => call.sql.includes('UPDATE feedback'))).toBeUndefined()
  })

  it('reads the newest un-notified rows', async () => {
    const { db, calls } = createMockD1(feedbackRows([{}]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleFeedbackNotifications(env as never)

    const selectCall = calls.find((call) => call.sql.includes('FROM feedback'))
    expect(selectCall?.sql).toContain('notified_at IS NULL')
    expect(selectCall?.sql).toContain('ORDER BY created_at DESC')
  })
})
