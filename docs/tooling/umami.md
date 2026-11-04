# Umami (Cmdr-specific)

Cookieless website analytics for getcmdr.com.

- **getcmdr.com website ID**: `5ea041ae-b99d-4c31-b031-89c4a0005456`

## Credentials

The website's local `.env` file (at `apps/website/.env`) contains Umami API credentials:

```
UMAMI_API_URL=https://anal.veszelovszki.com
UMAMI_USERNAME=...
UMAMI_PASSWORD='...'    # single-quoted because it contains special chars
```

These are for scripts and API calls only; the website runtime uses `PUBLIC_UMAMI_HOST` and `PUBLIC_UMAMI_WEBSITE_ID`
instead.

## Query events directly in the DB

Useful when the API shows zero but you need to verify:

```bash
ssh hetzner "docker exec umami-db psql -U umami -d umami -c \
  \"SELECT created_at, url_path FROM website_event \
   WHERE website_id = '5ea041ae-b99d-4c31-b031-89c4a0005456' \
   ORDER BY created_at DESC LIMIT 10;\""
```

## Gotchas

- **First-party proxy, so the CSP needs nothing for Umami**: the site loads the tracker from `/u/mami` and posts events
  to `/u/api/send`, both on getcmdr.com (`PUBLIC_UMAMI_HOST=/u`). Caddy's `getcmdr.com` block
  (`hetzner/services/caddy/Caddyfile` in the `infra` repo) rewrites `/u/mami` to Umami's `/script.js` and strips `/u`
  from everything else, so `'self'` in `apps/website/nginx-security-headers.conf` covers both. Don't add
  `anal.veszelovszki.com` back. Verify with `curl -s -o /dev/null -w '%{http_code}\n' https://getcmdr.com/u/mami`
  (expect 200; verified 2026-11-04 alongside the live CSP header).
- **`TRACKER_SCRIPT_NAME` does nothing here**: it's a build-time variable, so the prebuilt image ignores it (and some
  values blank the UI in v3). The obscured `/mami` name comes from the Caddy rewrite above.

## Changing Umami config

Umami's `docker-compose.yml` lives in the `infra` repo (`~/projects-git/vdavid/infra`) at `hetzner/services/umami/`,
whose `CLAUDE.md` covers upgrades and retention; the `/u/` proxy lives in `hetzner/services/caddy/Caddyfile`. The box
deploys from that repo's checkout at `~/infra`. **Never edit files directly on the server.** The process:

1. Edit the file in the local `infra` repo.
2. Commit and push.
3. On the server: `ssh hetzner "cd ~/infra && git pull"`.
4. Restart Umami: `ssh hetzner "cd ~/infra/hetzner/services/umami && docker compose up -d"`. For a Caddyfile change, run
   `docker compose restart caddy` in `~/infra/hetzner/services/caddy` instead (the infra repo's Hetzner host doc says
   why a reload isn't enough).
