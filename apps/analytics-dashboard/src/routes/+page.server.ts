import type { PageServerLoad } from './$types'
import type { TimeRange, SourceResult } from '$lib/server/types.js'
import type { UmamiData } from '$lib/server/sources/umami.js'
import type { CloudflareData } from '$lib/server/sources/cloudflare.js'
import type { PaddleData } from '$lib/server/sources/paddle.js'
import type { GitHubData } from '$lib/server/sources/github.js'
import type { PostHogData } from '$lib/server/sources/posthog.js'
import type { LicenseData } from '$lib/server/sources/license.js'
import { fetchUmamiData } from '$lib/server/sources/umami.js'
import { fetchCloudflareData } from '$lib/server/sources/cloudflare.js'
import { fetchPaddleData } from '$lib/server/sources/paddle.js'
import { fetchGitHubData } from '$lib/server/sources/github.js'
import { fetchPostHogData } from '$lib/server/sources/posthog.js'
import { fetchLicenseData } from '$lib/server/sources/license.js'

const validRanges = new Set<TimeRange>(['24h', '7d', '30d'])

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
    // In local dev (vite dev), platform.env is undefined. Use SvelteKit's $env/dynamic/private
    // which properly loads .env files (handling quoting, escaping, etc.).
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

export const load: PageServerLoad = async ({ url, platform }) => {
    const rangeParam = url.searchParams.get('range') ?? '7d'
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

    return {
        range,
        updatedAt: new Date().toISOString(),
        umami,
        cloudflare,
        paddle,
        github,
        posthog,
        license,
    } satisfies DashboardData
}
