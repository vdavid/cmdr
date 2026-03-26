# PostHog (website behavior tracking)

Cloud-hosted on the **EU instance** at `https://eu.posthog.com`. Used for session replay, heatmaps, and click tracking
on getcmdr.com (not the desktop app — that has no analytics). Free tier: 5K session replays/month, 1M events/month.

- **Dashboard**: https://eu.posthog.com/project/136072
- **Project settings**: https://eu.posthog.com/project/136072/settings/project-details
- **Ingest host**: `https://eu.i.posthog.com` (not `us` — the project is on the EU instance)
- **Project API key** (`phc_...`): public token baked into the website build via `PUBLIC_POSTHOG_KEY`. Safe to include
  in client-side code — PostHog designed it this way.

## API access

A personal API key (`phx_...`) is needed for the management API. It's stored in macOS Keychain.

```bash
POSTHOG_API_KEY=$(security find-generic-password -a "$USER" -s "POSTHOG_API_KEY" -w)

# Get project settings
curl -s "https://eu.posthog.com/api/projects/136072/" \
  -H "Authorization: Bearer ${POSTHOG_API_KEY}" | jq '{name, session_recording_opt_in, heatmaps_opt_in, recording_domains}'

# Update project settings (for example, add an authorized domain)
curl -s -X PATCH "https://eu.posthog.com/api/projects/136072/" \
  -H "Authorization: Bearer ${POSTHOG_API_KEY}" \
  -H "Content-Type: application/json" \
  -d '{"recording_domains": ["https://getcmdr.com"]}'
```

**Gotcha**: The project is on `eu.posthog.com`. API calls go to `eu.posthog.com/api/...`, and the JS SDK ingest
host is `eu.i.posthog.com`. Using `us.*` will silently fail (auth error or events go nowhere).
