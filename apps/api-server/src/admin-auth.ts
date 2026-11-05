/**
 * The one admin gate. Every `/admin/` route calls `verifyAdminAuth` first and returns its Response
 * when there is one; ❌ no route compares the token itself. `admin-auth.test.ts` walks the app's
 * route table, so a new `/admin/` route that skips this fails there.
 *
 * In order: the token must be configured (500 otherwise), the caller's IP must be under
 * `ADMIN_LIMITER` (429), and the `Authorization` header must carry the token, compared in constant
 * time (401). A 401 also counts an `admin-auth-rejected` ops event against the route, which the
 * next digest mails (`report-digest.ts`).
 */

import type { Context } from 'hono'
import { routePath } from 'hono/route'
import { constantTimeEqual } from './licensing/paddle'
import { countOpsEvent, rejectedAdminRequestMessage } from './ops-events'
import { type Bindings, enforceIpRateLimit, scheduleBackground } from './types'

/** Verify admin auth: the error Response to return, or null when the caller may proceed. */
export async function verifyAdminAuth(c: Context<{ Bindings: Bindings }>): Promise<Response | null> {
  const token = c.env.ADMIN_API_TOKEN
  if (!token) {
    return Response.json({ error: 'Admin API not configured' }, { status: 500 })
  }
  const limited = await enforceIpRateLimit(c.env.ADMIN_LIMITER, c.req)
  if (limited) return limited

  const authHeader = c.req.header('Authorization')
  if (!authHeader || !constantTimeEqual(authHeader, `Bearer ${token}`)) {
    await scheduleBackground(
      c,
      countOpsEvent(c.env.TELEMETRY_DB, 'admin-auth-rejected', rejectedAdminRequestMessage(c.req.method, routePath(c))),
    )
    return Response.json({ error: 'Unauthorized' }, { status: 401 })
  }
  return null
}

// THROWAWAY probe: an API-server-only change (never merge).
