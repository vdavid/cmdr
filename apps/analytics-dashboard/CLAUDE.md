# Analytics dashboard

Private SvelteKit dashboard consolidating Cmdr business metrics into a single view, organized by acquisition stage.
Deployed to Cloudflare Pages at `analdash.getcmdr.com`. Auth via Cloudflare Access (zero-trust, no application code).

## Stack

- **SvelteKit** with `@sveltejs/adapter-cloudflare` (CF Pages deployment)
- **Tailwind v4**: CSS-first config in `src/app.css`, dark mode only
- **uPlot**: lightweight canvas-based charts (line + bar)
- **Server routes**: all API keys stay server-side, proxied via `+server.ts` / `+page.server.ts`

## Key files

| File                                        | Purpose                                                                                                                                                                        |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/app.css`                               | Tailwind v4 theme (dark palette matching getcmdr.com)                                                                                                                          |
| `src/app.d.ts`                              | Platform env type declarations for CF Pages                                                                                                                                    |
| `src/routes/+page.svelte`                   | Single-page dashboard: 6 acquisition stages plus feedback & errors                                                                                                             |
| `src/routes/+page.server.ts`                | Server load: reads `?range=` param, delegates to `fetch-all.ts`                                                                                                                |
| `src/routes/api/report/+server.ts`          | Agent-readable plain-text report with all breakdowns                                                                                                                           |
| `src/lib/server/fetch-all.ts`               | Shared data-fetching logic used by both the page and report API                                                                                                                |
| `src/lib/components/Chart.svelte`           | Reusable uPlot chart with ResizeObserver and dark theme                                                                                                                        |
| `src/lib/components/StackedBarChart.svelte` | Discrete per-day stacked bars (plain elements, not uPlot) with an exact-numbers hover/focus tooltip. Used for the by-source new-installs chart and the by-version update chart |
| `src/lib/server/types.ts`                   | Shared types: `TimeRange`, `SourceResult`, time window helpers                                                                                                                 |
| `src/lib/server/cache.ts`                   | CF Cache API wrapper with in-memory Map fallback for local dev                                                                                                                 |
| `src/lib/server/sources/`                   | Data source modules (one per external API)                                                                                                                                     |
| `svelte.config.js`                          | Adapter-cloudflare config                                                                                                                                                      |
| `vitest.config.ts`                          | Vitest config for unit tests                                                                                                                                                   |

## Running locally

1. Copy `.env.example` to `.env` and fill in real values. Values containing `$` must use `\$` (Vite's dotenv expands
   `$VAR` in double-quoted values).
2. `pnpm install` from the repo root, then `pnpm dev:dashboard`.
3. Open `http://localhost:4830`.

### Local QA against a local worker

The download/update-activity/feedback/error charts come from the api-server worker, which defaults to
`https://api.getcmdr.com`. To QA them against seeded data with zero production impact, run the worker locally and point
the dashboard at it via `WORKER_BASE_URL`:

1. Start the worker on a local D1 with seeded rows (from `apps/api-server/`):
   ```bash
   pnpm exec wrangler d1 migrations apply cmdr-telemetry --local
   pnpm exec wrangler d1 execute cmdr-telemetry --local --file=scripts/seed-local-telemetry.sql
   pnpm exec wrangler dev --port 18900 --var ADMIN_API_TOKEN:local-qa-token
   ```
   The seed (`scripts/seed-local-telemetry.sql`) uses dates relative to `now`, includes a same-day duplicate IP (to show
   dedup), a pre-migration NULL-source row, and a 0.24→0.25 update rollout.
2. In `apps/analytics-dashboard/.env`, set `WORKER_BASE_URL=http://127.0.0.1:18900` and
   `LICENSE_SERVER_ADMIN_TOKEN=local-qa-token`, then `pnpm dev:dashboard`.

`WORKER_BASE_URL` is unset in production, so the worker sources fall back to `api.getcmdr.com`. Sources without local
creds (Umami, Paddle, PostHog) just show "Couldn't load" and don't block the worker-backed charts.

## Data sources

Each source gets its own module under `src/lib/server/sources/`:

| Module                   | Auth                                            | Data                                                                                                                                                                                                                                                                    |
| ------------------------ | ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `umami.ts`               | JWT (username/password login)                   | Page views, visitors, referrers, countries, download events for veszelovszki.com, getcmdr.com, and getprvw.com                                                                                                                                                          |
| `cloudflare.ts`          | Bearer token (via `LICENSE_SERVER_ADMIN_TOKEN`) | Downloads (by version/arch/country/source, with raw + same-day-deduped counts), true per-day DAU + beats from the heartbeat, and per-day update-check activity by version, from worker endpoints (`/admin/downloads`, `/admin/heartbeat-dau`, `/admin/update-activity`) |
| `paddle.ts`              | Bearer token, cursor pagination                 | Completed transactions, subscriptions by status                                                                                                                                                                                                                         |
| `github.ts`              | Optional Bearer token                           | Release download counts per asset; star history (daily + cumulative) for cmdr and mtp-rs via stargazers API with pagination                                                                                                                                             |
| `posthog.ts`             | Bearer personal API key                         | Pageview trends via Trends API (EU endpoint)                                                                                                                                                                                                                            |
| `license.ts`             | Bearer admin token                              | Activation count + active devices from `/admin/stats`                                                                                                                                                                                                                   |
| `feedback-and-errors.ts` | Bearer token (via `LICENSE_SERVER_ADMIN_TOKEN`) | In-app feedback messages and error-report bundle metadata from worker endpoints (`/admin/feedback`, `/admin/error-reports`). Pure aggregation helpers + row types live in `$lib/feedback-and-errors.ts` (client-safe, shared with the page)                             |
| `worker-endpoint.ts`     | (shared helper)                                 | `fetchWorkerEndpoint(token, path)`: GETs a worker admin endpoint with the bearer token, used by `cloudflare.ts` and `feedback-and-errors.ts`                                                                                                                            |

Each module exports a typed fetch function returning `SourceResult<T>` (ok + data, or error string). Results are cached
via `cache.ts` (5 min TTL for 24h/7d, 1 hour for 30d). The page server calls all sources in parallel, each capped at 20s
by `withTimeout` in `fetch-all.ts`: Workers `fetch` has no built-in timeout, so without the cap one hung upstream stalls
the whole `Promise.all` until Cloudflare's proxy returns a 524 at 100s. Keep new sources behind the same wrapper.

## Deployment

Auto-deploys to Cloudflare Pages on push to `main` when files in `apps/analytics-dashboard/` change. Workflow:
`.github/workflows/deploy-dashboard.yml`. Steps: checkout, install deps, build, deploy with wrangler.

**Manual setup required** (not in code):

- Create CF Pages project `cmdr-analytics-dashboard` in the CF dashboard (or `wrangler pages project create`)
- Add `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` to GitHub repo secrets (for wrangler deploy)
- Set all env vars below as CF Pages secrets (via `wrangler pages secret put` or CF dashboard). Remember to add
  `UMAMI_PRVW_WEBSITE_ID` when deploying.
- Configure custom domain `analdash.getcmdr.com` in CF Pages settings
- Set up Cloudflare Access policy for `analdash.getcmdr.com` in the CF dashboard

## Env vars

All set as CF Pages secrets, never in code.

| Variable                     | Notes                                                                        |
| ---------------------------- | ---------------------------------------------------------------------------- |
| `UMAMI_API_URL`              | `https://anal.veszelovszki.com`                                              |
| `UMAMI_USERNAME`             | Existing Umami credentials                                                   |
| `UMAMI_PASSWORD`             | Existing Umami credentials                                                   |
| `UMAMI_WEBSITE_ID`           | getcmdr.com website ID                                                       |
| `UMAMI_BLOG_WEBSITE_ID`      | veszelovszki.com website ID (env var name kept for CF secrets compatibility) |
| `UMAMI_PRVW_WEBSITE_ID`      | getprvw.com website ID                                                       |
| `PADDLE_API_KEY_LIVE`        | Live API key (not sandbox)                                                   |
| `POSTHOG_API_KEY`            | Personal `phx_...` key (not the public `phc_...` project key)                |
| `POSTHOG_PROJECT_ID`         | `136072`                                                                     |
| `POSTHOG_API_URL`            | `https://eu.posthog.com` (must be EU)                                        |
| `GITHUB_TOKEN`               | Optional, avoids rate limits on public repo API                              |
| `LICENSE_SERVER_ADMIN_TOKEN` | Dedicated admin secret, also set on the API server                           |

## Key decisions

**Decision**: Metrics are organized by acquisition stage, not as a cohort funnel. **Why**: Tracking is cookieless and
anonymous, so there's no way to follow an individual from blog visit to download to payment. The stages show independent
aggregate numbers. A true funnel would require cross-site user identity tracking and a cookie banner.

**Decision**: Dark mode only. **Why**: Internal tool, always viewed on a laptop. Saves effort.

**Decision**: Consistent color coding across the dashboard. **Why**: Visual clarity when scanning metrics.

- **Gold (`#ffc206`)**: getcmdr.com / vdavid/cmdr (the primary product)
- **Purple (`#a78bfa`)**: vdavid/mtp-rs (the library repo)
- **Autumn green (`#8faa3b`)**: veszelovszki.com (David's personal site)
- **Cyan (`#22d3ee`)**: getprvw.com (Prvw product site)

These colors are used in metric dots, chart strokes, and chart fills. Keep them consistent when adding new UI.

**Decision**: Single page, not multi-page. **Why**: A handful of sections. Scroll is simpler than navigation.

**Decision**: A "Feedback & errors" section reads the app's own stores via two worker admin endpoints (`/admin/feedback`
from D1, `/admin/error-reports` from the R2 bucket's `list` with `customMetadata`), not Discord. **Why**: the
`#feedback` and `#error-reports` Discord channels are private and denied to the community bot, and the worker already
holds the `TELEMETRY_DB` and `ERROR_REPORTS_BUCKET` bindings, so it can serve this with no extra token or service. The
agent-facing local digest (`/feedback-and-error-digest-from-app`) reads the same stores directly; see
`docs/tooling/feedback-and-error-digest.md`.

**Decision**: The feedback/error-report row types and pure aggregation helpers live in `$lib/feedback-and-errors.ts`,
outside `$lib/server`. **Why**: `+page.svelte` reaches the client bundle, and SvelteKit forbids runtime imports from
`$lib/server` there. Keeping the helpers client-safe lets the page, the report endpoint, and tests share one copy; the
server-only fetching stays in `$lib/server/sources/feedback-and-errors.ts`.

**Decision**: uPlot for charts. **Why**: ~45 KB, fast canvas rendering, simple API. No wrapper needed.

**Decision**: Local dev reads env vars via SvelteKit's `$env/dynamic/private` when `platform?.env` is undefined.
**Why**: CF Pages `platform.env` only exists in deployed environments. SvelteKit's env module properly loads `.env`
files with escaping support. Copy `.env.example` to `.env` and fill in real values.

**Decision**: The "Active use" section's daily-active count comes from the heartbeat (`/admin/heartbeat-dau`,
`COUNT(DISTINCT anal_id)` per day), not from update checks. **Why**: the old "Update checks (approximate active users)"
metric summed per-day active counts across the whole window, multiplying a ~10/day figure into a wildly inflated total
(~217). The heartbeat gives a true per-day distinct-install count, charted gold over the range, with `beats/day` as an
engagement signal. The `/admin/active-users` endpoint and its cron still run (other tooling may use them); the dashboard
just no longer displays the inflated number. The chart starts empty at release and fills as beta testers update.

**Decision**: "New installs" (Download section) and "Got the latest release" (Active use) are two distinct charts off
two distinct tables, never merged. **Why**: a DMG download (`downloads` table) is a fresh acquisition; an update check
(`update_checks` table) is an existing install updating in place. In-app auto-updates fetch the tarball straight from
GitHub and never hit the download endpoint, so the two populations don't overlap and stacking them in one bar would
mislead. New-installs bars stack by source (website/Homebrew/other) using the **deduped** same-day-distinct count
(`uniqueDownloads`); update bars stack by the version each install was on when it checked. Both charts carry a visible
"how this is measured" note (the `methodology` snippet) because opaque analytics numbers are worse than none.

**Decision**: PostHog uses the HogQL query API (`/api/projects/{id}/query/`), not the legacy Trends API
(`/insights/trend/`). **Why**: The Trends API returns "Legacy insight endpoints are not available" for newer accounts.

**Decision**: Umami metrics use `type=path` (not `type=url`). **Why**: Umami's API changed the type name. `url` and
`page` return 400, but `path`, `referrer`, `event`, and `country` work.

## Gotchas

**Gotcha**: `caches.default` and `caches.open()` are not available in `wrangler pages dev` local development. **Why**:
CF Workers Cache API isn't emulated locally. Use a simple in-memory Map as fallback.
