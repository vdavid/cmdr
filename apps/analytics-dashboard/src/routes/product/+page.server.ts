import type { PageServerLoad, Actions } from './$types'
import { fail } from '@sveltejs/kit'
import { fetchProductData } from '#lib/server/fetch-all.js'
import { dashboardEnv } from '#lib/server/env.js'
import { resolveSelection } from '#lib/server/types.js'
import { setFeedbackAnswered } from '#lib/server/sources/feedback-and-errors.js'

export type { ProductData } from '#lib/server/fetch-all.js'

/** Loads only the Product page's sources (Cloudflare, Paddle, license, feedback & errors). */
export const load: PageServerLoad = async ({ url }) => {
  const selection = resolveSelection(url.searchParams.get('range'))
  return fetchProductData(dashboardEnv(), selection)
}

/**
 * Marks a feedback message answered (or back to awaiting a reply). David replies from his own
 * mailbox, so the api-server can't see that a reply went out; this is how "Awaiting reply" learns it.
 * It proxies with the server-only bearer token, then the page reloads to show what actually landed.
 */
export const actions: Actions = {
  setAnswered: async ({ request }) => {
    const form = await request.formData()
    const id = Number(form.get('id'))
    const answered = form.get('answered') === 'true'
    if (!Number.isInteger(id) || id <= 0) return fail(400, { id: null, error: 'No feedback id to mark.' })

    const result = await setFeedbackAnswered(dashboardEnv(), { id, answered })
    if (!result.ok) return fail(502, { id, error: `Couldn't save: ${result.error}` })
    return { id, answeredAt: result.data }
  },
}
