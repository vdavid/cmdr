# Licensing details (backend)

Depth and rationale. `CLAUDE.md` holds the must-knows; the flows, decisions, and gotchas live here.

## Two-layer caching

1. In-memory `LICENSE_CACHE: Mutex<Option<LicenseInfo>>`: avoids re-parsing/verifying the Ed25519 signature on every
   call.
2. `license.json` via `tauri-plugin-store`: persists server validation across sessions. Keys: `cached_license_status`,
   `last_validation_timestamp`, `expiration_modal_shown`, `commercial_reminder_last_dismissed`.

Offline grace period: 30 days. After that, status reverts to Personal until the next successful server validation.

## Activation flow (verify/commit split)

Split into two phases so invalid keys are never persisted.

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

- `VerifyResult` fields: `info` (LicenseInfo), `full_key`, `short_code`.
- `LicenseInfo` fields: `email`, `transaction_id`, `issued_at`, `organization_name`, `license_type`, `short_code`.
- The frontend uses `license_type` to construct a fallback `LicenseStatus` when the server is unavailable.

Legacy `activate_license` / `activate_license_async` wrappers still exist for backward compatibility; they call
`commit_license` internally (verify + commit in one call).

## Key patterns

- Key format: `base64(JSON).base64(signature)`, split on a single `.`.
- Public key embedded at compile time as hex in `verification.rs` (`PUBLIC_KEY_HEX`).

## Key decisions

**Decision**: BSL 1.1 license model: free personal use, paid commercial ($59/year or $199 perpetual), converts to
AGPL-3.0 after 3 years.
**Why**: An earlier AGPL + trial model felt pushy for hobbyists (trial countdown, nagware, trivial bypass). BSL gives
friction-free personal use (no nags), clear commercial terms, and simpler enforcement (title bar shows license type).
"Source-available" positioning avoids confusing "open source but not really" messaging. Machine IDs aren't tracked; one
license works on unlimited personal machines.

**Decision**: Ed25519 offline verification with the public key compiled in, rather than server-side-only validation.
**Why**: A file manager must work offline. Network-required checks would degrade or nag on a plane or behind a
restrictive firewall. Offline crypto verification works instantly and permanently; server calls are only for
subscription expiry.

**Decision**: Two-layer caching (in-memory `Mutex<Option<LicenseInfo>>` + on-disk `license.json`).
**Why**: Ed25519 verification is fast (~microseconds) but the call chain involves store I/O and JSON parsing.
`get_license_info` runs on every `get_app_status` check (window title, menu state, frontend polling). The in-memory
cache avoids repeated store reads; the on-disk cache persists server results so the app doesn't re-validate every launch.

**Decision**: 30-day offline grace period, then revert to Personal.
**Why**: Balances trust vs. revenue protection. Shorter annoys legitimate users on extended trips; longer lets cancelled
subscriptions keep working. 30 days matches typical billing cycles.

**Decision**: 7-day server re-validation interval instead of checking every launch.
**Why**: Server calls are cheap but not free (network + startup latency). 7 days catches cancellations promptly while
avoiding a call on every launch.

**Decision**: Short codes (`CMDR-XXXX-XXXX-XXXX`) exchanged server-side for full crypto keys, rather than directly
verifiable.
**Why**: Short codes are human-friendly to type and share but too short to embed a full Ed25519 signature. The server
maps short codes to full keys, so users get a nice entry experience while the app gets a cryptographically verifiable
key for offline use.

**Decision**: `LicenseActivationError` typed enum instead of `Result<_, String>` for activation errors.
**Why**: The frontend was pattern-matching English substrings to pick an error message. A tagged enum
(`#[serde(tag = "code")]`) serializes as `{ code: "badSignature" }` so the frontend can `switch` on the code. Same
pattern as `MtpConnectionError`. Lives in `verification.rs`, used by `validation_client.rs` too.

**Decision**: Verify/commit split: `verify_license_async` (read-only) + `commit_license` (persist), instead of one
`activate_license_internal` that does both.
**Why**: The old flow stored the key before server validation. If the server said "invalid" and the user force-quit (or
the app crashed), the invalid key persisted. Now verify-then-validate-then-commit means invalid keys never touch disk,
eliminating defensive `resetLicense()` cleanup in error handlers.

**Decision**: `VerifyResult` is a separate struct from `LicenseInfo`.
**Why**: `VerifyResult` carries `full_key` (needed to call `commit_license` later) and `short_code`. These shouldn't leak
to the frontend via `get_license_info`; they're only meaningful during activation. Keeping them separate means
`LicenseInfo` stays clean for displaying license details.

**Decision**: `validate_license_async` is single-flight with a failure cooldown.
**Why**: Concurrent validation triggers (multiple windows, repeated startup calls) used to stampede the server with
identical requests and spam one network-error warn per call. A static `tokio::sync::Mutex` serializes validations; after
acquiring it, periodic revalidation (`transaction_id == None`) short-circuits when another caller just validated
successfully (`!needs_validation`) or when the last attempt failed under 60 s ago (`LAST_FAILED_VALIDATION_AT`).
Explicit activation (`transaction_id == Some`) always goes through: the user is actively waiting, and the cooldown must
not block a retry after a transient failure. The network-error log lives in `validation_client.rs` only (info in debug
where `localhost:8787` is usually down, warn in release).

**Decision**: `validate_license_async` accepts an optional `transaction_id` parameter.
**Why**: During activation the key isn't stored yet, so the function can't read the transaction ID from the store; the
frontend passes it explicitly. For periodic re-validation the parameter is `None` and the function reads from the stored
license. Avoids storing the key just to read the transaction ID back.

**Decision**: `CMDR_MOCK_LICENSE` env var bypasses all license logic including server calls.
**Why**: License UX testing needs every state (personal, commercial, expired, with/without modals). Without mocking you'd
need real keys per variant and a running API server. The mock skips network entirely.

## Gotchas

**Gotcha**: `should_show_commercial_reminder` initializes the timer on first call rather than showing immediately.
**Why**: On first launch the user hasn't evaluated the app. Showing "get a commercial license" immediately is a hostile
first impression. The 30-day timer starts silently so the reminder appears after a month of use.

**Gotcha**: `ValidationResponse` uses manual `#[serde(rename)]` per field, not `#[serde(rename_all)]`.
**Why**: The API server returns mixed conventions: `status` is lowercase, `organizationName`/`expiresAt` are camelCase,
`type` is a Rust keyword needing rename. The struct matches the API as-is.

**Gotcha**: `validate_with_server` returns `ValidationOutcome` (`Success`/`UpstreamError`/`NetworkError`), not
`Option<ValidationResponse>`.
**Why**: The API server returns HTTP 502 when it can't reach Paddle (upstream error) vs HTTP 200 with `status: "invalid"`
when Paddle actively says the transaction is unknown. Collapsing both into `None` made `validate_license_async` trust a
stale "invalid" from a transient Paddle outage and overwrite cached "active". Now `UpstreamError` and `NetworkError`
fall back to cached status without overwriting, while `Success` (even `status: "invalid"`) is definitive and cached.

**Gotcha**: `validate_license_async` returns `Result<AppStatus, String>`, not bare `AppStatus`.
**Why**: The Tauri command must propagate network/upstream errors so the frontend distinguishes "server rejected the key"
(`Ok(Personal)`) from "couldn't reach the server" (`Err`). Without this, the frontend's catch never fires (Tauri's
`invoke` only throws on `Err`), and stale cached `Personal` is misread as a server rejection.

## Dependencies

- External: `ed25519-dalek`, `base64`, `reqwest`, `tauri_plugin_store`, `sha2`, `core-foundation` (macOS).
- Internal: none.
