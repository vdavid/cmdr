# Licensing subsystem (backend)

Ed25519-signed license keys (`base64(JSON payload).base64(signature)`), verified offline against a compiled-in public
key. The server is asked every seven days, but only to learn a signed verdict (revoked, expired, renewed). Frontend
counterpart: `src/lib/licensing/CLAUDE.md`.

## Module map

- **`verification.rs`**: Ed25519 crypto, `PUBLIC_KEY_HEX`, `LicenseActivationError`, the verify/commit split
  (`verify_license_async`, `commit_license`), `get_license_info`, and `verify_validation_answer` (signed `/validate`
  answers).
- **`offline_policy.rs`**: `resolve_license_state`, the pure decision of what a license is worth without the server.
- **`app_status.rs`**: `AppStatus`, server re-validation, the cached verdict, the clock guard, reminder timer,
  `CMDR_MOCK_LICENSE`, and `refresh_window_title`. ❗ Every cached-status write calls it; keep it that way.
- **`validation_client.rs`**: `POST /validate` (with a nonce) and `POST /activate`; debug → `localhost:8787`, release →
  `api.getcmdr.com`, E2E → no request. Returns `ValidationOutcome`.
- **`device_id.rs`**: hashed `IOPlatformUUID` for fair-use tracking.

## Must-knows

- **Unreachability alone never downgrades a valid signed license.** Perpetual: valid forever, changed only by a verified
  `invalid`/`expired`. Dated key: its signed `expiresAt`. Renewing subscription: last reported period end + 30 days.
  Rules and every hostile case: `DETAILS.md` § Offline policy. ❌ Never reintroduce an age limit on a perpetual license.
- **Only a signed answer counts.** The server signs `/validate` with the license key over our nonce and transaction id
  (prefix `cmdr-validation-answer-v1\n`, matching `api-server/src/licensing/license.ts`). Anything unsigned, forged,
  replayed, or mismatched is `ValidationOutcome::Unverified` and keeps the cache, exactly like `NetworkError` and
  `UpstreamError`.
- **`PUBLIC_KEY_HEX` and `LICENSE_SERVER_URL` share the `debug_assertions` gate.** Each build trusts only its own server's
  signer. ❌ Never put the production private key in `apps/api-server/.dev.vars`. `DETAILS.md` § Signing keys.
- **Verify/commit split keeps invalid keys off disk.** Verify (nothing stored) → validate → `commit_license`.
  `commit_license` deliberately skips `last_validation_timestamp`, so `needs_validation()` stays true.
- **`validate_license_async` returns `Result`**: `Err` means "couldn't get a trustworthy answer", `Ok(Personal)` means
  "the server rejected it". Single-flight with a 60 s failure cooldown for periodic runs; explicit activation always
  goes through.
- **Time-limited licenses use `effective_now`** (the later of the clock and `license_clock_high_water`), so winding the
  clock back can't stretch one. Perpetual licenses never read the clock.
- **`CMDR_MOCK_LICENSE` bypasses everything** (debug and `playwright-e2e` builds only): `personal`, `personal_reminder`,
  `commercial`, `perpetual`, `expired`, `expired_no_modal`.
- **`should_show_commercial_reminder` starts its 30-day timer on first call** rather than showing on day one.

## Types

- `AppStatus`: `Personal`, `Commercial { license_type, organization_name, expires_at }`, `Expired`.
- `LicenseType`: `CommercialSubscription`, `CommercialPerpetual`.
- Short codes `CMDR-XXXX-XXXX-XXXX` are exchanged by `/activate` for the full key; the license email carries both, and
  the full key activates offline.

Activation flow, signing keys, offline policy, decisions, and gotchas: `DETAILS.md`.
