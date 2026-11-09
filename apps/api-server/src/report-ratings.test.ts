/**
 * Card ratings: the strict `Rating:` parser, then the cron job against a REAL SQLite (the production
 * migrations) and a fake GitHub. What's at stake is a rating applied twice, applied from a comment
 * nobody trusted, or failing without anyone hearing about it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'
import { RATINGS_PER_TICK, TRIAGE_COMMENT_MARKER, applyCardRatings, parseRatingLine } from './report-ratings'
import type { Bindings } from './types'

describe('parseRatingLine', () => {
  const comment = (last: string) => `${TRIAGE_COMMENT_MARKER}\n**Problem**: it breaks.\n**Size**: S, small.\n\n${last}`

  it('reads a valid line', () => {
    expect(parseRatingLine(comment('Rating: Severity=Medium Impact=Low Size=S'))).toEqual({
      kind: 'valid',
      rating: { severity: 'Medium', impact: 'Low', size: 'S' },
      line: 'Rating: Severity=Medium Impact=Low Size=S',
    })
  })

  it('reads Severity=none as a cleared severity', () => {
    const parsed = parseRatingLine(comment('Rating: Severity=none Impact=High Size=XL'))
    expect(parsed).toMatchObject({ kind: 'valid', rating: { severity: null, impact: 'High', size: 'XL' } })
  })

  it('tolerates CRLF line endings and trailing blank lines', () => {
    const parsed = parseRatingLine(
      `${TRIAGE_COMMENT_MARKER}\r\nText\r\nRating: Severity=Low Impact=Low Size=XS\r\n\r\n`,
    )
    expect(parsed.kind).toBe('valid')
  })

  it('says absent when there is no rating-like line', () => {
    expect(parseRatingLine(comment('**Duplicates**: none found'))).toEqual({ kind: 'absent' })
  })

  it.each([
    ['extra spaces', 'Rating:  Severity=Medium Impact=Low Size=S'],
    ['a trailing space', 'Rating: Severity=Medium Impact=Low Size=S '],
    ['a leading space', ' Rating: Severity=Medium Impact=Low Size=S'],
    ['a lowercase key', 'rating: Severity=Medium Impact=Low Size=S'],
    ['a lowercase value', 'Rating: Severity=medium Impact=Low Size=S'],
    ['a wrong-case none', 'Rating: Severity=None Impact=Low Size=S'],
    ['bold markup', '**Rating**: Severity=Medium Impact=Low Size=S'],
    ['backticks', '`Rating: Severity=Medium Impact=Low Size=S`'],
    ['an unknown severity', 'Rating: Severity=Urgent Impact=Low Size=S'],
    ['an unknown impact', 'Rating: Severity=Medium Impact=Huge Size=S'],
    ['an unknown size', 'Rating: Severity=Medium Impact=Low Size=XXL'],
    ['a missing field', 'Rating: Severity=Medium Impact=Low'],
    ['a missing severity', 'Rating: Impact=Low Size=S'],
    ['reordered fields', 'Rating: Impact=Low Severity=Medium Size=S'],
    ['an extra field', 'Rating: Severity=Medium Impact=Low Size=S Status=Done'],
    ['an empty value', 'Rating: Severity= Impact=Low Size=S'],
    ['no colon', 'Rating Severity=Medium Impact=Low Size=S'],
  ])('rejects %s', (_label, line) => {
    expect(parseRatingLine(comment(line)).kind).toBe('invalid')
  })

  it('rejects two Rating lines, even when both are valid', () => {
    const body = comment('Rating: Severity=Low Impact=Low Size=S\nRating: Severity=High Impact=High Size=L')
    expect(rejection(body)).toContain('2 Rating lines')
  })

  it('rejects a Rating line that is not the last line', () => {
    const body = `${TRIAGE_COMMENT_MARKER}\nRating: Severity=Low Impact=Low Size=S\n**Problem**: after it`
    expect(rejection(body)).toContain('last line')
  })
})

/** The reason a body was rejected for, or a failure when it was accepted. */
function rejection(body: string): string {
  const parsed = parseRatingLine(body)
  if (parsed.kind !== 'invalid') throw new Error(`expected a rejection, got ${parsed.kind}`)
  return parsed.reason
}

const REPO_URL = 'https://api.github.com/repos/vdavid/cmdr-reports'

interface FakeComment {
  id: number
  issueNumber: number
  author?: string
  body: string
  createdAt?: string
  updatedAt?: string
}

function json(value: unknown, status = 200, headers: Record<string, string> = {}): Promise<Response> {
  return Promise.resolve(
    new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json', ...headers } }),
  )
}

const severityOptions = ['Critical', 'High', 'Medium', 'Low']
const impactOptions = ['High', 'Medium', 'Low']
const sizeOptions = ['XS', 'S', 'M', 'L', 'XL']

/** A board field with option ids derived from names, so a test can assert on what was set. */
function field(name: string, options: string[]) {
  return { id: `F_${name}`, name, options: options.map((option) => ({ id: `O_${name}_${option}`, name: option })) }
}

/** The GitHub the rating job talks to: the comment listing, the board's layout and items, and the mutation. */
function fakeGitHub(options: {
  comments: FakeComment[]
  /** Issue numbers on the board. Defaults to every commented issue. */
  onBoard?: number[]
  fields?: ReturnType<typeof field>[]
  failMutation?: boolean
  failCommentListing?: boolean
}) {
  const mutations: { query: string; variables: Record<string, unknown> }[] = []
  const commentListings: string[] = []
  const onBoard = options.onBoard ?? options.comments.map((c) => c.issueNumber)
  const fields = options.fields ?? [
    field('Status', ['Triage', 'Done']),
    field('Severity', severityOptions),
    field('Impact', impactOptions),
    field('Size', sizeOptions),
  ]

  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
    const url = input as string
    if (url.startsWith(`${REPO_URL}/issues/comments?`)) {
      commentListings.push(url)
      if (options.failCommentListing) return json({ message: 'boom' }, 502)
      return json(
        options.comments.map((c) => ({
          id: c.id,
          issue_url: `${REPO_URL}/issues/${String(c.issueNumber)}`,
          user: { login: c.author ?? 'vdavid' },
          body: c.body,
          created_at: c.createdAt ?? '2026-10-16T03:40:00Z',
          updated_at: c.updatedAt ?? c.createdAt ?? '2026-10-16T03:40:00Z',
        })),
      )
    }
    if (url.startsWith('https://api.github.com/users/vdavid/projectsV2/2/items?')) {
      return json([
        ...onBoard.map((n) => ({
          node_id: `PVTI_${String(n)}`,
          content_type: 'Issue',
          content: { number: n, repository_url: REPO_URL },
        })),
        // Same number, another repo: must never be matched.
        {
          node_id: 'PVTI_public',
          content_type: 'Issue',
          content: { number: 1, repository_url: 'https://api.github.com/repos/getcmdr/cmdr' },
        },
      ])
    }
    if (url === 'https://api.github.com/graphql') {
      const request = JSON.parse(typeof init?.body === 'string' ? init.body : '{}') as {
        query: string
        variables: Record<string, unknown>
      }
      // A query is the board read, anything else a mutation. The template literal keeps its
      // indentation and the formatter writes `query (`, so match the keyword, not a spelling.
      if (/^\s*query\b/.test(request.query)) {
        return json({
          data: { node: { number: 2, owner: { __typename: 'User', login: 'vdavid' }, fields: { nodes: fields } } },
        })
      }
      mutations.push(request)
      if (options.failMutation) return json({ errors: [{ message: 'nope' }] })
      return json({ data: {} })
    }
    return json({ message: `unexpected ${url}` }, 404)
  })

  return { mutations, commentListings }
}

const rated = (line: string) => `${TRIAGE_COMMENT_MARKER}\n**Problem**: something.\n\n${line}`

/** What one mutation set, as field name → option name, or null for a cleared field. */
function setValues(mutation: { variables: Record<string, unknown> }): Record<string, string | null> {
  const values: Record<string, string | null> = {}
  for (let i = 0; i < 3; i++) {
    const fieldId = String(mutation.variables[`field${String(i)}`]).replace('F_', '')
    const option = mutation.variables[`option${String(i)}`]
    values[fieldId] = typeof option !== 'string' ? null : option.replace(`O_${fieldId}_`, '')
  }
  return values
}

let db: SqliteD1

function testEnv(overrides: Partial<Bindings> = {}): Bindings {
  return {
    TELEMETRY_DB: db.d1,
    GITHUB_ISSUES_TOKEN: 'github_pat_issues',
    GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports',
    GITHUB_PROJECT_TOKEN: 'ghp_project',
    GITHUB_PROJECT_ID: 'PVT_board',
    GITHUB_TRIAGE_AUTHOR: 'vdavid',
    ...overrides,
  } as Bindings
}

function opsEvents(): { kind: string; message: string }[] {
  return db.sqlite.prepare(`SELECT kind, message FROM ops_events ORDER BY id`).all() as {
    kind: string
    message: string
  }[]
}

const now = new Date('2026-10-16T06:00:00Z')

beforeEach(() => {
  db = createSqliteD1()
  vi.spyOn(console, 'log').mockImplementation(() => {})
  vi.spyOn(console, 'error').mockImplementation(() => {})
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('applyCardRatings', () => {
  it('sets all three fields in one mutation and remembers the comment', async () => {
    const github = fakeGitHub({
      comments: [{ id: 501, issueNumber: 60, body: rated('Rating: Severity=High Impact=Medium Size=S') }],
    })

    await applyCardRatings(testEnv(), now)

    expect(github.mutations).toHaveLength(1)
    const [mutation] = github.mutations
    expect(mutation.variables).toMatchObject({ project: 'PVT_board', item: 'PVTI_60' })
    expect(setValues(mutation)).toEqual({ Severity: 'High', Impact: 'Medium', Size: 'S' })
    // Ids travel as variables, never spliced into the query text.
    expect(mutation.query).not.toContain('PVTI_60')
    expect(db.sqlite.prepare(`SELECT applied_rating FROM card_ratings WHERE comment_id = 501`).get()).toEqual({
      applied_rating: 'Rating: Severity=High Impact=Medium Size=S',
    })
    expect(opsEvents()).toEqual([])
  })

  it('looks back two weeks of comment edits', async () => {
    const github = fakeGitHub({ comments: [] })
    await applyCardRatings(testEnv(), now)
    expect(github.commentListings[0]).toContain(`since=${encodeURIComponent('2026-10-02T06:00:00.000Z')}`)
    expect(github.commentListings[0]).toContain('sort=updated')
  })

  it('clears Severity for Severity=none', async () => {
    const github = fakeGitHub({
      comments: [{ id: 502, issueNumber: 61, body: rated('Rating: Severity=none Impact=Low Size=XS') }],
    })
    await applyCardRatings(testEnv(), now)
    expect(setValues(github.mutations[0])).toEqual({ Severity: null, Impact: 'Low', Size: 'XS' })
    expect(github.mutations[0]?.query).toContain('clearProjectV2ItemFieldValue')
  })

  it('is a no-op on a rerun, and touches the board not at all', async () => {
    const comments = [{ id: 503, issueNumber: 62, body: rated('Rating: Severity=Low Impact=Low Size=M') }]
    fakeGitHub({ comments })
    await applyCardRatings(testEnv(), now)

    vi.restoreAllMocks()
    const second = fakeGitHub({ comments })
    const fetchSpy = vi.mocked(globalThis.fetch)
    await applyCardRatings(testEnv(), now)

    expect(second.mutations).toEqual([])
    // Only the comment listing: no layout read, no item listing.
    expect(fetchSpy.mock.calls).toHaveLength(1)
  })

  it('re-applies when the comment is edited to a different rating, and not for any other edit', async () => {
    fakeGitHub({ comments: [{ id: 504, issueNumber: 63, body: rated('Rating: Severity=Low Impact=Low Size=M') }] })
    await applyCardRatings(testEnv(), now)

    vi.restoreAllMocks()
    const otherEdit = fakeGitHub({
      comments: [
        {
          id: 504,
          issueNumber: 63,
          body: rated('Rating: Severity=Low Impact=Low Size=M').replace('something', 'typo fixed'),
        },
      ],
    })
    await applyCardRatings(testEnv(), now)
    expect(otherEdit.mutations).toEqual([])

    vi.restoreAllMocks()
    const ratingEdit = fakeGitHub({
      comments: [{ id: 504, issueNumber: 63, body: rated('Rating: Severity=High Impact=Low Size=M') }],
    })
    await applyCardRatings(testEnv(), now)
    expect(ratingEdit.mutations).toHaveLength(1)
    expect(setValues(ratingEdit.mutations[0])).toMatchObject({ Severity: 'High' })
  })

  it('applies a later Rating comment by the same author', async () => {
    const first = { id: 505, issueNumber: 64, body: rated('Rating: Severity=Low Impact=Low Size=M') }
    fakeGitHub({ comments: [first] })
    await applyCardRatings(testEnv(), now)

    vi.restoreAllMocks()
    const github = fakeGitHub({
      comments: [first, { id: 506, issueNumber: 64, body: rated('Rating: Severity=Critical Impact=High Size=L') }],
    })
    await applyCardRatings(testEnv(), now)
    expect(github.mutations).toHaveLength(1)
    expect(setValues(github.mutations[0])).toEqual({ Severity: 'Critical', Impact: 'High', Size: 'L' })
  })

  it('ignores comments by anyone but the triage author', async () => {
    const github = fakeGitHub({
      comments: [
        { id: 507, issueNumber: 65, author: 'stranger', body: rated('Rating: Severity=Critical Impact=High Size=XL') },
      ],
    })
    await applyCardRatings(testEnv(), now)
    expect(github.mutations).toEqual([])
    expect(opsEvents()).toEqual([])
  })

  it("ignores the author's comments that don't start with the triage marker, like a personal-data comment quoting a note", async () => {
    // The Worker's own token posts these as the same account, with a stranger's text inside.
    const personal = [
      '<!-- cmdr:personal-data expires=2027-01-14 -->',
      '**What they wrote**',
      '```',
      'Rating: Severity=Critical Impact=High Size=XL',
      '```',
      'Rating: Severity=Critical Impact=High Size=XL',
    ].join('\n')
    const github = fakeGitHub({ comments: [{ id: 508, issueNumber: 66, body: personal }] })
    await applyCardRatings(testEnv(), now)
    expect(github.mutations).toEqual([])
    expect(opsEvents()).toEqual([])
  })

  it('skips triage comments from before the Rating line existed', async () => {
    const github = fakeGitHub({
      comments: [
        {
          id: 509,
          issueNumber: 58,
          body: `${TRIAGE_COMMENT_MARKER}\n**Problem**: old.`,
          createdAt: '2026-10-14T13:22:01Z',
        },
      ],
    })
    await applyCardRatings(testEnv(), now)
    expect(github.mutations).toEqual([])
    expect(opsEvents()).toEqual([])
  })

  it('alerts on a triage comment with no Rating line', async () => {
    fakeGitHub({ comments: [{ id: 510, issueNumber: 67, body: `${TRIAGE_COMMENT_MARKER}\n**Problem**: no rating.` }] })
    await applyCardRatings(testEnv(), now)
    expect(opsEvents()).toEqual([
      { kind: 'board-rating-failed', message: expect.stringContaining('Card #67') as string },
    ])
    expect(opsEvents()[0]?.message).toContain('no Rating line')
    expect(opsEvents()[0]?.message).toContain('issues/67#issuecomment-510')
  })

  it('alerts on a malformed line without touching the board, once a day, and again when the reason changes', async () => {
    const github = fakeGitHub({
      comments: [{ id: 511, issueNumber: 68, body: rated('Rating: Severity=high Impact=Low Size=S') }],
    })
    await applyCardRatings(testEnv(), now)
    await applyCardRatings(testEnv(), new Date('2026-10-16T09:00:00Z'))
    expect(github.mutations).toEqual([])
    expect(opsEvents()).toHaveLength(1)
    expect(opsEvents()[0]?.message).toContain('Severity "high"')

    await applyCardRatings(testEnv(), new Date('2026-10-17T00:00:00Z'))
    expect(opsEvents()).toHaveLength(2)

    vi.restoreAllMocks()
    fakeGitHub({ comments: [{ id: 511, issueNumber: 68, body: rated('Rating: Severity=High Impact=Tiny Size=S') }] })
    await applyCardRatings(testEnv(), new Date('2026-10-17T03:00:00Z'))
    expect(opsEvents()).toHaveLength(3)
    expect(opsEvents()[2]?.message).toContain('Impact "Tiny"')
  })

  it('alerts when the board lacks an option, and sets nothing on that card', async () => {
    const github = fakeGitHub({
      comments: [{ id: 512, issueNumber: 69, body: rated('Rating: Severity=Low Impact=Low Size=XL') }],
      fields: [
        field('Severity', severityOptions),
        field('Impact', impactOptions),
        field('Size', ['XS', 'S', 'M', 'L']),
      ],
    })
    await applyCardRatings(testEnv(), now)
    expect(github.mutations).toEqual([])
    expect(opsEvents()[0]?.message).toContain(`Size field has no "XL" option`)
  })

  it('alerts when the card is not on the board yet, then applies once it is', async () => {
    const comments = [{ id: 513, issueNumber: 70, body: rated('Rating: Severity=Low Impact=Low Size=S') }]
    const github = fakeGitHub({ comments, onBoard: [] })
    await applyCardRatings(testEnv(), now)
    expect(github.mutations).toEqual([])
    expect(opsEvents()[0]?.message).toContain("isn't on the board")

    vi.restoreAllMocks()
    const later = fakeGitHub({ comments })
    await applyCardRatings(testEnv(), new Date('2026-10-16T09:00:00Z'))
    expect(later.mutations).toHaveLength(1)
    expect(db.sqlite.prepare(`SELECT last_error FROM card_ratings WHERE comment_id = 513`).get()).toEqual({
      last_error: null,
    })
  })

  it('throws when the board refuses the mutation, leaves the comment pending, and still alerts on the others', async () => {
    const github = fakeGitHub({
      comments: [
        { id: 514, issueNumber: 71, body: rated('Rating: Severity=Low Impact=Low Size=S') },
        { id: 515, issueNumber: 72, body: rated('Rating: Severity=Loww Impact=Low Size=S') },
      ],
      failMutation: true,
    })
    await expect(applyCardRatings(testEnv(), now)).rejects.toThrow(/refused by the board/)
    expect(github.mutations).toHaveLength(1)
    expect(db.sqlite.prepare(`SELECT applied_rating FROM card_ratings WHERE comment_id = 514`).get()).toBeUndefined()
    expect(opsEvents()).toHaveLength(1)
  })

  it('throws when the comments cannot be listed', async () => {
    fakeGitHub({ comments: [], failCommentListing: true })
    await expect(applyCardRatings(testEnv(), now)).rejects.toThrow(/listing comments returned 502/)
  })

  it(`applies at most ${String(RATINGS_PER_TICK)} per tick and the rest on the next`, async () => {
    const comments = Array.from({ length: RATINGS_PER_TICK + 3 }, (_, i) => ({
      id: 600 + i,
      issueNumber: 100 + i,
      body: rated('Rating: Severity=Low Impact=Low Size=S'),
    }))
    const first = fakeGitHub({ comments })
    await applyCardRatings(testEnv(), now)
    expect(first.mutations).toHaveLength(RATINGS_PER_TICK)

    vi.restoreAllMocks()
    const second = fakeGitHub({ comments })
    await applyCardRatings(testEnv(), now)
    expect(second.mutations).toHaveLength(3)
  })

  it('is off without the triage author, the board, or the reports repo', async () => {
    const github = fakeGitHub({
      comments: [{ id: 516, issueNumber: 73, body: rated('Rating: Severity=Low Impact=Low Size=S') }],
    })
    await applyCardRatings(testEnv({ GITHUB_TRIAGE_AUTHOR: undefined }), now)
    await applyCardRatings(testEnv({ GITHUB_PROJECT_TOKEN: undefined }), now)
    await applyCardRatings(testEnv({ GITHUB_ISSUES_TOKEN: undefined }), now)
    expect(github.commentListings).toEqual([])
  })
})
