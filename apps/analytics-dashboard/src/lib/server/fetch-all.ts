import type { DashboardEnv, DashboardSelection, SourceResult } from './types.js'
import { resolveSelection } from './types.js'
import type { UmamiData } from './sources/umami.js'
import type { CloudflareData } from './sources/cloudflare.js'
import type { PaddleData } from './sources/paddle.js'
import type { GitHubData, GitHubStarsData } from './sources/github.js'
import type { PostHogData } from './sources/posthog.js'
import type { LicenseData } from './sources/license.js'
import type { FeedbackAndErrorsData } from './sources/feedback-and-errors.js'
import type { FunnelData } from './sources/funnel.js'
import type { FunnelCountriesData } from './sources/funnel-countries.js'
import type { SettingsAdoption } from './settings-defaults.js'
import { fetchUmamiData } from './sources/umami.js'
import { fetchCloudflareData } from './sources/cloudflare.js'
import { fetchPaddleData } from './sources/paddle.js'
import { fetchGitHubData, fetchGitHubStarsData } from './sources/github.js'
import { fetchPostHogData } from './sources/posthog.js'
import { fetchLicenseData } from './sources/license.js'
import { fetchFeedbackAndErrorsData } from './sources/feedback-and-errors.js'
import { fetchFunnelData } from './sources/funnel.js'
import { fetchFunnelCountriesData } from './sources/funnel-countries.js'
import { fetchSettingsAdoptionData } from './sources/settings-adoption.js'

/** The shared time selection + freshness stamp every page load carries (resolved in `+layout.server.ts`). */
export interface SelectionEnvelope {
  /** The resolved selection driving the aggregate sections (the range). */
  selection: DashboardSelection
  updatedAt: string
}

/** The Acquisition page's sources: the funnel, awareness, interest, and download sections. */
/**
 * Deep links to each site's own Umami dashboard. Plain config derived from env, not a fetched
 * source, so there's no failure mode: a site whose website ID isn't configured is `null` and the UI
 * drops its link rather than pointing at a broken page.
 */
export interface UmamiSiteUrls {
  /** getcmdr.com */
  website: string | null
  /** veszelovszki.com */
  personalSite: string | null
  /** getprvw.com */
  prvw: string | null
}

export interface AcquisitionData extends SelectionEnvelope {
  /** The `?country=` filter the trend charts are scoped to, or `null` for all countries. */
  country: string | null
  /** The last 180 UTC days, independent of `selection`, scoped to `country`: the trend charts and old table. */
  funnel: SourceResult<FunnelData>
  /** The per-country 30-day table and the country chips. Never scoped to `country`. */
  funnelCountries: SourceResult<FunnelCountriesData>
  umami: SourceResult<UmamiData>
  umamiSiteUrls: UmamiSiteUrls
  cloudflare: SourceResult<CloudflareData>
  github: SourceResult<GitHubData>
  githubStars: SourceResult<GitHubStarsData>
  posthog: SourceResult<PostHogData>
}

/** The Product page's sources: active use, settings adoption, payment, retention, and feedback & errors. */
export interface ProductData extends SelectionEnvelope {
  cloudflare: SourceResult<CloudflareData>
  paddle: SourceResult<PaddleData>
  license: SourceResult<LicenseData>
  feedbackAndErrors: SourceResult<FeedbackAndErrorsData>
  settingsAdoption: SourceResult<SettingsAdoption>
}

/** Every source, used by the agent-readable report endpoint (which dumps all sections at once). */
export interface DashboardData extends SelectionEnvelope {
  /** The last 180 UTC days, independent of `selection`, all countries (the report prints the last 30). */
  funnel: SourceResult<FunnelData>
  umami: SourceResult<UmamiData>
  cloudflare: SourceResult<CloudflareData>
  paddle: SourceResult<PaddleData>
  github: SourceResult<GitHubData>
  githubStars: SourceResult<GitHubStarsData>
  posthog: SourceResult<PostHogData>
  license: SourceResult<LicenseData>
  feedbackAndErrors: SourceResult<FeedbackAndErrorsData>
  settingsAdoption: SourceResult<SettingsAdoption>
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
      resolve({ ok: false, error: `${name}: timed out after ${String(sourceTimeoutMs / 1000)}s` })
    }, sourceTimeoutMs)
    promise
      .then((result) => {
        resolve(result)
      })
      .catch((e: unknown) => {
        resolve({ ok: false, error: `${name}: ${e instanceof Error ? e.message : String(e)}` })
      })
      .finally(() => {
        clearTimeout(timer)
      })
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

/**
 * Builds each site's Umami dashboard link from the website IDs already in env, so the IDs live in
 * exactly one place. The IDs aren't secret (each is embedded in its site's public tracking script),
 * but they're env-resolved, which is why this runs server-side and rides down with the page data.
 */
function umamiSiteUrls(env: DashboardEnv): UmamiSiteUrls {
  const base = env.UMAMI_API_URL.replace(/\/+$/, '')
  const link = (websiteId: string | undefined): string | null =>
    base && websiteId ? `${base}/websites/${websiteId}` : null
  return {
    website: link(env.UMAMI_WEBSITE_ID),
    personalSite: link(env.UMAMI_BLOG_WEBSITE_ID),
    prvw: link(env.UMAMI_PRVW_WEBSITE_ID),
  }
}

// --- Per-source loaders -----------------------------------------------------------------------
//
// Each wraps one source with its env-var guard and timeout, so pages compose only the subset they
// render. The worker-backed sources (funnel, cloudflare, feedback) share one admin token; the funnel
// also reaches Umami and Paddle but gates on the worker token (its columns are the table's core) and
// lets those degrade to dashes inside. `fetchCloudflareSource` is shared by both pages: it's cached
// per selection (`cache.ts`), so the Acquisition and Product loads hit one cache entry, not two fetches.

/** The per-day funnel source: the last 180 UTC days, independent of the selection, optionally per country. */
export function fetchFunnelSource(env: DashboardEnv, country: string | null = null): Promise<SourceResult<FunnelData>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'Funnel', () =>
    fetchFunnelData(
      {
        LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN,
        WORKER_BASE_URL: env.WORKER_BASE_URL,
        UMAMI_API_URL: env.UMAMI_API_URL,
        UMAMI_USERNAME: env.UMAMI_USERNAME,
        UMAMI_PASSWORD: env.UMAMI_PASSWORD,
        UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID,
        PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE,
      },
      country,
    ),
  )
}

/** The per-country 30-day funnel (table + chips). Gated on the worker token like the funnel itself. */
export function fetchFunnelCountriesSource(env: DashboardEnv): Promise<SourceResult<FunnelCountriesData>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'Countries', () =>
    fetchFunnelCountriesData({
      LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN,
      WORKER_BASE_URL: env.WORKER_BASE_URL,
      UMAMI_API_URL: env.UMAMI_API_URL,
      UMAMI_USERNAME: env.UMAMI_USERNAME,
      UMAMI_PASSWORD: env.UMAMI_PASSWORD,
      UMAMI_WEBSITE_ID: env.UMAMI_WEBSITE_ID,
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

export function fetchSettingsAdoptionSource(
  env: DashboardEnv,
  selection: DashboardSelection,
): Promise<SourceResult<SettingsAdoption>> {
  return guardedFetch(env.LICENSE_SERVER_ADMIN_TOKEN, 'Settings adoption', () =>
    fetchSettingsAdoptionData(
      { LICENSE_SERVER_ADMIN_TOKEN: env.LICENSE_SERVER_ADMIN_TOKEN, WORKER_BASE_URL: env.WORKER_BASE_URL },
      selection,
    ),
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
  env: DashboardEnv,
  selection: DashboardSelection,
  country: string | null = null,
): Promise<AcquisitionData> {
  const [funnel, funnelCountries, umami, cloudflare, github, githubStars, posthog] = await Promise.all([
    fetchFunnelSource(env, country),
    fetchFunnelCountriesSource(env),
    fetchUmamiSource(env, selection),
    fetchCloudflareSource(env, selection),
    fetchGitHubSource(env),
    fetchGitHubStarsSource(env),
    fetchPostHogSource(env, selection),
  ])
  return {
    selection,
    updatedAt: new Date().toISOString(),
    country,
    funnel,
    funnelCountries,
    umami,
    umamiSiteUrls: umamiSiteUrls(env),
    cloudflare,
    github,
    githubStars,
    posthog,
  }
}

/** Fetches only the sources the Product page (`/product`) renders, in parallel. */
export async function fetchProductData(env: DashboardEnv, selection: DashboardSelection): Promise<ProductData> {
  const [cloudflare, paddle, license, feedbackAndErrors, settingsAdoption] = await Promise.all([
    fetchCloudflareSource(env, selection),
    fetchPaddleSource(env, selection),
    fetchLicenseSource(env),
    fetchFeedbackAndErrorsSource(env, selection),
    fetchSettingsAdoptionSource(env, selection),
  ])
  return {
    selection,
    updatedAt: new Date().toISOString(),
    cloudflare,
    paddle,
    license,
    feedbackAndErrors,
    settingsAdoption,
  }
}

/** Fetches all dashboard data sources in parallel. Used by the agent-readable report API. */
export async function fetchDashboardData(env: DashboardEnv, rangeParam: string | null): Promise<DashboardData> {
  const selection = resolveSelection(rangeParam)

  const [
    funnel,
    umami,
    cloudflare,
    paddle,
    github,
    githubStars,
    posthog,
    license,
    feedbackAndErrors,
    settingsAdoption,
  ] = await Promise.all([
    fetchFunnelSource(env),
    fetchUmamiSource(env, selection),
    fetchCloudflareSource(env, selection),
    fetchPaddleSource(env, selection),
    fetchGitHubSource(env),
    fetchGitHubStarsSource(env),
    fetchPostHogSource(env, selection),
    fetchLicenseSource(env),
    fetchFeedbackAndErrorsSource(env, selection),
    fetchSettingsAdoptionSource(env, selection),
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
    settingsAdoption,
  }
}
