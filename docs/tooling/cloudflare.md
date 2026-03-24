# Cloudflare (DNS, Workers, analytics)

DNS, Workers, and analytics for `getcmdr.com` run on Cloudflare. For API server deployment,
custom domain config, and troubleshooting, see the
[API server README](../../apps/api-server/README.md#deployment).

## API token

`CLOUDFLARE_API_TOKEN` lives in `~/.zshenv`. Wrangler picks it up automatically for deploys. See
[CONTRIBUTING.md](../../CONTRIBUTING.md#cloudflare-access-api-server) for setup.

**Account ID**: `6a4433bf11c3cf86feda057f76f47991`

**Gotcha**: The token is a scoped API token (not a global API key). It works with wrangler and the REST API, but
the Bash tool's subshell doesn't always inherit it from `~/.zshenv`. Read it from the file when calling the API
directly:

```bash
TOKEN=$(grep CLOUDFLARE_API_TOKEN ~/.zshenv | head -1 | sed 's/.*=//' | tr -d '"' | tr -d "'")
curl -s "https://api.cloudflare.com/client/v4/..." \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json"
```

**Token permissions** (as of 2026-03): Workers Scripts Edit, Workers KV Storage Edit, Workers Routes Edit,
Zone DNS Edit, Account Analytics Read, Cloudflare Pages Edit. If a deploy fails with a permissions error, check
https://dash.cloudflare.com/profile/api-tokens and add the missing permission. The token value doesn't change
when permissions are updated.

## Zones and Workers

| Zone | Zone ID |
| --- | --- |
| `getcmdr.com` | `3b1ddce127d21ce9802588dac5aee4e9` |
| `gitstrata.com` | `6265d396a0d0bf5c0b22e64a2b7777af` |

| Worker | Domain | Config |
| --- | --- | --- |
| `cmdr-license-server` | `api.getcmdr.com` (`license.getcmdr.com` is a legacy alias) | `apps/api-server/wrangler.toml` |

| Pages project | Domain | Notes |
| --- | --- | --- |
| `cmdr-analytics-dashboard` | `analdash.getcmdr.com` | SvelteKit dashboard, auth via CF Access. Token needs `Cloudflare Pages: Edit` permission. |

## Common API operations

```bash
# List DNS records for a zone
curl -s "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records" \
  -H "Authorization: Bearer ${TOKEN}" | jq '.result[] | {id, type, name, content, proxied}'

# Delete a DNS record
curl -s -X DELETE "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{record_id}" \
  -H "Authorization: Bearer ${TOKEN}"

# List Worker custom domains
curl -s "https://api.cloudflare.com/client/v4/accounts/6a4433bf11c3cf86feda057f76f47991/workers/domains" \
  -H "Authorization: Bearer ${TOKEN}" | jq '.result[]'
```

## Telemetry (D1)

Downloads, update checks, and crash reports are stored in D1 (database: `cmdr-telemetry`). The only remaining Analytics
Engine dataset is `DEVICE_COUNTS` for fair-use device monitoring.

**Query via admin endpoints** (authenticated with `ADMIN_API_TOKEN`):

```bash
# Downloads (ranges: 24h, 7d, 30d, all)
curl -s "https://api.getcmdr.com/admin/downloads?range=30d" \
  -H "Authorization: Bearer ${ADMIN_API_TOKEN}"

# Active users (ranges: 7d, 30d, 90d, all)
curl -s "https://api.getcmdr.com/admin/active-users?range=30d" \
  -H "Authorization: Bearer ${ADMIN_API_TOKEN}"

# Crash reports (ranges: 7d, 30d, 90d, all)
curl -s "https://api.getcmdr.com/admin/crashes?range=30d" \
  -H "Authorization: Bearer ${ADMIN_API_TOKEN}"
```

The analytics dashboard fetches from these endpoints automatically.
