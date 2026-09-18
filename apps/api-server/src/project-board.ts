/**
 * Puts newly opened issues from the PUBLIC repo onto the Cmdr backlog project board.
 *
 * ## Why a webhook rather than the project's own workflow
 *
 * GitHub's built-in "Auto-add to project" is the better mechanism, and it is already spoken for:
 * the free plan allows exactly ONE, and it serves the PRIVATE `cmdr-reports` repo, which nothing
 * else can reach. Adding a private repo's issue to a user-owned project needs a classic PAT with
 * `repo` scope, which is full read/write over every repo the account owns; `github-issues.ts`
 * exists to keep a user's note away from exactly that kind of credential.
 *
 * ## Why this token is safe to hold
 *
 * The token here carries `project` scope ALONE. Verified against the live API on 2026-09-19: it
 * resolves public issues and cannot resolve a private repo's issue at all ("Could not resolve to a
 * node"). So a leak of this secret exposes the ability to rearrange project boards and nothing
 * else: no repository contents, no private data, no user reports. ❌ Never widen it to `repo` to
 * make private issues work here; the right answer for those is the built-in workflow, or moving the
 * project to an organization where a fine-grained token can be scoped properly.
 *
 * ## Fail loudly
 *
 * A classic PAT expires. The failure mode is issues quietly not reaching the board, so every
 * failure returns false and the route alerts Discord, per the CLAUDE.md rule that a failure has to
 * leave the Worker.
 */

import { constantTimeEqual } from './licensing/paddle'
import type { Bindings } from './types'

const GITHUB_GRAPHQL = 'https://api.github.com/graphql'

/** GitHub rejects API requests without a User-Agent. */
const USER_AGENT = 'cmdr-api-server'

/**
 * The Triage option on the board's Status field. A default rather than a required var: the board
 * this ships against has it, and a wrong id costs the column, never the item.
 */
export const TRIAGE_OPTION_ID = '6d6cdf53'

/** Where an issue goes. Absent config means the integration is off, never a guessed board. */
export interface BoardTarget {
  token: string
  projectId: string
  /** Optional: with no Status field the item is still added, just uncategorized. */
  statusFieldId?: string
  triageOptionId?: string
}

/**
 * Read the board target from config. Returns null unless BOTH a token and a project id are present,
 * so a half-configured environment adds nothing rather than guessing at a board.
 */
export function resolveBoardTarget(env: Bindings): BoardTarget | null {
  const token = env.GITHUB_PROJECT_TOKEN
  const projectId = env.GITHUB_PROJECT_ID
  if (!token || !projectId) return null

  return {
    token,
    projectId,
    statusFieldId: env.GITHUB_PROJECT_STATUS_FIELD_ID,
    triageOptionId: env.GITHUB_PROJECT_TRIAGE_OPTION_ID ?? TRIAGE_OPTION_ID,
  }
}

/**
 * Whether `signature` is the HMAC GitHub would have produced for `body` under `secret`.
 *
 * GitHub sends `sha256=<hex>` in `x-hub-signature-256`. Compared through `constantTimeEqual`, and
 * the `sha256=` prefix is required rather than tolerated: accepting a bare hex digest would also
 * accept the older, weaker `sha1` header if it were ever routed here.
 */
export async function verifyGitHubWebhook(body: string, signature: string, secret: string): Promise<boolean> {
  if (!signature.startsWith('sha256=')) return false

  const key = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const mac = await crypto.subtle.sign('HMAC', key, new TextEncoder().encode(body))
  const expected = [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, '0')).join('')

  return constantTimeEqual(signature.slice('sha256='.length), expected)
}

/** One GraphQL call. Returns the parsed data, or null when the call or the query failed. */
async function graphql(token: string, query: string): Promise<Record<string, unknown> | null> {
  try {
    const response = await fetch(GITHUB_GRAPHQL, {
      method: 'POST',
      headers: {
        authorization: `Bearer ${token}`,
        'content-type': 'application/json',
        'user-agent': USER_AGENT,
      },
      body: JSON.stringify({ query }),
    })
    if (!response.ok) {
      console.error(`Project board: GraphQL returned ${String(response.status)}`)
      return null
    }
    const parsed: unknown = await response.json()
    const payload = typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : {}
    if (payload['errors']) {
      console.error(`Project board: GraphQL errors ${JSON.stringify(payload['errors']).slice(0, 300)}`)
      return null
    }
    const data = payload['data']
    return typeof data === 'object' && data !== null ? (data as Record<string, unknown>) : null
  } catch (e) {
    console.error('Project board: GraphQL threw', e)
    return null
  }
}

/**
 * Add one issue to the board and drop it in Triage. Returns whether the ITEM landed.
 *
 * The status update is deliberately not part of that answer: an item on the board in the wrong
 * column is a cosmetic problem, while an item that never arrived is the thing worth alerting on.
 *
 * Adding an issue already on the board returns its existing item rather than a duplicate, so a
 * redelivered webhook is harmless.
 */
export async function addIssueToBoard(target: BoardTarget, issueNodeId: string): Promise<boolean> {
  const added = await graphql(
    target.token,
    `mutation { addProjectV2ItemById(input: {projectId: "${target.projectId}", contentId: "${issueNodeId}"}) { item { id } } }`,
  )

  const item = (added?.['addProjectV2ItemById'] as { item?: { id?: unknown } } | undefined)?.item
  const itemId = typeof item?.id === 'string' ? item.id : null
  if (!itemId) {
    console.error(`Project board: adding ${issueNodeId} produced no item`)
    return false
  }

  if (target.statusFieldId && target.triageOptionId) {
    const updated = await graphql(
      target.token,
      `mutation { updateProjectV2ItemFieldValue(input: {projectId: "${target.projectId}", itemId: "${itemId}", fieldId: "${target.statusFieldId}", value: {singleSelectOptionId: "${target.triageOptionId}"}}) { projectV2Item { id } } }`,
    )
    if (!updated) {
      console.error(`Project board: ${itemId} was added but could not be set to Triage`)
    }
  }

  return true
}
