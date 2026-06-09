# Analytics (beta usage stats)

Anonymous beta usage analytics. A background loop posts a `/heartbeat` (true daily-active signal +
a PII-free config snapshot) on launch and hourly. PostHog feature events ride the same consent gate
and the same install id, added on top of this foundation.

The two install ids live in the neutral [`crate::install_id`] module (not here), so the crash and
error reporters can depend on it without pulling in `analytics`.

## Two ids that never meet

We mint two random per-install ids (both in `install_id.rs`, both written to one Rust-owned
`install-ids.json`):

- `anal_<uuid>` ([`install_id::analytics_id`]): the heartbeat key and the PostHog `distinct_id`.
  Never attached to a crash or error report.
- `diag_<uuid>` ([`install_id::diagnostics_id`]): attached only to crash and error reports so
  sequential reports from one install group together. Never sent through the analytics pipeline.

Why two: a tester can voluntarily attach their email to a report so we can reply. If reports carried
the analytics id, then email → analytics-id → the install's whole usage history would be joinable on
our servers, exactly the linkage we promise not to have. With a separate `diag_` id, an attached
email links only to the diagnostics stream; the analytics stream stays unjoinable to any identity.
The GDPR principle: if two datasets *can* be joined, treat them as joined, so we make them genuinely
unjoinable. The `anal_`/`diag_` prefixes are intentional and make the ids self-identifying in
payloads, PostHog, and the D1 tables.

## Why the ids are Rust-owned, AppHandle-free files

The ids live in `install-ids.json`, not `settings.json`, and resolve their data dir WITHOUT an
`AppHandle` (mirroring `settings/loader.rs`'s `early_load_*` helpers: `CMDR_DATA_DIR` if set, else
the OS default for the bundle id).

- **Rust-owned, not a setting**: the frontend owns every `settings.json` write; Rust only reads it.
  Minting an id into `settings.json` from Rust would race the frontend's store ownership on first
  launch. A separate Rust-owned file sidesteps that.
- **AppHandle-free accessors**: `analytics_id()` / `diagnostics_id()` stay no-arg so they're
  callable from the panic hook, next-launch crash assembly, and the analytics loop alike, none of
  which always have an `AppHandle` at hand.

**Signal-safety**: the crash signal handler is async-signal-safe (no alloc, no locks). It must NOT
call `diagnostics_id()`. `install_id::init()` (run at startup) snapshots the diag id into a cheap
`OnceLock<String>` that the panic-hook path reads via `diagnostics_id_snapshot()`; the signal path
attaches the diag id at next-launch report assembly (full stdlib), not inside the handler.

## Consent is tri-state, default-on, fully-silent opt-out

The opt-out is `analytics.enabled` in `settings.json`. The frontend store only persists non-default
values, so an opted-in install has NO key. The gate
([`analytics_consent_granted`](mod.rs)) treats:

- `None` (no key, the opted-in default) → granted.
- `Some(true)` → granted.
- `Some(false)` → opted out.

This single helper is the one consent gate for both the heartbeat loop and (later) `track_event`.
Opt-out is **fully silent**: an opted-out install sends nothing at all, not even an "I opted out"
bit (that would be collecting from someone who declined). So we can't measure the opt-out rate from
this channel; estimate it indirectly from the update-check denominator (everyone sends update
checks regardless of this toggle).

## Dev/CI suppression and the FORCE override

`suppressed()` returns true in debug builds and when `CI` is set, so dev runs and CI never pollute
production analytics. Set `CMDR_ANALYTICS_FORCE=1` to override and force beats even in a debug/CI
build, so an integration test can drive the loop against a localhost Worker.

## PII-free by allowlist, never by redaction

`config_shape.rs` builds the config snapshot by an explicit allowlist, never by redacting a free-form
blob. Settings hold SMB hostnames, paths, recent lists, AI key refs, and the beta email, all as
strings, so a denylist would eventually leak one. The rule (the ONE place it lives):

- Include every key whose JSON value is a boolean or number (auto-extends as new bool/number
  settings land, zero maintenance; bools/numbers are PII-free by nature).
- Plus the small `CATEGORICAL_STRING_KEYS` allowlist: categorical enum-strings (theme, app color,
  size/date palettes, date format, density, size display/unit, sort mode, AI provider, etc.) that
  are non-PII despite being strings.
- Exclude every other string, and all objects and arrays.
- Add `fdaGranted` (runtime state, not a setting) explicitly.

The `excludes_pii_shaped_strings` test is the privacy invariant: a seeded settings JSON with an
email, an SMB host, a recents list, and a path produces a snapshot containing none of them. When you
add a new categorical string setting worth shipping, add its id to `CATEGORICAL_STRING_KEYS`; never
loosen the bool/number rule to "include all strings."

Hard nevers across the whole pipeline: file names, contents, paths, search queries, AI prompts,
keystrokes, screenshots.

## Heartbeat payload

`HeartbeatPayload` (camelCase on the wire, `Option::None` → `null`) matches the M2 Worker's
validator:

- `analId` (required): `anal_` + lowercase hyphenated v4 UUID, `^anal_[0-9a-f-]{36}$`.
- `appVersion` (required): semver from `CARGO_PKG_VERSION`.
- `osVersion` (required): from the shared `crate::platform::os_version()`, always non-empty.
- `arch` (required): `std::env::consts::ARCH`.
- `buildMode` (optional): `"release"` / `"debug"`.
- `config` (optional): the config-shape object, stored verbatim.

Fire-and-forget POST mirroring the crash/error reporters (10 s timeout, errors logged at debug, the
next hourly tick retries). Endpoint: `http://localhost:8787/heartbeat` (debug) /
`https://api.getcmdr.com/heartbeat` (release).

## PostHog feature events

Curated product events ride the SAME consent gate, dev/CI suppression, and `anal_` install id as
the heartbeat. `posthog::capture(event, props)` builds the `/capture/` body and fire-and-forget
POSTs to `https://eu.i.posthog.com/capture/` (EU cloud, project `136072`). Shape:

```json
{ "api_key": "phc_...", "event": "<name>", "distinct_id": "anal_<uuid>",
  "properties": { "source": "desktop", ...props }, "$set": <config-shape> }
```

- **`$set` is the config-shape, verbatim.** Person properties reuse `config_shape::build_config_shape`
  (the same allowlisted object the heartbeat ships): one source of truth, no second PII surface.
- **`source: "desktop"`** is injected first and can't be shadowed by a caller `source` prop (so the
  dashboard always splits desktop events from the website's).
- **The key is `option_env!("CMDR_POSTHOG_KEY")`**, baked at build time (a GitHub secret on the
  `tauri-action` step in `release.yml`; `build.rs` has a `rerun-if-env-changed` for it). `None`
  locally → `capture` is a no-op (logged once at debug). The key is public by design (PostHog ingest
  keys are safe in client code).
- **Backend events call `posthog::capture` directly; frontend events call the `track_event` IPC**
  (`commands/analytics.rs`), which is a thin pass-through to `capture`. ONE backend path, ONE consent
  gate. The IPC takes `props_json: String` (the frontend's typed `trackEvent` wrapper does the
  `JSON.stringify`) because the prop set is open and `serde_json::Value` can't cross specta. No
  capability entry needed (custom app commands aren't ACL-gated).

### The open event API + how to add an event

Events are an OPEN set: `capture(name, props)` / `track_event(name, props)` take an arbitrary name
and an arbitrary PII-free prop map. Adding one is a one-liner, no enum, no schema:

- **Backend event**: at the success chokepoint, `crate::analytics::posthog::capture("my_event", serde_json::json!({ "kind": some_enum }))`.
- **Frontend event**: `import { trackEvent } from '$lib/tauri-commands'`, then `void trackEvent('my_event', { kind: someEnum })`.
- **Name internals after the UI** (project rule): the event name uses the feature's user-facing
  vocabulary (`pane_navigated`, `search_used`), and props are categorical (`volume_kind`, `mode`).

### PII-free convention + the `sanitize_props` net

Every prop value MUST be a categorical enum, a count (or coarse bucket), or a bool. NEVER a path,
file name, search query, AI prompt, or hostname. This is enforced by review, NOT by redaction.
`posthog::sanitize_props` is a **debug-build backstop**: it scans string prop values and logs a
scoped `warn!` if one looks PII-shaped (contains `/`, `\`, `@`, or a `~/` prefix). It only warns
(never strips), so production behavior is identical with the guard compiled out, and a leak surfaces
loudly in dev before shipping. It's a safety net, not a license to pass free-form strings.

### The starter event set (where each fires)

PII-free; this set grows over time. Backend events fire at success chokepoints; frontend events ride
`track_event`.

- `app_launched` (backend, `lib.rs` setup) — no props.
- `pane_navigated` (frontend, `FilePane.svelte` `handleListingComplete`) — `volume_kind` enum
  (`local`/`smb`/`mtp`/`network`/`search-results`); never the path.
- `search_used` (frontend, `SearchDialog.svelte` `runSearch`) — `mode` enum; never the query.
- `select_files_used` (frontend, `SelectionDialog.svelte` `commitMatches`) — `mode` (match mode) +
  `action` (add/remove); never the pattern.
- `file_transfer_completed` (backend, `write_operations/types.rs` `TauriEventSink::emit_complete`) —
  `op` (copy/move), `item_count` bucket, `had_conflicts` bool (proxied from `files_skipped > 0`, since
  skips happen only via conflict resolution); never names/paths.
- `delete_used` (backend, same sink) — `trashed` bool, `item_count` bucket.
- `smb_connected` (backend, `backends/smb.rs` `connect_smb_volume`) — no host/share/credential props.
- `mtp_connected` (backend, `mtp/connection/mod.rs` `connect`) — no device/product props.
- `settings_opened` (frontend, `command-handlers/app-dialog-handlers.ts` `app.settings`) — no props.
- `error_encountered` (backend, `listing/streaming.rs` `TauriListingEventSink::emit_error`) —
  `category` enum (from the FriendlyError); never the path/message/provider.

## Files

- `mod.rs`: the heartbeat loop (launch beat + hourly), the consent gate, the payload struct, the
  fire-and-forget send, and the shared helpers (`suppressed`, `read_raw_settings`, `APP_HANDLE`,
  `analytics_consent_granted`) that `posthog` reuses. `init(app)` + `start()` mirror `space_poller`'s
  spawn pattern, wired from `lib.rs` setup.
- `posthog.rs`: the PostHog `capture(event, props)` path, the pure `build_capture_body`, the
  debug-build `sanitize_props` PII net, and the `option_env!` key mechanism.
- `config_shape.rs`: the pure, unit-tested config-shape builder and the `CATEGORICAL_STRING_KEYS`
  allowlist. The only place the PII-free rule lives. Mirrored as both the heartbeat `config` and the
  PostHog `$set` person properties.

## Wiring

`analytics::init(app.handle())` + `analytics::start()` run from `lib.rs` setup, alongside
`space_poller`. `install_id::init()` runs earlier (before the crash reporter) to snapshot the diag
id for the panic hook.
