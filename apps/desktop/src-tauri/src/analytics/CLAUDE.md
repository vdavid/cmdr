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

## Files

- `mod.rs`: the heartbeat loop (launch beat + hourly), the consent gate, the payload struct, the
  fire-and-forget send. `init(app)` + `start()` mirror `space_poller`'s spawn pattern, wired from
  `lib.rs` setup.
- `config_shape.rs`: the pure, unit-tested config-shape builder and the `CATEGORICAL_STRING_KEYS`
  allowlist. The only place the PII-free rule lives.

## Wiring

`analytics::init(app.handle())` + `analytics::start()` run from `lib.rs` setup, alongside
`space_poller`. `install_id::init()` runs earlier (before the crash reporter) to snapshot the diag
id for the panic hook.
