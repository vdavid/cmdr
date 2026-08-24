/**
 * Renders the whole dashboard as one plain-text report: the agent- and David-readable dump behind
 * `GET /api/report`. Pure formatting, no fetching, so `format-report.test.ts` can pin the exact
 * output byte for byte.
 *
 * The imports are relative rather than `$lib/...` on purpose: the vitest config doesn't load the
 * SvelteKit plugin, so the alias isn't resolvable from a plain unit test.
 */
import type { DashboardData } from '../../../lib/server/fetch-all.js'
import type { SourceResult } from '../../../lib/server/types.js'
import type {
  CloudflareData,
  DownloadRow,
  HeartbeatDauRow,
  UpdateActivityRow,
} from '../../../lib/server/sources/cloudflare.js'
import type { UmamiData, UmamiMetricItem } from '../../../lib/server/sources/umami.js'
import type { GitHubData, GitHubRelease, GitHubStarsData } from '../../../lib/server/sources/github.js'
import type { PostHogData } from '../../../lib/server/sources/posthog.js'
import type { PaddleData } from '../../../lib/server/sources/paddle.js'
import type { LicenseData } from '../../../lib/server/sources/license.js'
import type { FunnelData, FunnelRow } from '../../../lib/server/sources/funnel.js'
import type { SettingAdoption, SettingsAdoption } from '../../../lib/server/settings-defaults.js'
import type { FeedbackAndErrorsData } from '../../../lib/server/sources/feedback-and-errors.js'
import type { FeedbackRow, ErrorReportRow } from '../../../lib/feedback-and-errors.js'
import {
  countFeedbackWithReplyTo,
  tallyErrorReportsByField,
  errorReportsByDay,
} from '../../../lib/feedback-and-errors.js'
import { aggregateChannels, aggregateReferers, aggregateUaFamilies } from '../../../lib/funnel.js'
import { boundsByDay, formatBound, largestUnseenShare, latestBound } from '../../../lib/active-installs.js'
import {
  formatShare,
  formatShareUnlike,
  formatValueShare,
  mostChanged,
  settingByKey,
  shareOnDefault,
  topOverride,
  unchangedNote,
} from '../../../lib/settings-adoption.js'

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

/** One ranked breakdown: a key and its total, biggest first (unless re-sorted by the caller). */
type Breakdown = Array<{ key: string; total: number }>

/** Aggregates download rows by a field, returning sorted [{key, total}] pairs. */
function aggregateBy(rows: DownloadRow[], field: keyof DownloadRow): Breakdown {
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

/**
 * Collects the report's lines. `section()` emits the blank line that separates it from whatever came
 * before, so no section has to remember a trailing blank and the sections stay reorderable.
 */
class ReportWriter {
  private readonly lines: string[] = []

  title(text: string): void {
    this.lines.push(`# ${text}`, '')
  }

  section(text: string): void {
    this.lines.push('', `## ${text}`, '')
  }

  line(text: string): void {
    this.lines.push(text)
  }

  blank(): void {
    this.lines.push('')
  }

  text(): string {
    return this.lines.join('\n')
  }
}

/** A blank line, a heading, then one `  key: value` line per entry. Nothing at all when empty. */
function writeList<T>(w: ReportWriter, heading: string, items: T[], limit: number, render: (item: T) => string): void {
  if (items.length === 0) return
  w.blank()
  w.line(heading)
  for (const item of items.slice(0, limit)) w.line(render(item))
}

/** A blank line, a heading, then one `  label: count (share%)` line per entry. Always written. */
function writeShares(
  w: ReportWriter,
  heading: string,
  entries: Breakdown,
  total: number,
  label: (key: string) => string,
  limit?: number,
): void {
  w.blank()
  w.line(heading)
  for (const entry of entries.slice(0, limit ?? entries.length)) {
    w.line(`  ${label(entry.key)}: ${num(entry.total)} (${pct(entry.total, total)})`)
  }
}

// ── 0. Daily funnel (always the last 30 UTC days, independent of the selected range) ──────────────

function writeFunnelRows(w: ReportWriter, rows: FunnelRow[]): void {
  const dash = (n: number | null) => (n === null ? '-' : num(n))
  const pctOrDash = (f: number | null) => (f === null ? '-' : `${(f * 100).toFixed(0)}%`)
  w.line('day | visitors | dl clicks | server dls | installs | D7 | signups | purchases')
  // Most recent day first, to match the dashboard table.
  for (const r of [...rows].reverse()) {
    w.line(
      `${r.date} | ${dash(r.visitors)} | ${dash(r.downloadClicks)} | ${dash(r.serverDownloads)} | ` +
        `${dash(r.newInstalls)} | ${pctOrDash(r.d7Retention)} | ${dash(r.newsletterSignups)} | ${dash(r.purchases)}`,
    )
  }
}

/**
 * First-touch ref rolled up over the whole 30-day window. "(none)" = no channel known (Homebrew,
 * direct links, return visits, cross-device journeys, and pre-2026-06-12 rows).
 */
function writeFunnelChannels(w: ReportWriter, rows: FunnelRow[]): void {
  const channels = aggregateChannels(rows)
  writeList(
    w,
    'Channels (last 30 days), downloads by first-touch ref:',
    channels,
    channels.length,
    (c) => `- ${c.ref}: ${num(c.count)}`,
  )
}

/**
 * The raw `Referer` host of each `/download` hit. Unlike the first-touch ref above (set only by the
 * website button), this is captured on every hit, so it reveals where the direct, no-ref downloads
 * came from. "(none)" = no usable referer (typed URL, privacy browser, referrer-policy strip,
 * Homebrew/curl, pre-2026-06-25 rows).
 */
function writeFunnelReferers(w: ReportWriter, rows: FunnelRow[]): void {
  const referers = aggregateReferers(rows)
  writeList(
    w,
    "Download referrers (last 30 days), by the /download hit's Referer host:",
    referers,
    referers.length,
    (r) => `- ${r.ref}: ${num(r.count)}`,
  )
}

/**
 * The `/download` User-Agent family. Cmdr is macOS-only, so a non-macOS UA can't be a real install.
 * "Human installs" drops only those provably-impossible bot hits and keeps the ambiguous ones, so the
 * headline stops reading as half noise. Note the scraper spoofs Mac browser UAs, so "human" is "could
 * be real", not proof; "unknown" (no/odd UA) stays counted, never excluded.
 */
function writeFunnelUaFamilies(w: ReportWriter, rows: FunnelRow[]): void {
  const ua = aggregateUaFamilies(rows)
  if (ua.total === 0) return
  w.blank()
  w.line('Downloads by client (last 30 days), by User-Agent family:')
  w.line(`- Human installs: ${num(ua.humanInstalls)} (of ${num(ua.total)} raw server downloads)`)
  w.line(`- human (Mac browser, Homebrew, or curl/wget): ${num(ua.human)}`)
  w.line(`- bot / impossible install (Windows, Android, Linux, or X11 UA, excluded): ${num(ua.bot)}`)
  w.line(`- unknown (no or unrecognized UA, kept in the count): ${num(ua.unknown)}`)
}

function writeFunnelSection(w: ReportWriter, funnel: SourceResult<FunnelData>): void {
  w.section('Daily funnel: the last 30 days, one row per UTC day')
  if (!funnel.ok) {
    w.line(`Couldn't load: ${funnel.error}`)
    return
  }
  const { rows } = funnel.data
  writeFunnelRows(w, rows)
  writeFunnelChannels(w, rows)
  writeFunnelReferers(w, rows)
  writeFunnelUaFamilies(w, rows)
}

// ── 1. Awareness ─────────────────────────────────────────────────────────────────────────────────

function writeGitHubStars(w: ReportWriter, githubStars: SourceResult<GitHubStarsData>): void {
  if (!githubStars.ok) return
  const s = githubStars.data
  w.blank()
  w.line(`GitHub stars: ${num(s.totalStars)} total`)
  for (const repo of s.repos) {
    const recent7 = repo.daily
      .filter((d) => new Date(d.day) >= new Date(Date.now() - 7 * 86_400_000))
      .reduce((sum, d) => sum + d.newStars, 0)
    const recent30 = repo.daily
      .filter((d) => new Date(d.day) >= new Date(Date.now() - 30 * 86_400_000))
      .reduce((sum, d) => sum + d.newStars, 0)
    w.line(`  ${repo.repo}: ${num(repo.totalStars)} (last 7d: +${String(recent7)}, last 30d: +${String(recent30)})`)
  }
}

function writeTopReferrers(w: ReportWriter, heading: string, items: UmamiMetricItem[]): void {
  const total = items.reduce((s, r) => s + r.y, 0)
  writeList(w, heading, items, 15, (ref) => `  ${ref.x || '(direct)'}: ${num(ref.y)} (${pct(ref.y, total)})`)
}

function writeAwarenessSection(
  w: ReportWriter,
  umami: SourceResult<UmamiData>,
  githubStars: SourceResult<GitHubStarsData>,
): void {
  w.section('Awareness: how many people see Cmdr content?')
  if (!umami.ok) {
    w.line(`Couldn't load: ${umami.error}`)
    return
  }
  const u = umami.data
  const totalPv = u.personalSite.pageviews.value + u.website.pageviews.value + u.prvw.pageviews.value
  const prevPv = u.personalSite.pageviews.prev + u.website.pageviews.prev + u.prvw.pageviews.prev
  w.line(`- Total page views: ${num(totalPv)}${delta(totalPv, prevPv)}`)
  w.line(
    `- veszelovszki.com views: ${num(u.personalSite.pageviews.value)}${delta(u.personalSite.pageviews.value, u.personalSite.pageviews.prev)}`,
  )
  w.line(
    `- getcmdr.com views: ${num(u.website.pageviews.value)}${delta(u.website.pageviews.value, u.website.pageviews.prev)}`,
  )
  w.line(`- getprvw.com views: ${num(u.prvw.pageviews.value)}${delta(u.prvw.pageviews.value, u.prvw.pageviews.prev)}`)
  w.line(
    `- veszelovszki.com visitors: ${num(u.personalSite.visitors.value)}${delta(u.personalSite.visitors.value, u.personalSite.visitors.prev)}`,
  )
  w.line(
    `- getcmdr.com visitors: ${num(u.website.visitors.value)}${delta(u.website.visitors.value, u.website.visitors.prev)}`,
  )
  w.line(`- getprvw.com visitors: ${num(u.prvw.visitors.value)}${delta(u.prvw.visitors.value, u.prvw.visitors.prev)}`)

  writeGitHubStars(w, githubStars)
  writeTopReferrers(w, 'Top referrers (getcmdr.com):', u.websiteReferrers)
  writeTopReferrers(w, 'Top referrers (getprvw.com):', u.prvwReferrers)
}

// ── 2. Interest ──────────────────────────────────────────────────────────────────────────────────

function writeInterestUmami(w: ReportWriter, u: UmamiData): void {
  w.line(
    `- getcmdr.com page views: ${num(u.website.pageviews.value)}${delta(u.website.pageviews.value, u.website.pageviews.prev)}`,
  )
  w.line(
    `- Unique visitors: ${num(u.website.visitors.value)}${delta(u.website.visitors.value, u.website.visitors.prev)}`,
  )
  w.line(
    `- Bounce rate: ${u.website.pageviews.value > 0 ? pct(u.website.bounces.value, u.website.visits.value) : 'N/A'}`,
  )

  writeList(w, 'Download button clicks:', u.downloadEvents, 10, (ev) => `  ${ev.x}: ${num(ev.y)}`)
  writeList(w, 'Top pages:', u.websitePages, 15, (page) => `  ${page.x}: ${num(page.y)} views`)
  const totalCountry = u.websiteCountries.reduce((s, c) => s + c.y, 0)
  writeList(
    w,
    'Website visitors by country:',
    u.websiteCountries,
    15,
    (c) => `  ${formatCountry(c.x)}: ${num(c.y)} (${pct(c.y, totalCountry)})`,
  )
}

function writeInterestSection(
  w: ReportWriter,
  umami: SourceResult<UmamiData>,
  posthog: SourceResult<PostHogData>,
): void {
  w.section('Interest: how many engage with the product page?')
  if (!umami.ok && !posthog.ok) {
    w.line(`Couldn't load: ${[umami.error, posthog.error].filter(Boolean).join('; ')}`)
    return
  }
  if (umami.ok) writeInterestUmami(w, umami.data)
  if (posthog.ok) {
    writeList(
      w,
      'Daily page views (PostHog):',
      posthog.data.dailyPageviews,
      posthog.data.dailyPageviews.length,
      (row) => `  ${row.day}: ${num(row.views)}`,
    )
  }
}

// ── 3. Download ──────────────────────────────────────────────────────────────────────────────────

/** One `  day: total` line per day, newest first, summing the given per-row field. */
function writeDailyTotals(
  w: ReportWriter,
  heading: string,
  rows: DownloadRow[],
  field: 'downloads' | 'uniqueDownloads',
): void {
  w.blank()
  w.line(heading)
  const byDay = new Map<string, number>()
  for (const row of rows) byDay.set(row.day, (byDay.get(row.day) ?? 0) + row[field])
  for (const [day, count] of [...byDay.entries()].sort(([a], [b]) => b.localeCompare(a))) {
    w.line(`  ${day}: ${num(count)}`)
  }
}

/** New installs by source (deduped). Keeps the order website, homebrew, other. */
function writeNewInstallsBySource(w: ReportWriter, rows: DownloadRow[], totalNew: number): void {
  const sourceOrder = ['website', 'homebrew', 'other']
  const newBySource = new Map<string, number>()
  for (const row of rows) newBySource.set(row.source, (newBySource.get(row.source) ?? 0) + row.uniqueDownloads)
  w.blank()
  w.line('New installs by source (deduped):')
  for (const key of sourceOrder) {
    const count = newBySource.get(key)
    if (count === undefined) continue
    w.line(`  ${key}: ${num(count)} (${pct(count, totalNew)})`)
  }
}

/** The top 10 countries crossed with a second dimension, as `  country: key: n, key: n`. */
function writeCountryCross(
  w: ReportWriter,
  heading: string,
  rows: DownloadRow[],
  byCountry: Breakdown,
  breakdown: (countryRows: DownloadRow[]) => Breakdown,
): void {
  w.blank()
  w.line(heading)
  for (const c of byCountry.slice(0, 10)) {
    const parts = breakdown(rows.filter((r) => r.country === c.key)).map((e) => `${e.key}: ${num(e.total)}`)
    w.line(`  ${formatCountry(c.key)}: ${parts.join(', ')}`)
  }
}

function writeDailyDownloadsByVersion(w: ReportWriter, rows: DownloadRow[], byVersion: Breakdown): void {
  w.blank()
  const topVersionKeys = byVersion.slice(0, 5).map((v) => v.key)
  w.line(`Daily downloads by version (top ${String(topVersionKeys.length)}):`)
  const days = [...new Set(rows.map((r) => r.day))].sort()
  for (const day of days) {
    const dayRows = rows.filter((r) => r.day === day)
    const parts = topVersionKeys
      .map((v) => {
        const count = dayRows.filter((r) => r.version === v).reduce((s, r) => s + r.downloads, 0)
        return count > 0 ? `${v}: ${String(count)}` : null
      })
      .filter((part) => part !== null)
    w.line(`  ${day}: ${parts.join(', ') || '(none)'}`)
  }
}

function writeDownloadBreakdowns(w: ReportWriter, rows: DownloadRow[], totalDl: number, totalNew: number): void {
  const byVersion = aggregateBy(rows, 'version').sort((a, b) => compareSemverDesc(a.key, b.key))
  const byArch = aggregateBy(rows, 'arch')
  const byCountry = aggregateBy(rows, 'country')

  writeNewInstallsBySource(w, rows, totalNew)
  writeDailyTotals(w, 'Daily new installs (deduped):', rows, 'uniqueDownloads')
  writeShares(w, 'By version:', byVersion, totalDl, (key) => key)
  writeShares(w, 'By architecture:', byArch, totalDl, (key) => key)
  writeShares(w, 'By country:', byCountry, totalDl, formatCountry, 20)
  writeDailyTotals(w, 'Daily downloads:', rows, 'downloads')
  writeCountryCross(w, 'Top countries by architecture:', rows, byCountry, (countryRows) =>
    aggregateBy(countryRows, 'arch'),
  )
  writeCountryCross(w, 'Top countries by version:', rows, byCountry, (countryRows) =>
    aggregateBy(countryRows, 'version')
      .sort((a, b) => compareSemverDesc(a.key, b.key))
      .slice(0, 5),
  )
  writeDailyDownloadsByVersion(w, rows, byVersion)
}

function writeCloudflareDownloads(w: ReportWriter, cf: CloudflareData, github: SourceResult<GitHubData>): void {
  const totalDl = cf.downloads.reduce((s, r) => s + r.downloads, 0)
  const totalNew = cf.downloads.reduce((s, r) => s + r.uniqueDownloads, 0)
  // Methodology, stated up front so the numbers below aren't a black box.
  w.line(
    '(New installs = DMG downloads via getcmdr.com, deduplicated to distinct people per day by daily-hashed IP, ' +
      'bot/link-preview hits dropped by user agent, in-app auto-updates excluded. Raw = every download request.)',
  )
  w.line(`- New installs (deduped): ${num(totalNew)}`)
  w.line(`- Download requests (raw): ${num(totalDl)}`)

  if (github.ok) {
    w.line(`- Downloads (GitHub, all-time): ${num(github.data.totalDownloads)}`)
  }

  if (cf.downloads.length > 0) writeDownloadBreakdowns(w, cf.downloads, totalDl, totalNew)
}

function writeGitHubReleases(w: ReportWriter, releases: GitHubRelease[]): void {
  writeList(
    w,
    'GitHub releases (all-time):',
    releases,
    10,
    (rel) => `  ${rel.tagName}: ${num(rel.totalDownloads)} downloads (published ${rel.publishedAt.split('T')[0]})`,
  )
}

function writeDownloadSection(
  w: ReportWriter,
  cloudflare: SourceResult<CloudflareData>,
  github: SourceResult<GitHubData>,
): void {
  w.section('Download: how many actually download?')
  if (!cloudflare.ok && !github.ok) {
    w.line(`Couldn't load: ${[cloudflare.error, github.error].filter(Boolean).join('; ')}`)
    return
  }
  if (cloudflare.ok) writeCloudflareDownloads(w, cloudflare.data, github)
  if (github.ok) writeGitHubReleases(w, github.data.releases)
}

// ── 4. Active use ────────────────────────────────────────────────────────────────────────────────

/**
 * Active installs as a range. The low end is what the heartbeat proves ran; the high end is how far
 * the update checks reach, which catches opted-out installs. Neither alone is the answer, and the
 * gap between them is the size of the blind spot.
 */
function writeHeartbeatDau(w: ReportWriter, dau: HeartbeatDauRow[], updateActivity: UpdateActivityRow[]): void {
  const bounds = boundsByDay(dau, updateActivity)
  const peakDau = dau.reduce((max, r) => Math.max(max, r.dau), 0)
  const totalBeats = dau.reduce((s, r) => s + r.beats, 0)
  const totalDau = dau.reduce((s, r) => s + r.dau, 0)
  const beatsPerActive = totalDau > 0 ? (totalBeats / totalDau).toFixed(1) : '0'
  const unseen = largestUnseenShare(bounds)

  w.line(`- Active installs (latest day): ${formatBound(latestBound(bounds))}`)
  w.line(`- Peak confirmed running: ${num(peakDau)}`)
  w.line(`- Beats per active install: ${beatsPerActive}`)
  if (unseen !== null) {
    w.line(`- Widest blind spot: ${String(Math.round(unseen * 100))}% of the high end never sent a heartbeat`)
  }

  w.blank()
  w.line('The low end counts install ids we heard from on the hourly heartbeat, so those installs definitely ran')
  w.line('Cmdr. The high end counts distinct addresses that checked for updates, a separate consent that installs')
  w.line('with analytics off still ride. The high end is a rough reach, not a ceiling: addresses are not installs,')
  w.line('a shared connection counts an office or household once, a changing home address counts one install more')
  w.line('than once across days, and anyone with automatic update checks off never appears at all.')

  w.blank()
  w.line('Active installs (by day, heard from / checked for updates):')
  for (const row of [...dau].sort((a, b) => b.date.localeCompare(a.date))) {
    const bound = bounds.find((b) => b.day === row.date)
    const reach = bound?.reach ?? null
    w.line(
      `  ${row.date}: ${num(row.dau)} heard from, ${reach === null ? 'no update data' : `${num(reach)} checked`}, ${num(row.beats)} beats`,
    )
  }
}

/** Distinct update-enabled installs that checked per day, stacked by version. */
function writeUpdateActivity(w: ReportWriter, ua: UpdateActivityRow[]): void {
  if (ua.length === 0) return
  w.blank()
  w.line('Got the latest release per day (update-enabled installs that checked, deduped, by version):')
  const updateDays = [...new Set(ua.map((r) => r.day))].sort((a, b) => b.localeCompare(a))
  for (const day of updateDays) {
    const dayRows = ua.filter((r) => r.day === day).sort((a, b) => compareSemverDesc(a.version, b.version))
    const total = dayRows.reduce((s, r) => s + r.updaters, 0)
    const parts = dayRows.map((r) => `v${r.version}: ${num(r.updaters)}`).join(', ')
    w.line(`  ${day}: ${num(total)} total (${parts})`)
  }
}

function writeLicenseTotals(w: ReportWriter, license: SourceResult<LicenseData>): void {
  if (!license.ok) return
  const lic = license.data
  w.blank()
  w.line(`- Total activations: ${num(lic.totalActivations)}`)
  if (lic.activeDevices !== null) {
    w.line(`- Active devices: ${num(lic.activeDevices)}`)
  }
}

function writeActiveUseSection(
  w: ReportWriter,
  cloudflare: SourceResult<CloudflareData>,
  license: SourceResult<LicenseData>,
): void {
  w.section('Active use: how many run the app?')
  if (!cloudflare.ok) {
    w.line(`Couldn't load: ${cloudflare.error}`)
    return
  }
  const cf = cloudflare.data
  if (cf.heartbeatDau.length > 0) {
    writeHeartbeatDau(w, cf.heartbeatDau, cf.updateActivity)
    writeUpdateActivity(w, cf.updateActivity)
  } else {
    w.line('- Active installs: none yet (the heartbeat fills as beta testers update and run the new build)')
  }
  writeLicenseTotals(w, license)
}

// ── 5. Settings adoption ─────────────────────────────────────────────────────────────────────────

/** One line per setting somebody has moved, most-moved first. */
function writeChangedSettings(w: ReportWriter, settings: SettingAdoption[]): void {
  const changed = mostChanged(settings)
  w.blank()
  if (changed.length === 0) {
    w.line('Nobody has moved a setting off its default yet.')
    return
  }
  w.line(`Settings people change (most-moved first; the other ${unchangedNote(settings.length - changed.length)}):`)
  for (const setting of changed) {
    const share = shareOnDefault(setting)
    const override = topOverride(setting)
    const parts = [
      `default ${setting.defaultLabel ?? '(moved between versions)'}`,
      `${num(setting.eligible)} installs`,
      `${share === null ? 'n/a' : formatShare(share)} on default`,
    ]
    if (override) parts.push(`most common change: ${override.label} (${num(override.installs)})`)
    w.line(`  ${setting.key}: ${parts.join(', ')}`)
  }
}

/**
 * Adoption, which the raw config shape can't give: the app saves only settings someone changed, so
 * an absent key is resolved against the defaults that shipped in that install's version.
 */
function writeSettingsAdoptionSection(w: ReportWriter, settingsAdoption: SourceResult<SettingsAdoption>): void {
  w.section('Settings adoption: what do people actually turn on?')
  if (!settingsAdoption.ok) {
    w.line(`Couldn't load: ${settingsAdoption.error}`)
    return
  }
  const data = settingsAdoption.data
  const readable = data.totalInstalls - data.unresolvedInstalls
  if (readable === 0) {
    w.line('- No installs to read yet (settings adoption fills as beta testers run a build that reports a config).')
    return
  }

  w.line(`- Installs we can read: ${num(readable)}`)
  if (data.unresolvedInstalls > 0) {
    w.line(`- Not counted: ${num(data.unresolvedInstalls)} on a version older than the settings history goes back`)
  }
  w.line(`- Drive indexing on: ${formatValueShare(settingByKey(data.settings, 'indexing.enabled'), 'on')}`)
  w.line(`- Image search on: ${formatValueShare(settingByKey(data.settings, 'mediaIndex.enabled'), 'on')}`)
  w.line(`- AI switched on: ${formatShareUnlike(settingByKey(data.settings, 'ai.provider'), 'off')}`)
  w.blank()
  w.line(
    'Each install counts once, at its latest heartbeat. A setting is scored only against installs whose build ' +
      'actually had it, so a young setting shows a smaller total than an old one.',
  )
  writeChangedSettings(w, data.settings)
}

// ── 6. Payment ───────────────────────────────────────────────────────────────────────────────────

function writePaymentSection(w: ReportWriter, paddle: SourceResult<PaddleData>): void {
  w.section('Payment: how many pay?')
  if (!paddle.ok) {
    w.line(`Couldn't load: ${paddle.error}`)
    return
  }
  const p = paddle.data
  const totalRevenue = p.transactions.reduce((s, t) => s + Number(t.total), 0)
  const curr = p.transactions[0]?.currencyCode ?? 'USD'

  w.line(`- Revenue: ${currency(totalRevenue, curr)}`)
  w.line(`- Transactions: ${num(p.transactions.length)}`)
  w.line(`- Active subscriptions: ${num(p.activeSubscriptions.length)}`)

  writeList(
    w,
    'Recent transactions:',
    p.transactions,
    15,
    (txn) => `  ${txn.createdAt.split('T')[0]}: ${currency(txn.total, txn.currencyCode)} (${txn.status})`,
  )
}

// ── 7. Retention ─────────────────────────────────────────────────────────────────────────────────

function writeRetentionSection(w: ReportWriter, paddle: SourceResult<PaddleData>): void {
  w.section('Retention: do they stay?')
  if (!paddle.ok) {
    w.line(`Couldn't load: ${paddle.error}`)
    return
  }
  const p = paddle.data
  const statusEntries = Object.entries(p.subscriptionsByStatus)
  const totalSubs = statusEntries.reduce((s, e) => s + e[1], 0)
  const activeSubs = p.subscriptionsByStatus['active'] ?? 0
  const canceledSubs = p.subscriptionsByStatus['canceled'] ?? 0
  const churn = totalSubs > 0 ? `${((canceledSubs / totalSubs) * 100).toFixed(1)}%` : 'N/A'

  w.line(`- Active subscriptions: ${num(activeSubs)}`)
  w.line(`- Churn rate: ${churn}`)

  writeList(
    w,
    'Subscriptions by status:',
    statusEntries,
    statusEntries.length,
    ([status, count]) => `  ${status}: ${num(count)} (${pct(count, totalSubs)})`,
  )
}

// ── 8. Feedback & errors ─────────────────────────────────────────────────────────────────────────

function writeErrorReportBreakdowns(w: ReportWriter, errorReports: ErrorReportRow[]): void {
  if (errorReports.length === 0) return
  const byKind = tallyErrorReportsByField(errorReports, 'kind')
  writeList(w, 'Error reports by kind:', byKind, byKind.length, (k) => `  ${k.key}: ${num(k.count)}`)
  const byVersion = tallyErrorReportsByField(errorReports, 'appVersion')
  writeList(w, 'Error reports by version:', byVersion, byVersion.length, (v) => `  ${v.key}: ${num(v.count)}`)
  const byDay = errorReportsByDay(errorReports)
  writeList(w, 'Error reports by day:', byDay, byDay.length, (d) => `  ${d.date}: ${num(d.count)}`)
}

function writeRecentFeedback(w: ReportWriter, feedback: FeedbackRow[]): void {
  writeList(w, 'Recent feedback:', feedback, 30, (msg) => {
    const replyTo = msg.email ? ` [reply-to: ${msg.email}]` : ''
    const text = msg.feedback.replace(/\s+/g, ' ').slice(0, 280)
    return `  ${msg.createdAt.split(' ')[0]} (v${msg.appVersion})${replyTo}: ${text}`
  })
}

function writeFeedbackSection(w: ReportWriter, feedbackAndErrors: SourceResult<FeedbackAndErrorsData>): void {
  w.section('Feedback & errors: what are users telling us?')
  if (!feedbackAndErrors.ok) {
    w.line(`Couldn't load: ${feedbackAndErrors.error}`)
    return
  }
  const fe = feedbackAndErrors.data
  w.line(`- Feedback messages: ${num(fe.feedback.length)}`)
  w.line(`- Awaiting reply (have a reply-to email): ${num(countFeedbackWithReplyTo(fe.feedback))}`)
  w.line(`- Error reports: ${num(fe.errorReports.length)}`)
  writeErrorReportBreakdowns(w, fe.errorReports)
  writeRecentFeedback(w, fe.feedback)
}

// ── The report ───────────────────────────────────────────────────────────────────────────────────

/**
 * The whole report, section by section, in the order the dashboard tells its story: funnel first,
 * then the acquisition stages, then what users say back. Every section degrades to a "Couldn't load"
 * line on its own, so one dead source never blanks the rest.
 */
export function formatReport(data: DashboardData): string {
  const w = new ReportWriter()

  const selectionLabel = data.selection.range === 'day' ? data.selection.day : data.selection.range
  w.title(`Cmdr analytics report (${selectionLabel ?? data.selection.range})`)
  w.line(`Generated: ${data.updatedAt}`)

  writeFunnelSection(w, data.funnel)
  writeAwarenessSection(w, data.umami, data.githubStars)
  writeInterestSection(w, data.umami, data.posthog)
  writeDownloadSection(w, data.cloudflare, data.github)
  writeActiveUseSection(w, data.cloudflare, data.license)
  writeSettingsAdoptionSection(w, data.settingsAdoption)
  writePaymentSection(w, data.paddle)
  writeRetentionSection(w, data.paddle)
  writeFeedbackSection(w, data.feedbackAndErrors)

  return w.text()
}
