import type { DashboardSelection, SourceResult } from './types.js'
import { resolveSelection } from './types.js'
import type { UmamiData } from './sources/umami.js'
import type { CloudflareData } from './sources/cloudflare.js'
import type { PaddleData } from './sources/paddle.js'
import type { GitHubData, GitHubStarsData } from './sources/github.js'
import type { PostHogData } from './sources/posthog.js'
import type { LicenseData } from './sources/license.js'
import type { FeedbackAndErrorsData } from './sources/feedback-and-errors.js'
import type { FunnelData } from './sources/funnel.js'
import { fetchUmamiData } from './sources/umami.js'
import { fetchCloudflareData } from './sources/cloudflare.js'
import { fetchPaddleData } from './sources/paddle.js'
import { fetchGitHubData, fetchGitHubStarsData } from './sources/github.js'
import { fetchPostHogData } from './sources/posthog.js'
import { fetchLicenseData } from './sources/license.js'
import { fetchFeedbackAndErrorsData } from './sources/feedback-and-errors.js'
import { fetchFunnelData } from './sources/funnel.js'

export interface DashboardData {
  /** The resolved selection driving the aggregate sections (range + optional specific day). */
  selection: DashboardSelection
  updatedAt: string
  /** Always the last 30 UTC days, independent of `selection` (the funnel section's own window). */
  funnel: SourceResult<FunnelData>
  umami: SourceResult<UmamiData>
  cloudflare: SourceResult<CloudflareData>
  paddle: SourceResult<PaddleData>
  github: SourceResult<GitHubData>
  githubStars: SourceResult<GitHubStarsData>
  posthog: SourceResult<PostHogData>
  license: SourceResult<LicenseData>
  feedbackAndErrors: SourceResult<FeedbackAndErrorsData>
}

const sourceTimeoutMs = 20_000

/**
 * Resolves to an error result if the source doesn't settle within 20s. Workers `fetch` has no
 * built-in timeout, and the sources run under `Promise.all`, so without this cap a single hung
 * upstream stalls the whole response until Cloudflare's proxy cuts it at 100s with a 524. With it,
 * the hung source degrades to its "Couldn't load" section and everything else still renders.
 */
export function withTimeout<T>(name: string, promise: Promise<SourceResult<T>>): Promise<SourceResult<T>> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      resolve({ ok: false, error: `${name}: timed out after ${sourceTimeoutMs / 1000}s` })
    }, sourceTimeoutMs)
    promise
      .then((result) => resolve(result))
      .catch((e) => resolve({ ok: false, error: `${name}: ${e instanceof Error ? e.message : String(e)}` }))
      .finally(() => clearTimeout(timer))
  })
}

/** Runs `fn` (capped at the source timeout) if `envKey` is set, otherwise returns a "not configured" error. */
function guardedFetch<T>(
  envKey: string | undefined,
  name: string,
  fn: () => Promise<SourceResult<T>>,
): Promise<SourceResult<T>> {
  return envKey
    ? withTimeout(name, fn())
    : Promise.resolve({ ok: false, error: `${name}: not configured (missing env vars)` })
}

/** Returns the env object from CF Pages platform, falling back to $env/dynamic/private for local dev. */
async function resolveEnv(platform: App.Platform | undefined): Promise<NonNullable<App.Platform['env']>> {
  if (platform?.env) return platform.env
  const { env } = await import('$env/dynamic/private')
  return {
    UMAMI_API_URL: env.UMAMI_API_URL ?? '',
    UMAMI_USERNAME: env.UMAMI_USERNAME ?? '',
    UMAMI_PASSWORD: env.UMAMI_PASSWORD ?? '',
    UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID ?? '',
    UMAMI_BLOG_WEBSITE_ID: env.UMAMI_BLOG_WEBSITE_ID ?? '',
    UMAMI_PRVW_WEBSITE_ID: env.UMAMI_PRVW_WEBSITE_ID ?? '',
    PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE ?? '',
    POSTHOG_API_KEY: env.POSTHOG_API_KEY ?? '',
    POSTHOG_PROJECT_ID: env.POSTHOG_PROJECT_ID ?? '',
    POSTHOG_API_URL: env.POSTHOG_API_URL ?? '',
    GITHUB_TOKEN: env.GITHUB_TOKEN || undefined,
    LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN ?? '',
    WORKER_BASE_URL: env.WORKER_BASE_URL || undefined,
  }
}

/** Fetches all dashboard data sources in parallel. Used by both the page and the report API. */
export async function fetchDashboardData(
  platform: App.Platform | undefined,
  rangeParam: string | null,
  dayParam: string | null = null,
): Promise<DashboardData> {
  const selection = resolveSelection(rangeParam, dayParam)
  const env = await resolveEnv(platform)

  const [funnel, umami, cloudflare, paddle, github, githubStars, posthog, license, feedbackAndErrors] =
    await Promise.all([
      // The funnel needs the worker token plus Umami and Paddle; the worker token is the floor (its
      // columns are the core of the table), so gate on it and let Umami/Paddle degrade to dashes inside.
      guardedFetch(env?.LICENSE_SERVER_ADMIN_TOKEN, 'Funnel', () =>
        fetchFunnelData({
          LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN,
          WORKER_BASE_URL: env.WORKER_BASE_URL,
          UMAMI_API_URL: env.UMAMI_API_URL,
          UMAMI_USERNAME: env.UMAMI_USERNAME,
          UMAMI_PASSWORD: env.UMAMI_PASSWORD,
          UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID,
          PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE,
        }),
      ),
      guardedFetch(env?.UMAMI_API_URL, 'Umami', () =>
        fetchUmamiData(
          {
            UMAMI_API_URL: env.UMAMI_API_URL,
            UMAMI_USERNAME: env.UMAMI_USERNAME,
            UMAMI_PASSWORD: env.UMAMI_PASSWORD,
            UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID,
            UMAMI_BLOG_WEBSITE_ID: env.UMAMI_BLOG_WEBSITE_ID,
            UMAMI_PRVW_WEBSITE_ID: env.UMAMI_PRVW_WEBSITE_ID,
          },
          selection,
        ),
      ),
      guardedFetch(env?.LICENSE_SERVER_ADMIN_TOKEN, 'Cloudflare', () =>
        fetchCloudflareData(
          { LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN, WORKER_BASE_URL: env.WORKER_BASE_URL },
          selection,
        ),
      ),
      guardedFetch(env?.PADDLE_API_KEY_LIVE, 'Paddle', () =>
        fetchPaddleData({ PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE }, selection),
      ),
      withTimeout('GitHub', fetchGitHubData({ GITHUB_TOKEN: env?.GITHUB_TOKEN })),
      withTimeout('GitHub stars', fetchGitHubStarsData({ GITHUB_TOKEN: env?.GITHUB_TOKEN })),
      guardedFetch(env?.POSTHOG_API_KEY, 'PostHog', () =>
        fetchPostHogData(
          {
            POSTHOG_API_KEY: env.POSTHOG_API_KEY,
            POSTHOG_PROJECT_ID: env.POSTHOG_PROJECT_ID,
            POSTHOG_API_URL: env.POSTHOG_API_URL,
          },
          selection,
        ),
      ),
      guardedFetch(env?.LICENSE_SERVER_ADMIN_TOKEN, 'License server', () =>
        fetchLicenseData({ LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN }),
      ),
      guardedFetch(env?.LICENSE_SERVER_ADMIN_TOKEN, 'Feedback & errors', () =>
        fetchFeedbackAndErrorsData(
          { LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN, WORKER_BASE_URL: env.WORKER_BASE_URL },
          selection,
        ),
      ),
    ])

  return {
    selection,
    updatedAt: new Date().toISOString(),
    funnel,
    umami,
    cloudflare,
    paddle,
    github,
    githubStars,
    posthog,
    license,
    feedbackAndErrors,
  }
}
