# PostHog (Cmdr-specific)

Used for session replay, heatmaps, and click tracking on getcmdr.com, and for anonymous desktop-app feature events (beta
usage analytics).

- **Project**: https://eu.posthog.com/project/136072
- **Project settings**: https://eu.posthog.com/project/136072/settings/project-details
- **Host**: EU cloud (`https://eu.i.posthog.com`). The desktop app posts events to `.../capture/`.
- **Project API key** (`phc_...`): public ingest token. Safe to include in client-side code (PostHog designed it this
  way). The website bakes it via `PUBLIC_POSTHOG_KEY`.

## Desktop feature events

The desktop app captures anonymous, PII-free product events (the beta usage analytics). They ride the same consent gate
and `anal_` install id as the heartbeat, and carry the allowlisted config-shape as `$set` person properties. Desktop
events carry `source: "desktop"` so the dashboard can split them from website events. Architecture and the event list:
`apps/desktop/src-tauri/src/analytics/CLAUDE.md`.

- **Key mechanism**: the same public `phc_` key, baked into the desktop build via `option_env!("CMDR_POSTHOG_KEY")`.
  It's set as a GitHub secret on the `tauri-action` step in `.github/workflows/release.yml`. Local dev builds have no
  key (`option_env!` → `None`), so desktop feature events no-op in dev (and are suppressed in dev/CI regardless).
