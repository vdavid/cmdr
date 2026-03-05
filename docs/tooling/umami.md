# Umami (website analytics)

Self-hosted at `https://anal.veszelovszki.com`. Cookieless, GDPR-friendly. Used for getcmdr.com analytics.

- **Dashboard**: https://anal.veszelovszki.com (login required)
- **getcmdr.com website ID**: `5ea041ae-b99d-4c31-b031-89c4a0005456`

## API access

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

## Troubleshooting

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
