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

These are for scripts and API calls only — the website runtime uses `PUBLIC_UMAMI_HOST` and `PUBLIC_UMAMI_WEBSITE_ID`
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

- **CSP headers**: The nginx CSP in `apps/website/nginx.conf` must allow `anal.veszelovszki.com` in both `script-src`
  (loads the script) and `connect-src` (sends events). Run `curl -sI https://getcmdr.com | grep content-security-policy`
  to verify.
- **`TRACKER_SCRIPT_NAME`**: Set to `mami` in the Umami `docker-compose.yml`. The script is served at `/mami` (no `.js`
  extension — Umami uses the value literally as the path). Short names like `s` break the Umami API because Next.js
  middleware matches any URL path containing the tracker name and rewrites it to serve the script. Keep the name
  distinctive.

## Changing Umami config

Umami's `docker-compose.yml` and Caddy config live in the **`hetzner-server`** repo. **Never edit files directly on the
server.** The process:

1. Edit the file in the local `hetzner-server` repo
2. Commit and push
3. On the server: `ssh hetzner "cd ~/hetzner-server && git pull"`
4. Restart: `ssh hetzner "cd ~/hetzner-server/umami && docker compose down && docker compose up -d"`
