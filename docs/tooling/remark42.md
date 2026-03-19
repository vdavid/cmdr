# Remark42 (comments)

Self-hosted commenting engine (v1.15.0). Runs as a Docker container alongside the website on the Hetzner VPS.

- **Host URL**: https://comments.getcmdr.com
- **Docker image**: `umputun/remark42:v1.15.0`
- **Container name**: `remark42`
- **Docker Compose**: `apps/website/docker-compose.yml`

## Sites served

| Site ID | Website | Remark42 host |
| --- | --- | --- |
| `getcmdr` | getcmdr.com blog | `comments.getcmdr.com` (this instance) |

To add a site, append its ID to the `SITE=` env var (comma-separated) and restart the container.

## Infrastructure

- **DNS**: A record `comments.getcmdr.com` → `37.27.245.171` (Cloudflare, NOT proxied)
- **Caddy route**: `comments.getcmdr.com { reverse_proxy remark42:8080 }` (in `hetzner-server` repo)
- **Data**: Docker volume `remark42-data` mounted at `/srv/var` inside the container

## Required secrets

Stored on the server at `apps/website/.env`:

| Variable | Purpose |
| --- | --- |
| `REMARK42_SECRET` | Signing secret. Generate with `openssl rand -hex 32` |
| `AUTH_GITHUB_CID` | GitHub OAuth app client ID |
| `AUTH_GITHUB_CSEC` | GitHub OAuth app client secret |
| `AUTH_GOOGLE_CID` | Google OAuth app client ID |
| `AUTH_GOOGLE_CSEC` | Google OAuth app client secret |

## OAuth callback URLs

- **GitHub**: `https://comments.getcmdr.com/auth/github/callback`
- **Google**: `https://comments.getcmdr.com/auth/google/callback`

These must match exactly in the OAuth app settings on GitHub / Google Cloud Console.

## Common operations

```bash
# Start the container
docker compose up -d remark42

# Health check (expect "pong")
curl -s https://comments.getcmdr.com/ping

# View logs
docker logs remark42

# Add a new site: append to the SITE= env var (comma-separated), then restart
docker compose up -d remark42
```

## Related files

- [`apps/website/src/components/Remark42Comments.astro`](../../apps/website/src/components/Remark42Comments.astro) — Astro component that embeds the comment widget
- [`docs/guides/deploying-remark42.md`](../guides/deploying-remark42.md) — Step-by-step deployment guide
