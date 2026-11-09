/**
 * The morning brief against a REAL SQLite and a fake GitHub: what's waiting on the board, what
 * arrived in the last day, and the tripwires that say something didn't make it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'
/* eslint-disable-next-line no-restricted-imports -- Node-side test; reads the real wrangler.toml. */
import { readFileSync } from 'node:fs'
import { MORNING_CRON, handleMorningBrief, isMorningBriefTick } from './morning-brief'
import worker from './index'
import type { Bindings } from './types'

interface SentEmail {
  to: string
  subject: string
  html: string
}

const mockSend = vi.hoisted(() => vi.fn<typeof import('./email/send').sendEmail>(() => Promise.resolve()))
vi.mock('./email/send', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./email/send')>()),
  sendEmail: mockSend,
}))

function lastEmail(): SentEmail {
  const payload = mockSend.mock.lastCall?.[1]
  if (!payload) throw new Error('No email was sent')
  return payload
}

interface FakeIssue {
  number: number
  title: string
  labels: string[]
  created_at: string
}

/** GitHub's open-issues listing, filtered by the `labels` query parameter like the real one. */
function fakeGitHub(issues: FakeIssue[], options: { down?: boolean } = {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) => {
    const url = new URL(input as string)
    if (options.down) return Promise.resolve(new Response('{}', { status: 502 }))
    const label = url.searchParams.get('labels')
    const matching = issues
      .filter((issue) => !label || issue.labels.includes(label))
      .map((issue) => ({
        ...issue,
        html_url: `https://github.com/vdavid/cmdr-reports/issues/${String(issue.number)}`,
        labels: issue.labels.map((name) => ({ name })),
      }))
    return Promise.resolve(Response.json(matching))
  })
}

/** An R2 bucket whose `list` answers with the given keys under the asked prefix. */
function fakeBucket(keys: string[] = []): R2Bucket {
  return {
    list: (options: { prefix?: string }) =>
      Promise.resolve({
        objects: keys.filter((key) => key.startsWith(options.prefix ?? '')).map((key) => ({ key })),
        truncated: false,
      }),
  } as unknown as R2Bucket
}

let db: SqliteD1

function testEnv(overrides: Partial<Bindings> = {}): Bindings {
  return {
    TELEMETRY_DB: db.d1,
    ERROR_REPORTS_BUCKET: fakeBucket(),
    CRASH_NOTIFICATION_EMAIL: 'david@example.com',
    SMTP2GO_API_KEY: 'test-smtp2go-key',
    GITHUB_ISSUES_TOKEN: 'ghp_test',
    GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports',
    ...overrides,
  } as Bindings
}

/** `YYYY-MM-DD HH:MM:SS`, `hours` ago. */
function hoursAgo(hours: number): string {
  return new Date(Date.now() - hours * 3_600_000).toISOString().slice(0, 19).replace('T', ' ')
}

function yesterdayUtc(): string {
  return new Date(Date.now() - 86_400_000).toISOString().slice(0, 10)
}

beforeEach(() => {
  db = createSqliteD1()
  mockSend.mockClear()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('isMorningBriefTick', () => {
  it('fires on the tick that is 06:30 in Stockholm, summer and winter alike', () => {
    // CEST (UTC+2): 04:30 UTC is 06:30 local; 05:30 UTC would be 07:30.
    expect(isMorningBriefTick(new Date('2026-07-14T04:30:00Z'))).toBe(true)
    expect(isMorningBriefTick(new Date('2026-07-14T05:30:00Z'))).toBe(false)
    // CET (UTC+1): the other way round.
    expect(isMorningBriefTick(new Date('2026-01-14T05:30:00Z'))).toBe(true)
    expect(isMorningBriefTick(new Date('2026-01-14T04:30:00Z'))).toBe(false)
  })
})

describe('handleMorningBrief', () => {
  it('leads with who is waiting on a reply and what nobody has looked at yet', async () => {
    fakeGitHub([
      {
        number: 56,
        title: 'Feedback #14 on 0.52.0',
        labels: ['in-app-feedback', 'needs-reply'],
        created_at: hoursAgo(30),
      },
      { number: 57, title: 'Crash in cmdr::x', labels: ['crash-report', 'unprocessed'], created_at: hoursAgo(2) },
      {
        number: 58,
        title: 'Zip copy loses the file',
        labels: ['error-report', 'needs-reply'],
        created_at: hoursAgo(5),
      },
    ])

    await handleMorningBrief(testEnv())

    const { subject, html, to } = lastEmail()
    expect(to).toBe('david@example.com')
    expect(subject).toBe('Cmdr morning: 2 waiting on a reply, 1 unprocessed')
    expect(html).toContain('Zip copy loses the file')
    expect(html).toContain('https://github.com/vdavid/cmdr-reports/issues/56')
    expect(html).toContain('Crash in cmdr::x')
  })

  it("counts the last day's arrivals, signups included, leaving debug builds out", async () => {
    fakeGitHub([])
    db.sqlite.exec(`
      INSERT INTO crash_reports (hashed_ip, created_at, app_version, os_version, arch, signal, top_function, backtrace, build_mode)
        VALUES ('', '${new Date().toISOString()}', '0.52.0', '27.0', 'aarch64', 'SIGSEGV', 'x', 'bt', 'release'),
               ('', '${new Date().toISOString()}', '0.52.0', '27.0', 'aarch64', 'SIGSEGV', 'x', 'bt', 'debug');
      INSERT INTO error_reports (short_id, created_at, kind, build_mode, app_version, os_version, arch, size_bytes, r2_key)
        VALUES ('ERR-AAAAA', '${hoursAgo(3)}', 'user', 'release', '0.52.0', '27.0', 'aarch64', 1, 'k1'),
               ('ERR-BBBBB', '${hoursAgo(3)}', 'auto', 'release', '0.52.0', '27.0', 'aarch64', 1, 'k2'),
               ('ERR-CCCCC', '${hoursAgo(30)}', 'auto', 'release', '0.52.0', '27.0', 'aarch64', 1, 'k3');
      INSERT INTO feedback (created_at, feedback, app_version, os_version, build_mode)
        VALUES ('${hoursAgo(1)}', 'hi', '0.52.0', '27.0', 'release');
      INSERT INTO ops_events (created_at, kind, message)
        VALUES ('${hoursAgo(4)}', 'beta-signup', 'x'), ('${hoursAgo(4)}', 'newsletter-signup', 'x'),
               ('${hoursAgo(4)}', 'beta-signup', 'x'), ('${hoursAgo(4)}', 'csp-violation', 'x'),
               ('${hoursAgo(4)}', 'subprocessor-updates-signup', 'x');
    `)

    await handleMorningBrief(testEnv())

    const { subject, html } = lastEmail()
    expect(subject).toBe('Cmdr morning: nothing waiting; in the last day 1 crash, 2 errors, 1 feedback')
    expect(html).toContain('1 hand-written, 1 auto-sent')
    expect(html).toContain('2 beta signups')
    expect(html).toContain('1 newsletter signup')
    expect(html).toContain('1 subprocessor updates signup')
    // A signup is news, never an alert: only the CSP violation counts here.
    expect(html).toContain('1 ops alert')
  })

  it('says plainly when the day was quiet, and still sends: the numbers are the point', async () => {
    fakeGitHub([])

    await handleMorningBrief(testEnv())

    expect(lastEmail().subject).toBe('Cmdr morning: nothing waiting, and a quiet day')
  })

  it('names reports still not on the board, the tripwire for a stuck outbox', async () => {
    fakeGitHub([])
    db.sqlite.exec(`
      INSERT INTO feedback (created_at, feedback, app_version, os_version, build_mode)
        VALUES ('${hoursAgo(50)}', 'old and unfiled', '0.52.0', '27.0', 'release');
    `)

    await handleMorningBrief(testEnv())

    expect(lastEmail().html).toContain('1 report still waiting for its card')
  })

  it("flags when R2 holds more of yesterday's bundles than the outbox knows about", async () => {
    fakeGitHub([])
    const day = yesterdayUtc()
    const bucket = fakeBucket([
      `error-reports/prod/${day}/ERR-AAAAA-1.zip`,
      `error-reports/prod/${day}/ERR-AAAAA-1.amend.json`,
      `error-reports/prod/${day}/ERR-BBBBB-2.zip`,
      `error-reports/dev/${day}/ERR-DDDDD-3.zip`,
    ])
    db.sqlite.exec(`
      INSERT INTO error_reports (short_id, created_at, kind, build_mode, app_version, os_version, arch, size_bytes, r2_key)
        VALUES ('ERR-AAAAA', '${day} 10:00:00', 'user', 'release', '0.52.0', '27.0', 'aarch64', 1, 'k');
    `)

    await handleMorningBrief(testEnv({ ERROR_REPORTS_BUCKET: bucket }))

    expect(lastEmail().html).toContain(`R2 holds 2 bundles from ${day}, but only 1 reached the outbox`)
  })

  it('still sends, saying so, when GitHub is unreachable', async () => {
    fakeGitHub([], { down: true })

    await handleMorningBrief(testEnv())

    expect(lastEmail().html).toContain("Couldn't read the board")
  })

  it('carries the CI line, saying what GitHub would not answer (`ci-brief.test.ts` covers the numbers)', async () => {
    fakeGitHub([], { down: true })

    await handleMorningBrief(testEnv({ CI_HEALTH_REPO: 'getcmdr/cmdr' }))

    expect(lastEmail().html).toContain(
      'CI: main not checked yet · nightly not checked yet · couldn&#39;t read lands from GitHub · cache size unknown',
    )
  })

  it('leaves the CI line out without CI_HEALTH_REPO', async () => {
    fakeGitHub([])
    await handleMorningBrief(testEnv())
    expect(lastEmail().html).not.toContain('CI: main')
  })

  it('stays quiet without a recipient or an SMTP2Go key', async () => {
    fakeGitHub([])
    await handleMorningBrief(testEnv({ SMTP2GO_API_KEY: undefined }))
    expect(mockSend).not.toHaveBeenCalled()
  })
})

describe('the morning cron trigger', () => {
  it('is the trigger wrangler.toml actually schedules, so the dispatch can never miss it', () => {
    const toml = readFileSync(new URL('../wrangler.toml', import.meta.url), 'utf8')
    expect(toml).toContain(`"${MORNING_CRON}"`)
  })

  it('runs only the brief, and only on the 06:30 Stockholm tick', async () => {
    fakeGitHub([])
    const env = testEnv()

    await worker.scheduled(
      { cron: '30 4,5 * * *', scheduledTime: Date.parse('2026-07-14T05:30:00Z') } as ScheduledEvent,
      env,
    )
    expect(mockSend).not.toHaveBeenCalled()

    await worker.scheduled(
      { cron: '30 4,5 * * *', scheduledTime: Date.parse('2026-07-14T04:30:00Z') } as ScheduledEvent,
      env,
    )
    expect(mockSend).toHaveBeenCalledTimes(1)
    expect(lastEmail().subject).toMatch(/^Cmdr morning: /)
  })
})
