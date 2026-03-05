# Cloudflare (DNS, Workers, analytics)

DNS, Workers, and analytics for `getcmdr.com` run on Cloudflare. For license-server-specific deployment,
custom domain config, and troubleshooting, see the
[license server README](../../apps/license-server/README.md#deployment).

## API token

`CLOUDFLARE_API_TOKEN` lives in `~/.zshenv`. Wrangler picks it up automatically for deploys. See
[CONTRIBUTING.md](../../CONTRIBUTING.md#cloudflare-access-license-server) for setup.

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
Zone DNS Edit, Account Analytics Read. If a deploy fails with a permissions error, check
https://dash.cloudflare.com/profile/api-tokens and add the missing permission. The token value doesn't change
when permissions are updated.

## Zones and Workers

| Zone | Zone ID |
| --- | --- |
| `getcmdr.com` | `3b1ddce127d21ce9802588dac5aee4e9` |
| `gitstrata.com` | `6265d396a0d0bf5c0b22e64a2b7777af` |

| Worker | Domain | Config |
| --- | --- | --- |
| `cmdr-license-server` | `license.getcmdr.com` | `apps/license-server/wrangler.toml` |

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

## Download tracking (Analytics Engine)

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
