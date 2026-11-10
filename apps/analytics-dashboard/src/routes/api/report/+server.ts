import type { RequestHandler } from './$types'
import { fetchDashboardData } from '#lib/server/fetch-all.js'
import { dashboardEnv } from '#lib/server/env.js'
import { formatReport } from './format-report.js'

export const GET: RequestHandler = async ({ url }) => {
  try {
    const data = await fetchDashboardData(dashboardEnv(), url.searchParams.get('range'))
    const report = formatReport(data)

    return new Response(report, {
      headers: { 'Content-Type': 'text/plain; charset=utf-8' },
    })
  } catch (e) {
    // Detail goes to the Workers log, not to the response: stacks name internals and env vars.
    console.error('Report generation failed:', e)
    return new Response("Couldn't generate the report. Check the Workers log for details.", {
      status: 500,
      headers: { 'Content-Type': 'text/plain; charset=utf-8' },
    })
  }
}
