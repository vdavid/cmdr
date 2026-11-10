import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import {
  recordAmendmentOnBoard,
  claimIssueSlot,
  recallIssueNumber,
  rememberIssueNumber,
  deleteComment,
  fileIssue,
  isRepoPrivate,
  listCommentsUpdatedSince,
  listExpiredPersonalComments,
  resolveIssueTarget,
  type IssueTarget,
} from './github-issues'
import { PERSONAL_COMMENT_MARKER, type ErrorReportIssueInput } from './report-cards'
import type { Bindings } from './types'

const target: IssueTarget = { token: 'ghp_test', owner: 'getcmdr', repo: 'cmdr-reports' }

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

/** A `Bindings` carrying only the fields under test. */
function env(fields: Partial<Bindings>): Bindings {
  return fields as Bindings
}

describe('resolveIssueTarget', () => {
  it('is off unless both the repo and the token are configured', () => {
    expect(resolveIssueTarget(env({}))).toBeNull()
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_REPO: 'getcmdr/cmdr-reports' }))).toBeNull()
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x' }))).toBeNull()
  })

  it('rejects a repo that is not exactly owner/name', () => {
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'cmdr-reports' }))).toBeNull()
    expect(resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'a/b/c' }))).toBeNull()
  })

  it('splits a configured repo into owner and name', () => {
    expect(
      resolveIssueTarget(env({ GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'getcmdr/cmdr-reports' })),
    ).toEqual({ token: 'ghp_x', owner: 'getcmdr', repo: 'cmdr-reports' })
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
    GITHUB_ISSUES_REPO: 'getcmdr/cmdr-reports',
    ERROR_REPORT_META: fakeKv(),
  })
}

describe('claimIssueSlot', () => {
  it('allows 20 cards per source per day, and counts each source separately', async () => {
    const kv = fakeKv()
    for (let i = 0; i < 20; i++) {
      await expect(claimIssueSlot(kv, 'crash-report', '2026-10-14')).resolves.toBe(true)
    }
    await expect(claimIssueSlot(kv, 'crash-report', '2026-10-14')).resolves.toBe(false)
    await expect(claimIssueSlot(kv, 'feedback', '2026-10-14')).resolves.toBe(true)
    await expect(claimIssueSlot(kv, 'crash-report', '2026-10-15')).resolves.toBe(true)
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
})

describe('recordAmendmentOnBoard', () => {
  /** The facts an auto-sent report's bundle carries, as the amend route reads them back off R2. */
  const autoFacts: ErrorReportIssueInput = { ...errorReport, kind: 'auto' }

  /** A reader that hands back `facts`, and records whether it was asked at all. */
  function factsReader(facts: ErrorReportIssueInput | null) {
    return vi.fn(() => Promise.resolve(facts))
  }

  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('re-checks that the repo is private before commenting', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ private: false, visibility: 'public' }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      recordAmendmentOnBoard(sharedEnv, {
        id: 'ERR-A2345',
        comment: 'secret note',
        readReportFacts: factsReader(autoFacts),
      }),
    ).resolves.toBe('skipped')
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

    await expect(
      recordAmendmentOnBoard(sharedEnv, {
        id: 'ERR-A2345',
        comment: 'note',
        addLabels: ['needs-reply'],
        readReportFacts: factsReader(autoFacts),
      }),
    ).resolves.toBe('commented')

    expect(requestUrl(fetchMock.mock.calls[2])).toBe(
      'https://api.github.com/repos/getcmdr/cmdr-reports/issues/12/labels',
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

    await expect(
      recordAmendmentOnBoard(sharedEnv, { id: 'ERR-A2345', comment: 'note', readReportFacts: factsReader(autoFacts) }),
    ).resolves.toBe('commented')
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

    await expect(
      recordAmendmentOnBoard(sharedEnv, {
        id: 'ERR-A2345',
        comment: 'note',
        addLabels: ['needs-reply'],
        readReportFacts: factsReader(autoFacts),
      }),
    ).resolves.toBe('commented')
  })

  it('posts to the remembered issue when the repo is private', async () => {
    const sharedEnv = configuredEnv()
    await rememberIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345', 12)
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ id: 5 }, 201))
    vi.stubGlobal('fetch', fetchMock)
    const readFacts = factsReader(autoFacts)

    await expect(
      recordAmendmentOnBoard(sharedEnv, { id: 'ERR-A2345', comment: 'the amendment', readReportFacts: readFacts }),
    ).resolves.toBe('commented')
    expect(requestUrl(fetchMock.mock.calls[1])).toBe(
      'https://api.github.com/repos/getcmdr/cmdr-reports/issues/12/comments',
    )
    expect(requestBody(fetchMock.mock.calls[1])).toContain('the amendment')
    // The card exists, so nothing reads the bundle to describe it.
    expect(readFacts).not.toHaveBeenCalled()
  })

  it('files a card for an auto-sent report, because a person typed the note', async () => {
    const sharedEnv = configuredEnv()
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ number: 31 }, 201))
      .mockResolvedValueOnce(jsonResponse({ id: 7 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      recordAmendmentOnBoard(sharedEnv, {
        id: 'ERR-A2345',
        comment: 'the note they added afterwards',
        readReportFacts: factsReader(autoFacts),
      }),
    ).resolves.toBe('filed')

    const created = JSON.parse(requestBody(fetchMock.mock.calls[1])) as { title: string; body: string }
    expect(created.title).toContain('ERR-A2345')
    // The card says the report was auto-sent and that someone added to it afterwards, so triage
    // knows why an auto-send has a card at all.
    expect(created.body).toContain('auto-sent')
    expect(created.body).toContain('added')
    // The note stays out of the body, which outlives the 90-day comment.
    expect(created.body).not.toContain('the note they added afterwards')
    expect(requestBody(fetchMock.mock.calls[2])).toContain('the note they added afterwards')
  })

  it('remembers the card it filed, so a second amendment comments instead of filing again', async () => {
    const sharedEnv = configuredEnv()
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(jsonResponse({ private: true }))
        .mockResolvedValueOnce(jsonResponse({ number: 31 }, 201))
        .mockResolvedValueOnce(jsonResponse({ id: 7 }, 201)),
    )

    await recordAmendmentOnBoard(sharedEnv, {
      id: 'ERR-A2345',
      comment: 'first note',
      readReportFacts: factsReader(autoFacts),
    })
    await expect(recallIssueNumber(sharedEnv.ERROR_REPORT_META, 'ERR-A2345')).resolves.toBe(31)
  })

  it('labels a filed card needs-reply when the amendment carried an address', async () => {
    const sharedEnv = configuredEnv()
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ private: true }))
      .mockResolvedValueOnce(jsonResponse({ number: 31 }, 201))
      .mockResolvedValueOnce(jsonResponse({ id: 7 }, 201))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      recordAmendmentOnBoard(sharedEnv, {
        id: 'ERR-A2345',
        comment: 'note plus address',
        addLabels: ['needs-reply'],
        readReportFacts: factsReader({ ...autoFacts, email: 'later@example.com' }),
      }),
    ).resolves.toBe('filed')

    const created = JSON.parse(requestBody(fetchMock.mock.calls[1])) as { labels: string[] }
    expect(created.labels).toContain('needs-reply')
    // The address itself belongs in the expiring comment, never on the card.
    expect(requestBody(fetchMock.mock.calls[1])).not.toContain('later@example.com')
  })

  it('files nothing for a debug build, which is our own E2E traffic', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      recordAmendmentOnBoard(configuredEnv(), {
        id: 'ERR-A2345',
        comment: 'note',
        readReportFacts: factsReader({ ...autoFacts, buildMode: 'debug' }),
      }),
    ).resolves.toBe('skipped')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('files nothing when the bundle can no longer describe itself', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      recordAmendmentOnBoard(configuredEnv(), {
        id: 'ERR-A2345',
        comment: 'note',
        readReportFacts: factsReader(null),
      }),
    ).resolves.toBe('skipped')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('respects the daily cap, the same allowance the upload path spends', async () => {
    const sharedEnv = configuredEnv()
    const today = new Date().toISOString().slice(0, 10)
    await sharedEnv.ERROR_REPORT_META.put(`gh_issue_count:error-report:${today}`, '20')
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      recordAmendmentOnBoard(sharedEnv, {
        id: 'ERR-A2345',
        comment: 'note',
        readReportFacts: factsReader(autoFacts),
      }),
    ).resolves.toBe('skipped')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('does nothing at all when the integration is unconfigured', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const readFacts = factsReader(autoFacts)

    await expect(
      recordAmendmentOnBoard(env({ ERROR_REPORT_META: fakeKv() }), {
        id: 'ERR-A2345',
        comment: 'note',
        readReportFacts: readFacts,
      }),
    ).resolves.toBe('skipped')
    expect(fetchMock).not.toHaveBeenCalled()
    expect(readFacts).not.toHaveBeenCalled()
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

describe('listCommentsUpdatedSince', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  // Regression anchor: one 522 from GitHub threw "Card ratings" for the 2026-10-24T18:00 tick.
  it('rides out a one-off 522 instead of throwing the rating job', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const comment = {
      id: 9,
      body: 'hi',
      issue_url: 'https://api.github.com/repos/getcmdr/cmdr-reports/issues/14',
      user: { login: 'vdavid' },
      created_at: '2026-10-24T10:00:00Z',
      updated_at: '2026-10-24T10:00:00Z',
    }
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(new Response('', { status: 522 }))
        .mockResolvedValueOnce(jsonResponse([comment])),
    )

    const { comments } = await listCommentsUpdatedSince(target, '2026-10-10T00:00:00Z', 5)

    expect(comments.map((c) => c.id)).toEqual([9])
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

    expect(requestUrl(fetchMock.mock.calls[1])).toBe('https://api.github.com/repos/getcmdr/cmdr-reports/issues')
    const issuePayload = JSON.parse(requestBody(fetchMock.mock.calls[1])) as Record<string, unknown>
    expect(issuePayload['body']).toBe('technical only')
    // The body the tracker keeps forever must never carry the personal half.
    expect(JSON.stringify(issuePayload)).not.toContain('the note')

    expect(requestUrl(fetchMock.mock.calls[2])).toBe(
      'https://api.github.com/repos/getcmdr/cmdr-reports/issues/7/comments',
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
