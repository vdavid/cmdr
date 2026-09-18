import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { TRIAGE_OPTION_ID, addIssueToBoard, resolveBoardTarget, verifyGitHubWebhook } from './project-board'
import type { Bindings } from './types'

const secret = 'a-webhook-secret'

/** The signature GitHub would send for `body` under `secret`. */
async function sign(body: string, key = secret): Promise<string> {
  const mac = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(key),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const signature = await crypto.subtle.sign('HMAC', mac, new TextEncoder().encode(body))
  const hex = [...new Uint8Array(signature)].map((b) => b.toString(16).padStart(2, '0')).join('')
  return `sha256=${hex}`
}

function env(fields: Partial<Bindings>): Bindings {
  return fields as Bindings
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })
}

/** The JSON body of a recorded fetch call, as text. Bodies here are always strings we set. */
function requestBody(call: unknown): string {
  const [, init] = call as [string, RequestInit]
  return typeof init.body === 'string' ? init.body : ''
}

describe('resolveBoardTarget', () => {
  it('is off unless the token and the project id are both configured', () => {
    expect(resolveBoardTarget(env({}))).toBeNull()
    expect(resolveBoardTarget(env({ GITHUB_PROJECT_TOKEN: 'ghp_x' }))).toBeNull()
    expect(resolveBoardTarget(env({ GITHUB_PROJECT_ID: 'PVT_x' }))).toBeNull()
  })

  it('carries the status field and option through when they are set', () => {
    const target = resolveBoardTarget(
      env({
        GITHUB_PROJECT_TOKEN: 'ghp_x',
        GITHUB_PROJECT_ID: 'PVT_x',
        GITHUB_PROJECT_STATUS_FIELD_ID: 'PVTSSF_x',
        GITHUB_PROJECT_TRIAGE_OPTION_ID: 'abc123',
      }),
    )
    expect(target).toEqual({
      token: 'ghp_x',
      projectId: 'PVT_x',
      statusFieldId: 'PVTSSF_x',
      triageOptionId: 'abc123',
    })
  })

  it('works with no status field, so an item is still added when the board has no Status', () => {
    const target = resolveBoardTarget(env({ GITHUB_PROJECT_TOKEN: 'ghp_x', GITHUB_PROJECT_ID: 'PVT_x' }))
    expect(target?.statusFieldId).toBeUndefined()
  })
})

describe('verifyGitHubWebhook', () => {
  it('accepts a signature GitHub would have produced', async () => {
    const body = '{"action":"opened"}'
    await expect(verifyGitHubWebhook(body, await sign(body), secret)).resolves.toBe(true)
  })

  it('rejects a signature made with a different secret', async () => {
    const body = '{"action":"opened"}'
    await expect(verifyGitHubWebhook(body, await sign(body, 'wrong'), secret)).resolves.toBe(false)
  })

  it('rejects a body that changed after signing', async () => {
    const signature = await sign('{"action":"opened"}')
    await expect(verifyGitHubWebhook('{"action":"closed"}', signature, secret)).resolves.toBe(false)
  })

  it('rejects a missing, empty, or wrongly prefixed header', async () => {
    const body = '{"action":"opened"}'
    await expect(verifyGitHubWebhook(body, '', secret)).resolves.toBe(false)
    await expect(verifyGitHubWebhook(body, 'sha1=abc', secret)).resolves.toBe(false)
    await expect(verifyGitHubWebhook(body, 'deadbeef', secret)).resolves.toBe(false)
  })
})

describe('addIssueToBoard', () => {
  const target = {
    token: 'ghp_x',
    projectId: 'PVT_x',
    statusFieldId: 'PVTSSF_x',
    triageOptionId: TRIAGE_OPTION_ID,
  }

  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('adds the item and then sets it to Triage', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ data: { addProjectV2ItemById: { item: { id: 'PVTI_1' } } } }))
      .mockResolvedValueOnce(
        jsonResponse({ data: { updateProjectV2ItemFieldValue: { projectV2Item: { id: 'PVTI_1' } } } }),
      )
    vi.stubGlobal('fetch', fetchMock)

    await expect(addIssueToBoard(target, 'I_node')).resolves.toBe(true)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(requestBody(fetchMock.mock.calls[0])).toContain('addProjectV2ItemById')
    expect(requestBody(fetchMock.mock.calls[1])).toContain(TRIAGE_OPTION_ID)
  })

  it('still counts as added when only the status update fails', async () => {
    // The item is on the board, which is the part that matters; a missing column is cosmetic.
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(jsonResponse({ data: { addProjectV2ItemById: { item: { id: 'PVTI_1' } } } }))
        .mockResolvedValueOnce(jsonResponse({ errors: [{ message: 'nope' }] })),
    )

    await expect(addIssueToBoard(target, 'I_node')).resolves.toBe(true)
  })

  it('skips the status call when the board has no Status field configured', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ data: { addProjectV2ItemById: { item: { id: 'PVTI_1' } } } }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(addIssueToBoard({ token: 'ghp_x', projectId: 'PVT_x' }, 'I_node')).resolves.toBe(true)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('reports failure when the add itself is refused', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          jsonResponse({ data: { addProjectV2ItemById: null }, errors: [{ message: 'Could not resolve to a node' }] }),
        ),
    )

    await expect(addIssueToBoard(target, 'I_node')).resolves.toBe(false)
  })

  it('reports failure rather than throwing when the network is down', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network down')))
    await expect(addIssueToBoard(target, 'I_node')).resolves.toBe(false)
  })

  it('reports failure on a non-200, which is what an expired token looks like', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ message: 'Bad credentials' }, 401)))
    await expect(addIssueToBoard(target, 'I_node')).resolves.toBe(false)
  })
})
