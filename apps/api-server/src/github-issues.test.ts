import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import {
  buildAmendmentComment,
  commentOnReportIssue,
  fileErrorReportIssue,
  fileFeedbackIssue,
  recallIssueNumber,
  rememberIssueNumber,
  PERSONAL_COMMENT_MARKER,
  buildErrorReportIssue,
  buildFeedbackIssue,
  buildPersonalComment,
  deleteComment,
  fencedBlock,
  fileIssue,
  isRepoPrivate,
  listExpiredPersonalComments,
  personalCommentExpiry,
  resolveIssueTarget,
  type ErrorReportIssueInput,
  type FeedbackIssueInput,
  type IssueTarget,
} from './github-issues'
import type { Bindings } from './types'

const target: IssueTarget = { token: 'ghp_test', owner: 'vdavid', repo: 'cmdr-reports' }

const errorReport: ErrorReportIssueInput = {
  id: 'ERR-A2345',
  kind: 'user',
  buildMode: 'release',
  appVersion: '0.46.0',
  osVersion: '15.3.1',
  arch: 'aarch64',
  sizeBytes: 1_234_567,
  r2Key: 'error-reports/prod/2026-09-18/ERR-A2345-abc.zip',
  uploadedUnixSeconds: 1_745_000_000,
}

const feedbackInput: FeedbackIssueInput = {
  rowId: 42,
  buildMode: 'release',
  appVersion: '0.46.0',
  osVersion: '15.3.1',
}

/** A response shaped like the GitHub REST calls we make. */
function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })
}

/** The JSON body of a recorded fetch call, as text. Bodies here are always strings we set. */
function requestBody(call: unknown): string {
  const [, init] = call as [string, RequestInit]
  return typeof init.body === 'string' ? init.body : ''
}

/** The URL of a recorded fetch call. */
function requestUrl(call: unknown): string {
  const [url] = call as [string, RequestInit]
  return url
}

/** Assert a built comment exists, and hand it back narrowed to a string. */
function requireComment(comment: string | null): string {
  if (comment === null) throw new Error('expected a personal comment, got null')
  return comment
}

/** Assert a comment carries an expiry stamp, and return how many days out it is. */
function daysUntilExpiry(commentBody: string): number {
  const expiry = personalCommentExpiry(commentBody)
  if (expiry === null) throw new Error('expected an expires= stamp')
  return (new Date(expiry).getTime() - Date.now()) / 86_400_000
}

/** A `Bindings` carrying only the fields under test. */
function env(fields: Partial<Bindings>): Bindings {
  return fields as Bindings
}

describe('resolveIssueTarget', () => {
  it('is off unless both the repo and the token are configured', () => {
    expect(resolveIssueTarget(env({}))).toBeNull()
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports' }))).toBeNull()
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x' }))).toBeNull()
  })

  it('rejects a repo that is not exactly owner/name', () => {
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'cmdr-reports' }))).toBeNull()
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'a/b/c' }))).toBeNull()
  })

  it('splits a configured repo into owner and name', () => {
    expect(
      resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports' })),
    ).toEqual({ token: 'ghp_x', owner: 'vdavid', repo: 'cmdr-reports' })
  })
})

describe('fencedBlock', () => {
  it('cannot be escaped by backticks in the text', () => {
    const hostile = '```\n### not a heading\n```'
    const block = fencedBlock(hostile)
    // The opening fence must be longer than the longest run inside, so the payload stays inert.
    expect(block.startsWith('````')).toBe(true)
    expect(block.endsWith('````')).toBe(true)
    expect(block).toContain(hostile)
  })

  it('keeps mentions and issue references inert', () => {
    const block = fencedBlock('cc @octocat see #1 and https://evil.test')
    expect(block).toBe('```\ncc @octocat see #1 and https://evil.test\n```')
  })

  it('handles an empty string without producing a broken fence', () => {
    expect(fencedBlock('')).toBe('```\n\n```')
  })
})

describe('buildErrorReportIssue', () => {
  it('keeps every personal field out of the title and body', () => {
    const { title, body, labels } = buildErrorReportIssue({
      ...errorReport,
      userNote: 'my name is Jane and my path is /Users/jane/taxes',
      email: 'jane@example.com',
    })

    expect(title).toBe('ERR-A2345: user report on 0.46.0 (macOS 15.3.1)')
    expect(body).not.toContain('Jane')
    expect(body).not.toContain('jane@example.com')
    expect(body).not.toContain('taxes')
    expect(body).toContain('ERR-A2345')
    expect(body).toContain('error-reports/prod/2026-09-18/ERR-A2345-abc.zip')
    expect(labels).toContain('error-report')
  })

  it('does not double the macOS prefix a client already sent', () => {
    // ERR-KVERS arrived titled "macOS macOS 10.15.8": some clients send the prefix, some don't.
    expect(buildErrorReportIssue({ ...errorReport, osVersion: 'macOS 10.15.8' }).title).toBe(
      'ERR-A2345: user report on 0.46.0 (macOS 10.15.8)',
    )
    expect(buildErrorReportIssue({ ...errorReport, osVersion: '15.3.1' }).title).toBe(
      'ERR-A2345: user report on 0.46.0 (macOS 15.3.1)',
    )
    expect(buildFeedbackIssue({ ...feedbackInput, osVersion: 'macOS 10.15.8' }).title).toBe(
      'Feedback #42 on 0.46.0 (macOS 10.15.8)',
    )
  })

  it('labels a report that carries a reply-to so it can be answered', () => {
    expect(buildErrorReportIssue({ ...errorReport, email: 'jane@example.com' }).labels).toContain('needs-reply')
    expect(buildErrorReportIssue(errorReport).labels).not.toContain('needs-reply')
  })
})

describe('buildFeedbackIssue', () => {
  it('keeps the message in the body but the reply-to out of it', () => {
    const { title, body, labels } = buildFeedbackIssue({
      ...feedbackInput,
      feedback: 'please add tabs, love from Bob',
      email: 'bob@example.com',
    })

    // The policy keeps feedback text so it can be acted on, so the message may live in the body.
    // The reply-to address expires, so it may not.
    expect(title).toBe('Feedback #42 on 0.46.0 (macOS 15.3.1)')
    expect(body).toContain('please add tabs')
    expect(body).not.toContain('bob@example.com')
    expect(labels).toEqual(['feedback', 'needs-reply'])
  })

  it('keeps the title free of the message, which is the part that could say anything', () => {
    const { title } = buildFeedbackIssue({ ...feedbackInput, feedback: 'my password is hunter2' })
    expect(title).not.toContain('hunter2')
  })

  it('fences the message so it cannot become markup', () => {
    const { body } = buildFeedbackIssue({ ...feedbackInput, feedback: 'ping @octocat\n```\nx\n```' })
    expect(body).toContain('````')
    expect(body).toContain('@octocat')
  })
})

/** A KV stand-in with just the two methods the cap counter uses. */
function fakeKv(): KVNamespace {
  const store = new Map<string, string>()
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
}

function configuredEnv(): Bindings {
  return env({
    GITHUB_ISSUES_TOKEN: 'ghp_x',
    GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports',
    ERROR_REPORT_META: fakeKv(),
  })
}

describe('fileErrorReportIssue', () => {
  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('files nothing at all when the integration is unconfigured', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    await expect(fileErrorReportIssue(env({ ERROR_REPORT_META: fakeKv() }), errorReport)).resolves.toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('skips auto-sent reports and debug builds without touching the network', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(fileErrorReportIssue(configuredEnv(), { ...errorReport, kind: 'auto' })).resolves.toBeNull()
    await expect(fileErrorReportIssue(configuredEnv(), { ...errorReport, buildMode: 'debug' })).resolves.toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('files a hand-written release report, with the note in the comment only', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ number: 3 }, 201))
      .mockResolvedValueOnce(jsonResponse({ id: 1 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    const number = await fileErrorReportIssue(configuredEnv(), {
      ...errorReport,
      userNote: 'it died opening /Users/jane/photos',
      email: 'jane@example.com',
    })

    expect(number).toBe(3)
    const issueBody = requestBody(fetchMock.mock.calls[1])
    expect(issueBody).not.toContain('jane')
    expect(issueBody).not.toContain('photos')
    const commentBody = requestBody(fetchMock.mock.calls[2])
    expect(commentBody).toContain('jane@example.com')
    expect(commentBody).toContain('photos')
  })

  it('stops filing once the daily cap is spent', async () => {
    const sharedEnv = configuredEnv()
    // A fresh Response per call: a Response body can only be read once, so a single shared
    // instance would fail the second read rather than the assertion under test.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(() => Promise.resolve(jsonResponse({ private: true, number: 1, id: 1 }))),
    )

    for (let i = 0; i < 20; i++) {
      await expect(fileErrorReportIssue(sharedEnv, errorReport)).resolves.toBe(1)
    }
    await expect(fileErrorReportIssue(sharedEnv, errorReport)).resolves.toBeNull()
  })
})

describe('fileFeedbackIssue', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('skips debug builds', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    await expect(fileFeedbackIssue(configuredEnv(), { ...feedbackInput, buildMode: 'debug' })).resolves.toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('posts no comment when no reply-to was attached', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ number: 5 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await expect(fileFeedbackIssue(configuredEnv(), { ...feedbackInput, feedback: 'nice app' })).resolves.toBe(5)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('stamps the reply-to comment with the two-year promise, not the ninety-day one', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ number: 5 }, 201))
      .mockResolvedValueOnce(jsonResponse({ id: 1 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await fileFeedbackIssue(configuredEnv(), { ...feedbackInput, feedback: 'hi', email: 'bob@example.com' })

    const commentPayload = JSON.parse(requestBody(fetchMock.mock.calls[2])) as { body: string }
    expect(daysUntilExpiry(commentPayload.body)).toBeGreaterThan(725)
  })
})

describe('remembering which issue a report got', () => {
  it('round-trips an issue number through KV', async () => {
    const kv = fakeKv()
    await expect(recallIssueNumber(kv, 'ERR-A2345')).resolves.toBeNull()
    await rememberIssueNumber(kv, 'ERR-A2345', 12)
    await expect(recallIssueNumber(kv, 'ERR-A2345')).resolves.toBe(12)
  })

  it('is null for a report that never got an issue, and for a corrupted value', async () => {
    const kv = fakeKv()
    await expect(recallIssueNumber(kv, 'ERR-NONE1')).resolves.toBeNull()
    await kv.put('gh_issue:ERR-BAD11', 'not a number')
    await expect(recallIssueNumber(kv, 'ERR-BAD11')).resolves.toBeNull()
  })

  it('remembers the number after filing, so an amendment can find it later', async () => {
    const sharedEnv = configuredEnv()
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(jsonResponse({ private: true }))
        .mockResolvedValueOnce(jsonResponse({ number: 77 }, 201)),
    )

    await expect(fileErrorReportIssue(sharedEnv, errorReport)).resolves.toBe(77)
    await expect(recallIssueNumber(sharedEnv.ERROR_REPORT_META, errorReport.id)).resolves.toBe(77)
    vi.unstubAllGlobals()
  })
})

describe('buildAmendmentComment', () => {
  it('carries the note and the reply-to, both fenced', () => {
    const comment = buildAmendmentComment({
      note: 'forgot to say: it only happens on the NAS ```x```',
      email: 'later@example.com',
      amendmentCount: 2,
      expiresOn: '2026-12-01',
    })
    expect(comment).toContain('forgot to say')
    expect(comment).toContain('later@example.com')
    expect(comment).toContain('````')
    expect(personalCommentExpiry(comment)).toBe('2026-12-01')
  })

  it('expires with the original report rather than 90 days from the amendment', () => {
    // A report amended on day 80 must still disappear on day 90, not day 170.
    const comment = buildAmendmentComment({ note: 'late note', amendmentCount: 1, expiresOn: '2026-10-01' })
    expect(personalCommentExpiry(comment)).toBe('2026-10-01')
  })

  it('says which amendment this is, so several read in order', () => {
    expect(buildAmendmentComment({ note: 'x', amendmentCount: 3, expiresOn: '2026-12-01' })).toContain('3')
  })
})

describe('commentOnReportIssue', () => {
  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('does nothing when the report never got an issue', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(commentOnReportIssue(configuredEnv(), 'ERR-A2345', 'hi')).resolves.toBe(false)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('re-checks that the repo is private before commenting', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ private: false, visibility: 'public' }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(commentOnReportIssue(sharedEnv, 'ERR-A2345', 'secret note')).resolves.toBe(false)
    // Only the privacy probe. The note was never posted.
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('adds needs-reply when the amendment brought a reply-to address', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ id: 5 }, 201))
      .mockResolvedValueOnce(jsonResponse([{ name: 'needs-reply' }], 200))
    vi.stubGlobal('fetch', fetchMock)

    await expect(commentOnReportIssue(sharedEnv, 'ERR-A2345', 'note', { addLabels: ['needs-reply'] })).resolves.toBe(
      true,
    )

    expect(requestUrl(fetchMock.mock.calls[2])).toBe(
      'https://api.github.com/repos/vdavid/cmdr-reports/issues/12/labels',
    )
    expect(requestBody(fetchMock.mock.calls[2])).toContain('needs-reply')
  })

  it('does not touch labels when the amendment brought no address', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ id: 5 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await expect(commentOnReportIssue(sharedEnv, 'ERR-A2345', 'note')).resolves.toBe(true)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('keeps the comment when only the label call fails', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(jsonResponse({ private: true }))
        .mockResolvedValueOnce(jsonResponse({ id: 5 }, 201))
        .mockResolvedValueOnce(jsonResponse({ message: 'boom' }, 500)),
    )

    await expect(commentOnReportIssue(sharedEnv, 'ERR-A2345', 'note', { addLabels: ['needs-reply'] })).resolves.toBe(
      true,
    )
  })

  it('posts to the remembered issue when the repo is private', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ id: 5 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await expect(commentOnReportIssue(sharedEnv, 'ERR-A2345', 'the amendment')).resolves.toBe(true)
    expect(requestUrl(fetchMock.mock.calls[1])).toBe(
      'https://api.github.com/repos/vdavid/cmdr-reports/issues/12/comments',
    )
    expect(requestBody(fetchMock.mock.calls[1])).toContain('the amendment')
  })
})

describe('personalCommentExpiry', () => {
  it('reads the stamp back off a comment it built', () => {
    const comment = requireComment(buildPersonalComment({ userNote: 'hi', retentionDays: 90 }))
    // 90 days out, give or take the second the clock ticks over.
    const daysOut = daysUntilExpiry(comment)
    expect(daysOut).toBeGreaterThan(88)
    expect(daysOut).toBeLessThan(91)
  })

  it('is null for a comment that is not ours', () => {
    expect(personalCommentExpiry('just a normal comment')).toBeNull()
    expect(personalCommentExpiry('<!-- cmdr:personal-data -->\nno date')).toBeNull()
    // A note that merely quotes the marker cannot forge a stamp: ours is on the first line.
    expect(personalCommentExpiry('hi\n<!-- cmdr:personal-data expires=1999-01-01 -->')).toBeNull()
  })
})

describe('listExpiredPersonalComments', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('returns only the comments whose stamp is due, ignoring everyone else’s', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse([
          { id: 1, body: '<!-- cmdr:personal-data expires=2026-01-01 -->\nold', issue_url: 'https://x/issues/11' },
          { id: 2, body: '<!-- cmdr:personal-data expires=2099-01-01 -->\nfresh', issue_url: 'https://x/issues/12' },
          { id: 3, body: 'a human comment', issue_url: 'https://x/issues/13' },
        ]),
      ),
    )

    const due = await listExpiredPersonalComments(target, '2026-09-18')
    expect(due).toEqual([{ issueNumber: 11, commentId: 1, expiresOn: '2026-01-01' }])
  })

  it('treats a comment due exactly today as due', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          jsonResponse([
            { id: 4, body: '<!-- cmdr:personal-data expires=2026-09-18 -->\ntoday', issue_url: 'https://x/issues/14' },
          ]),
        ),
    )
    await expect(listExpiredPersonalComments(target, '2026-09-18')).resolves.toHaveLength(1)
  })

  it('throws on a failed listing rather than reporting nothing to delete', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ message: 'nope' }, 500)))
    await expect(listExpiredPersonalComments(target, '2026-09-18')).rejects.toThrow('500')
  })
})

describe('deleteComment', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('treats an already-deleted comment as success', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 404 })))
    await expect(deleteComment(target, 1)).resolves.toBeUndefined()
  })

  it('throws on a real failure so the cron alarm fires', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 500 })))
    await expect(deleteComment(target, 1)).rejects.toThrow('500')
  })
})

describe('buildPersonalComment', () => {
  it('is null when the reporter attached nothing personal', () => {
    expect(buildPersonalComment({})).toBeNull()
    expect(buildPersonalComment({ userNote: '   ' })).toBeNull()
  })

  it('carries the marker the retention sweep matches on', () => {
    const comment = requireComment(buildPersonalComment({ userNote: 'it crashed' }))
    expect(comment.startsWith(PERSONAL_COMMENT_MARKER)).toBe(true)
  })

  it('fences the note so its content cannot become markup', () => {
    const comment = buildPersonalComment({ userNote: '@octocat ```js\nalert(1)\n```' })
    expect(comment).toContain('````')
    expect(comment).toContain('@octocat')
  })

  it('includes the reply-to address and the bundle link when present', () => {
    const comment = buildPersonalComment({
      email: 'jane@example.com',
      downloadUrl: 'https://r2.example/bundle.zip?sig=x',
      linkTtlDays: 7,
    })
    expect(comment).toContain('jane@example.com')
    expect(comment).toContain('https://r2.example/bundle.zip?sig=x')
    expect(comment).toContain('7 days')
  })

  it('truncates a note that would overflow what an issue comment accepts', () => {
    const comment = requireComment(buildPersonalComment({ userNote: 'x'.repeat(100_000) }))
    expect(comment.length).toBeLessThan(65_536)
    expect(comment).toContain('truncated')
  })
})

describe('isRepoPrivate', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('is true only when GitHub says the repo is private', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse({ private: true, visibility: 'private', archived: false })),
    )
    await expect(isRepoPrivate(target)).resolves.toBe(true)
  })

  it('is false for a public repo', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse({ private: false, visibility: 'public', archived: false })),
    )
    await expect(isRepoPrivate(target)).resolves.toBe(false)
  })

  it('fails closed when the lookup errors or the field is missing', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ message: 'Not Found' }, 404)))
    await expect(isRepoPrivate(target)).resolves.toBe(false)

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ visibility: 'private' })))
    await expect(isRepoPrivate(target)).resolves.toBe(false)

    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network down')))
    await expect(isRepoPrivate(target)).resolves.toBe(false)
  })
})

describe('fileIssue', () => {
  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('refuses to write anything when the target repo is public', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ private: false, visibility: 'public' }))
    vi.stubGlobal('fetch', fetchMock)

    const result = await fileIssue(target, { title: 't', body: 'b', labels: [], personalComment: 'secret' })

    expect(result).toBeNull()
    // Exactly one call: the privacy probe. Nothing was posted.
    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(init.method ?? 'GET').toBe('GET')
  })

  it('refuses to write anything when the privacy probe fails', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ message: 'Bad credentials' }, 401))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      fileIssue(target, { title: 't', body: 'b', labels: [], personalComment: 'secret' }),
    ).resolves.toBeNull()
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('creates the issue and posts the personal comment separately', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true, visibility: 'private' }))
      .mockResolvedValueOnce(jsonResponse({ number: 7 }, 201))
      .mockResolvedValueOnce(jsonResponse({ id: 99 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    const number = await fileIssue(target, {
      title: 'ERR-A2345: user report',
      body: 'technical only',
      labels: ['error-report'],
      personalComment: `${PERSONAL_COMMENT_MARKER}\nthe note`,
    })

    expect(number).toBe(7)
    expect(fetchMock).toHaveBeenCalledTimes(3)

    expect(requestUrl(fetchMock.mock.calls[1])).toBe('https://api.github.com/repos/vdavid/cmdr-reports/issues')
    const issuePayload = JSON.parse(requestBody(fetchMock.mock.calls[1])) as Record<string, unknown>
    expect(issuePayload['body']).toBe('technical only')
    // The body the tracker keeps forever must never carry the personal half.
    expect(JSON.stringify(issuePayload)).not.toContain('the note')

    expect(requestUrl(fetchMock.mock.calls[2])).toBe(
      'https://api.github.com/repos/vdavid/cmdr-reports/issues/7/comments',
    )
    expect(requestBody(fetchMock.mock.calls[2])).toContain('the note')
  })

  it('keeps the issue when the personal comment fails to post', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true, visibility: 'private' }))
      .mockResolvedValueOnce(jsonResponse({ number: 7 }, 201))
      .mockResolvedValueOnce(jsonResponse({ message: 'boom' }, 500))
    vi.stubGlobal('fetch', fetchMock)

    await expect(fileIssue(target, { title: 't', body: 'b', labels: [], personalComment: 'note' })).resolves.toBe(7)
  })

  it('skips the comment call entirely when there is nothing personal', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true, visibility: 'private' }))
      .mockResolvedValueOnce(jsonResponse({ number: 8 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await expect(fileIssue(target, { title: 't', body: 'b', labels: [], personalComment: null })).resolves.toBe(8)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('returns null when issue creation itself fails', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true, visibility: 'private' }))
      .mockResolvedValueOnce(jsonResponse({ message: 'nope' }, 422))
    vi.stubGlobal('fetch', fetchMock)

    await expect(fileIssue(target, { title: 't', body: 'b', labels: [], personalComment: null })).resolves.toBeNull()
  })
})
