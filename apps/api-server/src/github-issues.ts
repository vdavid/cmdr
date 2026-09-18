/**
 * Files in-app error reports and feedback as issues in a PRIVATE GitHub repo, so triage happens on
 * one board instead of across Discord and an inbox.
 *
 * Two invariants hold this together, and both are load-bearing for the privacy policy
 * (`apps/website/src/pages/privacy-policy.astro`):
 *
 * 1. **The repo must be private, checked on every write.** {@link fileIssue} probes the repo and
 *    refuses to post anything unless GitHub answers `private: true`. It fails CLOSED: a lookup that
 *    errors, 404s, or omits the field writes nothing. That's what stops a misconfigured
 *    `GITHUB_ISSUES_REPO` (or a repo someone flips to public) from publishing a user's note.
 * 2. **Personal data lives only in a comment that expires.** The issue BODY carries technical facts
 *    that name nobody and may be kept indefinitely. Anything a person wrote or attached goes in a
 *    separate comment stamped with its own deletion date, which the retention sweep deletes on time
 *    (`scheduled.ts`). Deleting a comment removes it outright; editing a body would leave the
 *    original text in GitHub's revision history, which is why the split exists at all.
 *
 * Untrusted text is always wrapped with {@link fencedBlock}, so a note can't become markup, an
 * `@mention`, or a cross-repo reference.
 */

import { formatBytes, type Bindings } from './types'

const GITHUB_API = 'https://api.github.com'

/**
 * GitHub rejects API requests without a User-Agent. Naming the Worker makes a misbehaving
 * integration identifiable in GitHub's own logs rather than anonymous traffic.
 */
const USER_AGENT = 'cmdr-api-server'

/**
 * Hard ceiling GitHub puts on an issue body and on a comment. We stay under it with margin rather
 * than at it, because the chrome around a note is counted too.
 */
const GITHUB_TEXT_LIMIT = 65_536
const SAFE_TEXT_BUDGET = GITHUB_TEXT_LIMIT - 5_536

/**
 * The prefix every personal-data comment starts with. The retention sweep matches on this to find
 * what it may delete, so ❌ never post personal data in a comment without it: an unmarked comment is
 * invisible to the sweep and would outlive its promise.
 */
export const PERSONAL_COMMENT_MARKER = '<!-- cmdr:personal-data'

/**
 * Default life of a personal-data comment, in days. Matches the error-report retention promise, and
 * it is the DEFAULT so that a caller who forgets to state one deletes too early rather than too
 * late.
 */
const DEFAULT_PERSONAL_RETENTION_DAYS = 90

/** Where issues are filed. Absent config means the integration is off, never a fallback repo. */
export interface IssueTarget {
  token: string
  owner: string
  repo: string
}

/**
 * Read the target from config. Returns null (integration off) unless BOTH a token and an
 * `owner/name` repo are present, so a half-configured environment posts nothing instead of guessing.
 */
export function resolveIssueTarget(env: Bindings): IssueTarget | null {
  const token = env.GITHUB_ISSUES_TOKEN
  const repo = env.GITHUB_ISSUES_REPO
  if (!token || !repo) return null

  const parts = repo.split('/')
  if (parts.length !== 2) return null
  const [owner, name] = parts
  if (!owner || !name) return null

  return { token, owner, repo: name }
}

/** Narrow a parsed JSON value to something whose keys can be read. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function githubHeaders(token: string): Record<string, string> {
  return {
    authorization: `Bearer ${token}`,
    accept: 'application/vnd.github+json',
    'x-github-api-version': '2022-11-28',
    'user-agent': USER_AGENT,
  }
}

/**
 * Wrap untrusted text in a code fence long enough that the text cannot close it. Everything inside
 * is inert: no headings, no `@mentions` that would notify a stranger, no `#123` cross-references.
 */
export function fencedBlock(text: string): string {
  let longestRun = 0
  let run = 0
  for (const char of text) {
    run = char === '`' ? run + 1 : 0
    if (run > longestRun) longestRun = run
  }
  const fence = '`'.repeat(Math.max(3, longestRun + 1))
  return `${fence}\n${text}\n${fence}`
}

/** Cut `text` to `maxChars`, marking the cut so nobody reads a truncated note as the whole note. */
function truncate(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text
  return `${text.slice(0, maxChars)}\n\n[truncated at ${String(maxChars)} characters]`
}

/** `YYYY-MM-DD`, `days` from now. The stamp the retention sweep reads. */
function expiryDate(days: number): string {
  return new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10)
}

/** Read the deletion date off a personal-data comment. Null when it carries no valid stamp. */
export function personalCommentExpiry(commentBody: string): string | null {
  const match = /^<!-- cmdr:personal-data expires=(\d{4}-\d{2}-\d{2}) -->/.exec(commentBody)
  return match?.[1] ?? null
}

/** What a personal-data comment may carry. Every field is optional; all-absent means no comment. */
export interface PersonalCommentInput {
  userNote?: string | null
  email?: string | null
  downloadUrl?: string | null
  linkTtlDays?: number
  retentionDays?: number
  /**
   * An absolute `YYYY-MM-DD` deletion date, overriding `retentionDays`. An amendment uses this to
   * expire with the report it amends rather than 90 days from when it was written, which would
   * outlive the bundle it belongs to.
   */
  expiresOn?: string
}

/**
 * Build the one comment that holds everything a person wrote or attached, stamped with the date the
 * retention sweep must delete it. Returns null when there is nothing personal to store, so an
 * anonymous auto-report never gets an empty comment that would later need deleting.
 */
export function buildPersonalComment(input: PersonalCommentInput): string | null {
  const note = input.userNote?.trim() ?? ''
  const email = input.email?.trim() ?? ''
  const hasLink = Boolean(input.downloadUrl)
  if (!note && !email && !hasLink) return null

  const retentionDays = input.retentionDays ?? DEFAULT_PERSONAL_RETENTION_DAYS
  const expiresOn = input.expiresOn ?? expiryDate(retentionDays)
  const sections: string[] = [
    `${PERSONAL_COMMENT_MARKER} expires=${expiresOn} -->`,
    `_Deleted automatically on ${expiresOn}, per the privacy policy. The issue above stays._`,
  ]

  if (note) {
    sections.push('**What they wrote**', fencedBlock(truncate(note, SAFE_TEXT_BUDGET / 2)))
  }
  if (email) {
    sections.push('**Reply to**', fencedBlock(email))
  }
  if (input.downloadUrl) {
    const ttl = input.linkTtlDays
    const suffix = ttl ? ` (link expires in ${String(ttl)} days)` : ''
    sections.push('**Bundle**', `[Download the bundle](${input.downloadUrl})${suffix}`)
  }

  return truncate(sections.join('\n\n'), SAFE_TEXT_BUDGET)
}

/**
 * `macOS <version>`, without doubling the prefix when the client already sent one.
 *
 * Older clients send a bare `15.3.1`, some send `macOS 10.15.8`. ERR-KVERS was titled
 * "macOS macOS 10.15.8" before this existed.
 */
function macOsLabel(osVersion: string): string {
  const trimmed = osVersion.trim()
  return /^macos\b/i.test(trimmed) ? trimmed : `macOS ${trimmed}`
}

/** The three parts of an issue, built before anything touches the network. */
export interface IssueContent {
  title: string
  body: string
  labels: string[]
}

/** Technical facts about one uploaded bundle. Nothing here identifies a person. */
export interface ErrorReportIssueInput {
  id: string
  kind: 'user' | 'auto'
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  sizeBytes: number
  r2Key: string
  uploadedUnixSeconds: number
  userNote?: string | null
  email?: string | null
}

/**
 * The issue for one error report. The note and the reply-to address are deliberately NOT here: they
 * belong in the expiring comment, because this body is kept for as long as the bug is.
 */
export function buildErrorReportIssue(input: ErrorReportIssueInput): IssueContent {
  const kindWord = input.kind === 'user' ? 'user' : 'auto'
  const uploadedAt = new Date(input.uploadedUnixSeconds * 1000).toISOString().replace('T', ' ').slice(0, 16)

  const body = [
    `**Report id**: \`${input.id}\``,
    `**Kind**: ${input.kind === 'user' ? 'hand-written' : 'auto-sent'}`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)} · **Arch**: ${input.arch}`,
    `**Bundle**: ${formatBytes(input.sizeBytes)}, uploaded ${uploadedAt} UTC`,
    `**R2 key**: \`${input.r2Key}\``,
    '',
    'The note, the reply-to address, and the download link are in the first comment, which is deleted',
    'after 90 days. To fetch the bundle after the link expires, see',
    '`docs/tooling/feedback-and-error-digest.md`.',
  ].join('\n')

  const labels = ['error-report']
  if (input.email?.trim()) labels.push('needs-reply')

  return {
    title: `${input.id}: ${kindWord} report on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body,
    labels,
  }
}

/** One in-app feedback submission, already stored in D1. */
export interface FeedbackIssueInput {
  rowId: number
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  feedback?: string | null
  email?: string | null
}

/**
 * The issue for one feedback message. Unlike an error report, the MESSAGE lives in the body: the
 * privacy policy keeps feedback text so it can be acted on, and only the reply-to address expires.
 * So only the address goes in the expiring comment.
 */
export function buildFeedbackIssue(input: FeedbackIssueInput): IssueContent {
  const message = input.feedback?.trim() ?? ''
  const body = [
    `**Feedback row**: \`${String(input.rowId)}\` in the D1 \`feedback\` table`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)}`,
    '',
    '**What they wrote**',
    '',
    fencedBlock(truncate(message, SAFE_TEXT_BUDGET / 2)),
    '',
    input.email?.trim()
      ? 'A reply-to address is in the first comment, which is deleted after two years.'
      : 'No reply-to address was attached.',
  ].join('\n')

  const labels = ['feedback']
  if (input.email?.trim()) labels.push('needs-reply')

  return {
    title: `Feedback #${String(input.rowId)} on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body: truncate(body, SAFE_TEXT_BUDGET),
    labels,
  }
}

/**
 * Whether GitHub says this repo is private, right now.
 *
 * Fails CLOSED in every uncertain case: a network error, a non-200, or a payload without an explicit
 * boolean `private: true` all answer false. This is the gate that keeps a user's note off a public
 * repo, so "we couldn't tell" has to mean "don't write".
 */
export async function isRepoPrivate(target: IssueTarget): Promise<boolean> {
  try {
    const response = await fetch(`${GITHUB_API}/repos/${target.owner}/${target.repo}`, {
      headers: githubHeaders(target.token),
    })
    if (!response.ok) {
      console.error(`GitHub issues: repo probe returned ${String(response.status)}; refusing to write`)
      return false
    }
    const repo: unknown = await response.json()
    // Strictly `true`, never truthy: "private" as a string, or a missing field, must not pass.
    return isRecord(repo) && repo['private'] === true
  } catch (e) {
    console.error('GitHub issues: repo probe failed; refusing to write', e)
    return false
  }
}

/** An issue plus the optional personal half that gets its own, expiring, comment. */
export interface FileIssueInput extends IssueContent {
  personalComment: string | null
}

/**
 * Create one issue, then attach its personal-data comment.
 *
 * Returns the issue number, or null when nothing was created. The privacy probe runs FIRST and a
 * negative answer returns before any write: that ordering is the whole guarantee, so ❌ never reorder
 * these two calls or make the probe conditional.
 *
 * A comment that fails to post still leaves the issue: the technical record is worth keeping, and
 * the note is not lost (it's in the bundle in R2, or the D1 row).
 */
export async function fileIssue(target: IssueTarget, input: FileIssueInput): Promise<number | null> {
  if (!(await isRepoPrivate(target))) {
    console.error(
      `GitHub issues: ${target.owner}/${target.repo} is not private (or could not be verified); nothing was written`,
    )
    return null
  }

  let issueNumber: number
  try {
    const response = await fetch(`${GITHUB_API}/repos/${target.owner}/${target.repo}/issues`, {
      method: 'POST',
      headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
      body: JSON.stringify({ title: input.title, body: input.body, labels: input.labels }),
    })
    if (!response.ok) {
      console.error(`GitHub issues: create failed with ${String(response.status)}`)
      return null
    }
    const created: unknown = await response.json()
    const number = isRecord(created) ? created['number'] : undefined
    if (typeof number !== 'number') {
      console.error('GitHub issues: create returned no issue number')
      return null
    }
    issueNumber = number
  } catch (e) {
    console.error('GitHub issues: create threw', e)
    return null
  }

  if (input.personalComment) {
    try {
      const response = await fetch(
        `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/${String(issueNumber)}/comments`,
        {
          method: 'POST',
          headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
          body: JSON.stringify({ body: input.personalComment }),
        },
      )
      if (!response.ok) {
        console.error(
          `GitHub issues: personal comment failed with ${String(response.status)} on #${String(issueNumber)}`,
        )
      }
    } catch (e) {
      console.error(`GitHub issues: personal comment threw on #${String(issueNumber)}`, e)
    }
  }

  return issueNumber
}

/**
 * Issues one source may file per UTC day.
 *
 * Real volume is a handful, so this never touches legitimate traffic. It exists because `kind` and
 * `buildMode` come from the client's manifest: a buggy or hostile build that labels every auto-send
 * as hand-written costs 20 issues and then nothing. Nothing is lost when it trips, since Discord,
 * R2, and D1 all still have the report.
 */
const DAILY_ISSUE_CAP = 20

/** Counted per source so a feedback burst can never silence error reports, or the other way round. */
export type IssueSource = 'error-report' | 'feedback'

/** Day-scoped keys outlive their day, then expire on their own. Mirrors `error-report-intake.ts`. */
const DAY_KEY_TTL_SECONDS = 48 * 60 * 60

/**
 * Take one slot from a source's daily allowance. Racy like the other KV counters here, which at
 * worst shifts the cutoff by an issue or two.
 */
async function claimIssueSlot(kv: KVNamespace, source: IssueSource, date: string): Promise<boolean> {
  const key = `gh_issue_count:${source}:${date}`
  const count = parseInt((await kv.get(key)) ?? '0', 10) + 1
  await kv.put(key, String(count), { expirationTtl: DAY_KEY_TTL_SECONDS })
  return count <= DAILY_ISSUE_CAP
}

/** Today in UTC, `YYYY-MM-DD`. */
function todayUtc(): string {
  return new Date().toISOString().slice(0, 10)
}

/**
 * File one error report, applying the rules about which reports earn a card.
 *
 * Skipped without a word: auto-sends (one bad install makes dozens, and Discord absorbs those) and
 * debug builds (our own E2E traffic). Returns the issue number, or null when nothing was filed for
 * any reason.
 */
export async function fileErrorReportIssue(
  env: Bindings,
  input: ErrorReportIssueInput & { downloadUrl?: string | null; linkTtlDays?: number },
): Promise<number | null> {
  const target = resolveIssueTarget(env)
  if (!target) return null
  if (input.kind !== 'user') return null
  if (input.buildMode === 'debug') return null
  if (!(await claimIssueSlot(env.ERROR_REPORT_META, 'error-report', todayUtc()))) {
    console.error('GitHub issues: daily error-report cap reached; not filing')
    return null
  }

  const content = buildErrorReportIssue(input)
  const issueNumber = await fileIssue(target, {
    ...content,
    personalComment: buildPersonalComment({
      userNote: input.userNote,
      email: input.email,
      downloadUrl: input.downloadUrl,
      linkTtlDays: input.linkTtlDays,
      retentionDays: ERROR_REPORT_RETENTION_DAYS,
    }),
  })

  // Remembered so a later amendment lands on this same card. A failure here costs the amendment its
  // comment, never the issue, so it's logged rather than propagated.
  if (issueNumber !== null) {
    try {
      await rememberIssueNumber(env.ERROR_REPORT_META, input.id, issueNumber)
    } catch (e) {
      console.error('GitHub issues: remembering the issue number failed; amendments will not find it', e)
    }
  }

  return issueNumber
}

/**
 * File one feedback message. Debug builds are skipped; everything else earns a card, because
 * feedback is hand-written by definition.
 */
export async function fileFeedbackIssue(env: Bindings, input: FeedbackIssueInput): Promise<number | null> {
  const target = resolveIssueTarget(env)
  if (!target) return null
  if (input.buildMode === 'debug') return null
  if (!(await claimIssueSlot(env.ERROR_REPORT_META, 'feedback', todayUtc()))) {
    console.error('GitHub issues: daily feedback cap reached; not filing')
    return null
  }

  const content = buildFeedbackIssue(input)
  return fileIssue(target, {
    ...content,
    // Only the reply-to address expires; the message itself lives in the body above.
    personalComment: buildPersonalComment({
      email: input.email,
      retentionDays: FEEDBACK_EMAIL_RETENTION_DAYS,
    }),
  })
}

/**
 * KV key remembering which issue one report got, so an amendment months later can find its card.
 *
 * Its OWN key rather than a field on the `report:{id}` index: that entry is written before the 200
 * and carries the amend credential's hash, so adding to it would mean a read-modify-write against
 * an eventually-consistent store, with the credential as the thing at risk. A separate key is
 * written once and read once, and a miss simply means no comment.
 */
function issueNumberKey(id: string): string {
  return `gh_issue:${id}`
}

/**
 * Remember an issue number for {@link recallIssueNumber}. TTL matches the report index and the R2
 * lifecycle, so the pointer never outlives the report it points at.
 */
export async function rememberIssueNumber(kv: KVNamespace, id: string, issueNumber: number): Promise<void> {
  await kv.put(issueNumberKey(id), String(issueNumber), { expirationTtl: REPORT_RETENTION_SECONDS })
}

/** The issue one report got, or null when it never got one (auto-send, debug build, cap, outage). */
export async function recallIssueNumber(kv: KVNamespace, id: string): Promise<number | null> {
  const raw = await kv.get(issueNumberKey(id))
  if (raw === null) return null
  const parsed = Number(raw)
  return Number.isInteger(parsed) ? parsed : null
}

/** 90 days, the window a report and everything pointing at it share. */
const REPORT_RETENTION_SECONDS = 90 * 24 * 60 * 60

/** One amendment, as it goes onto the card. */
export interface AmendmentCommentInput {
  note?: string | null
  email?: string | null
  amendmentCount: number
  /** The ORIGINAL report's deletion date, so an amendment can't outlive the bundle it belongs to. */
  expiresOn: string
}

/**
 * The comment one amendment adds to its report's card.
 *
 * Always personal data by definition (an amendment is a note, an address, or both), so it is always
 * stamped and always swept. It carries the original report's date rather than its own: a report
 * amended on day 80 still has to disappear on day 90.
 */
export function buildAmendmentComment(input: AmendmentCommentInput): string {
  const ordinal = `Amendment #${String(input.amendmentCount)}`
  const comment = buildPersonalComment({
    userNote: input.note,
    email: input.email,
    expiresOn: input.expiresOn,
  })
  // `buildPersonalComment` returns null only when there is nothing personal, and the amend route
  // rejects a body carrying neither a note nor an address, so this cannot be empty in practice.
  const parts = comment ?? `${PERSONAL_COMMENT_MARKER} expires=${input.expiresOn} -->`
  return parts.replace('\n\n', `\n\n**${ordinal}**, added by the reporter after sending.\n\n`)
}

/**
 * Add a comment to the issue one report got. Returns whether anything was posted.
 *
 * Runs the privacy probe again through {@link fileIssue}'s own gate rather than trusting that the
 * repo was private when the issue was filed: months can pass between an upload and its amendment,
 * and the answer is allowed to have changed.
 */
export async function commentOnReportIssue(
  env: Bindings,
  id: string,
  body: string,
  options: { addLabels?: string[] } = {},
): Promise<boolean> {
  const target = resolveIssueTarget(env)
  if (!target) return false

  const issueNumber = await recallIssueNumber(env.ERROR_REPORT_META, id)
  if (issueNumber === null) return false

  if (!(await isRepoPrivate(target))) {
    console.error(`GitHub issues: ${target.owner}/${target.repo} is not private; the amendment was not posted`)
    return false
  }

  try {
    const response = await fetch(
      `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/${String(issueNumber)}/comments`,
      {
        method: 'POST',
        headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
        body: JSON.stringify({ body }),
      },
    )
    if (!response.ok) {
      console.error(
        `GitHub issues: amendment comment failed with ${String(response.status)} on #${String(issueNumber)}`,
      )
      return false
    }
  } catch (e) {
    console.error(`GitHub issues: amendment comment threw on #${String(issueNumber)}`, e)
    return false
  }

  if (options.addLabels?.length) {
    await addIssueLabels(target, issueNumber, options.addLabels)
  }
  return true
}

/**
 * Add labels to an issue, leaving the ones already on it alone: GitHub's add endpoint is additive
 * and idempotent, so re-adding one is a no-op rather than an error.
 *
 * Failure is logged and swallowed. The comment it accompanies is already posted, and losing a label
 * must not read to the caller as losing the amendment.
 */
async function addIssueLabels(target: IssueTarget, issueNumber: number, labels: string[]): Promise<void> {
  try {
    const response = await fetch(
      `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/${String(issueNumber)}/labels`,
      {
        method: 'POST',
        headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
        body: JSON.stringify({ labels }),
      },
    )
    if (!response.ok) {
      console.error(`GitHub issues: labelling #${String(issueNumber)} returned ${String(response.status)}`)
    }
  } catch (e) {
    console.error(`GitHub issues: labelling #${String(issueNumber)} threw`, e)
  }
}

/**
 * The two retention promises this module has to keep, from
 * `apps/website/src/pages/privacy-policy.astro` § "How long we keep your data". They mirror the
 * constants the D1 retention sweep uses in `scheduled.ts`; change one and the other has to follow.
 */
const ERROR_REPORT_RETENTION_DAYS = 90
const FEEDBACK_EMAIL_RETENTION_DAYS = 730

/** The three fields we read off a GitHub comment, once it has been checked to actually have them. */
interface GitHubComment {
  id: number
  body: string
  issueUrl: string
}

/** Narrow one element of GitHub's comment listing, or null when it isn't shaped as we need. */
function asGitHubComment(raw: unknown): GitHubComment | null {
  if (!raw || typeof raw !== 'object') return null
  const record = raw as Record<string, unknown>
  const id = record['id']
  const body = record['body']
  const issueUrl = record['issue_url']
  if (typeof id !== 'number' || typeof body !== 'string') return null
  return { id, body, issueUrl: typeof issueUrl === 'string' ? issueUrl : '' }
}

/**
 * The issue number at the end of a comment's `issue_url`. Zero when it can't be read, which only
 * costs the log line its issue reference: the comment id is what the delete actually needs.
 */
function issueNumberFromUrl(issueUrl: string): number {
  const last = issueUrl.split('/').pop() ?? ''
  const parsed = Number(last)
  return Number.isFinite(parsed) ? parsed : 0
}

/** One comment the retention sweep may delete, with the date it was promised to go. */
export interface ExpiringComment {
  issueNumber: number
  commentId: number
  expiresOn: string
}

/**
 * Every personal-data comment in the repo that is due for deletion on `today` or earlier.
 *
 * Walks issue comments repo-wide (newest issues carry the youngest stamps, but a 730-day feedback
 * stamp can sit behind 90-day error-report ones, so the walk can't stop early on the first
 * unexpired comment).
 */
export async function listExpiredPersonalComments(target: IssueTarget, today: string): Promise<ExpiringComment[]> {
  const due: ExpiringComment[] = []
  const perPage = 100

  for (let page = 1; page <= 20; page++) {
    const url = `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/comments?per_page=${String(perPage)}&page=${String(page)}`
    const response = await fetch(url, { headers: githubHeaders(target.token) })
    if (!response.ok) {
      throw new Error(`GitHub issues: listing comments returned ${String(response.status)}`)
    }
    const payload: unknown = await response.json()
    const comments: unknown[] = Array.isArray(payload) ? payload : []
    if (comments.length === 0) return due

    for (const raw of comments) {
      const comment = asGitHubComment(raw)
      if (!comment) continue
      const expiresOn = personalCommentExpiry(comment.body)
      if (!expiresOn || expiresOn > today) continue
      due.push({ issueNumber: issueNumberFromUrl(comment.issueUrl), commentId: comment.id, expiresOn })
    }

    if (comments.length < perPage) return due
  }

  return due
}

/** Delete one comment. Throws on failure so the cron's alarm path reports it. */
export async function deleteComment(target: IssueTarget, commentId: number): Promise<void> {
  const response = await fetch(
    `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/comments/${String(commentId)}`,
    { method: 'DELETE', headers: githubHeaders(target.token) },
  )
  // 404 means it's already gone, which is the state we wanted.
  if (!response.ok && response.status !== 404) {
    throw new Error(`GitHub issues: deleting comment ${String(commentId)} returned ${String(response.status)}`)
  }
}
