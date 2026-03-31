import type { PageServerLoad } from './$types'
import { fetchDashboardData } from '$lib/server/fetch-all.js'

export type { DashboardData } from '$lib/server/fetch-all.js'

export const load: PageServerLoad = async ({ url, platform }) => {
  return fetchDashboardData(platform, url.searchParams.get('range') ?? '7d')
}
