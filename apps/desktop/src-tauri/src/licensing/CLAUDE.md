# Licensing subsystem (backend)

Ed25519 offline license verification with optional server-side subscription validation. License keys are self-contained:
`base64(JSON payload).base64(Ed25519 signature)`, verified offline against a compiled-in public key. Server validation
only checks subscription expiry. Frontend counterpart: `src/lib/licensing/CLAUDE.md`.

## Module map

- **`verification.rs`**: Ed25519 crypto, `LicenseActivationError` typed enum, `PUBLIC_KEY_HEX` (compiled-in public key).
  Verify/commit split: `verify_license_async` (read-only check + short-code exchange) and `commit_license` (persist +
  update caches). `get_license_info` (lazy, cached). `VerifyResult` = `LicenseInfo` + `full_key` + `short_code`.
- **`app_status.rs`**: `AppStatus` enum, server re-validation and offline-grace logic, commercial-use reminder timer,
  `CMDR_MOCK_LICENSE` override.
- **`validation_client.rs`**: HTTP client (`POST /validate`, `POST /activate`); debug → `localhost:8787`, release →
  `api.getcmdr.com`. Returns the `ValidationOutcome` enum (Success/UpstreamError/NetworkError).
- **`device_id.rs`**: stable hashed device id for fair-use tracking (`IOPlatformUUID` via IOKit, salted + SHA-256,
  `v1:` prefix; `None` on failure, Linux stub returns `None`).

## Must-knows

- **Grace period is 30 days; server re-validation interval is 7 days** (`OFFLINE_GRACE_PERIOD_SECS`,
  `VALIDATION_INTERVAL_SECS` in `app_status.rs`). After the grace window with no successful validation, status reverts to
  Personal.
- **Verify/commit split keeps invalid keys off disk.** Frontend calls verify (nothing stored), validates with the
  server, and only `commit_license` persists. Don't reintroduce a path that stores the key before server validation:
  that's the bug this split fixed (invalid key persisting on force-quit). `commit_license` writes `license.json` + the
  initial `cached_license_status` + updates `LICENSE_CACHE`, but deliberately NOT `last_validation_timestamp` (so
  `needs_validation()` stays true until a real validation lands).
- **`validate_with_server` returns `ValidationOutcome`, not `Option`.** `UpstreamError` (HTTP 502, Paddle unreachable)
  and `NetworkError` must fall back to cached status WITHOUT overwriting it; only `Success` (even `status: "invalid"`) is
  definitive and cached. Collapsing these to `None` lets a transient Paddle outage overwrite a cached "active".
- **`validate_license_async` returns `Result<AppStatus, String>`, not bare `AppStatus`.** The `Err` lets the frontend
  distinguish "server rejected the key" (`Ok(Personal)`) from "couldn't reach the server" (`Err`); without it the
  frontend's catch never fires and stale `Personal` reads as a rejection.
- **`validate_license_async` is single-flight with a 60 s failure cooldown.** A static `tokio::sync::Mutex` serializes
  validations; periodic re-validation (`transaction_id == None`) short-circuits when another caller just succeeded or the
  last attempt failed under 60 s ago. Explicit activation (`transaction_id == Some`) always goes through (the user is
  waiting). The network-error log lives only in `validation_client.rs`; don't log it a second time here.
- **Two-layer cache**: in-memory `LICENSE_CACHE: Mutex<Option<LicenseInfo>>` (avoids re-verifying per call) + on-disk
  `license.json` via `tauri-plugin-store` (persists server result across sessions).
- **`CMDR_MOCK_LICENSE` bypasses ALL license logic including server calls** (debug only). Values: `personal`,
  `personal_reminder`, `commercial`, `perpetual`, `expired`, `expired_no_modal`.
- **`should_show_commercial_reminder` starts the 30-day timer on first call, it doesn't show immediately.** Showing it on
  first launch would be a hostile first impression.

## Types

- `AppStatus`: `Personal { show_commercial_reminder }`, `Commercial { license_type, organization_name, expires_at }`,
  `Expired { organization_name, expired_at, show_modal }`.
- `LicenseType`: `CommercialSubscription`, `CommercialPerpetual`.
- Short codes: `CMDR-XXXX-XXXX-XXXX`, exchanged server-side for the full crypto key (too short to embed an Ed25519 sig).

Key generation / test-key setup: see `apps/api-server/CLAUDE.md` and `README.md#first-time-setup`.

Full details (activation flow diagram, BSL model rationale, all decisions and gotchas): `DETAILS.md`.
