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

**Full API docs**: https://umami.is/docs/api

## Other services

| Service | Where | Access | Docs |
| --- | --- | --- | --- |
| **Cloudflare** (DNS, Workers) | cloudflare.com dashboard | Login required | [License server README](../../apps/license-server/README.md) |
| **Paddle** (payments) | vendors.paddle.com | Login required | — |
| **UptimeRobot** (monitoring) | uptimerobot.com | Login required | [monitoring.md](monitoring.md) (alerts, status page) |
| **Resend** (email) | resend.com | Login required | [Listmonk README](../../infra/listmonk/README.md) |
