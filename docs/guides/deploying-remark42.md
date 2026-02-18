# Deploying Remark42

Remark42 provides blog comments at `comments.getcmdr.com`. It runs as a Docker container alongside the website.

## Prerequisites

- Docker and Docker Compose on the server
- The `proxy-net` Docker network (created during website deployment)
- Caddy (or another reverse proxy) for TLS termination
- OAuth credentials for at least one provider (GitHub, Google)

## Configuration

### 1. Set environment variables

On the server, create or update `.env` in `apps/website/`:

```
REMARK42_SECRET=<random-secret-for-signing>
AUTH_GITHUB_CID=<github-oauth-app-client-id>
AUTH_GITHUB_CSEC=<github-oauth-app-client-secret>
AUTH_GOOGLE_CID=<google-oauth-client-id>
AUTH_GOOGLE_CSEC=<google-oauth-client-secret>
```

Generate the secret with `openssl rand -hex 32`.

### 2. Start the container

```bash
cd /opt/cmdr/apps/website
docker compose up -d remark42
```

### 3. Add Caddy route

In your Caddyfile, add:

```
comments.getcmdr.com {
    reverse_proxy remark42:8080
}
```

Then reload Caddy: `docker compose restart caddy` (from the Caddy directory).

## Verification

```bash
# Check the container is running
docker ps | grep remark42

# Test the endpoint
curl -s https://comments.getcmdr.com/ping
# Should return "pong"
```

## Data

Remark42 data is stored in the `remark42-data` Docker volume at `/srv/var` inside the container. Back up this
volume regularly.
