# Analytics dashboard

Private SvelteKit dashboard at `https://analdash.getcmdr.com`. Consolidates metrics from Umami, Cloudflare, Paddle,
GitHub, PostHog, and the license server into a single page organized by acquisition stage.

- **Code**: `apps/analytics-dashboard/`
- **Architecture docs**: `apps/analytics-dashboard/CLAUDE.md`

## Agent-readable report

The `/api/report` endpoint returns a plain-text report with all dashboard data, including breakdowns that are only
visible via hover tooltips on the visual dashboard. This is the recommended way for agents to read analytics data.

```
GET /api/report?range=7d
```

Accepts `range` query param: `24h`, `7d` (default), `30d`.

The report includes:
- **Awareness**: page views, visitors, referrers (with percentages and deltas vs prior period)
- **Interest**: product page views, bounce rate, download button clicks, top pages, visitors by country
- **Download**: totals by version, architecture, and country; daily breakdown; cross-breakdowns (country × architecture,
  country × version, daily × version)
- **Active use**: update check counts by version, license activations, active devices
- **Payment**: revenue, transactions, active subscriptions
- **Retention**: churn rate, subscriptions by status

### Fetching the report

The dashboard is behind Cloudflare Access. Authenticate with the service token stored in `~/.zshenv`:

```bash
curl -s "https://analdash.getcmdr.com/api/report?range=7d" \
  -H "CF-Access-Client-Id: ${CF_ACCESS_CLIENT_ID}" \
  -H "CF-Access-Client-Secret: ${CF_ACCESS_CLIENT_SECRET}"
```

**Gotcha**: The Bash tool's subshell doesn't always inherit env vars from `~/.zshenv`. Read them from the file:

```bash
CF_ID=$(grep CF_ACCESS_CLIENT_ID ~/.zshenv | head -1 | sed 's/.*=//' | tr -d '"')
CF_SECRET=$(grep CF_ACCESS_CLIENT_SECRET ~/.zshenv | head -1 | sed 's/.*=//' | tr -d '"')
curl -s "https://analdash.getcmdr.com/api/report?range=7d" \
  -H "CF-Access-Client-Id: ${CF_ID}" \
  -H "CF-Access-Client-Secret: ${CF_SECRET}"
```

The service token expires 2027-03-22. See [cloudflare.md](cloudflare.md) for token management.

## Running locally

1. Copy `apps/analytics-dashboard/.env.example` to `.env` and fill in real values.
2. `pnpm dev:dashboard` from the repo root.
3. Dashboard at `http://localhost:4830`, report at `http://localhost:4830/api/report` (no auth needed locally).
