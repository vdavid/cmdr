# Remark42 (comments)

Self-hosted commenting engine (v1.15.0). Runs as a Docker container alongside the website on the Hetzner VPS.

- **Host URL**: https://comments.getcmdr.com
- **Docker image**: `umputun/remark42:v1.15.0`
- **Container name**: `remark42`
- **Docker Compose**: `apps/website/docker-compose.yml`

**Gotcha**: a second Remark42 container, `remark42-blog`, runs on the same box for David's personal blog
(`comments.veszelovszki.com`, `SITE=vdavid-blog`, its own OAuth clients). It's deployed from the `infra` repo at
`hetzner/services/remark42-blog/` and has nothing to do with Cmdr. Match the container name exactly before restarting
or reading env.

## Sites served

| Site ID   | Website          | Remark42 host                          |
| --------- | ---------------- | -------------------------------------- |
| `getcmdr` | getcmdr.com blog | `comments.getcmdr.com` (this instance) |

To add a site, append its ID to the `SITE=` env var (comma-separated) and restart the container.

## Infrastructure

- **DNS**: A record `comments.getcmdr.com` → `37.27.245.171` (Cloudflare, NOT proxied)
- **Caddy route**: `comments.getcmdr.com { reverse_proxy remark42:8080 }` (in `hetzner-server` repo)
- **Data**: Docker volume `remark42-data` mounted at `/srv/var` inside the container

## Required secrets

Stored on the server at `apps/website/.env`:

- **`REMARK42_SECRET`**: Signing secret. Generate with `openssl rand -hex 32`
- **`AUTH_GITHUB_CID`**: GitHub OAuth app client ID
- **`AUTH_GITHUB_CSEC`**: GitHub OAuth app client secret
- **`AUTH_GOOGLE_CID`**: Google OAuth app client ID
- **`AUTH_GOOGLE_CSEC`**: Google OAuth app client secret

## OAuth callback URLs

- **GitHub**: `https://comments.getcmdr.com/auth/github/callback`
- **Google**: `https://comments.getcmdr.com/auth/google/callback`

These must match exactly in the OAuth app settings on GitHub / Google Cloud Console.

## OAuth clients

- **Google**: client named `Cmdr Remark42` in Google Cloud project `gen-lang-client-0179352958` (the auto-created
  Gemini API project). Manage it under Google Auth Platform → Clients.
- **GitHub**: OAuth app `Ov23lihhp2lm7WROt4VX`.

**Guardrail**: Google auto-deletes an OAuth client after six months with no token request and no settings change, which
would silently break the Google sign-in button on the blog (GitHub login would keep working, so the outage is easy to
miss). Traffic here is low enough that the client can idle past the limit on its own. Either sign in with Google on the
blog occasionally, or rename the client / rotate its secret in the console; both count as activity and reset the clock.
Google emails a warning first, and a deleted client is restorable for ~30 days under Clients → "Restore deleted OAuth
clients". (Verified in the Google Cloud console, 2026-08-27.)

Google supports two live secrets per client, so rotate without downtime: add a secret, update `AUTH_GOOGLE_CSEC` in the
server `.env`, `docker compose up -d remark42`, confirm a real Google sign-in works, then delete the old secret.

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

- `apps/website/src/components/Remark42Comments.astro`: Astro component that embeds the comment widget
- `../guides/deploying-remark42.md`: Step-by-step deployment guide
