/**
 * The nightly crash email: what `handleCrashNotifications` reads out of `crash_reports` and how
 * `sendCrashNotificationEmail` renders it. Split from `scheduled.test.ts` because it's the one
 * cron job whose output is a document rather than a DB write, so it grows on its own axis.
 */

import { describe, expect, it, vi, beforeEach } from 'vitest'
import { handleCrashNotifications } from './index'
import { createBaseEnv, createMockD1 } from './cron-test-helpers'

/** The fields of the Resend payload these tests read back. */
interface SentEmail {
  from: string
  to: string
  subject: string
  html: string
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

/** Minimal un-notified `crash_reports` rows, for the tests that care about ordering, not rendering. */
function crashRows(overrides: Record<string, unknown>[]): Map<string, unknown> {
  return new Map<string, unknown>([
    [
      'SELECT id',
      {
        results: overrides.map((o, i) => ({
          id: i + 1,
          app_version: '1.0.0',
          os_version: '15.3',
          arch: 'arm64',
          signal: 'SIGSEGV',
          top_function: 'cmdr::sync::run',
          created_at: '2026-03-23T10:00:00Z',
          build_mode: 'release',
          short_id: 'CRASH-A2345',
          ...o,
        })),
      },
    ],
  ])
}

beforeEach(() => {
  mockSend.mockClear()
})

describe('handleCrashNotifications', () => {
  it('surfaces the panic message in the email', async () => {
    // The message is what makes a crash diagnosable at a glance; without it the email
    // only names a function. Rows from older clients have none and must still render.
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'panic',
              top_function: 'cmdr_lib::file_system::listing::caching::notify_directory_changed',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-26SBB',
              email: null,
              panic_message: 'there is no reactor running, must be called from the context of a Tokio 1.x runtime',
            },
            {
              id: 2,
              app_version: '0.9.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T11:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-B6789',
              email: null,
              panic_message: null,
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const emailCall = lastEmailCall()
    expect(emailCall.html).toContain('there is no reactor running')
    // The signal-crash row has no message and renders an em-dash placeholder, not "null".
    expect(emailCall.html).not.toContain('>null<')
  })

  it('escapes HTML in the panic message', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'panic',
              top_function: 'cmdr_lib::x::y',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
              email: null,
              panic_message: '<img src=x onerror=alert(1)>',
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(lastEmailCall().html).not.toContain('<img src=x')
    expect(lastEmailCall().html).toContain('&lt;img src=x')
  })

  it('sends one row per un-notified crash report', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
            },
            {
              id: 2,
              app_version: '1.0.1',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T11:00:00Z',
              build_mode: 'debug',
              short_id: 'CRASH-B6789',
            },
            {
              id: 3,
              app_version: '1.0.0',
              os_version: '14.5',
              arch: 'x86_64',
              signal: 'SIGABRT',
              top_function: 'cmdr_lib::indexer::build',
              created_at: '2026-03-23T12:00:00Z',
              build_mode: null,
              short_id: null,
            },
          ],
        },
      ],
    ])
    const { db, calls } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    // Verify email was sent
    expect(mockSend).toHaveBeenCalledOnce()
    const emailCall = lastEmailCall()
    expect(emailCall.subject).toBe('Cmdr: 3 new crash reports')
    expect(emailCall.to).toBe('test@example.com')
    expect(emailCall.from).toBe('Cmdr Crash Alerts <noreply@getcmdr.com>')
    // Per-row rendering: each crash shows up with its top_function and short id.
    expect(emailCall.html).toContain('cmdr::sync::run')
    expect(emailCall.html).toContain('cmdr_lib::indexer::build')
    expect(emailCall.html).toContain('CRASH-A2345')
    expect(emailCall.html).toContain('CRASH-B6789')
    // Env column shows friendly labels.
    expect(emailCall.html).toContain('>prod<')
    expect(emailCall.html).toContain('>dev<')
    // Row 3 has neither build_mode nor short_id; both render as `?`.
    const questionMarkCells = emailCall.html.match(/>\?</g)?.length ?? 0
    expect(questionMarkCells).toBeGreaterThanOrEqual(2)

    // Verify rows were marked as notified (UPDATE query was called)
    const updateCall = calls.find((c) => c.sql.includes('UPDATE crash_reports'))
    expect(updateCall).toBeDefined()
    // Bindings: [now, ...ids]
    const bindings = updateCall?.bindings ?? []
    expect(bindings.length).toBe(4) // now + 3 ids
    expect(bindings[1]).toBe(1)
    expect(bindings[2]).toBe(2)
    expect(bindings[3]).toBe(3)
  })

  it('stamps notified_at only after the send succeeds', async () => {
    const { db, calls } = createMockD1(crashRows([{ id: 1 }, { id: 2 }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    let stampedBeforeSend: boolean | undefined
    mockSend.mockImplementationOnce(() => {
      stampedBeforeSend = calls.some((call) => call.sql.includes('UPDATE crash_reports'))
      return Promise.resolve({ id: 'test-email-id' })
    })

    await handleCrashNotifications(env as never)

    expect(stampedBeforeSend).toBe(false)
    expect(calls.find((call) => call.sql.includes('UPDATE crash_reports'))).toBeDefined()
  })

  it('leaves the rows un-notified when the send is rejected, so the next tick retries', async () => {
    const { db, calls } = createMockD1(crashRows([{ id: 1 }]))
    const env = createBaseEnv({ TELEMETRY_DB: db })

    mockSend.mockImplementationOnce(() => Promise.resolve({ error: { message: 'API key is invalid' } }))

    await expect(handleCrashNotifications(env as never)).rejects.toThrow()

    expect(calls.find((call) => call.sql.includes('UPDATE crash_reports'))).toBeUndefined()
  })

  it('surfaces the attached contact email so the maintainer can reply', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
              email: 'tester@example.com',
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const emailCall = lastEmailCall()
    expect(emailCall.html).toContain('tester@example.com')
  })

  it('sends singular subject for one crash report', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const emailCall = lastEmailCall()
    expect(emailCall.subject).toBe('Cmdr: 1 new crash report')
  })

  /**
   * A crash the app died of and a background panic it walked away from are different
   * severities, and the email is the only place that ranking is read. Without the fate the
   * two are indistinguishable in a row that says `signal: panic` either way.
   */
  it('ranks a survived panic apart from a crash the app went down with', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'panic',
              top_function: 'cmdr_lib::watcher::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
              email: null,
              panic_message: 'index out of bounds',
              app_fate: 'keptRunning',
            },
            {
              id: 2,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'panic',
              top_function: 'cmdr_lib::sync::run',
              created_at: '2026-03-23T11:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-B6789',
              email: null,
              panic_message: 'index out of bounds',
              app_fate: 'ended',
            },
          ],
        },
      ],
    ])
    const { db, calls } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const selectCall = calls.find((c) => c.sql.includes('SELECT id'))
    expect(selectCall?.sql).toContain('app_fate')

    const { html, subject } = lastEmailCall()
    expect(html).toContain('kept running')
    expect(html).toContain('crashed')
    // The subject is all you see without opening, so the mix belongs there too.
    expect(subject).toBe('Cmdr: 2 new crash reports (1 kept running)')
  })

  it('says in the subject when the app kept running through every report', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'panic',
              top_function: 'cmdr_lib::watcher::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
              email: null,
              panic_message: 'index out of bounds',
              app_fate: 'keptRunning',
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(lastEmailCall().subject).toBe('Cmdr: 1 new crash report, the app kept running')
  })

  it('claims nothing about a row written before the column existed', async () => {
    // An old row reads NULL. Rendering that as "crashed" would invent the one fact the
    // column was added to stop guessing at.
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
              email: null,
              panic_message: null,
              app_fate: null,
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const { html, subject } = lastEmailCall()
    expect(html).not.toContain('kept running')
    expect(html).not.toContain('>crashed<')
    // An unknown fate is not a survivor, so the subject stays the plain one.
    expect(subject).toBe('Cmdr: 1 new crash report')
  })

  it('keeps the table rectangular: header cells, row cells, and the message colspan agree', async () => {
    // A column added to one of the three and not the others renders a skewed table in every
    // mail client, and nothing else here would notice.
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'panic',
              top_function: 'cmdr_lib::watcher::run',
              created_at: '2026-03-23T10:00:00Z',
              build_mode: 'release',
              short_id: 'CRASH-A2345',
              email: 'tester@example.com',
              panic_message: 'index out of bounds',
              app_fate: 'ended',
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const { html } = lastEmailCall()
    const headerCells = html.match(/<th\b/g)?.length ?? 0
    const bodyCells = html.match(/<td\b(?![^>]*colspan)/g)?.length ?? 0
    const colspan = Number(/colspan="(\d+)"/.exec(html)?.[1] ?? 0)

    expect(headerCells).toBe(8)
    expect(bodyCells).toBe(headerCells)
    expect(colspan).toBe(headerCells)
  })

  it('does not send email when there are no un-notified crashes', async () => {
    const { db } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('skips when CRASH_NOTIFICATION_EMAIL is not set', async () => {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ CRASH_NOTIFICATION_EMAIL: undefined, TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
    // Should not even query D1
    expect(calls).toHaveLength(0)
  })

  it('skips when RESEND_API_KEY is not set', async () => {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ RESEND_API_KEY: undefined, TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
    expect(calls).toHaveLength(0)
  })
})
