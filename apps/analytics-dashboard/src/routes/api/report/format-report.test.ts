import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest'
import type { DashboardData } from '../../../lib/server/fetch-all.js'
import { formatReport } from './format-report.js'

// A characterization test: the report is consumed by agents and by David, so the exact text is the
// contract. Each case pins the whole output string, not a sample of it, and the fixtures are built to
// hit the branches that matter: a section with data, a section whose source is down, a source that
// loaded but is empty, and the sections that read two sources and lose only one. The funnel fixture
// mixes `null` (unknown, renders as `-`) with a real `0` on the same day, because those mean different
// things everywhere in the dashboard.

const site = (pv: number, prevPv: number, visitors: number, prevVisitors: number) => ({
  pageviews: { value: pv, prev: prevPv },
  visitors: { value: visitors, prev: prevVisitors },
  visits: { value: visitors, prev: prevVisitors },
  bounces: { value: Math.floor(visitors / 2), prev: 0 },
  totaltime: { value: 0, prev: 0 },
})

/** Every source loaded, with enough shape to reach each breakdown. */
const richData: DashboardData = {
  selection: { range: 'day', day: '2026-08-01' },
  updatedAt: '2026-08-02T03:04:05.000Z',
  funnel: {
    ok: true,
    data: {
      rows: [
        {
          date: '2026-07-30',
          visitors: 100,
          downloadClicks: 10,
          serverDownloads: 8,
          downloadsByRef: { twitter: 5, '(none)': 3 },
          downloadsByReferer: { 'news.ycombinator.com': 4, '(none)': 4 },
          downloadsByUaFamily: { human: 6, bot: 1, unknown: 1 },
          humanInstalls: 7,
          newInstalls: 4,
          d7Retention: 0.5,
          d7Retained: 2,
          newsletterSignups: 3,
          purchases: 1,
        },
        {
          // A day where the web side is unknown (null) but the server side is a real zero.
          date: '2026-07-31',
          visitors: null,
          downloadClicks: 0,
          serverDownloads: 0,
          downloadsByRef: {},
          downloadsByReferer: null,
          downloadsByUaFamily: null,
          humanInstalls: null,
          newInstalls: 0,
          d7Retention: null,
          d7Retained: null,
          newsletterSignups: null,
          purchases: 0,
        },
        {
          date: '2026-08-01',
          visitors: 250,
          downloadClicks: 25,
          serverDownloads: 20,
          downloadsByRef: { twitter: 10, reddit: 5 },
          downloadsByReferer: { 'news.ycombinator.com': 6 },
          downloadsByUaFamily: { human: 15, bot: 3, unknown: 2 },
          humanInstalls: 17,
          newInstalls: 9,
          d7Retention: 0.333,
          d7Retained: 3,
          newsletterSignups: 2,
          purchases: 2,
        },
      ],
    },
  },
  umami: {
    ok: true,
    data: {
      personalSite: site(1200, 1000, 800, 900),
      // A zero `prev` is the "no prior period" case: the delta suffix drops out entirely.
      website: site(5400, 5400, 3200, 0),
      prvw: site(120, 0, 90, 45),
      websiteReferrers: [
        { x: 'news.ycombinator.com', y: 900 },
        { x: '', y: 400 },
      ],
      websitePages: [
        { x: '/', y: 3000 },
        { x: '/pricing', y: 700 },
      ],
      websiteCountries: [
        { x: 'US', y: 1800 },
        { x: 'SE', y: 600 },
        // Not a region code: `formatCountry` has to fall back to the raw key.
        { x: '(unknown)', y: 100 },
      ],
      downloadEvents: [{ x: 'download-mac', y: 240 }],
      prvwReferrers: [{ x: 'google.com', y: 60 }],
      prvwPages: [{ x: '/', y: 120 }],
    },
  },
  cloudflare: {
    ok: true,
    data: {
      downloads: [
        {
          version: '1.2.0',
          arch: 'aarch64',
          country: 'US',
          source: 'website',
          day: '2026-07-31',
          downloads: 40,
          uniqueDownloads: 30,
        },
        {
          version: '1.2.0',
          arch: 'x86_64',
          country: 'US',
          source: 'homebrew',
          day: '2026-07-31',
          downloads: 12,
          uniqueDownloads: 9,
        },
        {
          // 1.10.0 sorts above 1.2.0 only under semver comparison, never lexically.
          version: '1.10.0',
          arch: 'aarch64',
          country: 'SE',
          source: 'website',
          day: '2026-08-01',
          downloads: 25,
          uniqueDownloads: 20,
        },
        {
          version: '1.10.0',
          arch: 'aarch64',
          country: '(unknown)',
          source: 'other',
          day: '2026-08-01',
          downloads: 3,
          uniqueDownloads: 3,
        },
      ],
      heartbeatDau: [
        { date: '2026-07-31', dau: 40, beats: 320 },
        { date: '2026-08-01', dau: 55, beats: 500 },
      ],
      updateActivity: [
        { day: '2026-07-31', version: '1.2.0', updaters: 30 },
        { day: '2026-08-01', version: '1.10.0', updaters: 25 },
        { day: '2026-08-01', version: '1.2.0', updaters: 5 },
      ],
    },
  },
  paddle: {
    ok: true,
    data: {
      transactions: [
        { id: 'txn_1', status: 'completed', createdAt: '2026-08-01T10:00:00Z', total: '3900', currencyCode: 'USD' },
        { id: 'txn_2', status: 'refunded', createdAt: '2026-07-31T09:00:00Z', total: '8900', currencyCode: 'EUR' },
      ],
      activeSubscriptions: [{ id: 'sub_1', status: 'active', customerId: 'ctm_1', currentBillingPeriod: null }],
      subscriptionsByStatus: { active: 3, canceled: 1, paused: 0 },
    },
  },
  github: {
    ok: true,
    data: {
      totalDownloads: 1234,
      releases: [
        { tagName: 'v1.10.0', publishedAt: '2026-08-01T08:00:00Z', assets: [], totalDownloads: 800 },
        { tagName: 'v1.2.0', publishedAt: '2026-07-01T08:00:00Z', assets: [], totalDownloads: 434 },
      ],
    },
  },
  githubStars: {
    ok: true,
    data: {
      totalStars: 310,
      repos: [
        {
          repo: 'vdavid/cmdr',
          // One star day inside 7d, one inside 30d only, one outside both.
          totalStars: 300,
          daily: [
            { day: '2026-01-01', newStars: 250, cumulative: 250 },
            { day: '2026-07-20', newStars: 40, cumulative: 290 },
            { day: '2026-08-01', newStars: 10, cumulative: 300 },
          ],
        },
        { repo: 'vdavid/mtp-rs', totalStars: 10, daily: [{ day: '2026-01-01', newStars: 10, cumulative: 10 }] },
      ],
      combinedDaily: [],
    },
  },
  posthog: {
    ok: true,
    data: {
      totalPageviews: 900,
      dailyPageviews: [
        { day: '2026-07-31', views: 400 },
        { day: '2026-08-01', views: 500 },
      ],
    },
  },
  license: { ok: true, data: { totalActivations: 42, activeDevices: 7 } },
  feedbackAndErrors: {
    ok: true,
    data: {
      feedback: [
        {
          id: 1,
          createdAt: '2026-08-01 12:33:44',
          feedback: 'Love   it,\n  especially the\tpreview pane.',
          email: 'someone@example.com',
          appVersion: '1.10.0',
          osVersion: '15.5',
          buildMode: 'release',
        },
        {
          id: 2,
          createdAt: '2026-07-31 08:00:00',
          // Longer than the 280-character cut-off.
          feedback: 'x'.repeat(300),
          email: null,
          appVersion: '1.2.0',
          osVersion: '14.7',
          buildMode: null,
        },
      ],
      errorReports: [
        {
          id: 'er_1',
          kind: 'auto',
          appVersion: '1.10.0',
          osVersion: '15.5',
          arch: 'aarch64',
          date: '2026-08-01',
          generatedAt: '2026-08-01T12:00:00Z',
        },
        {
          id: 'er_2',
          kind: 'auto',
          appVersion: '1.2.0',
          osVersion: '14.7',
          arch: 'x86_64',
          date: '2026-07-31',
          generatedAt: '2026-07-31T12:00:00Z',
        },
        {
          id: 'er_3',
          kind: 'user',
          appVersion: '1.10.0',
          osVersion: '15.5',
          arch: 'aarch64',
          date: '2026-08-01',
          generatedAt: '2026-08-01T13:00:00Z',
        },
      ],
    },
  },
}

/** Every source down. */
const allFailedData: DashboardData = {
  selection: { range: '7d', day: null },
  updatedAt: '2026-08-02T03:04:05.000Z',
  funnel: { ok: false, error: 'Funnel: timed out after 20s' },
  umami: { ok: false, error: 'Umami: not configured (missing env vars)' },
  cloudflare: { ok: false, error: 'Cloudflare: timed out after 20s' },
  paddle: { ok: false, error: 'Paddle: not configured (missing env vars)' },
  github: { ok: false, error: 'GitHub: offline' },
  githubStars: { ok: false, error: 'GitHub stars: offline' },
  posthog: { ok: false, error: 'PostHog: not configured (missing env vars)' },
  license: { ok: false, error: 'License server: not configured (missing env vars)' },
  feedbackAndErrors: { ok: false, error: 'Feedback & errors: timed out after 20s' },
}

/** Every source loaded, every collection empty: the "present but nothing in it" shape. */
const emptyData: DashboardData = {
  selection: { range: '30d', day: null },
  updatedAt: '2026-08-02T03:04:05.000Z',
  funnel: { ok: true, data: { rows: [] } },
  umami: {
    ok: true,
    data: {
      personalSite: site(0, 0, 0, 0),
      website: site(0, 0, 0, 0),
      prvw: site(0, 0, 0, 0),
      websiteReferrers: [],
      websitePages: [],
      websiteCountries: [],
      downloadEvents: [],
      prvwReferrers: [],
      prvwPages: [],
    },
  },
  cloudflare: { ok: true, data: { downloads: [], heartbeatDau: [], updateActivity: [] } },
  paddle: { ok: true, data: { transactions: [], activeSubscriptions: [], subscriptionsByStatus: {} } },
  github: { ok: true, data: { totalDownloads: 0, releases: [] } },
  githubStars: { ok: true, data: { totalStars: 0, repos: [], combinedDaily: [] } },
  posthog: { ok: true, data: { totalPageviews: 0, dailyPageviews: [] } },
  license: { ok: true, data: { totalActivations: 0, activeDevices: null } },
  feedbackAndErrors: { ok: true, data: { feedback: [], errorReports: [] } },
}

/** Half the sources down: the sections that read two sources have to degrade one at a time. */
const mixedData: DashboardData = {
  selection: { range: '24h', day: null },
  updatedAt: '2026-08-02T03:04:05.000Z',
  funnel: { ok: false, error: 'Funnel: offline' },
  umami: { ok: false, error: 'Umami: offline' },
  cloudflare: { ok: false, error: 'Cloudflare: offline' },
  paddle: richData.paddle,
  github: richData.github,
  githubStars: { ok: false, error: 'GitHub stars: offline' },
  posthog: richData.posthog,
  license: { ok: false, error: 'License server: offline' },
  feedbackAndErrors: {
    ok: true,
    data: {
      feedback: [],
      errorReports: richData.feedbackAndErrors.ok ? richData.feedbackAndErrors.data.errorReports : [],
    },
  },
}

const richReport = `# Cmdr analytics report (2026-08-01)

Generated: 2026-08-02T03:04:05.000Z

## Daily funnel: the last 30 days, one row per UTC day

day | visitors | dl clicks | server dls | installs | D7 | signups | purchases
2026-08-01 | 250 | 25 | 20 | 9 | 33% | 2 | 2
2026-07-31 | - | 0 | 0 | 0 | - | - | 0
2026-07-30 | 100 | 10 | 8 | 4 | 50% | 3 | 1

Channels (last 30 days), downloads by first-touch ref:
- twitter: 15
- reddit: 5
- (none): 3

Download referrers (last 30 days), by the /download hit's Referer host:
- news.ycombinator.com: 10
- (none): 4

Downloads by client (last 30 days), by User-Agent family:
- Human installs: 24 (of 28 raw server downloads)
- human (Mac browser, Homebrew, or curl/wget): 21
- bot / impossible install (Windows, Android, Linux, or X11 UA, excluded): 4
- unknown (no or unrecognized UA, kept in the count): 3

## Awareness: how many people see Cmdr content?

- Total page views: 6,720 (+5.0% vs prior period)
- veszelovszki.com views: 1,200 (+20.0% vs prior period)
- getcmdr.com views: 5,400 (+0.0% vs prior period)
- getprvw.com views: 120
- veszelovszki.com visitors: 800 (-11.1% vs prior period)
- getcmdr.com visitors: 3,200
- getprvw.com visitors: 90 (+100.0% vs prior period)

GitHub stars: 310 total
  vdavid/cmdr: 300 (last 7d: +10, last 30d: +50)
  vdavid/mtp-rs: 10 (last 7d: +0, last 30d: +0)

Top referrers (getcmdr.com):
  news.ycombinator.com: 900 (69.2%)
  (direct): 400 (30.8%)

Top referrers (getprvw.com):
  google.com: 60 (100.0%)

## Interest: how many engage with the product page?

- getcmdr.com page views: 5,400 (+0.0% vs prior period)
- Unique visitors: 3,200
- Bounce rate: 50.0%

Download button clicks:
  download-mac: 240

Top pages:
  /: 3,000 views
  /pricing: 700 views

Website visitors by country:
  United States (US): 1,800 (72.0%)
  Sweden (SE): 600 (24.0%)
  (unknown): 100 (4.0%)

Daily page views (PostHog):
  2026-07-31: 400
  2026-08-01: 500

## Download: how many actually download?

(New installs = DMG downloads via getcmdr.com, deduplicated to distinct people per day by daily-hashed IP, bot/link-preview hits dropped by user agent, in-app auto-updates excluded. Raw = every download request.)
- New installs (deduped): 62
- Download requests (raw): 80
- Downloads (GitHub, all-time): 1,234

New installs by source (deduped):
  website: 50 (80.6%)
  homebrew: 9 (14.5%)
  other: 3 (4.8%)

Daily new installs (deduped):
  2026-08-01: 23
  2026-07-31: 39

By version:
  1.10.0: 28 (35.0%)
  1.2.0: 52 (65.0%)

By architecture:
  aarch64: 68 (85.0%)
  x86_64: 12 (15.0%)

By country:
  United States (US): 52 (65.0%)
  Sweden (SE): 25 (31.3%)
  (unknown): 3 (3.8%)

Daily downloads:
  2026-08-01: 28
  2026-07-31: 52

Top countries by architecture:
  United States (US): aarch64: 40, x86_64: 12
  Sweden (SE): aarch64: 25
  (unknown): aarch64: 3

Top countries by version:
  United States (US): 1.2.0: 52
  Sweden (SE): 1.10.0: 25
  (unknown): 1.10.0: 3

Daily downloads by version (top 2):
  2026-07-31: 1.2.0: 52
  2026-08-01: 1.10.0: 28

GitHub releases (all-time):
  v1.10.0: 800 downloads (published 2026-08-01)
  v1.2.0: 434 downloads (published 2026-07-01)

## Active use: how many run the app?

- Daily active installs (latest day): 55
- Peak daily active: 55
- Beats per active install: 8.6

Daily active installs (by day):
  2026-08-01: 55 active, 500 beats
  2026-07-31: 40 active, 320 beats

Got the latest release per day (update-enabled installs that checked, deduped, by version):
  2026-08-01: 30 total (v1.10.0: 25, v1.2.0: 5)
  2026-07-31: 30 total (v1.2.0: 30)

- Total activations: 42
- Active devices: 7

## Payment: how many pay?

- Revenue: $128.00
- Transactions: 2
- Active subscriptions: 1

Recent transactions:
  2026-08-01: $39.00 (completed)
  2026-07-31: €89.00 (refunded)

## Retention: do they stay?

- Active subscriptions: 3
- Churn rate: 25.0%

Subscriptions by status:
  active: 3 (75.0%)
  canceled: 1 (25.0%)
  paused: 0 (0.0%)

## Feedback & errors: what are users telling us?

- Feedback messages: 2
- Awaiting reply (have a reply-to email): 1
- Error reports: 3

Error reports by kind:
  auto: 2
  user: 1

Error reports by version:
  1.10.0: 2
  1.2.0: 1

Error reports by day:
  2026-07-31: 1
  2026-08-01: 2

Recent feedback:
  2026-08-01 (v1.10.0) [reply-to: someone@example.com]: Love it, especially the preview pane.
  2026-07-31 (v1.2.0): ${'x'.repeat(280)}`

const allFailedReport = `# Cmdr analytics report (7d)

Generated: 2026-08-02T03:04:05.000Z

## Daily funnel: the last 30 days, one row per UTC day

Couldn't load: Funnel: timed out after 20s

## Awareness: how many people see Cmdr content?

Couldn't load: Umami: not configured (missing env vars)

## Interest: how many engage with the product page?

Couldn't load: Umami: not configured (missing env vars); PostHog: not configured (missing env vars)

## Download: how many actually download?

Couldn't load: Cloudflare: timed out after 20s; GitHub: offline

## Active use: how many run the app?

Couldn't load: Cloudflare: timed out after 20s

## Payment: how many pay?

Couldn't load: Paddle: not configured (missing env vars)

## Retention: do they stay?

Couldn't load: Paddle: not configured (missing env vars)

## Feedback & errors: what are users telling us?

Couldn't load: Feedback & errors: timed out after 20s`

const emptyReport = `# Cmdr analytics report (30d)

Generated: 2026-08-02T03:04:05.000Z

## Daily funnel: the last 30 days, one row per UTC day

day | visitors | dl clicks | server dls | installs | D7 | signups | purchases

## Awareness: how many people see Cmdr content?

- Total page views: 0
- veszelovszki.com views: 0
- getcmdr.com views: 0
- getprvw.com views: 0
- veszelovszki.com visitors: 0
- getcmdr.com visitors: 0
- getprvw.com visitors: 0

GitHub stars: 0 total

## Interest: how many engage with the product page?

- getcmdr.com page views: 0
- Unique visitors: 0
- Bounce rate: N/A

## Download: how many actually download?

(New installs = DMG downloads via getcmdr.com, deduplicated to distinct people per day by daily-hashed IP, bot/link-preview hits dropped by user agent, in-app auto-updates excluded. Raw = every download request.)
- New installs (deduped): 0
- Download requests (raw): 0
- Downloads (GitHub, all-time): 0

## Active use: how many run the app?

- Daily active installs: none yet (heartbeat fills as beta testers update and run the new build)

- Total activations: 0

## Payment: how many pay?

- Revenue: $0.00
- Transactions: 0
- Active subscriptions: 0

## Retention: do they stay?

- Active subscriptions: 0
- Churn rate: N/A

## Feedback & errors: what are users telling us?

- Feedback messages: 0
- Awaiting reply (have a reply-to email): 0
- Error reports: 0`

// The doubled blank line under Interest and Download is the current shape: the section heading already
// emits one, and the first sub-block that survives adds its own separator.
const mixedReport = `# Cmdr analytics report (24h)

Generated: 2026-08-02T03:04:05.000Z

## Daily funnel: the last 30 days, one row per UTC day

Couldn't load: Funnel: offline

## Awareness: how many people see Cmdr content?

Couldn't load: Umami: offline

## Interest: how many engage with the product page?


Daily page views (PostHog):
  2026-07-31: 400
  2026-08-01: 500

## Download: how many actually download?


GitHub releases (all-time):
  v1.10.0: 800 downloads (published 2026-08-01)
  v1.2.0: 434 downloads (published 2026-07-01)

## Active use: how many run the app?

Couldn't load: Cloudflare: offline

## Payment: how many pay?

- Revenue: $128.00
- Transactions: 2
- Active subscriptions: 1

Recent transactions:
  2026-08-01: $39.00 (completed)
  2026-07-31: €89.00 (refunded)

## Retention: do they stay?

- Active subscriptions: 3
- Churn rate: 25.0%

Subscriptions by status:
  active: 3 (75.0%)
  canceled: 1 (25.0%)
  paused: 0 (0.0%)

## Feedback & errors: what are users telling us?

- Feedback messages: 0
- Awaiting reply (have a reply-to email): 0
- Error reports: 3

Error reports by kind:
  auto: 2
  user: 1

Error reports by version:
  1.10.0: 2
  1.2.0: 1

Error reports by day:
  2026-07-31: 1
  2026-08-01: 2`

describe('formatReport', () => {
  // The GitHub-stars block measures its 7- and 30-day windows against the wall clock.
  beforeAll(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-08-02T00:00:00Z'))
  })
  afterAll(() => {
    vi.useRealTimers()
  })

  it('renders every section when every source loaded', () => {
    expect(formatReport(richData)).toBe(richReport)
  })

  it('gives each section its own "Couldn\'t load" line when every source is down', () => {
    expect(formatReport(allFailedData)).toBe(allFailedReport)
  })

  it('skips the breakdown blocks when the sources loaded but hold nothing', () => {
    expect(formatReport(emptyData)).toBe(emptyReport)
  })

  it('keeps the surviving half of a section when one of its two sources is down', () => {
    expect(formatReport(mixedData)).toBe(mixedReport)
  })
})
