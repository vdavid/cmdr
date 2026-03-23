# Analytics dashboard

Private SvelteKit dashboard consolidating Cmdr business metrics into a single view, organized by acquisition stage.
Deployed to Cloudflare Pages at `analdash.getcmdr.com`. Auth via Cloudflare Access (zero-trust, no application code).

## Stack

- **SvelteKit** with `@sveltejs/adapter-cloudflare` (CF Pages deployment)
- **Tailwind v4** — CSS-first config in `src/app.css`, dark mode only
- **uPlot** — lightweight canvas-based charts (line + bar)
- **Server routes** — all API keys stay server-side, proxied via `+server.ts` / `+page.server.ts`

## Key files

| File | Purpose |
| --- | --- |
| `src/app.css` | Tailwind v4 theme (dark palette matching getcmdr.com) |
| `src/app.d.ts` | Platform env type declarations for CF Pages |
| `src/routes/+page.svelte` | Single-page dashboard with 6 acquisition stage sections |
| `src/routes/+page.server.ts` | Server load: reads `?range=` param, delegates to `fetch-all.ts` |
| `src/routes/api/report/+server.ts` | Agent-readable plain-text report with all breakdowns |
| `src/lib/server/fetch-all.ts` | Shared data-fetching logic used by both the page and report API |
| `src/lib/components/Chart.svelte` | Reusable uPlot chart with ResizeObserver and dark theme |
| `src/lib/server/types.ts` | Shared types: `TimeRange`, `SourceResult`, time window helpers |
| `src/lib/server/cache.ts` | CF Cache API wrapper with in-memory Map fallback for local dev |
| `src/lib/server/sources/` | Data source modules (one per external API) |
| `svelte.config.js` | Adapter-cloudflare config |
| `vitest.config.ts` | Vitest config for unit tests |

## Running locally

1. Copy `.env.example` to `.env` and fill in real values. Values containing `$` must use `\$` (Vite's dotenv expands
   `$VAR` in double-quoted values).
2. `pnpm install` from the repo root, then `pnpm dev:dashboard`.
3. Open `http://localhost:4830`.

## Data sources

Each source gets its own module under `src/lib/server/sources/`:

| Module | Auth | Data |
| --- | --- | --- |
| `umami.ts` | JWT (username/password login) | Page views, visitors, referrers, countries, download events for blog + getcmdr.com |
| `cloudflare.ts` | Bearer token | Analytics Engine SQL: download counts, update check counts by version/arch/country. Note: `cmdr_crash_reports` dataset is also available for crash data (not yet integrated). |
| `paddle.ts` | Bearer token, cursor pagination | Completed transactions, subscriptions by status |
| `github.ts` | Optional Bearer token | Release download counts per asset |
| `posthog.ts` | Bearer personal API key | Pageview trends via Trends API (EU endpoint) |
| `license.ts` | Bearer admin token | Activation count + active devices from `/admin/stats` |

Each module exports a typed fetch function returning `SourceResult<T>` (ok + data, or error string). Results are cached
via `cache.ts` (5 min TTL for 24h/7d, 1 hour for 30d). The page server calls all sources in parallel.

## Deployment

Auto-deploys to Cloudflare Pages on push to `main` when files in `apps/analytics-dashboard/` change.
Workflow: `.github/workflows/deploy-dashboard.yml`. Steps: checkout, install deps, build, deploy with wrangler.

**Manual setup required** (not in code):
- Create CF Pages project `cmdr-analytics-dashboard` in the CF dashboard (or `wrangler pages project create`)
- Add `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` to GitHub repo secrets
- Set all env vars below as CF Pages secrets (via `wrangler pages secret put` or CF dashboard)
- Configure custom domain `analdash.getcmdr.com` in CF Pages settings
- Set up Cloudflare Access policy for `analdash.getcmdr.com` in the CF dashboard

## Env vars

All set as CF Pages secrets, never in code.

| Variable | Notes |
| --- | --- |
| `UMAMI_API_URL` | `https://anal.veszelovszki.com` |
| `UMAMI_USERNAME` | Existing Umami credentials |
| `UMAMI_PASSWORD` | Existing Umami credentials |
| `UMAMI_WEBSITE_ID` | getcmdr.com website ID |
| `UMAMI_BLOG_WEBSITE_ID` | Blog website ID |
| `CLOUDFLARE_API_TOKEN` | Needs `Account Analytics Read` permission |
| `CLOUDFLARE_ACCOUNT_ID` | `6a4433bf...` |
| `PADDLE_API_KEY_LIVE` | Live API key (not sandbox) |
| `POSTHOG_API_KEY` | Personal `phx_...` key (not the public `phc_...` project key) |
| `POSTHOG_PROJECT_ID` | `136072` |
| `POSTHOG_API_URL` | `https://eu.posthog.com` (must be EU) |
| `GITHUB_TOKEN` | Optional, avoids rate limits on public repo API |
| `LICENSE_SERVER_ADMIN_TOKEN` | Dedicated admin secret, also set on the license server |

## Key decisions

**Decision**: Dark mode only. **Why**: Internal tool, always viewed on a laptop. Saves effort.

**Decision**: Single page, not multi-page. **Why**: Only six sections. Scroll is simpler than navigation.

**Decision**: uPlot for charts. **Why**: ~45 KB, fast canvas rendering, simple API. No wrapper needed.

**Decision**: Local dev reads env vars via SvelteKit's `$env/dynamic/private` when `platform?.env` is undefined.
**Why**: CF Pages `platform.env` only exists in deployed environments. SvelteKit's env module properly loads `.env` files
with escaping support. Copy `.env.example` to `.env` and fill in real values.

**Decision**: PostHog uses the HogQL query API (`/api/projects/{id}/query/`), not the legacy Trends API
(`/insights/trend/`). **Why**: The Trends API returns "Legacy insight endpoints are not available" for newer accounts.

**Decision**: Umami metrics use `type=path` (not `type=url`). **Why**: Umami's API changed the type name. `url` and
`page` return 400, but `path`, `referrer`, `event`, and `country` work.

## Gotchas

**Gotcha**: CF Analytics Engine data is only retained for 90 days. **Why**: CF limitation. Historical comparisons beyond
90 days are not possible for download/update-check metrics.

**Gotcha**: `caches.default` and `caches.open()` are not available in `wrangler pages dev` local development. **Why**: CF
Workers Cache API isn't emulated locally. Use a simple in-memory Map as fallback.
