import { Hono } from 'hono'
import type { Bindings } from '../types'
import { handleListSignup, siteFormCors } from './listmonk-signup'

const subprocessorUpdatesSignup = new Hono<{ Bindings: Bindings }>()

subprocessorUpdatesSignup.options('/subprocessor-updates-signup', (c) => {
  siteFormCors(c)
  return c.body(null, 204)
})

/**
 * getcmdr.com/trust's form: the list through which Cmdr gives 30 days' notice before a subprocessor
 * change. Same flow as `/newsletter-signup`, on its own list.
 */
subprocessorUpdatesSignup.post('/subprocessor-updates-signup', async (c) => {
  siteFormCors(c)
  return handleListSignup(c, {
    list: 'subprocessor-updates',
    listmonkUrl: c.env.LISTMONK_API_URL,
    listUuid: c.env.LISTMONK_SUBPROCESSOR_LIST_UUID,
  })
})

export { subprocessorUpdatesSignup }
