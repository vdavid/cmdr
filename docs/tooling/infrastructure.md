# Infrastructure access

Production infrastructure for getcmdr.com and related services. This doc is for maintainers and agents that need to
interact with the production environment.

## VPS (Hetzner)

The production server is a Hetzner VPS. SSH access is configured in the maintainer's `~/.ssh/config` as `hetzner`.

```bash
ssh hetzner
```

### Layout

The repo is cloned at `/opt/cmdr`, owned by the `deploy-cmdr` user. The `david` user (SSH login) is in the
`deploy-cmdr` group and has write access.

```
/opt/cmdr/                          # git clone of this repo
├── apps/website/                   # Astro static site (Docker + nginx)
│   └── .env                        # PUBLIC_* vars baked into the static build
├── apps/website/docker-compose.yml # getcmdr-static container
└── infra/
    ├── listmonk/                   # Newsletter (Docker + Postgres)
    └── deploy-webhook/             # GitHub Actions deploy hook
```

Caddy runs as a reverse proxy in front of everything on the `proxy-net` Docker network.

### Common operations

```bash
# Read/edit the website env vars
cat /opt/cmdr/apps/website/.env
nano /opt/cmdr/apps/website/.env

# Manual website deploy (normally happens automatically via GitHub Actions webhook)
cd /opt/cmdr/apps/website
docker compose down && docker compose build --no-cache && docker compose up -d

# Check container status
docker ps
docker logs getcmdr-static

# Pull latest code
cd /opt/cmdr && git pull origin main
```

For full setup details, see [deploying the website](../guides/deploy-website.md) and the
[Listmonk README](../../infra/listmonk/README.md).

## Umami (website analytics)

Self-hosted at `https://anal.veszelovszki.com`. Cookieless, GDPR-friendly. Used for getcmdr.com analytics.

- **Dashboard**: https://anal.veszelovszki.com (login required)
- **getcmdr.com website ID**: `5ea041ae-b99d-4c31-b031-89c4a0005456`

### API access

The website's local `.env` file (at `apps/website/.env`) contains Umami API credentials:

```
UMAMI_API_URL=https://anal.veszelovszki.com
UMAMI_USERNAME=...
UMAMI_PASSWORD='...'    # single-quoted because it contains special chars
```

These are for scripts and API calls only — the website runtime uses `PUBLIC_UMAMI_HOST` and
`PUBLIC_UMAMI_WEBSITE_ID` instead.

**Authenticate** (returns a JWT token):

```bash
cd apps/website && set -a && source .env && set +a
TOKEN=$(curl -s -X POST "${UMAMI_API_URL}/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg u "$UMAMI_USERNAME" --arg p "$UMAMI_PASSWORD" '{username: $u, password: $p}')" \
  | jq -r '.token')
```

**List websites**:

```bash
curl -s "${UMAMI_API_URL}/api/websites" -H "Authorization: Bearer $TOKEN" | jq '.'
```

**Query stats** (for example, last 30 days):

```bash
START=$(($(date +%s) * 1000 - 30 * 86400000))
END=$(($(date +%s) * 1000))
curl -s "${UMAMI_API_URL}/api/websites/5ea041ae-b99d-4c31-b031-89c4a0005456/stats?startAt=${START}&endAt=${END}" \
  -H "Authorization: Bearer $TOKEN" | jq '.'
```

**Query events directly in the DB** (useful when the API shows zero but you need to verify):

```bash
ssh hetzner "docker exec umami-db psql -U umami -d umami -c \
  \"SELECT created_at, url_path FROM website_event \
   WHERE website_id = '5ea041ae-b99d-4c31-b031-89c4a0005456' \
   ORDER BY created_at DESC LIMIT 10;\""
```

**Full API docs**: https://umami.is/docs/api

### Troubleshooting

If events aren't recording:

1. **Check CSP headers**: The nginx CSP in `apps/website/nginx.conf` must allow `anal.veszelovszki.com` in both
   `script-src` (loads the script) and `connect-src` (sends events). Run
   `curl -sI https://getcmdr.com | grep content-security-policy` to verify.
2. **Check the Umami container**: `ssh hetzner "docker logs --tail 20 umami"`. The "Failed to find Server Action"
   errors are harmless (Next.js UI cache mismatch). Look for DB connection errors.
3. **Check the DB directly**: Use the psql query above. If the API returns zero but the DB has rows, it's a time
   range or timezone issue in the API query.
4. **Bot filtering**: Umami silently drops events with minimal `User-Agent` strings. Real browsers work fine. A
   `curl` test needs a full browser-like UA to be recorded.

## Download tracking (Cloudflare Analytics Engine)

The license server has a `GET /download/:version/:arch` endpoint that logs downloads to Cloudflare Analytics Engine
(dataset: `cmdr_downloads`) and 302-redirects to the GitHub Releases .dmg. The website routes download links through
this endpoint when `PUBLIC_DOWNLOAD_BASE_URL` is set.

**Data schema**: indexes=[version], blobs=[version, arch, country, continent], doubles=[1].

**Query downloads** via the [CF Analytics Engine SQL API](https://developers.cloudflare.com/analytics/analytics-engine/sql-api/).
Create an API token with `Account Analytics Read` permission, then:

```bash
curl -s "https://api.cloudflare.com/client/v4/accounts/{account_id}/analytics_engine/sql" \
  -H "Authorization: Bearer {api_token}" \
  -d "SELECT blob1 AS version, blob2 AS arch, blob3 AS country, SUM(_sample_interval) AS downloads
      FROM cmdr_downloads
      WHERE timestamp > NOW() - INTERVAL '30' DAY
      GROUP BY version, arch, country
      ORDER BY downloads DESC"
```

The dataset is created automatically on the first write — no setup needed beyond deploying the license server.

## Other services

| Service | Where | Access | Docs |
| --- | --- | --- | --- |
| **Cloudflare** (DNS, Workers) | cloudflare.com dashboard | `CLOUDFLARE_API_TOKEN` in `~/.zshenv` (see [CONTRIBUTING.md](../../CONTRIBUTING.md#cloudflare-access-license-server)) | [License server README](../../apps/license-server/README.md) |
| **Paddle** (payments) | vendors.paddle.com | Login required | — |
| **UptimeRobot** (monitoring) | uptimerobot.com | Login required | [monitoring.md](monitoring.md) (alerts, status page) |
| **Resend** (email) | resend.com | Login required | [Listmonk README](../../infra/listmonk/README.md) |
