# Less network chatter: throttled heartbeat, event relay, 3-hour update check

Tracks getcmdr/cmdr-reports#22. Why: direct calls to a third-party domain (PostHog) raise eyebrows with people who watch
their network traffic, and hourly beats and update checks are more chatter than Cmdr needs. Part of making Cmdr friendly
to company policies (#118).

## Decisions (settled by David)

- **Relay first.** The desktop app stops calling PostHog. Feature events ride the heartbeat to `api.getcmdr.com`; the
  Worker stores them in D1 AND forwards them to PostHog. Dropping PostHog later must be "delete one server-side
  forward", so the forward lives in one isolated module.
- **Spool to disk.** Events accumulate in an append-only JSONL spool in the app data dir, survive crashes and quits, and
  get replayed on the next beat. This also fixes "offline means lost".
- **Engagement:** the beat carries uptime seconds, and analdash normalizes the old row-count series into the same unit.
- **Geo-IP off** for relayed events (`$geoip_disable: true`); we don't forward the caller IP.
- **Separate pipelines stay separate.** Update check and heartbeat both move to 3 hours but never merge: an opted-out
  install still checks for updates.
- The privacy policy stays as is (it keeps naming PostHog).

## Throttle semantics (heartbeat and update check)

- At most one successful send per 3 hours. A burst of triggers (relaunch, wake) collapses into one send and never pushes
  the next send further out (throttle, not debounce).
- The timestamp of the last successful send is persisted, so relaunches don't reset it.
- A failed send retries no more often than every 15 minutes.
- The loop wakes on a short tick (a few minutes) and checks "is it due?"; it doesn't sleep a fixed 3 h.

## Wire contract: `POST /heartbeat`

Existing fields unchanged (`analId`, `appVersion`, `osVersion`, `arch`, `buildMode`, `config`). New, both optional so
old clients (hourly, no events) keep working for as long as they exist:

- `uptimeSeconds`: integer ≥ 0. App runtime this beat accounts for, not yet reported by an earlier successful beat.
  Unreported uptime is persisted (updated every few minutes), so a short session that ended before its beat still counts
  on the next one. Server clamps to a sane max (7 days) and stores it.
- `events`: array, max 500 items. Each item:
  - `event`: string, 1–100 chars, `^[a-z0-9_$]+$`-ish (match what the app sends today).
  - `timestamp`: RFC 3339 UTC string, the moment the event fired on the client.
  - `id`: optional lowercase hyphenated v4 UUID, minted by the client at append time. A beat stored but whose response
    got lost is retried, so the server ignores an event whose id it already has (unique index, `INSERT OR IGNORE`) and
    forwards it to PostHog as the event's `uuid`, which PostHog dedupes on too.
  - `appVersion`: optional semver string, the app version that produced the event. Events spooled under one release can
    ship after an update, so the server prefers this over the beat's `appVersion` (falls back to the beat's when absent
    or invalid) for both the D1 row and the forward.
  - `properties`: JSON object (may be empty). Only the event's own properties: the server adds identity (distinct id
    from `analId`, OS, arch, the app version above, and `source: "desktop"`, which the dashboard uses to split desktop
    from website events) and the config snapshot when forwarding, so the client doesn't repeat them.

Limits: request body cap goes from 32 KB to 256 KB; config blob cap stays 16 KB. Past 500 events the server keeps the
first 500 and counts the rest as dropped (logged). The client never sends more than 500 per beat; the rest stay in the
spool for the next beat. The client caps the spool (oldest dropped first) so a long-offline install can't grow it
without bound.

A beat is acknowledged with 2xx only after the heartbeat row AND the events are in D1. The PostHog forward runs in
`waitUntil` and its failure never fails the beat. The client truncates the spool only up to what that 2xx covered.

## Server storage

- `heartbeat.uptime_seconds INTEGER` (nullable; old rows and old clients stay null).
- New `analytics_event` table: `anal_id`, `event`, `occurred_at` (client timestamp), `received_at`, `app_version`,
  `properties_json`. Indexed for per-day and per-event queries. No IP.

## Engagement metric

App hours per day = `SUM(COALESCE(uptime_seconds, 3600)) / 3600`. An old-client row stands for one hourly beat, so the
old and new series join in one unit. DAU stays `COUNT(DISTINCT anal_id)`.

## Out of scope (follow-ups)

- An analdash source reading `analytics_event` from D1, then deleting the PostHog forward.
- The website's in-browser PostHog (session recordings, heatmaps).
