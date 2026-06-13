import type { SourceResult } from '../types.js'
import type { LinkCodeMap } from '../../link-codes.js'

/**
 * Server-only proxy to the api-server's `?r=` admin CRUD (`/admin/r-codes`). The admin bearer token
 * (`LICENSE_SERVER_ADMIN_TOKEN`) stays in this module and `+page.server.ts`; it never reaches the
 * browser. The `/links` page renders the listed map; its form actions call the upsert/delete here.
 *
 * No caching: the admin map is tiny and David edits it interactively, so a stale read would surprise
 * (the public `/r-codes.json` is the edge-cached path; this admin view always reflects the live KV).
 */

interface LinkCodesEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  /** Optional local-QA override for the api-server base URL (defaults to production). */
  WORKER_BASE_URL?: string
}

const defaultWorkerBaseUrl = 'https://api.getcmdr.com'

function baseUrl(env: LinkCodesEnv): string {
  return env.WORKER_BASE_URL || defaultWorkerBaseUrl
}

function authHeaders(env: LinkCodesEnv): HeadersInit {
  return { Authorization: `Bearer ${env.LICENSE_SERVER_ADMIN_TOKEN}` }
}

/** GET the full admin map (code -> {utm_source, utm_medium?, note?}). */
export async function fetchLinkCodes(env: LinkCodesEnv): Promise<SourceResult<LinkCodeMap>> {
  try {
    const response = await fetch(`${baseUrl(env)}/admin/r-codes`, { headers: authHeaders(env) })
    if (!response.ok) {
      throw new Error(`returned ${String(response.status)}`)
    }
    const data = (await response.json()) as LinkCodeMap
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Link codes: ${e instanceof Error ? e.message : String(e)}` }
  }
}

/** The body sent on an upsert (the page validates and normalizes before calling this). */
export interface UpsertLinkCodeInput {
  code: string
  utm_source: string
  utm_medium?: string
  note?: string
}

/** PUT a single code (create or overwrite). Returns the worker's error text on a non-2xx response. */
export async function upsertLinkCode(env: LinkCodesEnv, input: UpsertLinkCodeInput): Promise<SourceResult<true>> {
  const body: Record<string, string> = { utm_source: input.utm_source }
  if (input.utm_medium) body.utm_medium = input.utm_medium
  if (input.note) body.note = input.note
  try {
    const response = await fetch(`${baseUrl(env)}/admin/r-codes/${encodeURIComponent(input.code)}`, {
      method: 'PUT',
      headers: { ...authHeaders(env), 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!response.ok) {
      const text = await response.text()
      throw new Error(text || `returned ${String(response.status)}`)
    }
    return { ok: true, data: true }
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) }
  }
}

/** DELETE a single code. Succeeds even if the code didn't exist (idempotent on the worker side). */
export async function deleteLinkCode(env: LinkCodesEnv, code: string): Promise<SourceResult<true>> {
  try {
    const response = await fetch(`${baseUrl(env)}/admin/r-codes/${encodeURIComponent(code)}`, {
      method: 'DELETE',
      headers: authHeaders(env),
    })
    if (!response.ok) {
      const text = await response.text()
      throw new Error(text || `returned ${String(response.status)}`)
    }
    return { ok: true, data: true }
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) }
  }
}
