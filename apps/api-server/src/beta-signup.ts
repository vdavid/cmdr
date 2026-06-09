import { Hono, type Context } from 'hono'
import { type Bindings, isValidEmail, redactEmail } from './types'

const betaSignup = new Hono<{ Bindings: Bindings }>()

/**
 * Beta-tester contact signup. The whole point of this route is the privacy invariant: it accepts an
 * email and NO install id of any kind (no `anal_`, no `diag_`). The email and the analytics ids must
 * never co-occur on our servers, so the analytics stream stays unjoinable to any identity. The email
 * is subscribed to a dedicated double-opt-in Listmonk list; Listmonk then sends its own confirmation
 * mail, which stops prank signups.
 *
 * Caps the body, rate-limits by IP (the IP is used only for the limiter, never stored), and never
 * reveals whether the address already existed (no enumeration). On a Listmonk error it returns a
 * soft 502 the app can surface gently, rather than failing silently.
 */

/** A signup body is just an email; nothing else is read, so an install id can never sneak through. */
const maxBetaSignupBytes = 1024

/** Resolved Listmonk config; `null` when any piece is missing (route returns 500). */
interface ListmonkConfig {
  url: string
  user: string
  token: string
  listId: number
}

function resolveListmonkConfig(env: Bindings): ListmonkConfig | null {
  const {
    LISTMONK_API_URL: url,
    LISTMONK_API_USER: user,
    LISTMONK_API_TOKEN: token,
    LISTMONK_BETA_LIST_ID: listId,
  } = env
  if (!url || !user || !token || typeof listId !== 'number') return null
  return { url, user, token, listId }
}

/** Read the request body, enforcing the size cap, and extract ONLY the email. Returns the validated
 * email, or a `Response` to short-circuit with (a 400). We deliberately read nothing but the email,
 * so no install id (or any other field) a client might send can reach Listmonk or our logs. */
async function readSignupEmail(c: Context<{ Bindings: Bindings }>): Promise<string | Response> {
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > maxBetaSignupBytes) {
    return c.json({ error: 'Request too large' }, 400)
  }

  let rawBody: string
  try {
    rawBody = await c.req.text()
  } catch {
    return c.json({ error: 'Could not read request body' }, 400)
  }
  if (rawBody.length > maxBetaSignupBytes) {
    return c.json({ error: 'Request too large' }, 400)
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(rawBody)
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  if (!parsed || typeof parsed !== 'object') {
    return c.json({ error: 'Invalid JSON' }, 400)
  }

  const email = (parsed as Record<string, unknown>).email
  if (typeof email !== 'string' || !isValidEmail(email)) {
    return c.json({ error: 'Please enter a valid email address' }, 400)
  }
  return email
}

betaSignup.post('/beta-signup', async (c) => {
  // Rate-limit by the caller IP before any work. Signups are rare, so the dedicated limiter is
  // tighter than the heartbeat's. The IP keys the sliding window only and is never stored. The
  // binding is optional, so the gate is a no-op when absent (tests, incomplete envs).
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  if (c.env.BETA_SIGNUP_LIMITER) {
    const { success } = await c.env.BETA_SIGNUP_LIMITER.limit({ key: ip })
    if (!success) {
      return c.json({ error: 'Too many requests' }, 429)
    }
  }

  const email = await readSignupEmail(c)
  if (email instanceof Response) return email

  const listmonk = resolveListmonkConfig(c.env)
  if (!listmonk) {
    console.error('Beta signup: Listmonk not configured (missing URL, user, token, or list id)')
    return c.json({ error: 'Beta signup is not configured' }, 500)
  }

  // Subscribe with subscriber `status: "enabled"` (Listmonk's subscriber-status enum only accepts
  // enabled/disabled/blocklisted; "unconfirmed" is the per-LIST subscription status, not a subscriber
  // status, and Postgres rejects it). On a double-opt-in list, omitting `preconfirm_subscriptions`
  // leaves the per-list subscription `unconfirmed` and makes Listmonk send its own confirmation email,
  // which is what blocks prank signups for someone else's address.
  let listmonkResponse: Response
  try {
    listmonkResponse = await fetch(`${listmonk.url}/api/subscribers`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `token ${listmonk.user}:${listmonk.token}`,
      },
      body: JSON.stringify({ email, lists: [listmonk.listId], status: 'enabled' }),
    })
  } catch (e) {
    console.error('Beta signup: Listmonk fetch failed:', e)
    return c.json({ error: 'Could not reach the signup service' }, 502)
  }

  // A 409 means "subscriber already exists." We treat it as success and return the identical empty
  // 204, so the response never reveals whether the address was already on the list (no enumeration).
  if (listmonkResponse.ok || listmonkResponse.status === 409) {
    return c.body(null, 204)
  }

  console.error(`Beta signup: Listmonk returned ${String(listmonkResponse.status)} for ${redactEmail(email)}`)
  return c.json({ error: 'Could not sign up right now' }, 502)
})

export { betaSignup }
