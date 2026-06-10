import { Hono, type Context } from 'hono'
import { type Bindings, isValidEmail, redactEmail } from './types'
import { postBetaSignupNotification, type BetaSignupNotification } from './discord'

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

/** What `subscribe` resolved to, so the caller knows whether (and how) to ping Discord. */
type SubscribeOutcome =
  | { kind: 'new' } // Fresh subscriber created; Listmonk sent its own opt-in mail.
  | { kind: 'added-existing' } // Existing subscriber; we added the beta list + triggered the opt-in mail.
  | { kind: 'already-on-list' } // Existing subscriber already on the beta list; stay quiet (re-signup).
  | { kind: 'error' } // Listmonk unreachable or errored; surface a soft 502.

function listmonkHeaders(c: ListmonkConfig): Record<string, string> {
  return { 'Content-Type': 'application/json', Authorization: `token ${c.user}:${c.token}` }
}

/**
 * Subscribe `email` to the double-opt-in beta list and report what actually happened.
 *
 * The fresh `POST /api/subscribers` path makes Listmonk send its own confirmation email (we omit
 * `preconfirm_subscriptions` on purpose). A 409 means the address already exists (for example it's on
 * the newsletter list), and a plain 409→204 would leave that person OFF the beta list. So on 409 we
 * look the subscriber up, and if they're not yet on the beta list we add it and explicitly trigger the
 * opt-in email (`POST /api/subscribers/{id}/optin` — the list-add endpoint alone does NOT send it).
 * A subscriber already on the beta list is a silent re-signup: no list change, no email, no ping.
 */
async function subscribe(email: string, listmonk: ListmonkConfig): Promise<SubscribeOutcome> {
  let res: Response
  try {
    res = await fetch(`${listmonk.url}/api/subscribers`, {
      method: 'POST',
      headers: listmonkHeaders(listmonk),
      // Subscriber `status: "enabled"` (the subscriber-status enum only accepts enabled/disabled/
      // blocklisted; "unconfirmed" is the per-LIST subscription status, which Postgres rejects as a
      // subscriber status). Omitting `preconfirm_subscriptions` keeps the list subscription unconfirmed
      // and makes Listmonk send its own confirmation mail, blocking prank signups for someone else.
      body: JSON.stringify({ email, lists: [listmonk.listId], status: 'enabled' }),
    })
  } catch (e) {
    console.error('Beta signup: Listmonk fetch failed:', e)
    return { kind: 'error' }
  }

  if (res.ok) return { kind: 'new' }
  if (res.status === 409) return recoverExistingSubscriber(email, listmonk)

  console.error(`Beta signup: Listmonk returned ${String(res.status)} for ${redactEmail(email)}`)
  return { kind: 'error' }
}

/** Minimal shape of the bits of `GET /api/subscribers` we read. */
interface SubscriberLookup {
  id: number
  lists: { id: number }[]
}

/**
 * Handle the 409 "already exists" path: find the subscriber and, if they're not yet on the beta list,
 * add it and send the opt-in confirmation email. Any error here returns a soft `error` outcome so the
 * app surfaces a gentle retry rather than a false success.
 */
async function recoverExistingSubscriber(email: string, listmonk: ListmonkConfig): Promise<SubscribeOutcome> {
  let subscriber: SubscriberLookup | null
  try {
    subscriber = await lookupSubscriber(email, listmonk)
  } catch (e) {
    console.error('Beta signup: Listmonk lookup failed:', e)
    return { kind: 'error' }
  }
  // A 409 with no findable subscriber shouldn't happen; treat it as already-handled and stay quiet
  // rather than risk an enumeration oracle or a misleading ping.
  if (!subscriber) return { kind: 'already-on-list' }

  if (subscriber.lists.some((l) => l.id === listmonk.listId)) {
    return { kind: 'already-on-list' }
  }

  try {
    await addToBetaListAndConfirm(subscriber.id, listmonk)
  } catch (e) {
    console.error('Beta signup: adding existing subscriber to the beta list failed:', e)
    return { kind: 'error' }
  }
  return { kind: 'added-existing' }
}

/** Look the subscriber up by exact email. Returns the first match or `null`. Throws on a non-2xx. */
async function lookupSubscriber(email: string, listmonk: ListmonkConfig): Promise<SubscriberLookup | null> {
  // Listmonk's only lookup is a SQL expression in the `query` param. The email is already validated
  // (`isValidEmail`), and we double the single quote so an apostrophe can't break out of the literal.
  const query = `subscribers.email = '${email.replace(/'/g, "''")}'`
  const url = `${listmonk.url}/api/subscribers?query=${encodeURIComponent(query)}&per_page=1`
  const res = await fetch(url, { method: 'GET', headers: listmonkHeaders(listmonk) })
  if (!res.ok) throw new Error(`Listmonk lookup HTTP ${String(res.status)}`)
  const body: { data?: { results?: SubscriberLookup[] } } = await res.json()
  return body.data?.results?.[0] ?? null
}

/**
 * Add an existing subscriber to the beta list (`PUT /api/subscribers/lists`, `action: "add"`,
 * `status: "unconfirmed"`) then trigger the double-opt-in confirmation email
 * (`POST /api/subscribers/{id}/optin`). The list-add endpoint does NOT send the email by itself, so
 * the explicit optin call is what keeps the consent story honest for this path. Throws on any non-2xx.
 */
async function addToBetaListAndConfirm(subscriberId: number, listmonk: ListmonkConfig): Promise<void> {
  const addRes = await fetch(`${listmonk.url}/api/subscribers/lists`, {
    method: 'PUT',
    headers: listmonkHeaders(listmonk),
    body: JSON.stringify({
      ids: [subscriberId],
      action: 'add',
      target_list_ids: [listmonk.listId],
      status: 'unconfirmed',
    }),
  })
  if (!addRes.ok) throw new Error(`Listmonk list-add HTTP ${String(addRes.status)}`)

  const optinRes = await fetch(`${listmonk.url}/api/subscribers/${String(subscriberId)}/optin`, {
    method: 'POST',
    headers: listmonkHeaders(listmonk),
  })
  if (!optinRes.ok) throw new Error(`Listmonk optin HTTP ${String(optinRes.status)}`)
}

/**
 * Fire the Discord ping in the background after the response ships, mirroring the feedback route:
 * `waitUntil` when the execution context is available, await inline as a test/standalone fallback.
 * Drop-on-failure lives in `postBetaSignupNotification`, so the 204 is never held hostage to Discord.
 */
async function pingDiscord(
  c: Context<{ Bindings: Bindings }>,
  webhookUrl: string,
  notification: BetaSignupNotification,
): Promise<void> {
  const notify = postBetaSignupNotification(webhookUrl, notification)
  try {
    c.executionCtx.waitUntil(notify)
  } catch {
    // executionCtx unavailable (for example, in tests); await inline as fallback.
    await notify
  }
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

  const outcome = await subscribe(email, listmonk)
  if (outcome.kind === 'error') {
    return c.json({ error: 'Could not sign up right now' }, 502)
  }

  // Ping Discord ONLY when a beta subscription was actually newly established: a fresh subscribe, or an
  // existing subscriber we just added to the beta list. A silent re-signup (already on the list) stays
  // quiet, which also keeps the no-enumeration guarantee intact. The ping never blocks the 204, and any
  // Discord failure is dropped (see `postBetaSignupNotification`).
  if (outcome.kind === 'new' || outcome.kind === 'added-existing') {
    const webhookUrl = c.env.DISCORD_BETA_SIGNUP_WEBHOOK_URL ?? c.env.DISCORD_WEBHOOK_URL
    if (webhookUrl) {
      // The email is the ONLY identity in the notification: no install id ever reaches this route, so
      // the analytics/diagnostics streams stay unjoinable to the email (the route's whole point).
      await pingDiscord(c, webhookUrl, {
        email,
        signupUnixSeconds: Math.floor(Date.now() / 1000),
        listAdminUrl: `${listmonk.url}/admin/subscribers?lists=${String(listmonk.listId)}`,
        status: outcome.kind,
      })
    }
  }

  // Always an empty 204 toward the app: new, already-subscribed, and added-to-list outcomes are
  // indistinguishable in the response, so it never reveals whether the address already existed.
  return c.body(null, 204)
})

export { betaSignup }
