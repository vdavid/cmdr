import { describe, expect, it } from 'vitest'
import {
  PERSONAL_COMMENT_MARKER,
  buildAmendmentComment,
  buildCrashIssue,
  buildErrorReportIssue,
  buildFeedbackIssue,
  buildPersonalComment,
  expiryAfter,
  fencedBlock,
  personalCommentExpiry,
  type CrashIssueInput,
  type ErrorReportIssueInput,
  type FeedbackIssueInput,
} from './report-cards'

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

    expect(title).toBe('Error report ERR-A2345: hand-written on 0.46.0 (macOS 15.3.1)')
    expect(body).not.toContain('Jane')
    expect(body).not.toContain('jane@example.com')
    expect(body).not.toContain('taxes')
    expect(body).toContain('ERR-A2345')
    expect(body).toContain('error-reports/prod/2026-09-18/ERR-A2345-abc.zip')
    expect(labels).toEqual(['error-report', 'unprocessed', 'needs-reply'])
  })

  it('names an auto-sent report, and one that only earned its card through a later note', () => {
    expect(buildErrorReportIssue({ ...errorReport, kind: 'auto' }).title).toBe(
      'Error report ERR-A2345: auto-sent on 0.46.0 (macOS 15.3.1)',
    )
    expect(buildErrorReportIssue({ ...errorReport, kind: 'auto', filedFromAmendment: true }).title).toBe(
      'Error report ERR-A2345: auto-sent, note added later on 0.46.0 (macOS 15.3.1)',
    )
  })

  it('does not double the macOS prefix, and prefixes a bare version number', () => {
    // ERR-KVERS arrived titled "macOS macOS 10.15.8": the app sends the prefix, fixtures often don't.
    expect(buildErrorReportIssue({ ...errorReport, osVersion: 'macOS 10.15.8' }).title).toBe(
      'Error report ERR-A2345: hand-written on 0.46.0 (macOS 10.15.8)',
    )
    expect(buildErrorReportIssue({ ...errorReport, osVersion: '15.3.1' }).title).toBe(
      'Error report ERR-A2345: hand-written on 0.46.0 (macOS 15.3.1)',
    )
    expect(buildFeedbackIssue({ ...feedbackInput, osVersion: 'macOS 10.15.8' }).title).toBe(
      'Feedback #42 on 0.46.0 (macOS 10.15.8)',
    )
  })

  it('shows a non-mac OS name as the client sent it', () => {
    // The app sends Linux's `PRETTY_NAME` as is, and the bare `std::env::consts::OS` elsewhere.
    expect(buildErrorReportIssue({ ...errorReport, osVersion: 'Ubuntu 24.04 LTS' }).title).toBe(
      'Error report ERR-A2345: hand-written on 0.46.0 (Ubuntu 24.04 LTS)',
    )
    expect(buildErrorReportIssue({ ...errorReport, osVersion: 'Linux (unknown distro)' }).title).toBe(
      'Error report ERR-A2345: hand-written on 0.46.0 (Linux (unknown distro))',
    )
    expect(buildFeedbackIssue({ ...feedbackInput, osVersion: 'Windows 11 24H2' }).title).toBe(
      'Feedback #42 on 0.46.0 (Windows 11 24H2)',
    )
    expect(buildFeedbackIssue({ ...feedbackInput, osVersion: 'windows' }).title).toBe(
      'Feedback #42 on 0.46.0 (windows)',
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
    expect(labels).toEqual(['in-app-feedback', 'unprocessed', 'needs-reply'])
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

describe('buildCrashIssue', () => {
  const crash: CrashIssueInput = {
    rowId: 9,
    shortId: 'CRASH-AB234',
    createdAt: '2026-10-13T08:12:00Z',
    appVersion: '0.52.0',
    osVersion: '27.0',
    arch: 'aarch64',
    signal: 'SIGABRT',
    topFunction: 'cmdr::listing::sort',
    appFate: 'ended',
    panicMessage: 'index out of bounds',
    osException: 'EXC_CRASH (SIGABRT)',
    osFrames: ['frame one', 'frame two'],
    imageBase: '0x100000000',
  }

  it('titles a crash by id, site, and signal, and keeps the technical facts in the body', () => {
    const { title, body, labels } = buildCrashIssue(crash)
    expect(title).toBe('Crash CRASH-AB234 in cmdr::listing::sort (SIGABRT) on 0.52.0 (macOS 27.0)')
    expect(body).toContain('index out of bounds')
    expect(body).toContain('EXC_CRASH (SIGABRT)')
    expect(body).toContain('frame two')
    expect(labels).toEqual(['crash-report', 'unprocessed'])
  })

  it('says "panic" when the app survived, because that ranks lower than a crash', () => {
    expect(buildCrashIssue({ ...crash, appFate: 'keptRunning' }).title).toMatch(/^Panic CRASH-AB234 /)
  })

  it('keeps the reply-to out of the title and body but labels the card for a reply', () => {
    const { title, body, labels } = buildCrashIssue({ ...crash, email: 'tester@example.com' })
    expect(title + body).not.toContain('tester@example.com')
    expect(labels).toContain('needs-reply')
  })

  it('clips a very long crash site so the version still fits in the title', () => {
    const { title } = buildCrashIssue({ ...crash, topFunction: 'x'.repeat(300) })
    expect(title).toContain('on 0.52.0')
    expect(title.length).toBeLessThan(160)
  })

  it('fences the panic message so it cannot become markup', () => {
    const { body } = buildCrashIssue({ ...crash, panicMessage: 'ping @octocat ```' })
    expect(body).toContain('````')
  })
})

describe('expiryAfter', () => {
  it('counts from the day the row was written, in either created_at shape', () => {
    expect(expiryAfter('2026-10-01 10:00:00', 90)).toBe('2026-12-30')
    expect(expiryAfter('2026-10-01T23:59:59Z', 90)).toBe('2026-12-30')
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
      linkTtlHours: 24,
    })
    expect(comment).toContain('jane@example.com')
    expect(comment).toContain('https://r2.example/bundle.zip?sig=x')
    expect(comment).toContain('24 hours')
  })

  it('truncates a note that would overflow what an issue comment accepts', () => {
    const comment = requireComment(buildPersonalComment({ userNote: 'x'.repeat(100_000) }))
    expect(comment.length).toBeLessThan(65_536)
    expect(comment).toContain('truncated')
  })
})
