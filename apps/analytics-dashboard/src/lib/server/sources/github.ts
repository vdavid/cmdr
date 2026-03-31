import type { SourceResult } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

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
