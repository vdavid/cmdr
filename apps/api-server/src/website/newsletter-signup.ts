import { Hono, type Context } from 'hono'
import type { Bindings } from '../types'
import { handleListSignup } from './listmonk-signup'

const newsletterSignup = new Hono<{ Bindings: Bindings }>()

const allowedOrigins = new Set(['https://getcmdr.com', 'https://www.getcmdr.com'])

/** getcmdr.com's newsletter form posts here cross-origin, and must read failures too. */
function newsletterCors(c: Context<{ Bindings: Bindings }>) {
  const origin = c.req.header('origin')
  if (origin && allowedOrigins.has(origin)) {
    c.header('Access-Control-Allow-Origin', origin)
    c.header('Access-Control-Allow-Methods', 'POST, OPTIONS')
    c.header('Access-Control-Allow-Headers', 'Content-Type')
    c.header('Vary', 'Origin')
  }
}

newsletterSignup.options('/newsletter-signup', (c) => {
  newsletterCors(c)
  return c.body(null, 204)
})

/** getcmdr.com's newsletter form. Same flow as `/beta-signup`, on the newsletter list. */
newsletterSignup.post('/newsletter-signup', async (c) => {
  newsletterCors(c)
  return handleListSignup(c, {
    list: 'newsletter',
    listmonkUrl: c.env.LISTMONK_API_URL,
    listUuid: c.env.LISTMONK_NEWSLETTER_LIST_UUID,
    listId: c.env.LISTMONK_NEWSLETTER_LIST_ID,
  })
})

export { newsletterSignup }
