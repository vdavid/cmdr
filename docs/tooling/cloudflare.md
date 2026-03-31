# Cloudflare (Cmdr-specific)

DNS, Workers, and analytics for Cmdr run on Cloudflare. For API server deployment, custom domain config, and
troubleshooting, see the [API server README](../../apps/api-server/README.md#deployment).

## Zones and Workers

| Zone            | Zone ID                            |
| --------------- | ---------------------------------- |
| `getcmdr.com`   | `3b1ddce127d21ce9802588dac5aee4e9` |
| `gitstrata.com` | `6265d396a0d0bf5c0b22e64a2b7777af` |

| Worker                | Domain                                                      | Config                          |
| --------------------- | ----------------------------------------------------------- | ------------------------------- |
| `cmdr-license-server` | `api.getcmdr.com` (`license.getcmdr.com` is a legacy alias) | `apps/api-server/wrangler.toml` |

| Pages project              | Domain                 | Notes                                                                                     |
| -------------------------- | ---------------------- | ----------------------------------------------------------------------------------------- |
| `cmdr-analytics-dashboard` | `analdash.getcmdr.com` | SvelteKit dashboard, auth via CF Access. Token needs `Cloudflare Pages: Edit` permission. |

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
