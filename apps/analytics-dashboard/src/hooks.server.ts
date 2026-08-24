import type { Handle, HandleServerError } from '@sveltejs/kit'
import { verifyAccessJwt } from './lib/server/access-jwt.js'

/**
 * Authenticates every server-handled request against Cloudflare Access.
 *
 * Cloudflare Access protects the custom hostname, but the Pages project's default
 * `cmdr-analytics-dashboard.pages.dev` alias reaches the same deployment without passing through
 * it. This gate runs inside the app, so it covers both hostnames and every route: pages, form
 * actions, and the `/api/report` endpoint alike. See `lib/server/access-jwt.ts`.
 *
 * Both people and machines pass it: a browser login and an Access service token are the same signed
 * assertion, so the agent-readable `/api/report` recipe works with no second credential. The
 * identity lands on `event.locals.identity` as a discriminated union, so a route that ever wants a
 * person has to say so rather than reading a possibly-absent email.
 *
 * `import.meta.env.DEV` is inlined at build time, so the deployed bundle contains no bypass branch
 * at all. `pnpm dev:dashboard` (`vite dev`) skips the gate; anything built enforces it, including
 * `wrangler pages dev`.
 */
export const handle: Handle = async ({ event, resolve }) => {
  if (!import.meta.env.DEV) {
    let identity = null
    try {
      identity = await verifyAccessJwt(event.request)
    } catch (e) {
      // Verification is written not to throw; if it ever does, refuse rather than fall through.
      console.error('Access verification threw:', e)
    }

    if (!identity) {
      return new Response('Forbidden', {
        status: 403,
        headers: { 'content-type': 'text/plain; charset=utf-8' },
      })
    }
    event.locals.identity = identity
  }

  return resolve(event)
}

/**
 * Keeps exception details server-side. The client gets a fixed string; the detail goes to the
 * Workers log, which is already the place we read for dashboard failures.
 */
export const handleError: HandleServerError = ({ error }) => {
  console.error(error)
  return { message: 'Internal Error' }
}
