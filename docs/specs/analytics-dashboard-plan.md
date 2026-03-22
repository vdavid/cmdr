# Analytics dashboard plan

A private SvelteKit dashboard at `apps/analytics-dashboard/` that consolidates all Cmdr business metrics into a
single view, organized by acquisition stage.

## Why

Analytics are scattered across Umami, PostHog, Cloudflare, Paddle, and GitHub — each with its own login, time range
picker, and mental model. A single dashboard lets you see all stages at a glance, spot trends across sources, and
link back to the originals when you need to dig deeper.

## Metrics by acquisition stage

The dashboard shows aggregate metrics organized by acquisition stage. Each stage maps to one or more data sources.

**Important**: This is NOT a cohort funnel. The tracking is cookieless and anonymous — there's no way to follow an
individual user from blog visit → download → payment. The stages show independent aggregate numbers. You can infer
approximate conversion rates from the ratios, but you can't say "of the people who downloaded last week, X% paid."
A true funnel would require cross-site user identity tracking and a cookie banner.

1. **Awareness** — How many people see Cmdr content?
   - Umami: blog page views + referrers + UTMs (blog website ID: `3ee5c901-...`, separate project at
     `~/projects-git/vdavid/blog/`)
   - Umami: getcmdr.com page views + referrers + UTMs (website ID: `5ea041ae-...`)
   - Both are tracked within the same Umami instance at `anal.veszelovszki.com`, using different website IDs.

2. **Interest** — How many engage with the product page?
   - Umami: getcmdr.com specific page stats (homepage, features page, pricing)
   - Umami: download button click events (`data-umami-event-arch` custom events)
   - PostHog: session/event counts via the Trends API (project `136072`). Heatmap and scroll depth data is not
     available via API — only in the PostHog UI. Keep PostHog integration minimal; Umami covers most of this stage.

3. **Download** — How many actually download?
   - Cloudflare Analytics Engine: `cmdr_downloads` dataset (version, arch, country)
   - GitHub Releases: download counts per asset (secondary/cross-check)

4. **Active use** — How many run the app?
   - Update check proxy: `GET /update-check/:version` on the license server logs to `cmdr_update_checks` Analytics
     Engine dataset, then redirects to `latest.json`. Covers all users (free + licensed). See milestone 0.
   - License server `POST /validate` calls as a secondary signal for licensed users

5. **Payment** — How many pay?
   - Paddle: transactions (completed payments, amounts, currency)
   - Paddle: active subscriptions count
   - License server: activation count (KV-based, needs admin endpoint)

6. **Retention** — Do they stay?
   - Paddle: subscription status (active vs canceled vs past_due)
   - Paddle: churn = canceled in period / active at start of period

## Time ranges

Every chart supports: last 24 hours, last 7 days, last 30 days. The time range picker is global (applies to all
charts at once). Optionally: custom date range later.

## Architecture

### Tech stack

- **SvelteKit** with `@sveltejs/adapter-cloudflare` for deployment to Cloudflare Pages.
- **Server routes** (`+server.ts` or `+page.server.ts`) hold all API keys and proxy calls. No secrets in the browser.
  On CF Pages, server-side code runs as CF Workers Functions under the hood.
- **Charts**: [uPlot](https://github.com/leeoniya/uPlot) — tiny (~45 KB), fast, canvas-based. Line charts for trends,
  bar charts for breakdowns. No wrapper library needed — uPlot's API is simple enough to use directly from Svelte.
- **Styling**: Tailwind v4 (consistent with the rest of the monorepo). Dark mode only is fine — this is an internal
  tool.
- **Auth**: Cloudflare Access (zero-trust). Free for up to 50 users. No custom auth code, no password brute-force
  concerns, no cookie/session management to build. Configured in the CF dashboard for `analdash.getcmdr.com`.

### Why SvelteKit on Cloudflare Pages

- Already using Svelte 5 in the desktop app — no new framework to learn.
- Server routes make API proxying trivial.
- CF Pages eliminates all deployment infrastructure: no Docker, no Caddy config, no proxy-net, no health endpoints,
  no process management, no VPS dependency. Deploy with `wrangler pages deploy`.
- Free tier: 500 builds/month, unlimited requests, 100K Worker invocations/day. A single-user dashboard uses ~50–100
  requests/day — nowhere near the limits.
- Same CF account as the Analytics Engine data and license server — no cross-account concerns.
- CF Access for auth is more secure than a custom password gate and requires zero application code.

### Data fetching pattern

Each data source gets its own server-side module under `src/lib/server/sources/`:

```
src/lib/server/sources/
  umami.ts        — JWT auth, page stats, custom events
  posthog.ts      — Personal API key, session/event queries
  cloudflare.ts   — Analytics Engine SQL API
  paddle.ts       — Transactions, subscriptions, customers
  github.ts       — Release download counts
  license.ts      — Admin stats endpoint (activation counts, device counts)
```

**Why separate modules**: Each source has different auth, pagination, and error handling. Isolating them makes it easy
to add/remove sources without touching the dashboard UI.

Each module exports its own typed return type (not a shared generic interface — the data shapes are too different
across sources). The page server load function calls all sources in parallel and passes the typed results to the page
component.

**Caching**: CF Pages Functions are stateless (no in-memory cache across requests). Use the Cache API
(`caches.open('analytics-dashboard')`) to cache source responses. Cache keys are constructed as synthetic `Request`
URLs (for example, `https://cache/umami/stats?range=7d`) since the Cache API is request-based. TTL: 5 min for recent
data, 1 hour for older ranges. Note: `caches.default` and `caches.open()` are not available in `wrangler pages dev`
local development — use a simple in-memory Map as a fallback when running locally.

### Page structure

Single page. Sections for each acquisition stage, top to bottom. Each section has:
- A headline metric (big number + delta vs previous period)
- A trend chart (line chart over the selected time range)
- A breakdown table where relevant (by country, by referrer, by version, etc.)
- A "View in [source]" link to the original dashboard

### Deployment

Deploy to Cloudflare Pages via CI.

- **Adapter**: `@sveltejs/adapter-cloudflare` produces the `_worker.js` and `_routes.json` CF Pages expects
- **CI**: Create `.github/workflows/deploy-dashboard.yml` (separate workflow, matching the existing
  `deploy-website.yml` pattern). Triggers on push to `main` affecting `apps/analytics-dashboard/**`. Runs
  `wrangler pages deploy .svelte-kit/cloudflare --project-name=cmdr-analytics-dashboard`.
- **Auth**: Cloudflare Access policy on `analdash.getcmdr.com` — configured in CF dashboard, not in code
- **DNS**: Add `analdash.getcmdr.com` CNAME in Cloudflare, pointed at the Pages project
- **Secrets**: Set via `wrangler pages secret put` or in the CF Pages dashboard. All API keys live there, not in code.
- **Repo secret**: Add `CLOUDFLARE_API_TOKEN` to GitHub repo secrets (needs `Cloudflare Pages: Edit` permission
  added to the existing token)

## Milestones

### Milestone 0: Active user tracking via update check proxy

**Decision**: Route update checks through the license server to count all users (free + licensed).

**Why this is a separate milestone**: This requires changes to a production Cloudflare Worker and the desktop app's
update URL — a different risk profile from building a new read-only dashboard.

Changes needed:

1. **License server** — Add `GET /update-check/:version` route that:
   - Logs to a new Analytics Engine dataset `cmdr_update_checks` with: `indexes=[hashedIp]`,
     `blobs=[version, arch]`, `doubles=[1]`. The IP hash (for example, SHA-256 of IP + daily salt) enables
     deduplication without storing PII.
   - 302-redirects to `https://getcmdr.com/latest.json`
   - Must not break update checks on failure — if the Analytics Engine write fails, still redirect. The analytics
     logging should be fire-and-forget (same pattern as the existing download tracking).
2. **`wrangler.toml`** — Add the new Analytics Engine dataset binding (`UPDATE_CHECKS` → `cmdr_update_checks`,
   matching the naming pattern of `DOWNLOADS` → `cmdr_downloads`)
3. **Desktop app** — Change the update check URL from `https://getcmdr.com/latest.json` to
   `https://license.getcmdr.com/update-check/{current_version}`. This is in `apps/desktop/src-tauri/src/updater/`
   (`MANIFEST_URL` constant in `mod.rs`). Note: this is actually a reliability upgrade — CF Workers have higher
   availability than the single Hetzner VPS currently serving `latest.json`. The app already handles check failures
   gracefully (silent catch, retry at next interval, no error UI).
4. **Dashboard data source** — Add `cmdr_update_checks` to the Cloudflare Analytics Engine module (milestone 2)

**Testing**: Deploy to sandbox/staging first. Verify the redirect works and the update check flow is unaffected.
Verify Analytics Engine receives data points.

### Milestone 1: Project scaffolding + auth

- Add `Cloudflare Pages: Edit` permission to the existing `CLOUDFLARE_API_TOKEN` (needed for all Pages operations).
  Update `docs/tooling/cloudflare.md` to reflect the new permission.
- Scaffold with `pnpm create svelte@latest apps/analytics-dashboard` (skeleton project, TypeScript, Tailwind).
  Check the latest SvelteKit version on npm first — don't trust training data.
- Use `@sveltejs/adapter-cloudflare` (not `adapter-auto` or `adapter-node`). Set `compatibility_date` to match the
  license server's (`2025-01-01`) for consistency.
- Add to `pnpm-workspace.yaml` (already covered by `apps/*` glob — no change needed)
- Verify licenses of all new dependencies (uPlot, etc.) per AGENTS.md rules before adding
- Add `dev:dashboard` and `build:dashboard` scripts to root `package.json`
- Create `apps/analytics-dashboard/CLAUDE.md` with initial architecture notes
- Verify `pnpm dev:dashboard` starts and shows the page
- Set up Cloudflare Access policy for `analdash.getcmdr.com` in the CF dashboard

**Testing**: Manual verification that CF Access blocks unauthenticated requests and allows authenticated ones.

### Milestone 2: Data source modules

Build the server-side data fetching modules one by one. Each module:
- Reads credentials from env vars (never hardcoded)
- Exports a typed fetch function for its specific data shape
- Handles errors gracefully (source down → show "unavailable", don't break the page)
- Has unit tests for response parsing (mock the HTTP calls)

Order (easiest to hardest, so you get early wins):

1. **Umami** — Two website IDs within the same Umami instance (blog + getcmdr.com). JWT auth flow. Endpoints:
   `/api/auth/login`, `/api/websites/{id}/stats`, `/api/websites/{id}/metrics`. Custom events for download clicks.
2. **Cloudflare Analytics Engine** — SQL queries to `cmdr_downloads` and `cmdr_update_checks` datasets (verify names
   in `apps/license-server/wrangler.toml`). Bearer token auth. The existing `CLOUDFLARE_API_TOKEN` has
   `Account Analytics Read` permission (per `docs/tooling/cloudflare.md`), which covers the SQL query API.
3. **Paddle** — List transactions, subscriptions. Bearer token. Pagination via `?after=` cursor.
4. **GitHub** — `GET /repos/{owner}/{repo}/releases` for download counts. No auth needed for public repos (but add a
   token to avoid rate limits).
5. **PostHog** — Trends API for event counts. Personal API key. EU endpoint (`eu.posthog.com`, not `us.*`).
6. **License server** — `GET /admin/stats` for activation and device counts (depends on milestone 4).

**Testing**: Unit tests for each module's response parsing. Use realistic fixture data (sanitized real responses).
Integration tests are not practical here (would need real API credentials).

### Milestone 3: Dashboard UI

- Single-page layout with the acquisition stages as sections
- Global time range picker (24h / 7d / 30d)
- For each stage:
  - Headline metric card (big number + % change vs previous period)
  - Line chart for the trend over the selected range
  - Optional breakdown table (referrers, countries, versions)
  - "View in [Source]" link opening the relevant external dashboard
- Loading states: skeleton placeholders while data loads
- Error states: per-section "Couldn't load [source] data" with retry button
- Responsive but desktop-first — this is an internal tool viewed on a laptop

**Why single page**: With only six sections, separate pages add navigation overhead without benefit. Scroll is fine.

**Testing**: Manual verification of charts rendering, time range switching, error states.

### Milestone 4: License server admin endpoint (for activation counts)

Add a `GET /admin/stats` endpoint to the license server that returns:
- Total activations (count of keys in KV)
- Active devices count (from `cmdr_device_counts` Analytics Engine)
- Auth: add a dedicated `ADMIN_API_TOKEN` secret to the license server (don't reuse the Paddle webhook secrets for
  this — coupling admin auth to Paddle verification is fragile). Note: the existing `/admin/generate` endpoint
  currently uses the Paddle webhook secret for auth. Consider migrating it to `ADMIN_API_TOKEN` too for consistency,
  but this is optional and can be done later.

**Why a new endpoint**: KV has no bulk query/count API. The worker needs to enumerate or maintain a counter.
A simple approach: increment a KV counter on each `/activate` call. Read it in `/admin/stats`. Note: the counter
starts from zero when deployed, losing historical count. To initialize it, do a one-time KV key listing via the
Cloudflare API (outside the Worker) and set the counter to the current count.

**Testing**: Unit test for the endpoint. Manual test against sandbox environment.

### Milestone 5: Deployment + documentation

- Set up CF Pages project via `wrangler pages project create` or the CF dashboard
- Create `.github/workflows/deploy-dashboard.yml` (matching `deploy-website.yml` pattern):
  - Triggers on push to `main` affecting `apps/analytics-dashboard/**`
  - Runs `wrangler pages deploy .svelte-kit/cloudflare --project-name=cmdr-analytics-dashboard`
- Add `CLOUDFLARE_API_TOKEN` to GitHub repo secrets (permission was added in milestone 1)
- Set all env vars as Pages secrets via `wrangler pages secret put` or CF dashboard
- Configure custom domain `analdash.getcmdr.com` in CF Pages settings (auto-creates DNS CNAME)
- Verify CF Access policy is active and blocking unauthenticated traffic
- Update `apps/analytics-dashboard/CLAUDE.md` with:
  - Deployment instructions
  - Env var reference
  - Known caveat: CF Analytics Engine data only retained for 90 days — historical comparisons beyond that are
    not possible
- Update `docs/architecture.md` to include the new app
- Update `docs/tooling/cloudflare.md` to list the Pages project and updated token permissions
- Update `AGENTS.md` file structure section

**Why docs are here, not a separate milestone**: Per project conventions, documentation should stay in sync with code
changes, not be deferred to the end.

**Testing**: Verify all stages load with real data. Check that CF Access blocks unauthenticated requests.

## Env vars needed

All set as CF Pages secrets (via `wrangler pages secret put` or CF dashboard), not in code.

| Variable | Source | Notes |
|----------|--------|-------|
| `UMAMI_API_URL` | `https://anal.veszelovszki.com` | |
| `UMAMI_USERNAME` | Existing | |
| `UMAMI_PASSWORD` | Existing | |
| `UMAMI_WEBSITE_ID` | `5ea041ae-...` | getcmdr.com |
| `UMAMI_BLOG_WEBSITE_ID` | `3ee5c901-...` | Blog |
| `CLOUDFLARE_API_TOKEN` | Existing | Needs `Account Analytics Read` permission |
| `CLOUDFLARE_ACCOUNT_ID` | `6a4433bf...` | |
| `PADDLE_API_KEY_LIVE` | Existing | Always hit live API, not sandbox |
| `POSTHOG_API_KEY` | Existing `phx_...` personal key | Not the public `phc_...` project key |
| `POSTHOG_PROJECT_ID` | `136072` | |
| `POSTHOG_API_URL` | `https://eu.posthog.com` | Must be EU — `us.*` silently fails |
| `GITHUB_TOKEN` | Optional | For rate limits on public repo API |
| `LICENSE_SERVER_ADMIN_TOKEN` | New dedicated secret | Also set via `wrangler secret put` on the license server |

## What this plan does NOT include

- **Real-time data**: All data is fetched on page load with caching. No WebSockets or polling.
- **Alerts/notifications**: No Slack alerts or email digests. This is a pull-based dashboard.
- **Historical data storage**: No local database. All data comes from the source APIs, which have their own retention
  (Umami: unlimited, CF Analytics Engine: 90 days, Paddle: unlimited, PostHog: depends on plan). The 90-day CF limit
  must be documented in the dashboard's CLAUDE.md as a known caveat.
- **Cohort funnel tracking**: No individual user journey tracking across stages. Would require cross-site identity
  and a cookie banner. The dashboard shows independent aggregate metrics per stage.
- **CI/CD for the dashboard itself**: Auto-deploys from CI on push to `main`. No staging environment — the dashboard
  is internal and low-risk.

## Decisions made

1. **Active user tracking**: Route update checks through the license server (milestone 0). Covers all users.
2. **Domain**: `analdash.getcmdr.com`
3. **PostHog**: Include it — session/event counts via Trends API. Heatmap/scroll data stays in the PostHog UI only.
4. **CF Analytics Engine retention**: Accept the 90-day limit for v1. Document the caveat in the dashboard's CLAUDE.md.
5. **Deployment**: Cloudflare Pages, not Docker/Hetzner. Eliminates all container/VPS infrastructure.
6. **Auth**: Cloudflare Access (zero-trust), not a custom password gate.
7. **Naming**: "Metrics by acquisition stage," not "funnel" — the data is aggregate, not per-user cohort.
