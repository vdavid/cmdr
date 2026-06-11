import type { RequestHandler } from './$types'
import type { DashboardData } from '$lib/server/fetch-all.js'
import type { DownloadRow } from '$lib/server/sources/cloudflare.js'
import { fetchDashboardData } from '$lib/server/fetch-all.js'
import { countFeedbackWithReplyTo, tallyErrorReportsByField, errorReportsByDay } from '$lib/feedback-and-errors.js'

const regionNames = new Intl.DisplayNames(['en'], { type: 'region' })

function formatCountry(code: string): string {
  try {
    const upper = code.toUpperCase()
    const name = regionNames.of(upper)
    return name && name !== upper ? `${name} (${upper})` : code
  } catch {
    return code
  }
}

function pct(value: number, total: number): string {
  if (total === 0) return '0%'
  return `${((value / total) * 100).toFixed(1)}%`
}

function delta(current: number, previous: number): string {
  if (previous === 0) return ''
  const change = ((current - previous) / previous) * 100
  const sign = change >= 0 ? '+' : ''
  return ` (${sign}${change.toFixed(1)}% vs prior period)`
}

function num(n: number): string {
  return n.toLocaleString('en-US')
}

function currency(cents: string | number, currencyCode = 'USD'): string {
  const value = Number(cents) / 100
  return new Intl.NumberFormat('en-US', { style: 'currency', currency: currencyCode }).format(value)
}

/** Aggregates download rows by a field, returning sorted [{key, total}] pairs. */
function aggregateBy(rows: DownloadRow[], field: keyof DownloadRow): Array<{ key: string; total: number }> {
  const map = new Map<string, number>()
  for (const row of rows) {
    const key = String(row[field])
    map.set(key, (map.get(key) ?? 0) + row.downloads)
  }
  return [...map.entries()].map(([key, total]) => ({ key, total })).sort((a, b) => b.total - a.total)
}

/** Compares two semver strings, descending (higher version first). */
function compareSemverDesc(a: string, b: string): number {
  const pa = a.split('.').map(Number)
  const pb = b.split('.').map(Number)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pb[i] ?? 0) - (pa[i] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

function formatReport(data: DashboardData): string {
  const lines: string[] = []
  const h1 = (text: string) => lines.push(`# ${text}`, '')
  const h2 = (text: string) => lines.push(`## ${text}`, '')
  const line = (text: string) => lines.push(text)
  const blank = () => lines.push('')

  h1(`Cmdr analytics report (${data.range})`)
  line(`Generated: ${data.updatedAt}`)
  blank()

  // 1. Awareness
  h2('Awareness: how many people see Cmdr content?')
  if (!data.umami.ok) {
    line(`Couldn't load: ${data.umami.error}`)
  } else {
    const u = data.umami.data
    const totalPv = u.personalSite.pageviews.value + u.website.pageviews.value + u.prvw.pageviews.value
    const prevPv = u.personalSite.pageviews.prev + u.website.pageviews.prev + u.prvw.pageviews.prev
    line(`- Total page views: ${num(totalPv)}${delta(totalPv, prevPv)}`)
    line(
      `- veszelovszki.com views: ${num(u.personalSite.pageviews.value)}${delta(u.personalSite.pageviews.value, u.personalSite.pageviews.prev)}`,
    )
    line(
      `- getcmdr.com views: ${num(u.website.pageviews.value)}${delta(u.website.pageviews.value, u.website.pageviews.prev)}`,
    )
    line(`- getprvw.com views: ${num(u.prvw.pageviews.value)}${delta(u.prvw.pageviews.value, u.prvw.pageviews.prev)}`)
    line(
      `- veszelovszki.com visitors: ${num(u.personalSite.visitors.value)}${delta(u.personalSite.visitors.value, u.personalSite.visitors.prev)}`,
    )
    line(
      `- getcmdr.com visitors: ${num(u.website.visitors.value)}${delta(u.website.visitors.value, u.website.visitors.prev)}`,
    )
    line(`- getprvw.com visitors: ${num(u.prvw.visitors.value)}${delta(u.prvw.visitors.value, u.prvw.visitors.prev)}`)

    if (data.githubStars.ok) {
      const s = data.githubStars.data
      blank()
      line(`GitHub stars: ${num(s.totalStars)} total`)
      for (const repo of s.repos) {
        const recent7 = repo.daily
          .filter((d) => new Date(d.day) >= new Date(Date.now() - 7 * 86_400_000))
          .reduce((sum, d) => sum + d.newStars, 0)
        const recent30 = repo.daily
          .filter((d) => new Date(d.day) >= new Date(Date.now() - 30 * 86_400_000))
          .reduce((sum, d) => sum + d.newStars, 0)
        line(`  ${repo.repo}: ${num(repo.totalStars)} (last 7d: +${recent7}, last 30d: +${recent30})`)
      }
    }

    if (u.websiteReferrers.length > 0) {
      blank()
      line('Top referrers (getcmdr.com):')
      const totalRef = u.websiteReferrers.reduce((s, r) => s + r.y, 0)
      for (const ref of u.websiteReferrers.slice(0, 15)) {
        line(`  ${ref.x || '(direct)'}: ${num(ref.y)} (${pct(ref.y, totalRef)})`)
      }
    }

    if (u.prvwReferrers.length > 0) {
      blank()
      line('Top referrers (getprvw.com):')
      const totalRef = u.prvwReferrers.reduce((s, r) => s + r.y, 0)
      for (const ref of u.prvwReferrers.slice(0, 15)) {
        line(`  ${ref.x || '(direct)'}: ${num(ref.y)} (${pct(ref.y, totalRef)})`)
      }
    }
  }
  blank()

  // 2. Interest
  h2('Interest: how many engage with the product page?')
  if (!data.umami.ok && !data.posthog.ok) {
    line(
      `Couldn't load: ${[!data.umami.ok ? data.umami.error : '', !data.posthog.ok ? data.posthog.error : ''].filter(Boolean).join('; ')}`,
    )
  } else {
    if (data.umami.ok) {
      const u = data.umami.data
      line(
        `- getcmdr.com page views: ${num(u.website.pageviews.value)}${delta(u.website.pageviews.value, u.website.pageviews.prev)}`,
      )
      line(
        `- Unique visitors: ${num(u.website.visitors.value)}${delta(u.website.visitors.value, u.website.visitors.prev)}`,
      )
      line(
        `- Bounce rate: ${u.website.pageviews.value > 0 ? pct(u.website.bounces.value, u.website.visits.value) : 'N/A'}`,
      )

      if (u.downloadEvents.length > 0) {
        blank()
        line('Download button clicks:')
        for (const ev of u.downloadEvents.slice(0, 10)) {
          line(`  ${ev.x}: ${num(ev.y)}`)
        }
      }

      if (u.websitePages.length > 0) {
        blank()
        line('Top pages:')
        for (const page of u.websitePages.slice(0, 15)) {
          line(`  ${page.x}: ${num(page.y)} views`)
        }
      }

      if (u.websiteCountries.length > 0) {
        blank()
        line('Website visitors by country:')
        const totalCountry = u.websiteCountries.reduce((s, c) => s + c.y, 0)
        for (const c of u.websiteCountries.slice(0, 15)) {
          line(`  ${formatCountry(c.x)}: ${num(c.y)} (${pct(c.y, totalCountry)})`)
        }
      }
    }

    if (data.posthog.ok && data.posthog.data.dailyPageviews.length > 0) {
      blank()
      line('Daily page views (PostHog):')
      for (const row of data.posthog.data.dailyPageviews) {
        line(`  ${row.day}: ${num(row.views)}`)
      }
    }
  }
  blank()

  // 3. Download
  h2('Download: how many actually download?')
  if (!data.cloudflare.ok && !data.github.ok) {
    line(
      `Couldn't load: ${[!data.cloudflare.ok ? data.cloudflare.error : '', !data.github.ok ? data.github.error : ''].filter(Boolean).join('; ')}`,
    )
  } else {
    if (data.cloudflare.ok) {
      const cf = data.cloudflare.data
      const totalDl = cf.downloads.reduce((s, r) => s + r.downloads, 0)
      line(`- Downloads (Analytics Engine): ${num(totalDl)}`)

      if (data.github.ok) {
        line(`- Downloads (GitHub, all-time): ${num(data.github.data.totalDownloads)}`)
      }

      if (cf.downloads.length > 0) {
        const byVersion = aggregateBy(cf.downloads, 'version').sort((a, b) => compareSemverDesc(a.key, b.key))
        const byArch = aggregateBy(cf.downloads, 'arch')
        const byCountry = aggregateBy(cf.downloads, 'country')

        blank()
        line('By version:')
        for (const v of byVersion) {
          line(`  ${v.key}: ${num(v.total)} (${pct(v.total, totalDl)})`)
        }

        blank()
        line('By architecture:')
        for (const a of byArch) {
          line(`  ${a.key}: ${num(a.total)} (${pct(a.total, totalDl)})`)
        }

        blank()
        line('By country:')
        for (const c of byCountry.slice(0, 20)) {
          line(`  ${formatCountry(c.key)}: ${num(c.total)} (${pct(c.total, totalDl)})`)
        }

        // Daily downloads
        blank()
        line('Daily downloads:')
        const byDay = new Map<string, number>()
        for (const row of cf.downloads) {
          byDay.set(row.day, (byDay.get(row.day) ?? 0) + row.downloads)
        }
        for (const [day, count] of [...byDay.entries()].sort(([a], [b]) => b.localeCompare(a))) {
          line(`  ${day}: ${num(count)}`)
        }

        // Cross-breakdown: top countries × architecture
        blank()
        line('Top countries by architecture:')
        for (const c of byCountry.slice(0, 10)) {
          const countryRows = cf.downloads.filter((r) => r.country === c.key)
          const countryArches = aggregateBy(countryRows, 'arch')
          const archStr = countryArches.map((a) => `${a.key}: ${num(a.total)}`).join(', ')
          line(`  ${formatCountry(c.key)}: ${archStr}`)
        }

        // Cross-breakdown: top countries × version
        blank()
        line('Top countries by version:')
        for (const c of byCountry.slice(0, 10)) {
          const countryRows = cf.downloads.filter((r) => r.country === c.key)
          const countryVersions = aggregateBy(countryRows, 'version')
            .sort((a, b) => compareSemverDesc(a.key, b.key))
            .slice(0, 5)
          const verStr = countryVersions.map((v) => `${v.key}: ${num(v.total)}`).join(', ')
          line(`  ${formatCountry(c.key)}: ${verStr}`)
        }

        // Daily downloads by version (top 5)
        blank()
        const topVersionKeys = byVersion.slice(0, 5).map((v) => v.key)
        line(`Daily downloads by version (top ${topVersionKeys.length}):`)
        const days = [...new Set(cf.downloads.map((r) => r.day))].sort()
        for (const day of days) {
          const dayRows = cf.downloads.filter((r) => r.day === day)
          const parts = topVersionKeys
            .map((v) => {
              const count = dayRows.filter((r) => r.version === v).reduce((s, r) => s + r.downloads, 0)
              return count > 0 ? `${v}: ${count}` : null
            })
            .filter(Boolean)
          line(`  ${day}: ${parts.join(', ') || '(none)'}`)
        }
      }
    }

    if (data.github.ok && data.github.data.releases.length > 0) {
      blank()
      line('GitHub releases (all-time):')
      for (const rel of data.github.data.releases.slice(0, 10)) {
        line(`  ${rel.tagName}: ${num(rel.totalDownloads)} downloads (published ${rel.publishedAt.split('T')[0]})`)
      }
    }
  }
  blank()

  // 4. Active use
  h2('Active use: how many run the app?')
  if (!data.cloudflare.ok) {
    line(`Couldn't load: ${data.cloudflare.error}`)
  } else {
    const cf = data.cloudflare.data
    const dau = cf.heartbeatDau

    if (dau.length > 0) {
      const latestDau = dau[dau.length - 1].dau
      const peakDau = dau.reduce((max, r) => Math.max(max, r.dau), 0)
      const totalBeats = dau.reduce((s, r) => s + r.beats, 0)
      const totalDau = dau.reduce((s, r) => s + r.dau, 0)
      const beatsPerActive = totalDau > 0 ? (totalBeats / totalDau).toFixed(1) : '0'

      line(`- Daily active installs (latest day): ${num(latestDau)}`)
      line(`- Peak daily active: ${num(peakDau)}`)
      line(`- Beats per active install: ${beatsPerActive}`)

      blank()
      line('Daily active installs (by day):')
      for (const row of [...dau].sort((a, b) => b.date.localeCompare(a.date))) {
        line(`  ${row.date}: ${num(row.dau)} active, ${num(row.beats)} beats`)
      }
    } else {
      line('- Daily active installs: none yet (heartbeat fills as beta testers update and run the new build)')
    }

    if (data.license.ok) {
      const lic = data.license.data
      blank()
      line(`- Total activations: ${num(lic.totalActivations)}`)
      if (lic.activeDevices !== null) {
        line(`- Active devices: ${num(lic.activeDevices)}`)
      }
    }
  }
  blank()

  // 5. Payment
  h2('Payment: how many pay?')
  if (!data.paddle.ok) {
    line(`Couldn't load: ${data.paddle.error}`)
  } else {
    const p = data.paddle.data
    const totalRevenue = p.transactions.reduce((s, t) => s + Number(t.total), 0)
    const curr = p.transactions[0]?.currencyCode ?? 'USD'

    line(`- Revenue: ${currency(totalRevenue, curr)}`)
    line(`- Transactions: ${num(p.transactions.length)}`)
    line(`- Active subscriptions: ${num(p.activeSubscriptions.length)}`)

    if (p.transactions.length > 0) {
      blank()
      line('Recent transactions:')
      for (const txn of p.transactions.slice(0, 15)) {
        line(`  ${txn.createdAt.split('T')[0]}: ${currency(txn.total, txn.currencyCode)} (${txn.status})`)
      }
    }
  }
  blank()

  // 6. Retention
  h2('Retention: do they stay?')
  if (!data.paddle.ok) {
    line(`Couldn't load: ${data.paddle.error}`)
  } else {
    const p = data.paddle.data
    const statusEntries = Object.entries(p.subscriptionsByStatus)
    const totalSubs = statusEntries.reduce((s, e) => s + e[1], 0)
    const activeSubs = p.subscriptionsByStatus['active'] ?? 0
    const canceledSubs = p.subscriptionsByStatus['canceled'] ?? 0
    const churn = totalSubs > 0 ? `${((canceledSubs / totalSubs) * 100).toFixed(1)}%` : 'N/A'

    line(`- Active subscriptions: ${num(activeSubs)}`)
    line(`- Churn rate: ${churn}`)

    if (statusEntries.length > 0) {
      blank()
      line('Subscriptions by status:')
      for (const [status, count] of statusEntries) {
        line(`  ${status}: ${num(count)} (${pct(count, totalSubs)})`)
      }
    }
  }
  blank()

  // 7. Feedback & errors
  h2('Feedback & errors: what are users telling us?')
  if (!data.feedbackAndErrors.ok) {
    line(`Couldn't load: ${data.feedbackAndErrors.error}`)
  } else {
    const fe = data.feedbackAndErrors.data
    line(`- Feedback messages: ${num(fe.feedback.length)}`)
    line(`- Awaiting reply (have a reply-to email): ${num(countFeedbackWithReplyTo(fe.feedback))}`)
    line(`- Error reports: ${num(fe.errorReports.length)}`)

    if (fe.errorReports.length > 0) {
      blank()
      line('Error reports by kind:')
      for (const k of tallyErrorReportsByField(fe.errorReports, 'kind')) {
        line(`  ${k.key}: ${num(k.count)}`)
      }

      blank()
      line('Error reports by version:')
      for (const v of tallyErrorReportsByField(fe.errorReports, 'appVersion')) {
        line(`  ${v.key}: ${num(v.count)}`)
      }

      blank()
      line('Error reports by day:')
      for (const d of errorReportsByDay(fe.errorReports)) {
        line(`  ${d.date}: ${num(d.count)}`)
      }
    }

    if (fe.feedback.length > 0) {
      blank()
      line('Recent feedback:')
      for (const msg of fe.feedback.slice(0, 30)) {
        const replyTo = msg.email ? ` [reply-to: ${msg.email}]` : ''
        const text = msg.feedback.replace(/\s+/g, ' ').slice(0, 280)
        line(`  ${msg.createdAt.split(' ')[0]} (v${msg.appVersion})${replyTo}: ${text}`)
      }
    }
  }

  return lines.join('\n')
}

export const GET: RequestHandler = async ({ url, platform }) => {
  try {
    const data = await fetchDashboardData(platform, url.searchParams.get('range') ?? '7d')
    const report = formatReport(data)

    return new Response(report, {
      headers: { 'Content-Type': 'text/plain; charset=utf-8' },
    })
  } catch (e) {
    const err = e instanceof Error ? `${e.message}\n${e.stack}` : String(e)
    return new Response(`Report generation failed:\n${err}`, {
      headers: { 'Content-Type': 'text/plain; charset=utf-8' },
    })
  }
}
