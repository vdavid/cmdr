import type { Context } from 'hono'
import { type Bindings, enforceIpRateLimit, isValidEmail, readCappedBody, redactEmail } from '../types'
import { postListSignupNotification, type SignupList } from '../discord'

/**
 * The one way an email joins a Listmonk list: `/beta-signup` (the desktop app) and
 * `/newsletter-signup` (getcmdr.com) both run {@link handleListSignup}, differing only in the list.
 *
 * It goes through Listmonk's PUBLIC subscription endpoint (`POST /api/public/subscription`), never
 * the admin `POST /api/subscribers`. That choice is the whole design:
 * - The public endpoint fails with a 500 when the opt-in mail can't be sent (it inserts with
 *   `assertOptin=true`). The admin one swallows that failure and answers 200, which is how a dead
 *   SMTP credential made every signup look successful for two months in 2026 while no confirmation
 *   mail went out (verified by reading listmonk v6.2.0, `cmd/subscribers.go::CreateSubscriber` vs
 *   `cmd/public.go::processSubForm`, 2026-10-05).
 * - It handles an existing subscriber itself: it adds the list and mails a confirmation for every
 *   still-unconfirmed double-opt-in list, so a retry by someone stuck as unconfirmed gets a fresh mail.
 * - It has no `preconfirm` field, so double opt-in can't be skipped from here.
 * - It needs no API token, and the lists must be `public` in Listmonk (a private list is refused).
 */

/** A signup body is just an email; nothing else is read, so an install id can never sneak through. */
const maxSignupBytes = 1024

export interface ListSignupConfig {
  list: SignupList
  listmonkUrl: string | undefined
  listUuid: string | undefined
  /** Only for the Discord deep link into the Listmonk admin. */
  listId: number | undefined
}

/** What Listmonk did with the signup. */
type SubscribeOutcome =
  | { kind: 'confirmation-sent' } // New subscription (or a still-unconfirmed one): Listmonk mailed the opt-in.
  | { kind: 'nothing-to-confirm' } // Already confirmed on this list: no mail, stay quiet.
  | { kind: 'rejected' } // Listmonk refused the address (its email check is stricter than ours).
  | { kind: 'failed' } // Listmonk unreachable, or it couldn't send the opt-in mail.

async function subscribe(email: string, listmonkUrl: string, listUuid: string): Promise<SubscribeOutcome> {
  let res: Response
  try {
    res = await fetch(`${listmonkUrl}/api/public/subscription`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, list_uuids: [listUuid] }),
    })
  } catch (e) {
    console.error('Signup: Listmonk fetch failed:', e)
    return { kind: 'failed' }
  }

  if (res.status === 400) return { kind: 'rejected' }
  if (!res.ok) {
    console.error(`Signup: Listmonk returned ${String(res.status)} for ${redactEmail(email)}`)
    return { kind: 'failed' }
  }

  const body = (await res.json().catch(() => null)) as { data?: { has_optin?: boolean } } | null
  return body?.data?.has_optin ? { kind: 'confirmation-sent' } : { kind: 'nothing-to-confirm' }
}

/** Read the capped body and extract ONLY the email: no install id (or any other field) a client
 * sends can reach Listmonk or our logs. Returns the email, or a 400 to short-circuit with. */
async function readSignupEmail(c: Context<{ Bindings: Bindings }>): Promise<string | Response> {
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > maxSignupBytes) {
    return c.json({ error: 'Request too large' }, 400)
  }
  const raw = c.req.raw.body ? await readCappedBody(c.req.raw.body, maxSignupBytes) : new ArrayBuffer(0)
  if (!raw) return c.json({ error: 'Request too large' }, 400)

  let parsed: unknown
  try {
    parsed = JSON.parse(new TextDecoder().decode(raw))
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  const email = parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>).email : undefined
  if (typeof email !== 'string' || !isValidEmail(email)) {
    return c.json({ error: 'Please enter a valid email address' }, 400)
  }
  return email
}

/**
 * Fire the Discord ping after the response ships: `waitUntil` when the execution context exists,
 * inline as a test fallback. Drop-on-failure lives in `postListSignupNotification`.
 */
async function pingDiscord(c: Context<{ Bindings: Bindings }>, cfg: ListSignupConfig, listmonkUrl: string) {
  const webhookUrl = c.env.DISCORD_BETA_SIGNUP_WEBHOOK_URL ?? c.env.DISCORD_WEBHOOK_URL
  if (!webhookUrl) return
  // No identity at all in the notification: the email stays out of Discord, and no install id
  // ever reaches these routes. The Listmonk link is how to see who signed up.
  const notify = postListSignupNotification(webhookUrl, {
    list: cfg.list,
    signupUnixSeconds: Math.floor(Date.now() / 1000),
    listAdminUrl: `${listmonkUrl}/admin/subscribers${cfg.listId === undefined ? '' : `?lists=${String(cfg.listId)}`}`,
  })
  try {
    c.executionCtx.waitUntil(notify)
  } catch {
    await notify
  }
}

/**
 * The shared route body. Every outcome toward the caller is an identical empty 204 (new, re-sent,
 * already confirmed), so the response never reveals whether an address existed. The exceptions
 * carry no such signal: 400 for a bad address, 429 over the limit, and a soft 502 when the
 * confirmation mail didn't go out, so the person knows to try again.
 */
export async function handleListSignup(c: Context<{ Bindings: Bindings }>, cfg: ListSignupConfig): Promise<Response> {
  // Before any work. The IP keys the sliding window only and is never stored.
  const limited = await enforceIpRateLimit(c.env.SIGNUP_LIMITER, c.req)
  if (limited) return limited

  const email = await readSignupEmail(c)
  if (email instanceof Response) return email

  const { listmonkUrl, listUuid } = cfg
  if (!listmonkUrl || !listUuid) {
    console.error(`Signup: Listmonk not configured for the ${cfg.list} list (missing URL or list UUID)`)
    return c.json({ error: 'Signup is not configured' }, 500)
  }

  const outcome = await subscribe(email, listmonkUrl, listUuid)
  if (outcome.kind === 'rejected') return c.json({ error: 'Please enter a valid email address' }, 400)
  if (outcome.kind === 'failed') return c.json({ error: 'Could not sign up right now' }, 502)

  // Only a mailed confirmation is news: a quiet re-signup of a confirmed address stays quiet.
  if (outcome.kind === 'confirmation-sent') await pingDiscord(c, cfg, listmonkUrl)

  return c.body(null, 204)
}
