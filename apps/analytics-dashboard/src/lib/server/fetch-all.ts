import type { TimeRange, SourceResult } from './types.js'
import type { UmamiData } from './sources/umami.js'
import type { CloudflareData } from './sources/cloudflare.js'
import type { PaddleData } from './sources/paddle.js'
import type { GitHubData } from './sources/github.js'
import type { PostHogData } from './sources/posthog.js'
import type { LicenseData } from './sources/license.js'
import { fetchUmamiData } from './sources/umami.js'
import { fetchCloudflareData } from './sources/cloudflare.js'
import { fetchPaddleData } from './sources/paddle.js'
import { fetchGitHubData } from './sources/github.js'
import { fetchPostHogData } from './sources/posthog.js'
import { fetchLicenseData } from './sources/license.js'

export interface DashboardData {
    range: TimeRange
    updatedAt: string
    umami: SourceResult<UmamiData>
    cloudflare: SourceResult<CloudflareData>
    paddle: SourceResult<PaddleData>
    github: SourceResult<GitHubData>
    posthog: SourceResult<PostHogData>
    license: SourceResult<LicenseData>
}

function missingEnv(name: string): SourceResult<never> {
    return { ok: false, error: `${name}: not configured (missing env vars)` }
}

/** Returns the env object from CF Pages platform, falling back to $env/dynamic/private for local dev. */
async function resolveEnv(platform: App.Platform | undefined): Promise<App.Platform['env']> {
    if (platform?.env) return platform.env
    const { env } = await import('$env/dynamic/private')
    return {
        UMAMI_API_URL: env.UMAMI_API_URL ?? '',
        UMAMI_USERNAME: env.UMAMI_USERNAME ?? '',
        UMAMI_PASSWORD: env.UMAMI_PASSWORD ?? '',
        UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID ?? '',
        UMAMI_BLOG_WEBSITE_ID: env.UMAMI_BLOG_WEBSITE_ID ?? '',
        CLOUDFLARE_API_TOKEN: env.CLOUDFLARE_API_TOKEN ?? '',
        CLOUDFLARE_ACCOUNT_ID: env.CLOUDFLARE_ACCOUNT_ID ?? '',
        PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE ?? '',
        POSTHOG_API_KEY: env.POSTHOG_API_KEY ?? '',
        POSTHOG_PROJECT_ID: env.POSTHOG_PROJECT_ID ?? '',
        POSTHOG_API_URL: env.POSTHOG_API_URL ?? '',
        GITHUB_TOKEN: env.GITHUB_TOKEN || undefined,
        LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN ?? '',
    }
}

const validRanges = new Set<TimeRange>(['24h', '7d', '30d'])

/** Fetches all dashboard data sources in parallel. Used by both the page and the report API. */
export async function fetchDashboardData(
    platform: App.Platform | undefined,
    rangeParam: string
): Promise<DashboardData> {
    const range: TimeRange = validRanges.has(rangeParam as TimeRange) ? (rangeParam as TimeRange) : '7d'
    const env = await resolveEnv(platform)

    const [umami, cloudflare, paddle, github, posthog, license] = await Promise.all([
        env?.UMAMI_API_URL
            ? fetchUmamiData(
                  {
                      UMAMI_API_URL: env.UMAMI_API_URL,
                      UMAMI_USERNAME: env.UMAMI_USERNAME,
                      UMAMI_PASSWORD: env.UMAMI_PASSWORD,
                      UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID,
                      UMAMI_BLOG_WEBSITE_ID: env.UMAMI_BLOG_WEBSITE_ID,
                  },
                  range
              )
            : Promise.resolve(missingEnv('Umami')),
        env?.CLOUDFLARE_API_TOKEN
            ? fetchCloudflareData(
                  { CLOUDFLARE_API_TOKEN: env.CLOUDFLARE_API_TOKEN, CLOUDFLARE_ACCOUNT_ID: env.CLOUDFLARE_ACCOUNT_ID },
                  range
              )
            : Promise.resolve(missingEnv('Cloudflare')),
        env?.PADDLE_API_KEY_LIVE
            ? fetchPaddleData({ PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE }, range)
            : Promise.resolve(missingEnv('Paddle')),
        fetchGitHubData({ GITHUB_TOKEN: env?.GITHUB_TOKEN }),
        env?.POSTHOG_API_KEY
            ? fetchPostHogData(
                  {
                      POSTHOG_API_KEY: env.POSTHOG_API_KEY,
                      POSTHOG_PROJECT_ID: env.POSTHOG_PROJECT_ID,
                      POSTHOG_API_URL: env.POSTHOG_API_URL,
                  },
                  range
              )
            : Promise.resolve(missingEnv('PostHog')),
        env?.LICENSE_SERVER_ADMIN_TOKEN
            ? fetchLicenseData({ LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN })
            : Promise.resolve(missingEnv('License server')),
    ])

    return { range, updatedAt: new Date().toISOString(), umami, cloudflare, paddle, github, posthog, license }
}
