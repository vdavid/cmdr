import { Hono } from 'hono'
import { type Bindings, verifyAdminAuth } from './types'

// Tracking-link short codes. A `?r=<code>` on getcmdr.com or the personal blog expands to UTM
// params client-side; the blogs fetch the code -> meaning map from GET /r-codes.json (edge-cached).
// David edits the map via the admin CRUD below, so inventing a new code needs no deploy.
//
// Storage model: the whole map lives under ONE KV key (`codes`) as a JSON object. The map is tiny
// (a handful of channels), so a single blob keeps the public endpoint a single KV get and writes
// trivially consistent (read-modify-write one value); key-per-code would buy nothing here.

const linkCodes = new Hono<{ Bindings: Bindings }>()

/** The single KV key holding the whole code -> meaning map. */
export const codesKey = 'codes'

/** Admin-facing entry: utm_source is required; utm_medium and note are optional. */
export interface LinkCodeEntry {
  utm_source: string
  utm_medium?: string
  /** Free-form admin reminder ("r/macapps comment"). Never exposed on the public endpoint. */
  note?: string
}

export type LinkCodeMap = Record<string, LinkCodeEntry>

/** Public entry: source + optional medium only. The admin `note` is stripped. */
type PublicLinkCodeEntry = { utm_source: string; utm_medium?: string }
type PublicLinkCodeMap = Record<string, PublicLinkCodeEntry>

const maxCodeLength = 64
const maxUtmLength = 120

// Codes are part of an inconspicuous public URL (getcmdr.com/?r=rmc), so keep the charset tight and
// URL-clean: lowercase alphanumerics plus `. _ -`. This mirrors the blogs' client-side sanitizer.
const codePattern = /^[a-z0-9._-]+$/

/** Whether a code is a valid map key: lowercase `[a-z0-9._-]`, 1..maxCodeLength chars. */
export function isValidCode(code: string): boolean {
  return code.length >= 1 && code.length <= maxCodeLength && codePattern.test(code)
}

/**
 * Normalize a UTM value the same way the blogs and the /download `ref` handler do: lowercase, drop
 * anything outside `[a-z0-9._-]`, cap length. Returns '' for nullish/empty input. The blogs apply
 * the identical rule to the pass-through fallback, so a stored value and a fallback value match.
 */
export function sanitizeUtmValue(value: string | undefined | null): string {
  if (!value) return ''
  return value
    .toLowerCase()
    .replace(/[^a-z0-9._-]/g, '')
    .slice(0, maxUtmLength)
}

/** Read the whole map from KV (empty object when unset or malformed). */
async function readMap(kv: KVNamespace): Promise<LinkCodeMap> {
  const map = await kv.get<LinkCodeMap>(codesKey, 'json')
  return map && typeof map === 'object' ? map : {}
}

/** Strip admin-only fields (`note`) so the public payload carries source + medium only. */
function toPublicMap(map: LinkCodeMap): PublicLinkCodeMap {
  const out: PublicLinkCodeMap = {}
  for (const [code, entry] of Object.entries(map)) {
    out[code] = entry.utm_medium
      ? { utm_source: entry.utm_source, utm_medium: entry.utm_medium }
      : { utm_source: entry.utm_source }
  }
  return out
}

// Public endpoint is the same non-sensitive config for both blogs (different origins), so allow any
// origin. Cache at the edge for a few minutes (300s): the map changes rarely, and this keeps blog
// page loads off KV. A new code is live within the TTL without a deploy.
const publicCacheControl = 'public, max-age=300'

function publicCors(c: { header: (name: string, value: string) => void }) {
  c.header('Access-Control-Allow-Origin', '*')
  c.header('Access-Control-Allow-Methods', 'GET, OPTIONS')
  c.header('Access-Control-Allow-Headers', 'Content-Type')
}

linkCodes.options('/r-codes.json', (c) => {
  publicCors(c)
  return c.body(null, 204)
})

linkCodes.get('/r-codes.json', async (c) => {
  const map = await readMap(c.env.LINK_CODES)
  publicCors(c)
  c.header('Cache-Control', publicCacheControl)
  return c.json(toPublicMap(map))
})

// --- Admin CRUD (Bearer ADMIN_API_TOKEN) ---

linkCodes.get('/admin/r-codes', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized
  const map = await readMap(c.env.LINK_CODES)
  return c.json(map)
})

linkCodes.put('/admin/r-codes/:code', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const code = c.req.param('code')
  if (!isValidCode(code)) {
    return c.json({ error: 'Invalid code: use lowercase [a-z0-9._-], up to 64 chars' }, 400)
  }

  let payload: { utm_source?: unknown; utm_medium?: unknown; note?: unknown }
  try {
    payload = await c.req.json()
  } catch {
    return c.json({ error: 'Invalid JSON body' }, 400)
  }

  const utmSource = sanitizeUtmValue(typeof payload.utm_source === 'string' ? payload.utm_source : '')
  if (!utmSource) {
    return c.json({ error: 'utm_source is required and must contain [a-z0-9._-]' }, 400)
  }
  const utmMedium = sanitizeUtmValue(typeof payload.utm_medium === 'string' ? payload.utm_medium : '')
  const note = typeof payload.note === 'string' ? payload.note.slice(0, 500) : undefined

  const entry: LinkCodeEntry = { utm_source: utmSource }
  if (utmMedium) entry.utm_medium = utmMedium
  if (note) entry.note = note

  const map = await readMap(c.env.LINK_CODES)
  map[code] = entry
  await c.env.LINK_CODES.put(codesKey, JSON.stringify(map))

  return c.json({ code, entry })
})

linkCodes.delete('/admin/r-codes/:code', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const code = c.req.param('code')
  const map = await readMap(c.env.LINK_CODES)
  const existed = code in map
  if (existed) {
    // Rebuild without the key rather than `delete` (avoids the dynamic-delete lint + deopt).
    const next = Object.fromEntries(Object.entries(map).filter(([k]) => k !== code))
    await c.env.LINK_CODES.put(codesKey, JSON.stringify(next))
  }
  return c.json({ code, deleted: existed })
})

export { linkCodes }
