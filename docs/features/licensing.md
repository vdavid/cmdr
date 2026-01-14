# Licensing

This document describes Cmdr's licensing and payment system.

## Overview

Cmdr uses a **14-day trial with one-time purchase** model:

1. Users download and try Cmdr for free for 14 days
2. After the trial, a $29 one-time license is required
3. Licenses are validated locally using Ed25519 cryptographic signatures

The source code is open under AGPL-3.0. Users can compile it themselves and use it without restriction — we sell the convenience of signed, auto-updating binaries.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                             Components                              │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐     ┌─────────────────┐     ┌───────────────┐  │
│  │  getcmdr.com    │────▶│  Paddle         │────▶│ License       │  │
│  │  (website)      │     │  (payment)      │     │ server        │  │
│  └─────────────────┘     └─────────────────┘     └───────┬───────┘  │
│                                                          ▼          │
│                                                  ┌───────────────┐  │
│                                                  │ Email         │  │
│                                                  │ (Resend)      │  │
│                                                  └───────┬───────┘  │
│                                                          ▼          │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Cmdr app                                                     │  │
│  │  - Trial tracking (14 days)                                   │  │
│  │  - License key input                                          │  │
│  │  - Ed25519 signature validation (offline)                     │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Related documentation

- [ADR 014: Payment provider choice](../artifacts/adr/014-payment-provider-paddle.md) — Why we chose Paddle
- [ADR 015: License model](../artifacts/adr/015-license-model-agpl-trial.md) — Why AGPL + trial
- [License server README](../../apps/license-server/README.md) — Setup and deployment

## User flow

### Trial period

1. User downloads Cmdr from getcmdr.com
2. On first launch, app records the timestamp
3. App shows remaining trial days in the UI
4. Full functionality is available during trial

### Purchase

1. User clicks "Buy license" (in app or on website)
2. Redirects to Paddle checkout
3. User pays $29
4. Paddle sends webhook to license server
5. License server generates Ed25519-signed key
6. User receives license key via email

### Activation

1. User opens Cmdr → Menu → Enter license key
2. User pastes license key
3. App validates Ed25519 signature locally (no network needed)
4. App stores license in macOS Keychain
5. App shows "Licensed" status

### Validation

The license key format is: `base64(payload).base64(signature)`

Payload contains:
```json
{
  "email": "user@example.com",
  "transactionId": "txn_xxx",
  "issuedAt": "2026-01-08T12:00:00Z"
}
```

The app embeds the Ed25519 public key at compile time. Validation is purely local — no server call needed.

## Implementation

### Tauri app (`apps/desktop/src-tauri/src/licensing/`)

| File              | Purpose                                        |
|-------------------|------------------------------------------------|
| `mod.rs`          | Module entry, shared types                     |
| `trial.rs`        | 14-day trial tracking using tauri-plugin-store |
| `verification.rs` | Ed25519 signature validation                   |

### Tauri commands

| Command              | Description                                    |
|----------------------|------------------------------------------------|
| `get_license_status` | Returns `Licensed`, `Trial`, or `TrialExpired` |
| `activate_license`   | Validates and stores a license key             |
| `get_license_info`   | Returns stored license info                    |
| `reset_trial`        | Debug only — resets trial for testing          |

### License server (`apps/license-server/`)

Cloudflare Worker that:
1. Receives Paddle webhooks
2. Generates Ed25519-signed license keys
3. Sends license emails via Resend

See [license server README](../../apps/license-server/README.md) for full documentation.

## Security considerations

- **Private key protection**: Ed25519 private key is stored as Cloudflare secret, never in code
- **Public key embedding**: Public key is embedded in compiled binary
- **Offline validation**: No server dependency for validation — works without internet
- **Webhook verification**: Paddle webhook signatures are verified to prevent forgery
- **Local storage**: License keys stored in tauri-plugin-store (SQLite-backed)

## Pricing

| Tier    | Price        | Includes                     |
|---------|--------------|------------------------------|
| Trial   | Free         | 14 days full access          |
| License | $29 one-time | Lifetime updates, 2 machines |

## What paying users get

Compared to self-compiling:

- ✅ Signed and notarized macOS binary (no Gatekeeper warnings)
- ✅ Automatic updates
- ✅ Priority support
- ✅ Supporting indie development

## Development and testing

### Mock mode (`CMDR_MOCK_LICENSE`)

For local development and testing, you can mock the license status using the `CMDR_MOCK_LICENSE` environment variable. This bypasses real license validation and returns a fixed status.

**Usage:**

```bash
CMDR_MOCK_LICENSE=expired pnpm tauri dev
```

**Possible values:**

| Value | Description | Window title |
|-------|-------------|--------------|
| `personal` | No license - personal use only (no reminder) | "Cmdr – Personal use only" |
| `personal_reminder` | Personal license (shows commercial reminder) | "Cmdr – Personal use only" |
| `supporter` | Supporter license (no reminder) | "Cmdr – Personal" |
| `supporter_reminder` | Supporter license (shows commercial reminder) | "Cmdr – Personal" |
| `commercial` | Commercial subscription license | "Cmdr" |
| `perpetual` | Commercial perpetual license | "Cmdr" |
| `expired` | Expired commercial license (shows modal) | "Cmdr – Personal use only" |
| `expired_no_modal` | Expired license (modal already dismissed) | "Cmdr – Personal use only" |

**Mock data:**

- Commercial/perpetual org: "Test Corporation" / "Perpetual Inc."
- Expiration date (commercial): 2027-01-10
- Expired at (expired): 2026-01-01

### Commercial reminder for personal users

Personal and Supporter license holders see a friendly reminder modal every 30 days encouraging them to get a commercial license if using Cmdr at work. The modal:

- Appears once every 30 days (not on every launch)
- Has two actions: "Get commercial license" and "Remind me in 30 days"
- Clicking "Remind me in 30 days" resets the 30-day timer
- State is stored in `license.json` and resets on app reinstall

To test:

```bash
CMDR_MOCK_LICENSE=personal_reminder pnpm tauri dev
```

**Notes:**

- Only works in debug builds (`pnpm tauri dev`, not `pnpm tauri build`)
- Environment variable must be set for the Tauri process, not just the terminal
- When `CMDR_MOCK_LICENSE` is set, server validation is skipped entirely
- The About window (Cmdr → About cmdr) reflects the mocked license status

**Troubleshooting:**

If mock mode doesn't seem to work:

1. Make sure you're using `pnpm tauri dev`, not `pnpm dev` (the latter runs only the frontend)
2. Check the terminal output for `[License]` log messages
3. Open DevTools (Cmd+Opt+I in dev mode) and check for `[License]` console logs
4. Verify the env var is set: `echo $CMDR_MOCK_LICENSE` before running
