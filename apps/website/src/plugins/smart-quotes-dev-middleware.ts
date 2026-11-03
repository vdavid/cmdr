/**
 * The dev server's half of `smart-quotes.ts`: curls the quotes in every HTML response, so
 * `pnpm dev` shows the same typography the production build writes into `dist/`. Registered by
 * the integration for `astro dev` only.
 */

import { defineMiddleware } from 'astro:middleware'
import { transformHtml } from './smart-quotes.ts'

export const onRequest = defineMiddleware(async (_context, next) => {
  const response = await next()
  if (!(response.headers.get('content-type') ?? '').includes('text/html')) return response
  const html = await response.text()
  const headers = new Headers(response.headers)
  // Curly quotes are three bytes in UTF-8, so the original length no longer holds.
  headers.delete('content-length')
  return new Response(transformHtml(html), { status: response.status, statusText: response.statusText, headers })
})
