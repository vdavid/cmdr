/**
 * The report outbox against a REAL SQLite (the production migrations) and a fake GitHub. What's at
 * stake is a row a WHERE clause quietly skips, so the SQL has to run for real.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { applyMigrationsFrom, createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'
import { fileCrashCards, fileErrorReportCards, fileFeedbackCards, postAmendmentsToBoard } from './report-outbox'
import { personalCommentExpiry } from './report-cards'
import type { Bindings } from './types'

interface CreatedIssue {
  number: number
  title: string
  body: string
  labels: string[]
}

function json(value: unknown, status = 200): Promise<Response> {
  return Promise.resolve(
    new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json' } }),
  )
}

/** A fake of the four GitHub calls the outbox makes, recording what it was asked to write. */
function fakeGitHub(options: { failCreates?: boolean; isPrivate?: boolean } = {}) {
  const issues: CreatedIssue[] = []
  const comments: { issueNumber: number; body: string }[] = []
  const labelAdds: { issueNumber: number; labels: string[] }[] = []
  let next = 100

  const createIssue = (body: Record<string, unknown>) => {
    if (options.failCreates) return json({ message: 'boom' }, 502)
    const issue = {
      number: next++,
      title: String(body['title']),
      body: String(body['body']),
      labels: body['labels'] as string[],
    }
    issues.push(issue)
    return json({ number: issue.number })
  }

  /** Writes to one issue: a comment or a label add. Null when `url` is neither. */
  const writeToIssue = (url: string, body: Record<string, unknown>) => {
    const match = /\/issues\/(\d+)\/(comments|labels)$/.exec(url)
    if (!match) return null
    const issueNumber = Number(match[1])
    if (match[2] === 'comments') {
      comments.push({ issueNumber, body: String(body['body']) })
      return json({ id: comments.length })
    }
    labelAdds.push({ issueNumber, labels: body['labels'] as string[] })
    return json([])
  }

  // The outbox always passes the URL as a string.
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
    const url = input as string
    const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as Record<string, unknown>) : {}
    if (init?.method !== 'POST') {
      return url.endsWith('/repos/vdavid/cmdr-reports') ? json({ private: options.isPrivate ?? true }) : json({}, 404)
    }
    if (url.endsWith('/repos/vdavid/cmdr-reports/issues')) return createIssue(body)
    return writeToIssue(url, body) ?? json({ message: `unexpected POST ${url}` }, 404)
  })

  return { issues, comments, labelAdds }
}

/** A KV good enough for the daily caps and the issue-number pointers. */
function memoryKv(): KVNamespace {
  const store = new Map<string, string>()
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
}

/** An R2 bucket that knows exactly the bundles it's given. */
function fakeBucket(objects: Record<string, Record<string, string>> = {}): R2Bucket {
  return {
    head: (key: string) =>
      Promise.resolve(
        key in objects
          ? { size: 2048, uploaded: new Date('2026-10-01T10:00:00Z'), customMetadata: objects[key] }
          : null,
      ),
  } as unknown as R2Bucket
}

let db: SqliteD1

function testEnv(overrides: Partial<Bindings> = {}): Bindings {
  return {
    TELEMETRY_DB: db.d1,
    ERROR_REPORT_META: memoryKv(),
    ERROR_REPORTS_BUCKET: fakeBucket(),
    GITHUB_ISSUES_TOKEN: 'ghp_test',
    GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports',
    ...overrides,
  } as Bindings
}

function insertCrash(fields: { buildMode?: string | null; email?: string | null; issueNumber?: number | null } = {}) {
  db.sqlite
    .prepare(
      `INSERT INTO crash_reports (hashed_ip, app_version, os_version, arch, signal, top_function, backtrace, build_mode, short_id, email, panic_message, app_fate, issue_number)
         VALUES ('', '0.52.0', '27.0', 'aarch64', 'SIGABRT', 'cmdr::listing::sort', 'bt', ?, 'CRASH-AB234', ?, 'index out of bounds', 'ended', ?)`,
    )
    .run(
      fields.buildMode === undefined ? 'release' : fields.buildMode,
      fields.email ?? null,
      fields.issueNumber ?? null,
    )
}

function insertErrorReport(
  fields: { shortId?: string; kind?: string; buildMode?: string; note?: string | null; email?: string | null } = {},
) {
  db.sqlite
    .prepare(
      `INSERT INTO error_reports (short_id, created_at, kind, build_mode, app_version, os_version, arch, size_bytes, r2_key, user_note, email)
         VALUES (?, '2026-10-01 10:00:00', ?, ?, '0.52.0', '27.0', 'aarch64', 2048, ?, ?, ?)`,
    )
    .run(
      fields.shortId ?? 'ERR-AB7PV',
      fields.kind ?? 'user',
      fields.buildMode ?? 'release',
      `error-reports/prod/2026-10-01/${fields.shortId ?? 'ERR-AB7PV'}-uuid.zip`,
      fields.note === undefined ? 'I copied a file into a zip and it vanished' : fields.note,
      fields.email ?? null,
    )
}

function insertFeedback(fields: { email?: string | null; buildMode?: string; errorReportId?: string | null } = {}) {
  db.sqlite
    .prepare(
      `INSERT INTO feedback (created_at, feedback, email, app_version, os_version, build_mode, error_report_id)
         VALUES ('2026-10-01 10:00:00', 'Please add a Date added column', ?, '0.52.0', '15.7.9', ?, ?)`,
    )
    .run(fields.email ?? null, fields.buildMode ?? 'release', fields.errorReportId ?? null)
}

function issueNumbers(table: string): (number | null)[] {
  const rows = db.sqlite.prepare(`SELECT issue_number FROM ${table} ORDER BY id`).all() as {
    issue_number: number | null
  }[]
  return rows.map((row) => row.issue_number)
}

beforeEach(() => {
  db = createSqliteD1()
  vi.spyOn(console, 'error').mockImplementation(() => {})
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('the migration', () => {
  it('stamps every pre-existing crash and feedback row as card-less, so the first tick files no history', async () => {
    db = createSqliteD1({ before: '0022' })
    db.sqlite.exec(
      `INSERT INTO crash_reports (hashed_ip, app_version, os_version, arch, signal, top_function, backtrace)
         VALUES ('', '0.51.0', '27.0', 'aarch64', 'SIGSEGV', 'old', 'bt');
       INSERT INTO feedback (feedback, app_version, os_version) VALUES ('old message', '0.51.0', '27.0');`,
    )
    applyMigrationsFrom(db.sqlite, '0022')
    const github = fakeGitHub()

    await fileCrashCards(testEnv())
    await fileFeedbackCards(testEnv())

    expect(github.issues).toHaveLength(0)
    expect(issueNumbers('crash_reports')).toEqual([0])
    expect(issueNumbers('feedback')).toEqual([0])
  })
})

describe('fileCrashCards', () => {
  it('files a pending release crash as an unprocessed crash-report card and remembers the number', async () => {
    const github = fakeGitHub()
    insertCrash()

    await fileCrashCards(testEnv())

    expect(github.issues).toHaveLength(1)
    const [issue] = github.issues
    expect(issue.labels).toEqual(['crash-report', 'unprocessed'])
    expect(issue.title).toContain('CRASH-AB234')
    expect(issue.title).toContain('cmdr::listing::sort')
    expect(issue.body).toContain('index out of bounds')
    expect(issueNumbers('crash_reports')).toEqual([issue.number])
  })

  it('marks a crash that carries a reply-to as needing a reply, with the address only in the expiring comment', async () => {
    const github = fakeGitHub()
    insertCrash({ email: 'tester@example.com' })

    await fileCrashCards(testEnv())

    const [issue] = github.issues
    expect(issue.labels).toContain('needs-reply')
    expect(issue.title + issue.body).not.toContain('tester@example.com')
    expect(github.comments).toHaveLength(1)
    expect(github.comments[0].body).toContain('tester@example.com')
    expect(personalCommentExpiry(github.comments[0].body)).not.toBeNull()
  })

  it('leaves debug builds and card-less rows alone', async () => {
    const github = fakeGitHub()
    insertCrash({ buildMode: 'debug' })
    insertCrash({ issueNumber: 0 })

    await fileCrashCards(testEnv())

    expect(github.issues).toHaveLength(0)
    expect(issueNumbers('crash_reports')).toEqual([null, 0])
  })

  it('treats a crash from a client too old to send a build mode as a release build', async () => {
    const github = fakeGitHub()
    insertCrash({ buildMode: null })

    await fileCrashCards(testEnv())

    expect(github.issues).toHaveLength(1)
  })

  it('keeps a crash pending when filing fails, so the next tick retries it, and says so loudly', async () => {
    fakeGitHub({ failCreates: true })
    insertCrash()

    await expect(fileCrashCards(testEnv())).rejects.toThrow(/retry/)
    expect(issueNumbers('crash_reports')).toEqual([null])

    const github = fakeGitHub()
    await fileCrashCards(testEnv())
    expect(github.issues).toHaveLength(1)
    expect(issueNumbers('crash_reports')).toEqual([github.issues[0].number])
  })

  it('writes nothing when the repo is not private', async () => {
    const github = fakeGitHub({ isPrivate: false })
    insertCrash()

    await expect(fileCrashCards(testEnv())).rejects.toThrow()
    expect(github.issues).toHaveLength(0)
    expect(issueNumbers('crash_reports')).toEqual([null])
  })

  it('stops at the daily cap and leaves the rest pending for a later day', async () => {
    const github = fakeGitHub()
    for (let i = 0; i < 22; i++) insertCrash()

    await fileCrashCards(testEnv())

    expect(github.issues).toHaveLength(20)
    expect(issueNumbers('crash_reports').filter((n) => n === null)).toHaveLength(2)
  })

  it('does nothing when the integration is unconfigured', async () => {
    const github = fakeGitHub()
    insertCrash()

    await fileCrashCards(testEnv({ GITHUB_ISSUES_TOKEN: undefined }))

    expect(github.issues).toHaveLength(0)
  })
})

describe('fileErrorReportCards', () => {
  it('files hand-written AND auto-sent reports, with the note only in the comment', async () => {
    const github = fakeGitHub()
    insertErrorReport({ shortId: 'ERR-AB7PV', kind: 'user' })
    insertErrorReport({ shortId: 'ERR-CD8QW', kind: 'auto', note: null })

    await fileErrorReportCards(testEnv())

    expect(github.issues).toHaveLength(2)
    const [user, auto] = github.issues
    expect(user.title).toContain('ERR-AB7PV')
    expect(user.title + user.body).not.toContain('zip and it vanished')
    expect(user.labels).toEqual(['error-report', 'unprocessed'])
    expect(auto.title).toContain('ERR-CD8QW')
    expect(auto.title).toContain('auto-sent')
    // Only the user report carries a note, so only it gets a personal comment.
    expect(github.comments.map((c) => c.issueNumber)).toEqual([user.number])
    expect(github.comments[0].body).toContain('zip and it vanished')
    expect(issueNumbers('error_reports')).toEqual([user.number, auto.number])
  })

  it('expires the comment 90 days after the UPLOAD, not 90 days after the card was filed', async () => {
    const github = fakeGitHub()
    insertErrorReport()

    await fileErrorReportCards(testEnv())

    expect(personalCommentExpiry(github.comments[0].body)).toBe('2026-12-30')
  })

  it('labels a report with a reply-to as needing a reply', async () => {
    const github = fakeGitHub()
    insertErrorReport({ email: 'person@example.com' })

    await fileErrorReportCards(testEnv())

    expect(github.issues[0].labels).toContain('needs-reply')
  })

  it('remembers the card in KV too, so an amendment to it comments instead of filing a second card', async () => {
    const github = fakeGitHub()
    const env = testEnv()
    insertErrorReport()

    await fileErrorReportCards(env)

    expect(await env.ERROR_REPORT_META.get('gh_issue:ERR-AB7PV')).toBe(String(github.issues[0].number))
  })

  it('skips debug builds', async () => {
    const github = fakeGitHub()
    insertErrorReport({ buildMode: 'debug' })

    await fileErrorReportCards(testEnv())

    expect(github.issues).toHaveLength(0)
  })
})

describe('fileFeedbackCards', () => {
  it('files feedback as an unprocessed in-app-feedback card with the message in the body', async () => {
    const github = fakeGitHub()
    insertFeedback()

    await fileFeedbackCards(testEnv())

    const [issue] = github.issues
    expect(issue.labels).toEqual(['in-app-feedback', 'unprocessed'])
    expect(issue.body).toContain('Please add a Date added column')
    expect(issue.title).not.toContain('Date added')
    expect(issueNumbers('feedback')).toEqual([issue.number])
  })

  it('keeps the reply-to for two years from when it was SENT', async () => {
    const github = fakeGitHub()
    insertFeedback({ email: 'fan@example.com' })

    await fileFeedbackCards(testEnv())

    expect(github.issues[0].labels).toContain('needs-reply')
    expect(personalCommentExpiry(github.comments[0].body)).toBe('2028-09-30')
  })
})

describe('feedback with logs', () => {
  const LOGS_NOTE = 'Logs sent with in-app feedback'

  it('files the feedback and its logs as ONE card, and the report never gets its own', async () => {
    const github = fakeGitHub()
    const env = testEnv()
    insertFeedback({ errorReportId: 'ERR-AB7PV' })
    insertErrorReport({ shortId: 'ERR-AB7PV', note: LOGS_NOTE })

    // The tick's order: error reports first, then feedback.
    await fileErrorReportCards(env)
    await fileFeedbackCards(env)
    await fileErrorReportCards(env)

    expect(github.issues).toHaveLength(1)
    const [card] = github.issues
    expect(card.labels).toEqual(['in-app-feedback', 'unprocessed'])
    expect(card.body).toContain('Please add a Date added column')
    expect(card.body).toContain('`ERR-AB7PV`')
    expect(card.body).toContain('error-reports/prod/2026-10-01/ERR-AB7PV-uuid.zip')
    expect(issueNumbers('feedback')).toEqual([card.number])
    expect(issueNumbers('error_reports')).toEqual([card.number])
    // An amendment to that report would find this card.
    expect(await env.ERROR_REPORT_META.get('gh_issue:ERR-AB7PV')).toBe(String(card.number))
  })

  it('says so on the card when the logs had not arrived yet', async () => {
    const github = fakeGitHub()
    insertFeedback({ errorReportId: 'ERR-AB7PV' })

    await fileFeedbackCards(testEnv())

    expect(github.issues[0].body).toContain('`ERR-AB7PV`')
    expect(github.issues[0].body).toContain("hadn't arrived")
  })

  it('gives late logs their own card, pointing at the feedback card', async () => {
    const github = fakeGitHub()
    const env = testEnv()
    insertFeedback({ errorReportId: 'ERR-AB7PV' })
    await fileFeedbackCards(env)
    insertErrorReport({ shortId: 'ERR-AB7PV', note: LOGS_NOTE })

    await fileErrorReportCards(env)

    expect(github.issues).toHaveLength(2)
    const [feedbackCard, reportCard] = github.issues
    expect(reportCard.body).toContain(`#${String(feedbackCard.number)}`)
  })

  it('never holds a report back for a debug-build feedback row, which never gets a card', async () => {
    const github = fakeGitHub()
    insertFeedback({ errorReportId: 'ERR-AB7PV', buildMode: 'debug' })
    insertErrorReport({ shortId: 'ERR-AB7PV' })

    await fileErrorReportCards(testEnv())

    expect(github.issues).toHaveLength(1)
  })
})

describe('postAmendmentsToBoard', () => {
  function insertAmendment(fields: { shortId?: string; email?: string | null; env?: string } = {}) {
    const shortId = fields.shortId ?? 'ERR-AB7PV'
    db.sqlite
      .prepare(
        `INSERT INTO error_report_amendments (short_id, report_date, env, r2_key, amendment_count, note, email)
           VALUES (?, '2026-10-01', ?, ?, 1, 'It happens every time', ?)`,
      )
      .run(shortId, fields.env ?? 'prod', `error-reports/prod/2026-10-01/${shortId}-uuid.zip`, fields.email ?? null)
  }

  function boardDone(): (string | null)[] {
    const rows = db.sqlite.prepare(`SELECT board_done_at FROM error_report_amendments ORDER BY id`).all() as {
      board_done_at: string | null
    }[]
    return rows.map((row) => row.board_done_at)
  }

  it('comments on the report card, adding needs-reply when the amendment brought an address', async () => {
    const github = fakeGitHub()
    const env = testEnv()
    insertErrorReport()
    await fileErrorReportCards(env)
    const card = github.issues[0].number
    insertAmendment({ email: 'late@example.com' })

    await postAmendmentsToBoard(env)

    expect(github.issues).toHaveLength(1)
    const amendmentComments = github.comments.filter((c) => c.body.includes('It happens every time'))
    expect(amendmentComments.map((c) => c.issueNumber)).toEqual([card])
    expect(personalCommentExpiry(amendmentComments[0].body)).toBe('2026-12-30')
    expect(github.labelAdds).toEqual([{ issueNumber: card, labels: ['needs-reply'] }])
    expect(boardDone()[0]).not.toBeNull()
  })

  it('waits while the report it amends is still waiting for its own card', async () => {
    const github = fakeGitHub()
    insertErrorReport()
    insertAmendment()

    await postAmendmentsToBoard(testEnv())

    expect(github.issues).toHaveLength(0)
    expect(github.comments).toHaveLength(0)
    expect(boardDone()).toEqual([null])
  })

  it('files an amended-report card for a report that predates the outbox', async () => {
    const github = fakeGitHub()
    const key = 'error-reports/prod/2026-10-01/ERR-OLD23-uuid.zip'
    insertAmendment({ shortId: 'ERR-OLD23' })

    await postAmendmentsToBoard(
      testEnv({
        ERROR_REPORTS_BUCKET: fakeBucket({ [key]: { kind: 'auto', appVersion: '0.50.0', osVersion: '15.8.1' } }),
      }),
    )

    expect(github.issues).toHaveLength(1)
    expect(github.issues[0].title).toContain('ERR-OLD23')
    expect(github.issues[0].labels).toContain('unprocessed')
    expect(boardDone()[0]).not.toBeNull()
  })

  it('never posts a debug report’s amendment', async () => {
    const github = fakeGitHub()
    insertAmendment({ env: 'dev' })

    await postAmendmentsToBoard(testEnv())

    expect(github.issues).toHaveLength(0)
    expect(boardDone()[0]).not.toBeNull()
  })
})
