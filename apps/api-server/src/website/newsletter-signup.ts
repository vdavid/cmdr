import { Hono } from 'hono'
import type { Bindings } from '../types'
import { handleListSignup, siteFormCors } from './listmonk-signup'

const newsletterSignup = new Hono<{ Bindings: Bindings }>()

newsletterSignup.options('/newsletter-signup', (c) => {
  siteFormCors(c)
  return c.body(null, 204)
})

/** getcmdr.com's newsletter form. Same flow as `/beta-signup`, on the newsletter list. */
newsletterSignup.post('/newsletter-signup', async (c) => {
  siteFormCors(c)
  return handleListSignup(c, {
    list: 'newsletter',
    listmonkUrl: c.env.LISTMONK_API_URL,
    listUuid: c.env.LISTMONK_NEWSLETTER_LIST_UUID,
  })
})

export { newsletterSignup }
