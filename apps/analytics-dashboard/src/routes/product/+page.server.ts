import type { PageServerLoad } from './$types'
import { fetchProductData } from '$lib/server/fetch-all.js'
import { resolveSelection } from '$lib/server/types.js'

export type { ProductData } from '$lib/server/fetch-all.js'

/** Loads only the Product page's sources (Cloudflare, Paddle, license, feedback & errors). */
export const load: PageServerLoad = async ({ url, platform }) => {
  const selection = resolveSelection(url.searchParams.get('range'), url.searchParams.get('day'))
  return fetchProductData(platform, selection)
}
