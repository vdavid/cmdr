# Analytics plan

## Intention

Cmdr is approaching its first public release. We need to understand user behavior so we can make good product decisions,
and track downloads so we know adoption momentum. We want to do this while keeping our privacy-friendly brand intact.

Currently, the privacy policy mentions PostHog and in-app analytics, but **neither is actually implemented**. Downloads
go directly to GitHub Releases with no tracking. This plan closes that gap.

## Guiding principles

- **Privacy first**: No PII, no individual tracking, no cookies where avoidable.
- **Website only**: All analytics live on getcmdr.com. The desktop app collects nothing (beyond license validation).
- **Self-hosted where practical**: Umami on our VM for page analytics. PostHog cloud for session replay/heatmaps
  (self-hosting PostHog is heavy and not worth it).
- **Honest privacy policy**: Only claim what we actually do. Remove the false in-app analytics claim.

## Architecture overview

```
┌──────────────────────────────────┐     ┌─────────────────────┐
│  getcmdr.com (Astro)             │     │  Download redirect  │
│                                  │     │  (CF Worker)        │
│  Umami script ───► Umami server  │     │                     │
│  (cookieless)      (self-hosted) │     │  Logs version +     │
│                                  │     │  arch + country     │
│  PostHog JS ─────► PostHog cloud │     │  then 302 → GitHub  │
│  (session replay,  (free tier)   │     │                     │
│   heatmaps, clicks)              │     │                     │
└──────────────────────────────────┘     └──────────┬──────────┘
                                                    │
                                                    ▼
                                              Analytics Engine
```

The desktop app has **no analytics**. All behavior tracking lives on the website.

## Milestone 1: Website analytics with Umami

### Why Umami instead of PostHog for the website

- Already self-hosted on the VM for other projects — zero additional cost.
- Cookieless by default — no cookie banner needed, simpler privacy policy.
- Lightweight script (~2 KB) vs PostHog's SDK — fits the static Astro site better.
- Good enough for website analytics (page views, referrers, geo, UTM params).
- Full REST API: websites can be created, events sent, and stats queried programmatically via
  `POST /api/websites`, `POST /api/send`, `GET /api/websites/:id/stats`, etc. No GUI interaction required for setup.

### Steps

1. **Register getcmdr.com in Umami** via `POST /api/websites` (or via the GUI, either works). Note the website ID.
2. **Add the Umami tracking script** to the website's `<head>` in `apps/website/src/layouts/Layout.astro`:
   ```html
   <script defer src="https://{umami-host}/script.js" data-website-id="{website-id}"></script>
   ```
   Use an env var for the Umami host and website ID so it's not hardcoded.
3. **Track download clicks** as custom events on the download button in `apps/website/src/components/Download.astro`.
   Umami supports `data-umami-event` attributes:
   ```html
   <a href="..." data-umami-event="download" data-umami-event-version="0.5.0" data-umami-event-arch="aarch64">
   ```
   This gives us download intent by version and architecture, with geo from Umami automatically.
4. **Disable in dev mode**: Only include the script when `import.meta.env.PROD` is true, or use a feature flag env
   var. Remember: `withGlobalTauri: true` in dev mode is a security risk for external scripts.
5. **Verify**: Check the Umami dashboard (or query `GET /api/websites/:id/stats`) to confirm data flows.

## Milestone 2: Download tracking with geo via Cloudflare Worker

### Why a separate endpoint

- Umami's download button events tell us **intent** (clicked download). A redirect endpoint tells us **actual
  downloads** (the browser fetched the .dmg).
- Cloudflare Workers have access to `request.cf.country`, `request.cf.city`, `request.cf.continent` for free — no
  external geo IP service needed.
- We already run a CF Worker for the license server. This can live alongside it or as a separate Worker.

### Design

`GET https://dl.getcmdr.com/download/:version/:arch` (or a path under the license server domain)

1. Extract `version`, `arch` from the path.
2. Read `request.cf.country` and `request.cf.continent`.
3. Log the event to **Cloudflare Analytics Engine** (free, built into Workers) or increment a counter in **KV**.
4. Respond with `302 → https://github.com/vdavid/cmdr/releases/download/v{version}/Cmdr_{version}_{arch}.dmg`.

#### Decision: Analytics Engine vs KV

- **Analytics Engine** (recommended): purpose-built for this. Write events with dimensions (version, arch, country),
  query with SQL via the dashboard or API. No aggregation logic needed. Free tier: 100K events/day.
- **KV**: simpler but requires manual aggregation (composite keys like `downloads:0.5.0:aarch64:SE`). Gets messy for
  querying. Better for simple counters only.

Analytics Engine is the better fit unless the free tier becomes a concern (unlikely at launch scale).

### Steps

1. **Decide**: separate Worker (`dl.getcmdr.com`) or new route in the license server. I'd lean toward a new route
   in the license server (`/download/:version/:arch`) to avoid another deployment target. But a separate subdomain
   is cleaner for CDN caching and separation of concerns.
2. **Implement the redirect endpoint** in Hono:
   ```ts
   app.get('/download/:version/:arch', async (c) => {
     const { version, arch } = c.req.param()
     const country = c.req.raw.cf?.country ?? 'unknown'
     // Log to Analytics Engine or KV
     const dmgName = `Cmdr_${version}_${arch}.dmg`
     return c.redirect(`https://github.com/vdavid/cmdr/releases/download/v${version}/${dmgName}`, 302)
   })
   ```
3. **Enable Analytics Engine** in `wrangler.toml` (if using it):
   ```toml
   [[analytics_engine_datasets]]
   binding = "DOWNLOADS"
   ```
4. **Update the website download links** in `Download.astro` to point to the redirect endpoint instead of directly
   to GitHub.
5. **Update `latest.json`** generation in the release workflow if it contains direct GitHub URLs.
6. **Test**: download via the new URL, verify the redirect works and the event is logged.

## Milestone 3: Website behavior tracking with PostHog

### Why PostHog on the website (not in the desktop app)

We want to understand how visitors interact with getcmdr.com: where they scroll, what they click, where they drop off.
This is a HotJar-like use case. PostHog provides session replay, heatmaps, and click tracking on its free tier.

The desktop app has **no telemetry**. The privacy policy's "aggregate usage statistics" claim will be removed (it was
never implemented). This is a deliberate choice: a file manager touching your files should collect nothing.

### Why PostHog alongside Umami (not instead of)

- **Umami**: Lightweight, cookieless page-level analytics (page views, referrers, geo, UTM). Self-hosted. Stays.
- **PostHog**: Session replay, heatmaps, click maps, scroll depth, funnels. These are qualitative — Umami can't do
  them. PostHog's JS snippet is needed for this.

They serve different purposes with minimal overlap. Umami tells us "how many people visited /pricing", PostHog tells
us "people scroll past the feature list without reading it".

### What PostHog gives us

- **Session replay**: Watch real visitor sessions (anonymized). See where they get confused or drop off.
- **Heatmaps**: See click density on each page. Find dead zones and missed CTAs.
- **Scroll depth**: See how far people scroll on the landing page. Optimize content placement.
- **Funnels**: Landing → Download → (later) Activate license. Where do people drop off?
- **Free tier**: 5K session replays/month, 1M events/month. More than enough for launch.

### Steps

1. **Add the PostHog JS snippet** to `Layout.astro`, similar to Umami. Use `PUBLIC_POSTHOG_KEY` env var.
   Guard with the same prod-only check. PostHog's lightweight snippet (`posthog-js`) auto-captures page views,
   clicks, and enables session replay and heatmaps.
2. **Configure PostHog** to respect privacy:
   - Disable autocapture of text input values (default off, but verify).
   - Enable session replay with DOM masking for sensitive elements (if any — our website is public content).
   - No need for identified users — all anonymous.
3. **Add env vars** to `.env.example`: `PUBLIC_POSTHOG_KEY`, `PUBLIC_POSTHOG_HOST` (defaults to `us.i.posthog.com`).
4. **Disable in dev mode**: Same pattern as Umami — only load when env var is set.
5. **Verify**: Check PostHog dashboard for session replays and heatmap data.

### Note on cookies

PostHog JS does set a first-party cookie (`ph_*`) to correlate page views within a session. This is necessary for
session replay to work. The privacy policy needs to mention this (unlike Umami which is cookieless). However, it's
a first-party analytics cookie, not cross-site tracking.

## Milestone 4: Update the privacy policy

The privacy policy currently mentions PostHog for the website and in-app analytics, but neither exists. After
implementing the above:

### Changes needed

1. **Website analytics section**: Currently says "PostHog" only. Rewrite to mention both Umami (self-hosted,
   cookieless page analytics) and PostHog (session replay, heatmaps, click tracking with a first-party cookie).
2. **In-app analytics section**: The current claim about "aggregate usage statistics" was never implemented. **Remove
   it entirely.** The desktop app collects nothing beyond license validation. Be honest.
3. **Download tracking**: Add a brief mention that we track download counts by version and country. No PII involved.
4. **Cookies section**: Update to reflect that Umami is cookieless but PostHog sets a first-party `ph_*` cookie for
   session replay. No third-party or cross-site tracking cookies.
5. **Data processors list**: Keep PostHog (now accurate — it's used for website behavior tracking). Add a note that
   Umami is self-hosted (no third-party processor for page analytics).
6. **Update `lastUpdated` date**.

### Net effect on privacy posture

- **More honest**: No more claiming in-app analytics that don't exist. The desktop app collects nothing.
- **Transparent**: Two website analytics tools, each explained clearly with different purposes.
- **Fair trade-off**: PostHog adds a cookie, but only first-party, and only for understanding website UX.
  The cookie section now has real content instead of being misleading.

## Task list

### Milestone 1: Website analytics (Umami)
- [x] Register getcmdr.com in Umami (API or GUI)
- [x] Add Umami script to `Layout.astro` with env var config, disabled in dev
- [x] Add download event tracking to `Download.astro`
- [x] Add env vars to `.env.example` and deployment config
- [x] Set `PUBLIC_UMAMI_HOST` and `PUBLIC_UMAMI_WEBSITE_ID` in production deployment env
- [x] Verify data flows in Umami dashboard

### Milestone 2: Download tracking (CF Worker)
- [x] Decide: new route in license server (avoids another deployment target)
- [x] Enable Analytics Engine in `wrangler.toml` (binding: `DOWNLOADS`, dataset: `cmdr_downloads`)
- [x] Implement `/download/:version/:arch` redirect endpoint in `index.ts`
- [x] Update website `release.ts` to use redirect when `PUBLIC_DOWNLOAD_BASE_URL` is set
- [x] Update `.env.example` with `PUBLIC_DOWNLOAD_BASE_URL`
- [x] Update license server `CLAUDE.md` with new route and Analytics Engine docs
- [x] Set `PUBLIC_DOWNLOAD_BASE_URL` in website production env
- [x] Deploy license server: `cd apps/license-server && pnpm cf:deploy`
- [x] Test end-to-end: download via redirect, verify event logged in Analytics Engine

### Milestone 3: Website behavior tracking (PostHog)
- [x] Create PostHog project, get API key
- [x] Add PostHog via `posthog-js` npm package with dynamic import in `Layout.astro` (prod-only, env var gated)
- [x] Configure: `person_profiles: 'identified_only'`, `capture_pageleave: true` (anonymous, no person profiles)
- [x] Add `PUBLIC_POSTHOG_KEY` and `PUBLIC_POSTHOG_HOST` to `.env.example`
- [x] Set `PUBLIC_POSTHOG_KEY` in production env (`/opt/cmdr/apps/website/.env` on Hetzner)
- [x] Enable session replay and heatmaps in PostHog project settings
- [x] Add getcmdr.com as authorized domain for PostHog toolbar
- [x] Verify session replays and heatmaps in PostHog dashboard

### Milestone 4: Privacy policy update
- [x] Rewrite website analytics section (Umami + PostHog with distinct roles)
- [x] Remove false in-app analytics claim, add "no telemetry" statement
- [x] Add download tracking mention (version, arch, country, no IP)
- [x] Update data processors list (PostHog, Cloudflare, Umami self-hosted note)
- [x] Rewrite cookies section (PostHog first-party cookie, Umami cookieless)
- [x] Update data storage section (Umami + Listmonk self-hosted in Europe)
- [x] Fix pre-existing issues: typo "apps verifies" → "app verifies", `_not_`/`_need_` → `<em>`
- [x] Remove misleading "Your PII" bullet from "What we don't collect" (we do collect email/payment)
- [x] Update `lastUpdated` date to 2026-03-08
- [x] Run checks: prettier, eslint, typecheck, build — all pass
