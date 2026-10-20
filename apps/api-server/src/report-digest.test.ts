/**
 * The three-hour digest against a REAL SQLite (the production migrations): which rows it reads,
 * what the one email says, and that it stamps exactly what it sent.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'
import { handleReportDigest } from './report-digest'
import type { Bindings } from './types'

interface SentEmail {
  from: string
  to: string
  subject: string
  html: string
  replyTo?: string
}

type SendOutcome = { id: string; error?: never } | { id?: never; error: { message: string } }

const mockSend = vi.fn<(payload: SentEmail) => Promise<SendOutcome>>(() => Promise.resolve({ id: 'test-email-id' }))
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

function lastEmail(): SentEmail {
  const payload = mockSend.mock.lastCall?.[0]
  if (!payload) throw new Error('No email was sent')
  return payload
}

let db: SqliteD1

function testEnv(overrides: Partial<Bindings> = {}): Bindings {
  return {
    TELEMETRY_DB: db.d1,
    CRASH_NOTIFICATION_EMAIL: 'david@example.com',
    RESEND_API_KEY: 'test-resend-key',
    GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports',
    ...overrides,
  } as Bindings
}

function insertCrash(fields: Record<string, string | number | null> = {}): void {
  const row = {
    app_version: '0.52.0',
    os_version: '27.0',
    arch: 'aarch64',
    signal: 'SIGSEGV',
    top_function: 'cmdr::sync::run',
    build_mode: 'release',
    short_id: 'CRASH-A2345',
    email: null,
    panic_message: null,
    app_fate: 'ended',
    image_base: null,
    os_exception: null,
    os_frames: null,
    issue_number: null,
    ...fields,
  }
  const columns = Object.keys(row)
  db.sqlite
    .prepare(
      `INSERT INTO crash_reports (hashed_ip, backtrace, ${columns.join(', ')}) VALUES ('', 'bt', ${columns.map(() => '?').join(', ')})`,
    )
    .run(...Object.values(row))
}

function insertErrorReport(fields: Record<string, string | number | null> = {}): void {
  const row = {
    short_id: 'ERR-AB7PV',
    kind: 'user',
    build_mode: 'release',
    app_version: '0.52.0',
    os_version: '27.0',
    arch: 'aarch64',
    size_bytes: 2048,
    r2_key: 'error-reports/prod/2026-10-14/ERR-AB7PV-uuid.zip',
    user_note: 'Copying into a zip stalls',
    email: null,
    issue_number: null,
    ...fields,
  }
  const columns = Object.keys(row)
  db.sqlite
    .prepare(`INSERT INTO error_reports (${columns.join(', ')}) VALUES (${columns.map(() => '?').join(', ')})`)
    .run(...Object.values(row))
}

function insertFeedback(fields: Record<string, string | null> = {}): void {
  const row = {
    feedback: 'Please add a Date added column',
    email: null,
    app_version: '0.52.0',
    os_version: '15.7.9',
    build_mode: 'release',
    ...fields,
  }
  const columns = Object.keys(row)
  db.sqlite
    .prepare(`INSERT INTO feedback (${columns.join(', ')}) VALUES (${columns.map(() => '?').join(', ')})`)
    .run(...Object.values(row))
}

function insertAmendment(fields: Record<string, string | number | null> = {}): void {
  const row = {
    short_id: 'ERR-AB7PV',
    report_date: '2026-10-14',
    env: 'prod',
    r2_key: 'error-reports/prod/2026-10-14/ERR-AB7PV-uuid.zip',
    amendment_count: 1,
    note: 'It also happens on a local disk',
    email: null,
    ...fields,
  }
  const columns = Object.keys(row)
  db.sqlite
    .prepare(
      `INSERT INTO error_report_amendments (${columns.join(', ')}) VALUES (${columns.map(() => '?').join(', ')})`,
    )
    .run(...Object.values(row))
}

function unnotified(table: string): number {
  const row = db.sqlite.prepare(`SELECT COUNT(*) AS n FROM ${table} WHERE notified_at IS NULL`).get() as { n: number }
  return row.n
}

beforeEach(() => {
  db = createSqliteD1()
  mockSend.mockClear()
  mockSend.mockImplementation(() => Promise.resolve({ id: 'test-email-id' }))
})

describe('when the digest goes out', () => {
  it('sends nothing when nothing arrived', async () => {
    await handleReportDigest(testEnv())
    expect(mockSend).not.toHaveBeenCalled()
  })

  it('sends ONE email covering all four kinds, with the counts in the subject', async () => {
    insertCrash()
    insertErrorReport()
    insertErrorReport({ short_id: 'ERR-CD8QW', kind: 'auto', user_note: null })
    insertAmendment()
    for (let i = 0; i < 4; i++) insertFeedback()

    await handleReportDigest(testEnv())

    expect(mockSend).toHaveBeenCalledTimes(1)
    const email = lastEmail()
    expect(email.to).toBe('david@example.com')
    expect(email.subject).toBe('Cmdr digest: 1 crash, 2 errors, 1 note added, 4 feedback in the past 3h')
  })

  it('names only the kinds that arrived', async () => {
    insertFeedback()
    await handleReportDigest(testEnv())
    expect(lastEmail().subject).toBe('Cmdr digest: 1 feedback in the past 3h')
  })

  it('leaves debug builds out entirely', async () => {
    insertCrash({ build_mode: 'debug' })
    insertErrorReport({ build_mode: 'debug' })
    insertAmendment({ env: 'dev' })
    insertFeedback({ build_mode: 'debug' })

    await handleReportDigest(testEnv())

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('counts a crash from a client too old to send a build mode as a release build', async () => {
    insertCrash({ build_mode: null })
    await handleReportDigest(testEnv())
    expect(lastEmail().subject).toBe('Cmdr digest: 1 crash in the past 3h')
  })

  it('prefers FEEDBACK_NOTIFICATION_EMAIL over the crash recipient', async () => {
    insertFeedback()
    await handleReportDigest(testEnv({ FEEDBACK_NOTIFICATION_EMAIL: 'inbox@example.com' }))
    expect(lastEmail().to).toBe('inbox@example.com')
  })

  it('stays quiet without a recipient or a Resend key', async () => {
    insertFeedback()
    await handleReportDigest(testEnv({ CRASH_NOTIFICATION_EMAIL: undefined }))
    await handleReportDigest(testEnv({ RESEND_API_KEY: undefined }))
    expect(mockSend).not.toHaveBeenCalled()
    expect(unnotified('feedback')).toBe(1)
  })
})

describe('stamping', () => {
  it('stamps every row it carried, so the next tick sends nothing', async () => {
    insertCrash()
    insertErrorReport()
    insertAmendment()
    insertFeedback()

    await handleReportDigest(testEnv())
    await handleReportDigest(testEnv())

    expect(mockSend).toHaveBeenCalledTimes(1)
    for (const table of ['crash_reports', 'error_reports', 'error_report_amendments', 'feedback']) {
      expect(unnotified(table)).toBe(0)
    }
  })

  it('leaves everything unstamped when the send is rejected, so the next tick retries', async () => {
    mockSend.mockImplementation(() => Promise.resolve({ error: { message: 'Resend is unhappy' } }))
    insertCrash()
    insertFeedback()

    await expect(handleReportDigest(testEnv())).rejects.toThrow()

    expect(unnotified('crash_reports')).toBe(1)
    expect(unnotified('feedback')).toBe(1)
  })
})

describe('reply-to', () => {
  it('sets replyTo when exactly one person in the whole digest left an address', async () => {
    insertCrash({ email: 'tester@example.com' })
    insertFeedback()
    await handleReportDigest(testEnv())
    expect(lastEmail().replyTo).toBe('tester@example.com')
  })

  it('treats the same person twice as one person', async () => {
    insertErrorReport({ email: 'same@example.com' })
    insertAmendment({ email: 'same@example.com' })
    await handleReportDigest(testEnv())
    expect(lastEmail().replyTo).toBe('same@example.com')
  })

  it('omits replyTo with several addresses, and links each one instead', async () => {
    insertCrash({ email: 'one@example.com' })
    insertFeedback({ email: 'two@example.com' })
    await handleReportDigest(testEnv())
    const email = lastEmail()
    expect(email.replyTo).toBeUndefined()
    expect(email.html).toContain('mailto:one@example.com')
    expect(email.html).toContain('mailto:two@example.com')
  })
})

describe('board links', () => {
  it('links each entry to its card, and says so when the card is still owed', async () => {
    insertCrash({ issue_number: 57 })
    insertErrorReport({ issue_number: null })
    insertFeedback()

    await handleReportDigest(testEnv())

    const { html } = lastEmail()
    expect(html).toContain('https://github.com/vdavid/cmdr-reports/issues/57')
    expect(html).toContain('Not on the board yet')
  })

  it("links an amendment to its report's card", async () => {
    insertErrorReport({ issue_number: 61 })
    insertAmendment()
    db.sqlite.exec(`UPDATE error_reports SET notified_at = 'x'`)

    await handleReportDigest(testEnv())

    expect(lastEmail().html).toContain('https://github.com/vdavid/cmdr-reports/issues/61')
  })
})

describe('the crash section', () => {
  it('surfaces the panic message, and renders a signal crash without "null"', async () => {
    insertCrash({ signal: 'panic', panic_message: 'there is no reactor running' })
    insertCrash({ short_id: 'CRASH-B6789' })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).toContain('there is no reactor running')
    expect(html).not.toContain('>null<')
  })

  it('shows the image base only when nothing better explains the crash', async () => {
    insertCrash({ image_base: '0x104870000' })
    await handleReportDigest(testEnv())
    expect(lastEmail().html).toContain('image base 0x104870000')
  })

  it("renders macOS's exception line and the top six frames of its stack", async () => {
    const frames = Array.from({ length: 10 }, (_, i) => `WebKit frame${String(i)} + 0`)
    insertCrash({
      image_base: '0x104870000',
      os_exception: 'EXC_BAD_ACCESS (SIGSEGV), KERN_INVALID_ADDRESS at 0x10',
      os_frames: JSON.stringify(frames),
    })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).toContain('KERN_INVALID_ADDRESS at 0x10')
    expect(html).toContain('WebKit frame5 + 0')
    expect(html).not.toContain('WebKit frame6 + 0')
    expect(html).toContain('+ 4 more')
    expect(html).not.toContain('image base')
  })

  it('survives an os_frames value truncated mid-array at write time', async () => {
    insertCrash({ os_frames: '["frame one", "frame tw' })
    await handleReportDigest(testEnv())
    expect(lastEmail().subject).toBe('Cmdr digest: 1 crash in the past 3h')
  })

  it('escapes HTML in the panic message', async () => {
    insertCrash({ panic_message: '<script>alert(1)</script>' })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).not.toContain('<script>alert(1)</script>')
    expect(html).toContain('&lt;script&gt;')
  })

  it('names a survived panic apart from a crash, in the subject too', async () => {
    insertCrash({ app_fate: 'ended' })
    insertCrash({ app_fate: 'keptRunning' })
    await handleReportDigest(testEnv())
    const { html, subject } = lastEmail()
    expect(subject).toBe('Cmdr digest: 2 crashes (1 kept running) in the past 3h')
    expect(html).toContain('kept running')
    expect(html).toContain('crashed')
  })

  it('says so when the app kept running through every crash', async () => {
    insertCrash({ app_fate: 'keptRunning' })
    await handleReportDigest(testEnv())
    expect(lastEmail().subject).toBe('Cmdr digest: 1 crash, app kept running in the past 3h')
  })

  it('claims nothing about a row whose fate is unknown', async () => {
    insertCrash({ app_fate: null })
    await handleReportDigest(testEnv())
    expect(lastEmail().subject).toBe('Cmdr digest: 1 crash in the past 3h')
  })

  it('keeps the table rectangular: header cells, row cells, and the detail colspan agree', async () => {
    insertCrash({ panic_message: 'boom' })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    const headers = (html.match(/<th /g) ?? []).length
    const firstRow = /<tbody>\s*<tr>([\s\S]*?)<\/tr>/.exec(html)?.[1] ?? ''
    expect((firstRow.match(/<td /g) ?? []).length).toBe(headers)
    expect(html).toContain(`colspan="${String(headers)}"`)
  })
})

describe('the error report and amendment sections', () => {
  it('shows the note, the kind, and the machine facts', async () => {
    insertErrorReport()
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).toContain('Copying into a zip stalls')
    expect(html).toContain('ERR-AB7PV')
    expect(html).toContain('hand-written')
    expect(html).toContain('2.00 KB')
  })

  it('says an auto-send carries no note', async () => {
    insertErrorReport({ kind: 'auto', user_note: null })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).toContain('auto-sent')
    expect(html).toContain('Auto-sent, so no note.')
  })

  it('escapes HTML in the note', async () => {
    insertErrorReport({ user_note: '<img src=x onerror=alert(1)>' })
    await handleReportDigest(testEnv())
    expect(lastEmail().html).not.toContain('<img src=x')
  })

  it('says plainly when no download link could be minted', async () => {
    insertErrorReport()
    await handleReportDigest(testEnv())
    expect(lastEmail().html).toContain('No download link this time')
  })

  it("shows an amendment's note and which amendment it is", async () => {
    insertAmendment({ amendment_count: 3 })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).toContain('It also happens on a local disk')
    expect(html).toContain('amendment 3 on this report')
  })
})

describe('feedback with logs', () => {
  it('shows the logs on the feedback entry and mails no separate error report for them', async () => {
    insertFeedback({ error_report_id: 'ERR-AB7PV' })
    insertErrorReport({ short_id: 'ERR-AB7PV', user_note: 'Logs sent with in-app feedback' })

    await handleReportDigest(testEnv())

    const email = lastEmail()
    expect(email.subject).toBe('Cmdr digest: 1 feedback in the past 3h')
    expect(email.html).toContain('Please add a Date added column')
    expect(email.html).toContain('Logs: ERR-AB7PV')
    expect(email.html).toContain('2.00 KB')
    expect(email.html).not.toContain('Error reports')
    // Carried, so stamped: the next tick doesn't mail the report on its own.
    expect(unnotified('error_reports')).toBe(0)
    expect(unnotified('feedback')).toBe(0)
  })

  it("says so when the logs hadn't arrived yet", async () => {
    insertFeedback({ error_report_id: 'ERR-AB7PV' })

    await handleReportDigest(testEnv())

    expect(lastEmail().html).toContain('Logs: ERR-AB7PV')
    expect(lastEmail().html).toContain("hadn't arrived")
  })

  it('still mails logs on their own when their feedback went out in an earlier digest', async () => {
    insertFeedback({ error_report_id: 'ERR-AB7PV' })
    await handleReportDigest(testEnv())
    insertErrorReport({ short_id: 'ERR-AB7PV', user_note: 'Logs sent with in-app feedback' })

    await handleReportDigest(testEnv())

    expect(lastEmail().subject).toBe('Cmdr digest: 1 error in the past 3h')
  })
})

describe('ops alerts', () => {
  function insertOpsEvent(kind: string, message: string): void {
    db.sqlite.prepare(`INSERT INTO ops_events (kind, message) VALUES (?, ?)`).run(kind, message)
  }

  it('carries the ops alerts, and counts them in the subject', async () => {
    insertOpsEvent('eviction-blocked', 'Error report intake is PAUSED.')
    insertFeedback()

    await handleReportDigest(testEnv())

    const { subject, html } = lastEmail()
    expect(subject).toBe('Cmdr digest: 1 feedback, 1 ops alert in the past 3h')
    expect(html).toContain('Error report intake is PAUSED.')
    expect(unnotified('ops_events')).toBe(0)
  })

  it('leaves signups for the morning brief: no email for them alone, and no stamp', async () => {
    insertOpsEvent('beta-signup', 'beta')

    await handleReportDigest(testEnv())

    expect(mockSend).not.toHaveBeenCalled()
    expect(unnotified('ops_events')).toBe(1)
  })

  function insertRejections(kind: string, message: string, occurrences: number): void {
    db.sqlite
      .prepare(`INSERT INTO ops_events (kind, message, occurrences) VALUES (?, ?, ?)`)
      .run(kind, message, occurrences)
  }

  it('flags even one rejected admin request, with how many times it happened', async () => {
    insertRejections('admin-auth-rejected', 'GET /admin/stats was turned away.', 1)
    insertRejections('admin-auth-rejected', 'GET /admin/crashes was turned away.', 4)

    await handleReportDigest(testEnv())

    const { subject, html } = lastEmail()
    expect(subject).toBe('Cmdr digest: 2 ops alerts in the past 3h')
    expect(html).toContain('GET /admin/stats was turned away.')
    expect(html).toContain('4 times')
    expect(unnotified('ops_events')).toBe(0)
  })

  it('keeps quiet about one or two bad webhook signatures in a window, and closes the window on them', async () => {
    insertRejections('webhook-signature-rejected', 'A Paddle webhook signature did not verify.', 2)

    await handleReportDigest(testEnv())

    expect(mockSend).not.toHaveBeenCalled()
    // Stamped anyway, so two now and two next window never add up to an alert.
    expect(unnotified('ops_events')).toBe(0)
  })

  it('flags bad webhook signatures once a window holds three of them', async () => {
    insertRejections('webhook-signature-rejected', 'A Paddle webhook signature did not verify.', 2)
    insertRejections('webhook-signature-rejected', 'A GitHub webhook signature did not verify.', 1)

    await handleReportDigest(testEnv())

    const { html } = lastEmail()
    expect(html).toContain('A Paddle webhook signature did not verify.')
    expect(html).toContain('A GitHub webhook signature did not verify.')
    expect(unnotified('ops_events')).toBe(0)
  })

  it('escapes an alert message, which can quote a URL a stranger chose', async () => {
    insertOpsEvent('csp-violation', 'blocked https://x.test/<script>')
    await handleReportDigest(testEnv())
    expect(lastEmail().html).not.toContain('<script>')
  })
})

describe('the feedback section', () => {
  it('keeps the line breaks, escapes HTML, and carries non-Latin text intact', async () => {
    insertFeedback({ feedback: 'Line one\nLine two <b>bold</b> — köszönöm, 谢谢' })
    await handleReportDigest(testEnv())
    const { html } = lastEmail()
    expect(html).toContain('white-space: pre-wrap')
    expect(html).toContain('&lt;b&gt;bold&lt;/b&gt;')
    expect(html).toContain('köszönöm, 谢谢')
  })

  it('says so plainly when there is no reply-to address', async () => {
    insertFeedback()
    await handleReportDigest(testEnv())
    expect(lastEmail().html).toContain('No reply-to address')
  })
})
