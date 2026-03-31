# Analytics dashboard

Private SvelteKit dashboard that consolidates all Cmdr business metrics into a single view. Deployed to Cloudflare Pages
at `analdash.getcmdr.com`.

## Running locally

1. Copy the example env file and fill in real values:
   ```bash
   cp .env.example .env
   ```
2. Install dependencies (from the repo root):
   ```bash
   pnpm install
   ```
3. Start the dev server:
   ```bash
   pnpm dev:dashboard
   ```
4. Open [http://localhost:4830](http://localhost:4830).

The dashboard works without env vars too, but each data source will show a "not configured" message instead of live
data. Fill in only the sources you need.

## Deployment

Auto-deploys to Cloudflare Pages on push to `main` when files in `apps/analytics-dashboard/` change. The workflow lives
at `.github/workflows/deploy-dashboard.yml`.

Env vars are set as CF Pages secrets (via `wrangler pages secret put` or the CF dashboard). Auth is handled by
Cloudflare Access (zero-trust policy on `analdash.getcmdr.com`), so there's no auth code in the app.

## Architecture

### Data flow

The single page (`+page.server.ts`) calls all six data sources in parallel on every request. Each source module lives
under `src/lib/server/sources/` and returns a typed `SourceResult<T>` (either data or an error string). Results are
cached via the CF Cache API in production (in-memory Map fallback for local dev) with a 5-minute TTL for 24h/7d ranges
and 1-hour TTL for 30d.

### Data sources

| Source                      | Module          | What it provides                                                                 |
| --------------------------- | --------------- | -------------------------------------------------------------------------------- |
| Umami                       | `umami.ts`      | Page views, visitors, referrers, countries, download events (blog + getcmdr.com) |
| Cloudflare Analytics Engine | `cloudflare.ts` | Download counts, update check counts by version/arch/country                     |
| Paddle                      | `paddle.ts`     | Completed transactions, subscriptions by status                                  |
| GitHub                      | `github.ts`     | Release download counts per asset (cumulative, not time-ranged)                  |
| PostHog                     | `posthog.ts`    | Pageview trends via the Trends API                                               |
| License server              | `license.ts`    | Total activations, active devices                                                |

### Auth

Cloudflare Access enforces authentication before requests reach the app. There's no login page or session management in
the codebase.

## Env var reference

| Variable                     | Required | Description                                              | Where to find it                             |
| ---------------------------- | -------- | -------------------------------------------------------- | -------------------------------------------- |
| `UMAMI_API_URL`              | Yes      | Umami instance URL                                       | Your Umami deployment                        |
| `UMAMI_USERNAME`             | Yes      | Umami login username                                     | Umami admin panel                            |
| `UMAMI_PASSWORD`             | Yes      | Umami login password                                     | Umami admin panel                            |
| `UMAMI_WEBSITE_ID`           | Yes      | getcmdr.com website UUID                                 | Umami > Websites                             |
| `UMAMI_BLOG_WEBSITE_ID`      | Yes      | Blog website UUID                                        | Umami > Websites                             |
| `PADDLE_API_KEY_LIVE`        | Yes      | Live API key (not sandbox)                               | Paddle > Developer Tools > API Keys          |
| `POSTHOG_API_KEY`            | Yes      | Personal API key (`phx_...`), not the public project key | PostHog > Settings > Personal API Keys       |
| `POSTHOG_PROJECT_ID`         | Yes      | PostHog project ID                                       | PostHog > Settings > Project                 |
| `POSTHOG_API_URL`            | Yes      | Must be `https://eu.posthog.com` for EU projects         | PostHog region setting                       |
| `GITHUB_TOKEN`               | No       | Avoids GitHub API rate limits on public repo endpoints   | GitHub > Settings > Developer Settings > PAT |
| `LICENSE_SERVER_ADMIN_TOKEN` | Yes      | Admin secret for the API server `/admin/stats` endpoint  | API server config                            |

## Further reading

- [CLAUDE.md](CLAUDE.md) (architecture decisions, gotchas, and AI agent context)
