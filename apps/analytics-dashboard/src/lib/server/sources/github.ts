import type { SourceResult } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

// ── Star history ──────────────────────────────────────────────────────

export interface DailyStarCount {
  day: string
  newStars: number
  cumulative: number
}

export interface RepoStarSummary {
  repo: string
  totalStars: number
  daily: DailyStarCount[]
}

export interface GitHubStarsData {
  repos: RepoStarSummary[]
  totalStars: number
  /** Merged daily counts across all repos. */
  combinedDaily: DailyStarCount[]
}

const TRACKED_REPOS = ['vdavid/cmdr', 'vdavid/mtp-rs']

interface RawStargazer {
  starred_at: string
}

/** Fetches all stargazers for a repo, paginating via Link header. */
async function fetchAllStargazers(repo: string, headers: Record<string, string>): Promise<string[]> {
  const dates: string[] = []
  let url: string | null = `https://api.github.com/repos/${repo}/stargazers?per_page=100`

  while (url) {
    const response = await fetch(url, {
      headers: { ...headers, Accept: 'application/vnd.github.star+json' },
    })
    if (!response.ok) throw new Error(`GitHub stargazers API returned ${response.status} for ${repo}`)

    const items = (await response.json()) as RawStargazer[]
    for (const item of items) dates.push(item.starred_at)

    // Follow pagination
    const linkHeader = response.headers.get('Link')
    const nextMatch = linkHeader?.match(/<([^>]+)>;\s*rel="next"/)
    url = nextMatch?.[1] ?? null
  }

  return dates
}

/** Groups star dates into daily counts with cumulative totals. */
function toDailyCounts(dates: string[]): DailyStarCount[] {
  const byDay = new Map<string, number>()
  for (const date of dates) {
    const day = date.split('T')[0]
    byDay.set(day, (byDay.get(day) ?? 0) + 1)
  }

  const sorted = [...byDay.entries()].sort(([a], [b]) => a.localeCompare(b))
  let cumulative = 0
  return sorted.map(([day, newStars]) => {
    cumulative += newStars
    return { day, newStars, cumulative }
  })
}

/** Merges daily counts from multiple repos into a combined timeline. */
function mergeDailyCounts(repoSummaries: RepoStarSummary[]): DailyStarCount[] {
  const byDay = new Map<string, number>()
  for (const repo of repoSummaries) {
    for (const entry of repo.daily) {
      byDay.set(entry.day, (byDay.get(entry.day) ?? 0) + entry.newStars)
    }
  }

  const sorted = [...byDay.entries()].sort(([a], [b]) => a.localeCompare(b))
  let cumulative = 0
  return sorted.map(([day, newStars]) => {
    cumulative += newStars
    return { day, newStars, cumulative }
  })
}

/**
 * Fetches star history for tracked repos. Cached aggressively (1 hour)
 * since star events are append-only.
 */
export async function fetchGitHubStarsData(env: GitHubEnv): Promise<SourceResult<GitHubStarsData>> {
  const cached = await cacheGet<GitHubStarsData>('github-stars', '30d')
  if (cached) return { ok: true, data: cached }

  try {
    const headers: Record<string, string> = { 'User-Agent': 'cmdr-analytics-dashboard' }
    if (env.GITHUB_TOKEN) headers.Authorization = `Bearer ${env.GITHUB_TOKEN}`

    const allDates = await Promise.all(TRACKED_REPOS.map((repo) => fetchAllStargazers(repo, headers)))

    const repos: RepoStarSummary[] = TRACKED_REPOS.map((repo, i) => ({
      repo,
      totalStars: allDates[i].length,
      daily: toDailyCounts(allDates[i]),
    }))

    const totalStars = repos.reduce((sum, r) => sum + r.totalStars, 0)
    const combinedDaily = mergeDailyCounts(repos)

    const data: GitHubStarsData = { repos, totalStars, combinedDaily }
    await cacheSet('github-stars', '30d', data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `GitHub stars: ${e instanceof Error ? e.message : String(e)}` }
  }
}

export interface GitHubAsset {
  name: string
  downloadCount: number
}

export interface GitHubRelease {
  tagName: string
  publishedAt: string
  assets: GitHubAsset[]
  totalDownloads: number
}

export interface GitHubData {
  releases: GitHubRelease[]
  totalDownloads: number
}

interface GitHubEnv {
  GITHUB_TOKEN?: string
}

interface GitHubRawAsset {
  name: string
  download_count: number
}

interface GitHubRawRelease {
  tag_name: string
  published_at: string
  assets: GitHubRawAsset[]
}

export function parseRelease(raw: GitHubRawRelease): GitHubRelease {
  const assets = raw.assets.map((a) => ({ name: a.name, downloadCount: a.download_count }))
  const totalDownloads = assets.reduce((sum, a) => sum + a.downloadCount, 0)
  return {
    tagName: raw.tag_name,
    publishedAt: raw.published_at,
    assets,
    totalDownloads,
  }
}

export function parseGitHubReleases(raw: GitHubRawRelease[]): GitHubData {
  const releases = raw.map(parseRelease)
  const totalDownloads = releases.reduce((sum, r) => sum + r.totalDownloads, 0)
  return { releases, totalDownloads }
}

/**
 * Fetches GitHub release download counts. Not time-range-dependent
 * (GitHub doesn't provide per-period download stats — counts are cumulative).
 * Cached under '30d' range since data changes slowly.
 */
export async function fetchGitHubData(env: GitHubEnv): Promise<SourceResult<GitHubData>> {
  const cached = await cacheGet<GitHubData>('github', '30d')
  if (cached) return { ok: true, data: cached }

  try {
    const headers: Record<string, string> = {
      Accept: 'application/vnd.github+json',
      'User-Agent': 'cmdr-analytics-dashboard',
    }
    if (env.GITHUB_TOKEN) {
      headers.Authorization = `Bearer ${env.GITHUB_TOKEN}`
    }

    const response = await fetch('https://api.github.com/repos/vdavid/cmdr/releases', { headers })
    if (!response.ok) {
      throw new Error(`GitHub API returned ${response.status}`)
    }

    const raw = (await response.json()) as GitHubRawRelease[]
    const data = parseGitHubReleases(raw)
    await cacheSet('github', '30d', data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `GitHub: ${e instanceof Error ? e.message : String(e)}` }
  }
}
