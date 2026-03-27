# PostHog (Cmdr-specific)

Used for session replay, heatmaps, and click tracking on getcmdr.com (not the desktop app — that has no analytics).

- **Project**: https://eu.posthog.com/project/136072
- **Project settings**: https://eu.posthog.com/project/136072/settings/project-details
- **Project API key** (`phc_...`): public token baked into the website build via `PUBLIC_POSTHOG_KEY`. Safe to include
  in client-side code — PostHog designed it this way.
