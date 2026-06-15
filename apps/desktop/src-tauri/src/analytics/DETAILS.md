# Analytics: details

Depth behind the must-knows in `CLAUDE.md`.

## Why two ids (the GDPR reasoning)

If two datasets *can* be joined, treat them as joined, so we make them genuinely unjoinable. With a separate `diag_` id,
an attached email links only to the diagnostics stream; the analytics stream stays unjoinable to any identity. The
`anal_`/`diag_` prefixes make the ids self-identifying in payloads, PostHog, and the D1 tables.

## Heartbeat payload

`HeartbeatPayload` (camelCase on the wire, `Option::None` → `null`) matches the Worker's validator:

- `analId` (required): `anal_` + lowercase hyphenated v4 UUID, `^anal_[0-9a-f-]{36}$`.
- `appVersion` (required): semver from `CARGO_PKG_VERSION`.
- `osVersion` (required): from `crate::platform::os_version()`, always non-empty.
- `arch` (required): `std::env::consts::ARCH`.
- `buildMode` (optional): `"release"` / `"debug"`.
- `config` (optional): the config-shape object, verbatim.

Fire-and-forget POST mirroring the crash/error reporters (10 s timeout, errors logged at debug, next hourly tick
retries). Endpoint: `http://localhost:8787/heartbeat` (debug) / `https://api.getcmdr.com/heartbeat` (release).

## PostHog `/capture/` body and key mechanism

`posthog::capture(event, props)` builds the body and fire-and-forget POSTs to `https://eu.i.posthog.com/capture/` (EU
cloud, project `136072`). Shape:

```json
{ "api_key": "phc_...", "event": "<name>", "distinct_id": "anal_<uuid>",
  "properties": { "source": "desktop", ...props }, "$set": <config-shape> }
```

- **`$set` is the config-shape verbatim**: person properties reuse `config_shape::build_config_shape` (same allowlisted
  object the heartbeat ships), so there's one source of truth and no second PII surface.
- **`source: "desktop"`** is injected first and can't be shadowed by a caller `source` prop, so the dashboard always
  splits desktop events from website events.
- **The key is `option_env!("CMDR_POSTHOG_KEY")`**, baked at build time (a GitHub secret on the `tauri-action` step in
  `release.yml`; `build.rs` has a `rerun-if-env-changed` for it). `None` locally → `capture` is a no-op (logged once at
  debug). The key is public by design (PostHog ingest keys are safe in client code).

## How to add an event

Open set, no enum, no schema:

- **Backend event**: at the success chokepoint,
  `crate::analytics::posthog::capture("my_event", serde_json::json!({ "kind": some_enum }))`.
- **Frontend event**: `import { trackEvent } from '$lib/tauri-commands'`, then `void trackEvent('my_event', { kind: someEnum })`.
- Name internals after the UI; keep props categorical.

## Starter event set (PII-free; grows over time)

Backend events fire at success chokepoints; frontend events ride `track_event`.

- `app_launched` (backend, `lib.rs` setup): no props.
- `pane_navigated` (frontend, `FilePane.svelte` `handleListingComplete`): `volume_kind` enum
  (`local`/`smb`/`mtp`/`network`/`search-results`); never the path.
- `search_used` (frontend, `SearchDialog.svelte` `runSearch`): `mode` enum; never the query.
- `select_files_used` (frontend, `SelectionDialog.svelte` `commitMatches`): `mode` + `action` (add/remove); never the
  pattern.
- `file_transfer_completed` (backend, `write_operations/types.rs` `TauriEventSink::emit_complete`): `op` (copy/move),
  `item_count` bucket, `had_conflicts` bool (proxied from `files_skipped > 0`); never names/paths.
- `delete_used` (backend, same sink): `trashed` bool, `item_count` bucket.
- `smb_connected` (backend, `backends/smb.rs` `connect_smb_volume`): no host/share/credential props.
- `mtp_connected` (backend, `mtp/connection/mod.rs` `connect`): no device/product props.
- `settings_opened` (frontend, `command-handlers/app-dialog-handlers.ts` `app.settings`): no props.
- `error_encountered` (backend, `listing/streaming.rs` `TauriListingEventSink::emit_error`): `category` enum (from the
  FriendlyError); never the path/message/provider.
