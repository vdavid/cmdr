# Analytics plan

## Intention

Cmdr is approaching its first public release. We need to understand user behavior so we can make good product decisions,
and track downloads so we know adoption momentum. We want to do this while keeping our privacy-friendly brand intact.

Currently, the privacy policy mentions PostHog and in-app analytics, but **neither is actually implemented**. Downloads
go directly to GitHub Releases with no tracking. This plan closes that gap.

## Guiding principles

- **Privacy first**: No PII, no individual tracking, no cookies where avoidable.
- **Self-hosted where practical**: Umami on our VM, PostHog only where self-hosting is impractical (in-app product
  analytics with funnels/retention).
- **Minimal footprint**: No heavy SDKs. HTTP POST calls where possible.
- **Honest privacy policy**: Only claim what we actually do. Update it to match reality.

## Architecture overview

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│  getcmdr.com    │     │  Cmdr desktop app │     │  Download redirect  │
│  (Astro)        │     │  (Tauri)          │     │  (CF Worker)        │
│                 │     │                   │     │                     │
│  Umami script ──┼──►  │  PostHog HTTP ────┼──►  │  Logs version +    │
│  (cookieless)   │     │  capture API      │     │  arch + country    │
│                 │     │  (anonymous)      │     │  then 302 → GitHub │
└────────┬────────┘     └────────┬──────────┘     └──────────┬──────────┘
         │                       │                           │
         ▼                       ▼                           ▼
   Umami server            PostHog cloud              Cloudflare KV or
   (self-hosted VM)        (free tier)                Analytics Engine
```

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

## Milestone 3: In-app product analytics with PostHog

### Why PostHog for in-app (not Umami)

- PostHog has funnels, retention, cohorts, and feature flags — Umami doesn't.
- PostHog's free tier (1M events/month) is more than enough for a desktop app's launch phase.
- We only need PostHog's HTTP capture API (`POST https://us.i.posthog.com/capture`), no SDK.

### Design principles for in-app telemetry

- **Anonymous by default**: Use a random `distinct_id` generated on first launch, stored locally. Not tied to email,
  license key, or any PII. Rotate it if the user resets the app. This keeps the "no individual tracking" promise.
- **Opt-out available**: Add a toggle in Settings. Respect it immediately.
- **Events, not sessions**: We're a desktop app, not a web app. Track discrete actions, not page views.
- **No content**: Never include file names, paths, AI prompts, or user content in events.

### What to track (starter set)

| Event | Properties | Why |
|---|---|---|
| `app_launched` | `version`, `os_version`, `arch` | Understand active user base |
| `feature_used` | `feature` (for example, `copy`, `network_browse`, `search`, `command_palette`) | Know which features matter |
| `settings_changed` | `setting_key` (not the value) | Understand what people customize |
| `update_installed` | `from_version`, `to_version` | Track update adoption |
| `license_activated` | `license_type` (`personal`/`commercial`) | Understand conversion |

Keep the event list small. Each event should answer a specific product question.

### Steps

1. **Create a PostHog project** (cloud, free tier). Get the project API key.
2. **Implement a telemetry module** in the Svelte frontend (`$lib/telemetry/`):
   - `telemetry.svelte.ts`: reactive state for opt-in/opt-out, `distinct_id` generation and persistence.
   - `capture.ts`: thin wrapper around `fetch('https://us.i.posthog.com/capture', ...)`. No SDK dependency.
   - Respect the opt-out setting. If opted out, `capture()` is a no-op.
3. **Add the opt-out toggle** in Settings, under a "Privacy" or "Telemetry" section.
4. **Instrument the starter events** listed above.
5. **Disable in dev mode**: Don't send events when running `pnpm dev`. Use an env var or check `import.meta.env.DEV`.
6. **Verify**: Check PostHog dashboard for incoming events.

### Open question: Rust-side events?

Some events (like file operation counts) might be easier to capture from Rust. Options:
- Emit them as Tauri events, let the frontend capture and forward to PostHog.
- Or call PostHog's HTTP API directly from Rust (adds a dependency: `reqwest` or similar).

Recommendation: start with frontend-only. Add Rust-side capture later if needed. Simpler to maintain one telemetry
path.

## Milestone 4: Update the privacy policy

The privacy policy currently mentions PostHog for the website and in-app analytics, but neither exists. After
implementing the above:

### Changes needed

1. **Website analytics section**: Replace PostHog with Umami. Mention it's self-hosted and cookieless.
2. **Remove or shrink the cookies section**: Umami doesn't set cookies. Only Paddle might (for checkout).
3. **In-app analytics section**: Keep the current language about aggregate stats. Add that PostHog is the provider.
   Emphasize the anonymous `distinct_id` and opt-out toggle.
4. **Download tracking**: Add a brief mention that we track download counts by version and country. No PII involved.
5. **Data processors list**: Remove PostHog from website, add it under in-app. Add a note that website analytics
   are self-hosted (no third-party processor).
6. **Update `lastUpdated` date**.

### Net effect on privacy posture

- **Better**: Self-hosted, cookieless website analytics. Fewer third-party processors for the website.
- **Honest**: In-app analytics now actually exist and match what the policy says.
- **Transparent**: Opt-out toggle gives users control.

## Task list

### Milestone 1: Website analytics (Umami)
- [x] Register getcmdr.com in Umami (API or GUI)
- [x] Add Umami script to `Layout.astro` with env var config, disabled in dev
- [x] Add download event tracking to `Download.astro`
- [x] Add env vars to `.env.example` and deployment config
- [x] Set `PUBLIC_UMAMI_HOST` and `PUBLIC_UMAMI_WEBSITE_ID` in production deployment env
- [ ] Verify data flows in Umami dashboard

### Milestone 2: Download tracking (CF Worker)
- [ ] Decide: new route in license server vs separate Worker
- [ ] Enable Analytics Engine in `wrangler.toml`
- [ ] Implement `/download/:version/:arch` redirect endpoint
- [ ] Update website download links to use the redirect
- [ ] Update release workflow if `latest.json` needs changes
- [ ] Test end-to-end: download via redirect, verify event logged

### Milestone 3: In-app analytics (PostHog)
- [ ] Create PostHog project, get API key
- [ ] Implement `$lib/telemetry/` module (capture, opt-out, anonymous ID)
- [ ] Add opt-out toggle in Settings
- [ ] Instrument starter events
- [ ] Disable in dev mode
- [ ] Verify events in PostHog dashboard

### Milestone 4: Privacy policy update
- [ ] Rewrite website analytics section (Umami, self-hosted, cookieless)
- [ ] Rewrite in-app analytics section (PostHog, anonymous, opt-out)
- [ ] Add download tracking mention
- [ ] Update data processors list
- [ ] Shrink cookies section
- [ ] Update `lastUpdated` date
- [ ] Run checks: `./scripts/check.sh --check website-prettier --check website-build`
