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

/** The shared time selection + freshness stamp every page load carries (resolved in `+layout.server.ts`). */
export interface SelectionEnvelope {
  /** The resolved selection driving the aggregate sections (range + optional specific day). */
  selection: DashboardSelection
  updatedAt: string
}

/** The Acquisition page's sources: the funnel, awareness, interest, and download sections. */
export interface AcquisitionData extends SelectionEnvelope {
  /** Always the last 30 UTC days, independent of `selection` (the funnel section's own window). */
  funnel: SourceResult<FunnelData>
  umami: SourceResult<UmamiData>
  cloudflare: SourceResult<CloudflareData>
  github: SourceResult<GitHubData>
  githubStars: SourceResult<GitHubStarsData>
  posthog: SourceResult<PostHogData>
}

/** The Product page's sources: active use, payment, retention, and feedback & errors. */
export interface ProductData extends SelectionEnvelope {
  cloudflare: SourceResult<CloudflareData>
  paddle: SourceResult<PaddleData>
  license: SourceResult<LicenseData>
  feedbackAndErrors: SourceResult<FeedbackAndErrorsData>
}

/** Every source, used by the agent-readable report endpoint (which dumps all sections at once). */
export interface DashboardData extends SelectionEnvelope {
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

/** The resolved env object, narrowed to what every source reads. */
type DashboardEnv = NonNullable<App.Platform['env']>

/** Returns the env object from CF Pages platform, falling back to $env/dynamic/private for local dev. */
export async function resolveEnv(platform: App.Platform | undefined): Promise<DashboardEnv> {
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

// --- Per-source loaders -----------------------------------------------------------------------
//
// Each wraps one source with its env-var guard and timeout, so pages compose only the subset they
// render. The worker-backed sources (funnel, cloudflare, feedback) share one admin token; the funnel
// also reaches Umami and Paddle but gates on the worker token (its columns are the table's core) and
// lets those degrade to dashes inside. `fetchCloudflareSource` is shared by both pages: it's cached
// per selection (`cache.ts`), so the Acquisition and Product loads hit one cache entry, not two fetches.

/** The funnel table source: always the last 30 UTC days, independent of the selection. */
export function fetchFunnelSource(env: DashboardEnv): Promise<SourceResult<FunnelData>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'Funnel', () =>
    fetchFunnelData({
      LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN,
      WORKER_BASE_URL: env.WORKER_BASE_URL,
      UMAMI_API_URL: env.UMAMI_API_URL,
      UMAMI_USERNAME: env.UMAMI_USERNAME,
      UMAMI_PASSWORD: env.UMAMI_PASSWORD,
      UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID,
      PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE,
    }),
  )
}

export function fetchUmamiSource(env: DashboardEnv, selection: DashboardSelection): Promise<SourceResult<UmamiData>> {
  return guardedFetch(env.UMAMI_API_URL, 'Umami', () =>
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
  )
}

export function fetchCloudflareSource(
  env: DashboardEnv,
  selection: DashboardSelection,
): Promise<SourceResult<CloudflareData>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'Cloudflare', () =>
    fetchCloudflareData(
      { LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN, WORKER_BASE_URL: env.WORKER_BASE_URL },
      selection,
    ),
  )
}

export function fetchPaddleSource(env: DashboardEnv, selection: DashboardSelection): Promise<SourceResult<PaddleData>> {
  return guardedFetch(env.PADDLE_API_KEY_LIVE, 'Paddle', () =>
    fetchPaddleData({ PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE }, selection),
  )
}

export function fetchGitHubSource(env: DashboardEnv): Promise<SourceResult<GitHubData>> {
  return withTimeout('GitHub', fetchGitHubData({ GITHUB_TOKEN: env.GITHUB_TOKEN }))
}

export function fetchGitHubStarsSource(env: DashboardEnv): Promise<SourceResult<GitHubStarsData>> {
  return withTimeout('GitHub stars', fetchGitHubStarsData({ GITHUB_TOKEN: env.GITHUB_TOKEN }))
}

export function fetchPostHogSource(
  env: DashboardEnv,
  selection: DashboardSelection,
): Promise<SourceResult<PostHogData>> {
  return guardedFetch(env.POSTHOG_API_KEY, 'PostHog', () =>
    fetchPostHogData(
      {
        POSTHOG_API_KEY: env.POSTHOG_API_KEY,
        POSTHOG_PROJECT_ID: env.POSTHOG_PROJECT_ID,
        POSTHOG_API_URL: env.POSTHOG_API_URL,
      },
      selection,
    ),
  )
}

export function fetchLicenseSource(env: DashboardEnv): Promise<SourceResult<LicenseData>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'License server', () =>
    fetchLicenseData({ LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN }),
  )
}

export function fetchFeedbackAndErrorsSource(
  env: DashboardEnv,
  selection: DashboardSelection,
): Promise<SourceResult<FeedbackAndErrorsData>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'Feedback & errors', () =>
    fetchFeedbackAndErrorsData(
      { LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN, WORKER_BASE_URL: env.WORKER_BASE_URL },
      selection,
    ),
  )
}

// --- Per-page composers -----------------------------------------------------------------------

/** Fetches only the sources the Acquisition page (`/`) renders, in parallel. */
export async function fetchAcquisitionData(
  platform: App.Platform | undefined,
  selection: DashboardSelection,
): Promise<AcquisitionData> {
  const env = await resolveEnv(platform)
  const [funnel, umami, cloudflare, github, githubStars, posthog] = await Promise.all([
    fetchFunnelSource(env),
    fetchUmamiSource(env, selection),
    fetchCloudflareSource(env, selection),
    fetchGitHubSource(env),
    fetchGitHubStarsSource(env),
    fetchPostHogSource(env, selection),
  ])
  return { selection, updatedAt: new Date().toISOString(), funnel, umami, cloudflare, github, githubStars, posthog }
}

/** Fetches only the sources the Product page (`/product`) renders, in parallel. */
export async function fetchProductData(
  platform: App.Platform | undefined,
  selection: DashboardSelection,
): Promise<ProductData> {
  const env = await resolveEnv(platform)
  const [cloudflare, paddle, license, feedbackAndErrors] = await Promise.all([
    fetchCloudflareSource(env, selection),
    fetchPaddleSource(env, selection),
    fetchLicenseSource(env),
    fetchFeedbackAndErrorsSource(env, selection),
  ])
  return { selection, updatedAt: new Date().toISOString(), cloudflare, paddle, license, feedbackAndErrors }
}

/** Fetches all dashboard data sources in parallel. Used by the agent-readable report API. */
export async function fetchDashboardData(
  platform: App.Platform | undefined,
  rangeParam: string | null,
  dayParam: string | null = null,
): Promise<DashboardData> {
  const selection = resolveSelection(rangeParam, dayParam)
  const env = await resolveEnv(platform)

  const [funnel, umami, cloudflare, paddle, github, githubStars, posthog, license, feedbackAndErrors] =
    await Promise.all([
      fetchFunnelSource(env),
      fetchUmamiSource(env, selection),
      fetchCloudflareSource(env, selection),
      fetchPaddleSource(env, selection),
      fetchGitHubSource(env),
      fetchGitHubStarsSource(env),
      fetchPostHogSource(env, selection),
      fetchLicenseSource(env),
      fetchFeedbackAndErrorsSource(env, selection),
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
