# Licensing subsystem

Ed25519 offline license verification with optional server-side subscription validation.

License keys are self-contained: `base64(JSON payload).base64(Ed25519 signature)`. The app verifies them offline using a compiled-in public key. Server validation is only needed to check subscription expiry status.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | `LicenseData` struct, `redact_email` helper, re-exports from sub-modules |
| `verification.rs` | Ed25519 crypto. Validates key format, verifies signature, caches result in `Mutex`. `activate_license` (sync), `activate_license_async` (handles short-code exchange), `get_license_info` (lazy, cached). |
| `app_status.rs` | `AppStatus` enum. 7-day server re-validation, 30-day offline grace period. Commercial use reminder timer. Debug: `CMDR_MOCK_LICENSE` env var overrides everything. |
| `validation_client.rs` | HTTP client: `POST /validate`, `POST /activate`. Debug → `localhost:8787`, release → `license.getcmdr.com`. Mock mode skips network entirely. |

## AppStatus variants

```
Personal { show_commercial_reminder }   — no license
Supporter { show_commercial_reminder }  — personal badge
Commercial { license_type, organization_name, expires_at }
Expired { organization_name, expired_at, show_modal }
```

`LicenseType`: `Supporter`, `CommercialSubscription`, `CommercialPerpetual`.

## Two-layer caching

1. In-memory `LICENSE_CACHE: Mutex<Option<LicenseInfo>>` — avoids re-parsing/verifying the Ed25519 signature on every call.
2. `license.json` via `tauri-plugin-store` — persists server validation result across sessions. Keys: `cached_license_status`, `last_validation_timestamp`, `expiration_modal_shown`, `commercial_reminder_last_dismissed`.

Offline grace period: 30 days. After that, status reverts to Personal until next successful server validation.

## Activation flow

```
User enters key or short code
  |
  |-- is_short_code("CMDR-XXXX-XXXX-XXXX")?
  |     YES → POST /activate → get full crypto key
  |
  v
validate_license_key()      ← Ed25519 verify offline
store to license.json       ← persisted
update LICENSE_CACHE        ← in-memory fast path
```

## Key patterns

- Short codes: `CMDR-XXXX-XXXX-XXXX` (3 segments × 4 alphanumeric chars after CMDR prefix).
- Key format: `base64(JSON).base64(signature)` — split on single `.`.
- Public key embedded at compile time as hex in `verification.rs` (`PUBLIC_KEY_HEX`).
- Mock values (`CMDR_MOCK_LICENSE`): `personal`, `personal_reminder`, `supporter`, `supporter_reminder`, `commercial`, `perpetual`, `expired`, `expired_no_modal`.
- Key gen: `cd apps/license-server && pnpm run generate-keys` then update `PUBLIC_KEY_HEX`.

## Dependencies

External: `ed25519-dalek`, `base64`, `reqwest`, `tauri_plugin_store`
Internal: none
