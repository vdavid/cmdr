# Hetzner VPS (Cmdr-specific)

The Cmdr repo is cloned at `/opt/cmdr` on the Hetzner VPS, owned by the `deploy-cmdr` user. The `david` user (SSH login)
is in the `deploy-cmdr` group and has write access.

```
/opt/cmdr/                          # git clone of this repo
├── apps/website/                   # Astro static site (Docker + nginx)
│   └── .env                        # PUBLIC_* vars baked into the static build
├── apps/website/docker-compose.yml # getcmdr-static container
└── infra/
    ├── listmonk/                   # Newsletter (Docker + Postgres)
    └── deploy-webhook/             # GitHub Actions deploy hook
```

## Common operations

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
cd /opt/cmdr && git fetch origin main && git reset --hard origin/main
```

For full setup details, see [deploying the website](../guides/deploy-website.md) and the
[Listmonk README](../../infra/listmonk/README.md).
