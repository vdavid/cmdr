import { Hono } from 'hono'
import type { Bindings } from '../types'
import { handleListSignup } from './listmonk-signup'

const betaSignup = new Hono<{ Bindings: Bindings }>()

/**
 * Beta-tester contact signup from the desktop app. The privacy invariant is the point: it accepts an
 * email and NO install id of any kind (no `anal_`, no `diag_`), so the email and the analytics ids
 * never co-occur on our servers and the analytics stream stays unjoinable to any identity.
 * `handleListSignup` reads nothing but the email, which is what keeps that true.
 */
betaSignup.post('/beta-signup', (c) =>
  handleListSignup(c, {
    list: 'beta',
    listmonkUrl: c.env.LISTMONK_API_URL,
    listUuid: c.env.LISTMONK_BETA_LIST_UUID,
    listId: c.env.LISTMONK_BETA_LIST_ID,
  }),
)

export { betaSignup }
