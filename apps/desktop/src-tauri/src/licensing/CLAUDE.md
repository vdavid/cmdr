# Licensing subsystem

Ed25519 offline license verification with optional server-side subscription validation.

License keys are self-contained: `base64(JSON payload).base64(Ed25519 signature)`. The app verifies them offline using a compiled-in public key. Server validation is only needed to check subscription expiry status.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | `LicenseData` struct, `redact_email` helper, re-exports from sub-modules |
| `verification.rs` | Ed25519 crypto. `LicenseActivationError` typed error enum. Validates key format, verifies signature, caches result in `Mutex`. Split into verify/commit: `verify_license_async` (read-only check + short-code exchange), `commit_license` (persist to disk + update caches). Legacy wrappers: `activate_license` (sync verify+commit), `activate_license_async` (async verify+commit). `get_license_info` (lazy, cached). `VerifyResult` struct wraps `LicenseInfo` + `full_key` + `short_code`. |
| `app_status.rs` | `AppStatus` enum. 7-day server re-validation, 30-day offline grace period. Commercial use reminder timer. Debug: `CMDR_MOCK_LICENSE` env var overrides everything. |
| `validation_client.rs` | HTTP client: `POST /validate`, `POST /activate`. Debug → `localhost:8787`, release → `license.getcmdr.com`. Mock mode skips network entirely. Returns `ValidationOutcome` enum (Success/UpstreamError/NetworkError). `ValidationRequest` includes an optional `deviceId` for fair-use tracking. |
| `device_id.rs` | Stable hashed device identifier for fair-use license tracking. Reads `IOPlatformUUID` via IOKit FFI (macOS), salts with `"cmdr:"`, SHA-256 hashes, prefixes with `v1:`. Cached in `OnceLock`. Returns `None` on failure (best-effort, never blocks validation). Linux stub returns `None`. |

## AppStatus variants

```
Personal { show_commercial_reminder }   — no license
Commercial { license_type, organization_name, expires_at }
Expired { organization_name, expired_at, show_modal }
```

`LicenseType`: `CommercialSubscription`, `CommercialPerpetual`.

## Two-layer caching

1. In-memory `LICENSE_CACHE: Mutex<Option<LicenseInfo>>` — avoids re-parsing/verifying the Ed25519 signature on every call.
2. `license.json` via `tauri-plugin-store` — persists server validation result across sessions. Keys: `cached_license_status`, `last_validation_timestamp`, `expiration_modal_shown`, `commercial_reminder_last_dismissed`.

Offline grace period: 30 days. After that, status reverts to Personal until next successful server validation.

## Activation flow (verify/commit split)

The activation flow is split into two phases to prevent invalid keys from being persisted to disk.

```
Frontend: verifyLicense(input)
  |
  |-- is_short_code("CMDR-XXXX-XXXX-XXXX")?
  |     YES → POST /activate → get full crypto key
  |
  v
validate_license_key()      ← Ed25519 verify offline
return VerifyResult         ← LicenseInfo + full_key + short_code (nothing stored)

Frontend: validateLicenseWithServer(transactionId)
  |                                   ↑ passed explicitly since key isn't stored yet
  v
Server says active            → commitLicense(fullKey, shortCode) → persist + onSuccess
Server says expired          → commitLicense(fullKey, shortCode) → persist + show error
Server says invalid          → DON'T commit. Show error. Nothing stored.
Network error                → commitLicense(fullKey, shortCode) → persist + fallback
```

`commit_license` does: store to `license.json`, write initial `cached_license_status`, update `LICENSE_CACHE`.

`VerifyResult` fields: `info` (LicenseInfo), `full_key`, `short_code`.
`LicenseInfo` fields: `email`, `transaction_id`, `issued_at`, `organization_name`, `license_type`, `short_code`.
The frontend uses `license_type` to construct a fallback `LicenseStatus` when the server is unavailable.

Legacy `activate_license`/`activate_license_async` wrappers still exist for backward compatibility — they call
`commit_license` internally (verify + commit in one call).

## Key patterns

- Short codes: `CMDR-XXXX-XXXX-XXXX` (3 segments × 4 alphanumeric chars after CMDR prefix).
- Key format: `base64(JSON).base64(signature)` — split on single `.`.
- Public key embedded at compile time as hex in `verification.rs` (`PUBLIC_KEY_HEX`).
- Mock values (`CMDR_MOCK_LICENSE`): `personal`, `personal_reminder`, `commercial`, `perpetual`, `expired`, `expired_no_modal`.
- Key gen: see [license server CLAUDE.md](../../../../apps/license-server/CLAUDE.md) and
  [README.md](../../../../apps/license-server/README.md#first-time-setup) for the full setup.

## Key decisions

**Decision**: Ed25519 offline verification with the public key compiled in, rather than server-side-only validation.
**Why**: A file manager must work offline. If license checks required a network call, the app would degrade or nag every time the user is on a plane or behind a restrictive firewall. Offline crypto verification means the license works instantly and permanently, with server calls only needed to check subscription expiry status.

**Decision**: Two-layer caching (in-memory `Mutex<Option<LicenseInfo>>` + on-disk `license.json`).
**Why**: Ed25519 verification is fast (~microseconds) but the call chain involves store I/O and JSON parsing. `get_license_info` is called on every `get_app_status` check, which can happen frequently (window title updates, menu state, frontend polling). The in-memory cache avoids repeated store reads. The on-disk cache persists server validation results so the app doesn't need to re-validate on every launch.

**Decision**: 30-day offline grace period, then revert to Personal.
**Why**: Balances trust vs. revenue protection. Shorter would annoy legitimate users on extended trips. Longer would let cancelled subscriptions keep working indefinitely. 30 days matches typical billing cycles — if someone cancels, they lose commercial status roughly when their paid period ends.

**Decision**: 7-day server re-validation interval instead of checking every launch.
**Why**: License server calls are cheap but not free — they require network access and add latency to startup. 7 days is frequent enough to catch cancellations promptly while avoiding unnecessary network calls on every app launch.

**Decision**: Short codes (`CMDR-XXXX-XXXX-XXXX`) exchanged server-side for full crypto keys, rather than being directly verifiable.
**Why**: Short codes are human-friendly for typing and sharing, but too short to embed a full Ed25519 signature. The server maps short codes to full keys, so users get a nice entry experience while the app still gets a cryptographically verifiable key for offline use.

**Decision**: `LicenseActivationError` typed enum instead of `Result<_, String>` for activation errors.
**Why**: The frontend was pattern-matching English substrings to decide which error message to show. A tagged enum (`#[serde(tag = "code")]`) serializes as `{ code: "badSignature" }` so the frontend can `switch` on the code. Follows the same pattern as `MtpConnectionError`. The enum lives in `verification.rs` and is used by `validation_client.rs` too.

**Decision**: Verify/commit split — `verify_license_async` (read-only) + `commit_license` (persist), instead of a single `activate_license_internal` that does both.
**Why**: The old flow stored the key before server validation. If the server said "invalid" and the user force-quit (or the app crashed), the invalid key persisted to disk. Now the frontend verifies first, validates with the server, and only commits on success or network fallback. Invalid keys never touch disk, eliminating the need for defensive `resetLicense()` cleanup in error handlers.

**Decision**: `VerifyResult` is a separate struct from `LicenseInfo`.
**Why**: `VerifyResult` carries `full_key` (needed to call `commit_license` later) and `short_code`. These fields shouldn't leak to the frontend via `get_license_info` — they're only meaningful during the activation flow. Keeping them separate means `LicenseInfo` stays clean for its primary use case (displaying license details).

**Decision**: `validate_license_async` accepts an optional `transaction_id` parameter.
**Why**: During activation, the key isn't stored yet, so the function can't read the transaction ID from the store. The frontend passes it explicitly. For periodic re-validation (7-day cycle), the parameter is `None` and the function falls back to reading from the stored license. This avoids storing the key just to read the transaction ID back.

**Decision**: `CMDR_MOCK_LICENSE` env var bypasses all license logic including server calls.
**Why**: License UX testing requires seeing every state (personal, commercial, expired, with/without modals). Without mocking, you'd need real license keys for each variant and a running license server. The mock skips network entirely, making UI development fast.

## Gotchas

**Gotcha**: `should_show_commercial_reminder` initializes the timer on first call rather than showing the reminder immediately.
**Why**: On first launch, the user hasn't had a chance to evaluate the app yet. Showing a "get a commercial license" modal immediately would be a hostile first impression. The 30-day timer starts silently on first launch so the reminder only appears after the user has been using the app for a month.

**Gotcha**: `ValidationResponse` uses manual `#[serde(rename)]` on individual fields instead of `#[serde(rename_all)]` on the struct.
**Why**: The license server API returns a mix of naming conventions — `status` is lowercase, but `organizationName` and `expiresAt` are camelCase, and `type` is a Rust keyword requiring rename. The struct matches the API as-is rather than imposing a consistent naming convention that would break deserialization.

**Gotcha**: `validate_with_server` returns `ValidationOutcome` (an enum with `Success`/`UpstreamError`/`NetworkError`), not `Option<ValidationResponse>`.
**Why**: The license server returns HTTP 502 when it can't reach Paddle (upstream error) vs HTTP 200 with `status: "invalid"` when Paddle actively says the transaction is unknown. The old code collapsed both into `None`, causing `validate_license_async` to trust a stale "invalid" response from a transient Paddle outage and overwrite the cached "active" status. Now `UpstreamError` and `NetworkError` both fall back to cached status without overwriting, while `Success` (even with `status: "invalid"`) is treated as definitive and cached.

**Gotcha**: `validate_license_async` returns `Result<AppStatus, String>`, not bare `AppStatus`.
**Why**: The Tauri command must propagate network/upstream errors to the frontend so it can distinguish "server actively rejected the key" (`Ok(Personal)`) from "couldn't reach the server" (`Err`). Without this, the frontend's catch block never fires — Tauri's `invoke` only throws on `Err` — and stale cached `Personal` status gets misinterpreted as a server rejection.

## Dependencies

External: `ed25519-dalek`, `base64`, `reqwest`, `tauri_plugin_store`, `sha2`, `core-foundation` (macOS)
Internal: none
