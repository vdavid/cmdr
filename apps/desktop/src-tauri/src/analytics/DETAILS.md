# Analytics: details

Depth behind the must-knows in `CLAUDE.md`.

## Wiring

`analytics::init(app.handle())` + `analytics::start()` run from `lib.rs` setup (mirroring `space_poller`): `init`
stores the app handle, `start` spawns the loop (one beat on launch, then hourly). `install_id::init()` runs earlier in
setup (before the crash reporter) so it can snapshot the diag id for the panic hook.

## Install-id storage and signal-safe read

The ids resolve their data dir without an `AppHandle` (mirroring `settings/loader.rs`'s `early_load_*`), so the no-arg
accessors work from the panic hook, next-launch crash assembly, and the analytics loop alike. `diagnostics_id()`
allocates and locks, which is unsafe in the async-signal-safe crash signal handler, so `install_id::init()` snapshots
the diag id into a `OnceLock` that the panic-hook path reads via `diagnostics_id_snapshot()`; the signal path attaches
the diag id later, at next-launch assembly, where the full stdlib is available.

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

## The `track_event` IPC

Frontend events call the `track_event` IPC (`commands/analytics.rs`), a thin pass-through to `capture`. It takes
`props_json: String` rather than a structured type because the prop set is open and `serde_json::Value` can't cross the
specta IPC boundary; the frontend's typed `trackEvent` wrapper `JSON.stringify`s the props. A malformed or non-object
`props_json` degrades to no props (the event still fires with `source: "desktop"`).

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
- `search_used` (frontend, `SearchDialog.svelte`, once per run when the run ENDS): `mode`, `trigger`, `ending`,
  `coverage`, `duration_bucket`, `abandoned_ground`, `capped`; never the query. See below.
- `search_cta_offered` / `search_cta_used` (frontend, same file): `cta` enum. See below.
- `select_files_used` (frontend, `SelectionDialog.svelte` `commitMatches`): `mode` + `action` (add/remove); never the
  pattern.
- `file_transfer_completed` (backend, `write_operations/types.rs` `TauriEventSink::emit_complete`): `op` (copy/move),
  `item_count` bucket, `had_conflicts` bool (proxied from `files_skipped > 0`); never names/paths.
- `delete_used` (backend, same sink): `trashed` bool, `item_count` bucket.
- `smb_connected` (backend, `volume/backends/smb/mod.rs` `connect_smb_volume`): no host/share/credential props.
- `mtp_connected` (backend, `mtp/connection/mod.rs` `connect`): no device/product props.
- `settings_opened` (frontend, `command-handlers/app-dialog-handlers.ts` `app.settings`): no props.
- `error_encountered` (backend, `listing/streaming.rs` `TauriListingEventSink::emit_error`): `category` enum (from the
  ListingError); never the path/message/provider.
- `first_index_started` / `first_index_home_covered` / `first_index_completed` (backend, `first_index.rs`): no props on
  the first, a `duration_bucket` on the other two. See below.
- `first_folder_size_shown` (frontend, `$lib/indexing/first-size-timing.ts`, once per launch): `seconds_bucket` +
  `covering` bool. See below for its population.

## The search events, in detail

Search is the one feature whose interesting question is not "did somebody use it?" but "did it have to walk, how long
did that take, and did they stay for it?" — since a search now covers unindexed ground by walking it
(`docs/specs/unindexed-search-plan.md`). The vocabulary is minted in `apps/desktop/src/lib/search/search-analytics.ts`, a pure module
that cannot see a query, a pattern, or a path.

**`search_used` fires ONCE per run, when the run ends.** Firing at the start would leave every one of those questions
unanswerable. The props:

- `mode` — the dialog's own mode enum (`filename` / `ai`).
- `trigger` — `run` (Enter or the run button, the path that walks) or `autoApply` (the debounce, which answers from the
  index alone, Decision 7). ❌ Don't fold them together: auto-apply fires on every typing pause, so it would drown the
  deliberate searches in the denominator.
- `ending` — `completed` / `interrupted` (the drive went away) / `cancelled` (Escape or the dialog closing) /
  `superseded`. The cancel rate is `cancelled` over the `trigger: run` total.
- `coverage` — `covered` (the index answered it all) / `live` (nothing was covered, every row came off the walk) /
  `mixed`, from the backend's own `SearchRunCoverage::kind`, or `unknown` for a run that ended before saying.
- `duration_bucket` — `<1s` / `1-5s` / `5-30s` / `30s-2m` / `2m+`. Absent for an index-only run, which answers inside
  one promise: timing it would measure the IPC round trip, not a wait anybody felt.
- `abandoned_ground` and `capped` — the two ways a run comes back short WITHOUT its ending saying so.

**`superseded` is the frontend's own word.** The backend never reports it: a run the user typed past keeps walking
(Decision 11) and no terminal event for it is coming, so the arrival of its successor is the only moment it can be
counted. That's why the run clock starts on the coverage callback's `null` (a run starting) rather than on
`searchFilesStreaming` resolving — a small folder's whole run can arrive before that promise does.

**CTA conversion is two events, not a prop.** `search_cta_offered` fires when the coverage note starts offering one and
`search_cta_used` when it's pressed, both carrying the same `cta` enum (`indexDrive` / `fullDiskAccess`), so conversion
is a ratio per CTA. It can't be one prop on `search_used`: the Full Disk Access offer depends on a TCC probe that
answers AFTER the run does, so an offer counted at settle time would miss every late one and put the rate over 100%.

## The first-index events, in detail

Covering a drive in phases is justified by a user-experience claim, so the four numbers that can falsify it ride the
same pipeline as everything else. Three are backend (`first_index.rs`, off the index's own `IndexEvent`
stream); the fourth is the frontend's, because only it knows what is on screen.

- **`first_index_started`** — a run announced itself with `covered_in_phases`. It is the DENOMINATOR, and that is its
  whole job.
- **`first_index_home_covered`** — `duration_bucket` (`<10s` / `10-30s` / `30s-2m` / `2-5m` / `5m+`) since that start,
  off the `IndexEvent::HomeCovered` report. The claim is that a user's own files answer in seconds, so the first two
  buckets are where it lives or dies.
- **`first_index_completed`** — `duration_bucket` (`<1m` / `1-3m` / `3-10m` / `10-30m` / `30m+`) since that start.
- **`first_folder_size_shown`** — `seconds_bucket` since the frontend booted, plus `covering` (was a phased first index
  running on that drive?). Fires on the first window of rows carrying a real `recursiveSize`, at most once per launch.
  This is the wow moment itself: not "the index finished", which nobody watches, but "I opened a folder and it told me
  how big it is". `covering` is what keeps a machine indexed weeks ago from drowning the measurement in zeroes.

  **Its population is launches that opened a folder**, in either list mode: both `views/full-list-cache.svelte.ts` and
  `views/BriefList.svelte` call the hook, at the two points in each where rows the user is looking at gain sizes. A
  launch that never opens a folder can't fire it, which is the one exclusion and an honest one — there was no moment to
  measure. ❌ If a call site is ever dropped, the population narrows to "launches in the OTHER mode" and nothing in the
  numbers says so, so say it here.

  The comparison the claim rests on is WITHIN the event: `covering: true` (a first index was running on that drive)
  against `covering: false` (the index was already there). That is what makes the wow moment falsifiable, and it needs
  no cross-event denominator at all.

**The interruption rate is a RATIO, not an event.** How often a first index never finishes is
`1 - first_index_completed / first_index_started`. ❌ Don't add a terminal "interrupted" event: a run that ends with the
process (a quit, a crash, a power cut) has no moment left to report in, so anything counted at the end under-counts
exactly the case being measured — and that case is the one the truncate-and-rebuild design lost entirely.

**A phased run is told from every other run by the clock's own presence.** `first_index.rs` keeps a per-volume start
`Instant` only for a run that announced `covered_in_phases`, so the `ScanComplete` of a change check on an
already-indexed drive finds no clock and counts as nothing.
