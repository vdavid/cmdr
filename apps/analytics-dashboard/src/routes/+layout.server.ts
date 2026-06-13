import type { LayoutServerLoad } from './$types'
import { resolveSelection } from '$lib/server/types.js'

/**
 * Resolves the shared time selection (range + optional specific day) from the URL once, in the layout,
 * so the sticky range/day picker and both data pages read one consistent selection. Each page's own
 * `+page.server.ts` re-resolves it for its data fetch (loads run independently), but the picker UI and
 * the active-day highlight all key off this.
 */
export const load: LayoutServerLoad = async ({ url }) => {
  return {
    selection: resolveSelection(url.searchParams.get('range'), url.searchParams.get('day')),
  }
}
