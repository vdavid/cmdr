/**
 * The Cmdr backlog project board, through the `project`-only token: puts newly opened issues from
 * the PUBLIC repo onto it, and sets the Severity, Impact, and Size fields on report cards
 * (`report-ratings.ts` decides which).
 *
 * ## Why a webhook rather than the project's own workflow
 *
 * GitHub's built-in "Auto-add to project" is the better mechanism, and it is already spoken for:
 * the free plan allows exactly ONE, and it serves the PRIVATE `cmdr-reports` repo, which nothing
 * else can reach. Adding a private repo's issue through this Worker would need a token that can
 * read that repo (a classic PAT with `repo` scope is full read/write over every repo the account
 * owns); `github-issues.ts` exists to keep a user's note away from exactly that kind of credential.
 *
 * ## Why this token is safe to hold
 *
 * The token here carries `project` scope ALONE. Verified against the live API on 2026-09-19: it
 * resolves public issues and cannot resolve a private repo's issue at all ("Could not resolve to a
 * node") through GraphQL. But the REST project-items endpoint DOES return private issue bodies to
 * the same token (verified 2026-10-14 with a `project`-only token: full `cmdr-reports` card bodies,
 * user reports included). So a leak of this secret exposes every board the account can reach: their
 * structure, and the bodies of private issues on them. No repository contents. ❌ Never widen it to `repo` to
 * make private issues work here; the right answer for those is the built-in workflow, or a
 * fine-grained token scoped to the `getcmdr` org's projects plus read on the one reports repo.
 *
 * ## Fail loudly
 *
 * A classic PAT expires. The failure mode is issues quietly not reaching the board, so every
 * failure returns false and the route records an ops event, per the CLAUDE.md rule that a failure has to
 * leave the Worker.
 */

import { githubFetch } from './github-fetch'
import { constantTimeEqual } from './licensing/paddle'
import type { Bindings } from './types'

const GITHUB_API = 'https://api.github.com'
const GITHUB_GRAPHQL = `${GITHUB_API}/graphql`

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

/**
 * One GraphQL call. Returns the parsed data, or null when the call or the query failed. `read` marks
 * a query, which `githubFetch` may retry; a mutation goes out once.
 */
async function graphql(
  token: string,
  query: string,
  { variables, read = false }: { variables?: Record<string, unknown>; read?: boolean } = {},
): Promise<Record<string, unknown> | null> {
  try {
    const response = await githubFetch(
      GITHUB_GRAPHQL,
      {
        method: 'POST',
        headers: {
          authorization: `Bearer ${token}`,
          'content-type': 'application/json',
          'user-agent': USER_AGENT,
        },
        body: JSON.stringify(variables ? { query, variables } : { query }),
      },
      { idempotent: read },
    )
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
 * redelivered webhook is harmless. A CONCURRENT add is different: an agent filing with
 * `gh issue create --project` races this webhook, and GitHub refuses whichever add comes second
 * ("Content already exists in this project", seen in the Worker logs 2026-10-20). So a refused add
 * asks whether the issue is on the board anyway before calling it a failure, and leaves the card's
 * fields to whoever won.
 */
export async function addIssueToBoard(target: BoardTarget, issueNodeId: string): Promise<boolean> {
  const added = await graphql(
    target.token,
    `mutation { addProjectV2ItemById(input: {projectId: "${target.projectId}", contentId: "${issueNodeId}"}) { item { id } } }`,
  )

  const item = (added?.['addProjectV2ItemById'] as { item?: { id?: unknown } } | undefined)?.item
  const itemId = typeof item?.id === 'string' ? item.id : null
  if (!itemId) {
    if (await isIssueOnBoard(target, issueNodeId)) {
      console.warn(`Project board: adding ${issueNodeId} was refused, but something else already put it on the board`)
      return true
    }
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

/** Whether the issue has an item on the target board. False when that can't be read either. */
async function isIssueOnBoard(target: BoardTarget, issueNodeId: string): Promise<boolean> {
  const data = await graphql(
    target.token,
    'query ($id: ID!) { node(id: $id) { ... on Issue { projectItems(first: 100) { nodes { project { id } } } } } }',
    { variables: { id: issueNodeId }, read: true },
  )
  const nodes = (data?.['node'] as { projectItems?: { nodes?: { project?: { id?: unknown } }[] } } | undefined)
    ?.projectItems?.nodes
  return (nodes ?? []).some((n) => n.project?.id === target.projectId)
}

/** One single-select field on the board: its id, and its option ids by option name. */
export interface BoardSelectField {
  id: string
  options: Map<string, string>
}

/** What a rating needs to know about the board, read once per tick that has work. */
export interface BoardLayout {
  /** The board's REST path, `users/<login>/projectsV2/<n>` or `orgs/<login>/projectsV2/<n>`. */
  restPath: string
  /** Single-select fields by name. */
  fields: Map<string, BoardSelectField>
}

/**
 * Read the board's owner, number, and single-select fields, so ratings resolve option NAMES to ids
 * at run time: a renamed option then fails loudly rather than setting a guessed one. Throws when the
 * board can't be read, which `runCronJob` turns into the alarm.
 */
export async function readBoardLayout(target: BoardTarget): Promise<BoardLayout> {
  const data = await graphql(
    target.token,
    `
      query ($id: ID!) {
        node(id: $id) {
          ... on ProjectV2 {
            number
            owner {
              __typename
              ... on User {
                login
              }
              ... on Organization {
                login
              }
            }
            fields(first: 50) {
              nodes {
                ... on ProjectV2SingleSelectField {
                  id
                  name
                  options {
                    id
                    name
                  }
                }
              }
            }
          }
        }
      }
    `,
    { variables: { id: target.projectId }, read: true },
  )
  const node = data?.['node'] as
    | {
        number?: unknown
        owner?: { __typename?: unknown; login?: unknown }
        fields?: { nodes?: { id?: unknown; name?: unknown; options?: { id?: unknown; name?: unknown }[] }[] }
      }
    | undefined
  if (!node || typeof node.number !== 'number') {
    throw new Error('Project board: the board could not be read (wrong GITHUB_PROJECT_ID, or an expired token)')
  }
  // The REST items path depends on whether a user or an organization owns the board.
  const ownerSegment = { User: 'users', Organization: 'orgs' }[String(node.owner?.__typename)]
  if (!ownerSegment || typeof node.owner?.login !== 'string') {
    throw new Error('Project board: the board has no user or organization owner, so its items path is unknown')
  }

  return {
    restPath: `${ownerSegment}/${node.owner.login}/projectsV2/${String(node.number)}`,
    fields: selectFieldsByName(node.fields?.nodes ?? []),
  }
}

/** The single-select fields among a board's field nodes (other kinds come back as `{}`), by name. */
function selectFieldsByName(
  nodes: { id?: unknown; name?: unknown; options?: { id?: unknown; name?: unknown }[] }[],
): Map<string, BoardSelectField> {
  const fields = new Map<string, BoardSelectField>()
  for (const field of nodes) {
    if (typeof field.id !== 'string' || typeof field.name !== 'string') continue
    const options = new Map<string, string>()
    for (const option of field.options ?? []) {
      if (typeof option.id === 'string' && typeof option.name === 'string') options.set(option.name, option.id)
    }
    fields.set(field.name, { id: field.id, options })
  }
  return fields
}

/** Pages of 100 the items listing may read: 2,000 items, far past this board's size. */
const ITEM_PAGES = 20

/**
 * The board items holding `repo`'s issues, as issue number → item node id.
 *
 * Over REST rather than GraphQL: with `project` scope alone, GraphQL shows a private repo's issue on
 * the board as `content: null`, while REST returns it whole (verified against the live API,
 * 2026-10-14), and the number and repo URL are what identify it. Pages follow GitHub's `Link`
 * header, and only to api.github.com. Throws on any failed page.
 */
export async function listBoardItemsForRepo(
  target: BoardTarget,
  layout: BoardLayout,
  repo: string,
): Promise<Map<number, string>> {
  const repositoryUrl = `${GITHUB_API}/repos/${repo}`
  const items = new Map<number, string>()
  let url: string | null = `${GITHUB_API}/${layout.restPath}/items?per_page=100&q=${encodeURIComponent(`repo:${repo}`)}`

  for (let page = 0; url && page < ITEM_PAGES; page++) {
    const response = await githubFetch(url, {
      headers: {
        authorization: `Bearer ${target.token}`,
        accept: 'application/vnd.github+json',
        'x-github-api-version': '2022-11-28',
        'user-agent': USER_AGENT,
      },
    })
    if (!response.ok) {
      throw new Error(`Project board: listing the board's items returned ${String(response.status)}`)
    }
    const payload: unknown = await response.json()
    for (const raw of Array.isArray(payload) ? payload : []) {
      const item = raw as { node_id?: unknown; content_type?: unknown; content?: Record<string, unknown> | null }
      if (item.content_type !== 'Issue' || typeof item.node_id !== 'string') continue
      const number = item.content?.['number']
      if (typeof number !== 'number' || item.content?.['repository_url'] !== repositoryUrl) continue
      items.set(number, item.node_id)
    }
    url = nextPageUrl(response.headers.get('link'))
  }
  return items
}

/** The `rel="next"` URL from a GitHub `Link` header, or null. Never a host other than the API's. */
function nextPageUrl(link: string | null): string | null {
  const next = /<([^>]+)>;\s*rel="next"/.exec(link ?? '')?.[1]
  return next?.startsWith(`${GITHUB_API}/`) ? next : null
}

/** One field to set on an item: an option id, or null to clear it. */
export interface ItemFieldValue {
  fieldId: string
  optionId: string | null
}

/**
 * Set several single-select fields on one item in ONE request, so a card's rating changes all
 * together or not at all. Ids go in as variables, never spliced into the query. Returns whether
 * GitHub accepted it; the reason is in the Workers logs.
 */
export async function setItemFields(target: BoardTarget, itemId: string, values: ItemFieldValue[]): Promise<boolean> {
  const declarations = ['$project: ID!', '$item: ID!']
  const mutations: string[] = []
  const variables: Record<string, unknown> = { project: target.projectId, item: itemId }
  values.forEach((value, i) => {
    declarations.push(`$field${String(i)}: ID!`)
    variables[`field${String(i)}`] = value.fieldId
    const scope = `projectId: $project, itemId: $item, fieldId: $field${String(i)}`
    if (value.optionId === null) {
      mutations.push(`f${String(i)}: clearProjectV2ItemFieldValue(input: {${scope}}) { clientMutationId }`)
    } else {
      declarations.push(`$option${String(i)}: String!`)
      variables[`option${String(i)}`] = value.optionId
      mutations.push(
        `f${String(i)}: updateProjectV2ItemFieldValue(input: {${scope}, value: {singleSelectOptionId: $option${String(i)}}}) { clientMutationId }`,
      )
    }
  })
  const data = await graphql(target.token, `mutation(${declarations.join(', ')}) { ${mutations.join(' ')} }`, {
    variables,
  })
  return data !== null
}
