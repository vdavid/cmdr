/**
 * `POST /webhook/github`: GitHub's `issues` webhook from the PUBLIC repo, which puts a newly opened
 * issue on the backlog board. Why a webhook at all, and why this token is safe: `project-board.ts`.
 *
 * The route answers 204 for everything it understands, including events it deliberately ignores. A
 * webhook that answers 4xx to a normal delivery shows up as a red cross in GitHub's UI forever, so
 * only a signature that doesn't verify is refused.
 */

import { Hono, type Context } from 'hono'
import { readCappedBody, scheduleBackground, type Bindings } from './types'
import { addIssueToBoard, resolveBoardTarget, verifyGitHubWebhook } from './project-board'
import { postCronFailureNotification } from './discord'

const webhookGitHub = new Hono<{ Bindings: Bindings }>()

/**
 * Byte cap on a delivery. GitHub caps its own payloads at 25 MB, but an `issues` payload is a few
 * KB; 1 MB is far above a real one and well under what would strain the isolate.
 */
const MAX_WEBHOOK_BYTES = 1024 * 1024

/**
 * Bots whose issues don't belong on the board. Renovate keeps one long-lived "Dependency Dashboard"
 * issue open and edits it; it carries no labels, so a label filter can't catch it.
 */
const IGNORED_AUTHORS = new Set(['renovate[bot]', 'dependabot[bot]'])

/** The one issue this delivery is about, once it has passed every check. */
interface OpenedIssue {
  nodeId: string
  /** `owner/repo#123` for the alert, or `?` where the payload didn't say. */
  ref: string
}

/** A string field of a parsed payload, or undefined when it isn't one. */
function readString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined
}

function readRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : undefined
}

/**
 * The issue a delivery opened, or null when this delivery isn't one we act on: a different action,
 * a bot we skip, or a payload without a node id.
 *
 * Split out from the route so the route stays a sequence of guards; the two together were over the
 * complexity limit.
 */
function openedIssueFrom(body: string): OpenedIssue | null {
  let payload: Record<string, unknown> | undefined
  try {
    payload = readRecord(JSON.parse(body))
  } catch {
    return null
  }
  if (!payload || payload['action'] !== 'opened') return null

  const issue = readRecord(payload['issue'])
  if (!issue) return null

  const author = readString(readRecord(issue['user'])?.['login'])
  if (author && IGNORED_AUTHORS.has(author)) return null

  const nodeId = readString(issue['node_id'])
  if (!nodeId) return null

  const repo = readString(readRecord(payload['repository'])?.['full_name']) ?? '?'
  const number = issue['number']
  const numberText = typeof number === 'number' ? String(number) : '?'

  return { nodeId, ref: `${repo}#${numberText}` }
}

/** Read the signed body under the byte cap, or the error Response to send. */
async function readSignedBody(c: Context<{ Bindings: Bindings }>, secret: string): Promise<string | Response> {
  const rawBody = c.req.raw.body
  if (!rawBody) return c.json({ error: 'Missing request body' }, 400)

  const bytes = await readCappedBody(rawBody, MAX_WEBHOOK_BYTES)
  if (!bytes) return c.json({ error: 'Payload too large' }, 413)
  const body = new TextDecoder().decode(bytes)

  // The ONE thing that refuses. Verified before the body is parsed, so an unsigned payload never
  // reaches a parser.
  const signature = c.req.header('x-hub-signature-256') ?? ''
  if (!(await verifyGitHubWebhook(body, signature, secret))) {
    console.error('GitHub webhook: signature did not verify')
    return c.json({ error: 'Bad signature' }, 401)
  }
  return body
}

webhookGitHub.post('/webhook/github', async (c) => {
  const secret = c.env.GITHUB_WEBHOOK_SECRET
  if (!secret) {
    console.error('GitHub webhook: no GITHUB_WEBHOOK_SECRET configured; ignoring the delivery')
    return c.body(null, 204)
  }

  const body = await readSignedBody(c, secret)
  if (body instanceof Response) return body

  if (c.req.header('x-github-event') !== 'issues') return c.body(null, 204)

  const issue = openedIssueFrom(body)
  if (!issue) return c.body(null, 204)

  const target = resolveBoardTarget(c.env)
  if (!target) return c.body(null, 204)

  // Behind the 204: GitHub's delivery should not wait on two GraphQL round-trips, and a slow
  // response is what turns into a retry and a duplicate delivery.
  await scheduleBackground(
    c,
    addIssueToBoard(target, issue.nodeId).then(async (added) => {
      if (added) return
      // A classic PAT expires, and the failure mode is issues quietly not reaching the board. This
      // is the alarm that makes that loud, on the same channel a failed cron job uses.
      if (c.env.DISCORD_WEBHOOK_URL) {
        await postCronFailureNotification(c.env.DISCORD_WEBHOOK_URL, {
          job: 'Add issue to project board',
          when: new Date().toISOString(),
          detail: `${issue.ref} did not reach the board. Check whether GITHUB_PROJECT_TOKEN has expired.`,
        })
      }
    }),
  )

  return c.body(null, 204)
})

export { webhookGitHub }
