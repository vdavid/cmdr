import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchAcquisitionData, fetchProductData, fetchDashboardData } from './fetch-all.js'
import { clearMemoryCache } from './cache.js'

// Each page must load ONLY the sources it renders. These tests pin that contract: the Acquisition page
// returns the funnel/awareness/interest/download sources, the Product page the active-use/payment/
// retention/feedback sources, and neither leaks the other's keys. We pass a platform with empty env so
// the guarded sources short-circuit to "not configured" (no network), and stub `fetch` so the two
// unguarded GitHub sources fail fast offline too. We assert the SHAPE (which sources a page loads),
// not the data.

const emptyEnv = {
  UMAMI_API_URL: '',
  UMAMI_USERNAME: '',
  UMAMI_PASSWORD: '',
  UMAMI_WEBSITE_ID: '',
  UMAMI_BLOG_WEBSITE_ID: '',
  UMAMI_PRVW_WEBSITE_ID: '',
  PADDLE_API_KEY_LIVE: '',
  POSTHOG_API_KEY: '',
  POSTHOG_PROJECT_ID: '',
  POSTHOG_API_URL: '',
  GITHUB_TOKEN: undefined,
  LICENSE_SERVER_ADMIN_TOKEN: '',
  WORKER_BASE_URL: undefined,
}

const platform = { env: emptyEnv } as unknown as App.Platform
const selection = { range: '7d', day: null } as const

/** Source keys (everything in the result except the shared `selection` / `updatedAt` envelope). */
function sourceKeys(data: object): string[] {
  return Object.keys(data)
    .filter((k) => k !== 'selection' && k !== 'updatedAt')
    .sort()
}

describe('per-page data-loading split', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    clearMemoryCache()
    // The unguarded GitHub sources still call fetch; make it fail fast so the test stays offline.
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')))
  })

  it('Acquisition loads exactly the funnel, awareness, interest, and download sources', async () => {
    const data = await fetchAcquisitionData(platform, selection)
    expect(sourceKeys(data)).toEqual(['cloudflare', 'funnel', 'github', 'githubStars', 'posthog', 'umami'])
    // It must NOT carry the Product-only sources.
    expect(data).not.toHaveProperty('paddle')
    expect(data).not.toHaveProperty('license')
    expect(data).not.toHaveProperty('feedbackAndErrors')
    expect(data.selection).toEqual(selection)
    expect(typeof data.updatedAt).toBe('string')
  })

  it('Product loads exactly the active-use, payment, retention, and feedback sources', async () => {
    const data = await fetchProductData(platform, selection)
    expect(sourceKeys(data)).toEqual(['cloudflare', 'feedbackAndErrors', 'license', 'paddle'])
    // It must NOT carry the Acquisition-only sources.
    expect(data).not.toHaveProperty('funnel')
    expect(data).not.toHaveProperty('umami')
    expect(data).not.toHaveProperty('github')
    expect(data).not.toHaveProperty('githubStars')
    expect(data).not.toHaveProperty('posthog')
    expect(data.selection).toEqual(selection)
  })

  it('the report still loads every source (the union of both pages)', async () => {
    const data = await fetchDashboardData(platform, '7d', null)
    expect(sourceKeys(data)).toEqual([
      'cloudflare',
      'feedbackAndErrors',
      'funnel',
      'github',
      'githubStars',
      'license',
      'paddle',
      'posthog',
      'umami',
    ])
  })

  it('every source on a page degrades to a result object (never throws) when unconfigured', async () => {
    const acq = await fetchAcquisitionData(platform, selection)
    for (const key of sourceKeys(acq)) {
      const result = acq[key as keyof typeof acq] as { ok: boolean }
      expect(result).toHaveProperty('ok')
      // Empty env / offline fetch means none of them can succeed here.
      expect(result.ok).toBe(false)
    }
  })
})
