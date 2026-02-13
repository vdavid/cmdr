# Licensing

This document describes Cmdr's licensing and payment system.

## Overview

Cmdr uses a **BSL 1.1 license with free personal use**:

1. Personal use is free forever — no trial, no nags. Optional $10 sponsor license also available.
2. Commercial use requires a paid license ($59/year or $199 perpetual)
3. Licenses are validated using Ed25519 cryptographic signatures
4. Source code converts to AGPL-3.0 after 3 years

## Pricing tiers

| Tier                    | Price    | Commercial use | Notes                            |
|-------------------------|----------|----------------|----------------------------------|
| Personal                | Free     | No             | All features, unlimited machines |
| Supporter               | $10      | No             | Badge in app, warm fuzzy feeling |
| Commercial subscription | $59/year | Yes            | Auto-renews annually             |
| Commercial perpetual    | $199     | Yes            | 3 years of updates included      |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Purchase flow                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. User clicks "Buy" on getcmdr.com                                │
│           ↓                                                          │
│  2. Paddle checkout opens (overlay on website)                      │
│           ↓                                                          │
│  3. User pays → Paddle sends webhook to license server              │
│           ↓                                                          │
│  4. Server fetches customer email via Paddle API                    │
│           ↓                                                          │
│  5. Server generates Ed25519-signed license key(s)                  │
│           ↓                                                          │
│  6. Server emails license key(s) to user via Resend                 │
│           ↓                                                          │
│  7. User enters key in Cmdr app                                     │
│           ↓                                                          │
│  8. App validates signature locally (no server call needed)         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Related documentation

- [ADR 014: Payment provider choice](../adr/014-payment-provider-paddle.md) — Why we chose Paddle
- [ADR 016: License model](../adr/016-license-model-bsl.md) — Why BSL + free personal use
- [License server README](../../apps/license-server/README.md) — Server setup and deployment

## User flow

### Personal use (free)

1. User downloads Cmdr from getcmdr.com
2. App shows "Personal use only" in title bar
3. Full functionality, no time limits, no nags

### Purchase

1. User clicks "Buy" on getcmdr.com/pricing
2. Paddle checkout overlay opens
3. User completes payment
4. License server generates key(s) and emails them
5. User receives email with license key(s)

### Activation

1. User opens Cmdr → Cmdr menu → Enter license key...
2. User pastes license key
3. App validates Ed25519 signature locally (no network needed)
4. App stores license in local storage
5. Title bar updates to reflect license type

### License types in the app

| License type          | Title bar                  | About window             |
|-----------------------|----------------------------|--------------------------|
| No license (personal) | "Cmdr – Personal use only" | "Personal use only"      |
| Supporter             | "Cmdr – Personal"          | "Supporter" with badge   |
| Commercial sub        | "Cmdr"                     | "Licensed to: Acme Corp" |
| Commercial perpetual  | "Cmdr"                     | "Licensed to: Acme Corp" |
| Expired commercial    | "Cmdr – Personal use only" | Shows expiration notice  |

## License key format

The license key format is: `base64(payload).base64(signature)`

Payload contains:

```json
{
  "email": "user@example.com",
  "transactionId": "txn_xxx",
  "issuedAt": "2026-01-08T12:00:00Z",
  "type": "commercial_subscription"
}
```

The app embeds the Ed25519 public key at compile time. Validation is purely local — no server call needed for
verification.

## Implementation

### Tauri app (`apps/desktop/src-tauri/src/licensing/`)

| File              | Purpose                      |
|-------------------|------------------------------|
| `mod.rs`          | Module entry, shared types   |
| `verification.rs` | Ed25519 signature validation |

### Tauri commands

| Command              | Description                        |
|----------------------|------------------------------------|
| `get_license_status` | Returns license status and type    |
| `activate_license`   | Validates and stores a license key |
| `get_license_info`   | Returns stored license info        |

### License server (`apps/license-server/`)

Cloudflare Worker that:

1. Receives Paddle webhooks (`transaction.completed`)
2. Fetches customer details via Paddle API
3. Generates Ed25519-signed license keys (one per quantity purchased)
4. Sends license email(s) via Resend

See [license server README](../../apps/license-server/README.md) for full setup instructions.

### Website (`apps/website/`)

The pricing page uses Paddle.js to open checkout overlays. Configuration is via environment variables:

```bash
# apps/website/.env
PUBLIC_PADDLE_CLIENT_TOKEN=test_xxx          # or live_xxx for production
PUBLIC_PADDLE_ENVIRONMENT=sandbox            # or live
PUBLIC_PADDLE_PRICE_ID_SUPPORTER=pri_xxx
PUBLIC_PADDLE_PRICE_ID_COMMERCIAL_SUBSCRIPTION=pri_xxx
PUBLIC_PADDLE_PRICE_ID_COMMERCIAL_PERPETUAL=pri_xxx
```

## Paddle setup

### Prerequisites

1. Paddle account (sandbox for testing, live for production)
2. Resend account for transactional emails
3. Ed25519 key pair (generate via `pnpm run generate-keys` in license-server)

### Sandbox setup (for local development)

1. **Create Paddle sandbox account** at https://sandbox-vendors.paddle.com

2. **Create products and prices**:
    - Go to https://sandbox-vendors.paddle.com/products-v2
    - Create "Cmdr" product
    - Add 3 prices: $10 one-time (supporter), $59/year recurring (subscription), $199 one-time (perpetual)
    - Note each price ID (`pri_xxx...`)

3. **Create webhook**:
    - Go to https://sandbox-vendors.paddle.com/notifications-v2
    - Click "New destination"
    - URL: Your ngrok/cloudflared URL + `/webhook/paddle`
    - Select event: `transaction.completed`
    - Copy the webhook secret (`pdl_ntfset_xxx...`)

4. **Create client-side token**:
    - Go to https://sandbox-vendors.paddle.com/authentication-v2
    - Click "Client-side tokens" tab
    - Create token (will start with `test_`)

5. **Set default payment link**:
    - Go to https://sandbox-vendors.paddle.com/checkout-settings
    - Set "Default payment link" to `http://localhost:4321` (or any URL)

6. **Configure license server** (`apps/license-server/.dev.vars`):
   ```
   PADDLE_WEBHOOK_SECRET_SANDBOX=pdl_ntfset_xxx
   PADDLE_API_KEY_SANDBOX=pdl_sdbx_apikey_xxx
   ED25519_PRIVATE_KEY=your_hex_private_key
   RESEND_API_KEY=re_xxx
   PRICE_ID_SUPPORTER=pri_xxx
   PRICE_ID_COMMERCIAL_SUBSCRIPTION=pri_xxx
   PRICE_ID_COMMERCIAL_PERPETUAL=pri_xxx
   ```

7. **Configure website** (`apps/website/.env`):
   ```
   PUBLIC_PADDLE_CLIENT_TOKEN=test_xxx
   PUBLIC_PADDLE_ENVIRONMENT=sandbox
   PUBLIC_PADDLE_PRICE_ID_SUPPORTER=pri_xxx
   PUBLIC_PADDLE_PRICE_ID_COMMERCIAL_SUBSCRIPTION=pri_xxx
   PUBLIC_PADDLE_PRICE_ID_COMMERCIAL_PERPETUAL=pri_xxx
   ```

8. **Test the flow**:
    - Start license server: `cd apps/license-server && pnpm dev`
    - Expose via ngrok: `ngrok http 8787`
    - Update webhook URL in Paddle to ngrok URL
    - Start website: `cd apps/website && pnpm dev`
    - Open http://localhost:4321/pricing and click a buy button
    - Use test card: `4000 0566 5566 5556`, any future expiry, CVC `100`

### Production setup

Same as sandbox, but:

- Use https://vendors.paddle.com instead of sandbox-vendors
- Client token starts with `live_` instead of `test_`
- Set `PUBLIC_PADDLE_ENVIRONMENT=live` in website
- Deploy license server: `cd apps/license-server && pnpm run deploy`
- Set production secrets via `npx wrangler secret put <NAME>`

## Security considerations

- **Private key protection**: Ed25519 private key is stored as Cloudflare secret, never in code
- **Public key embedding**: Public key is embedded in compiled binary
- **Offline validation**: No server dependency for validation — works without internet
- **Webhook verification**: Paddle webhook signatures verified via HMAC-SHA256
- **Customer data**: Email fetched via Paddle API (not included in webhook payload)

## Commercial reminder

Personal and Supporter license holders see a friendly reminder modal every 30 days encouraging them to get a commercial
license if using Cmdr at work. The modal:

- Appears once every 30 days (not on every launch)
- Has two actions: "Get commercial license" and "Remind me in 30 days"
- State is stored in `license.json` and resets on app reinstall

## Development and testing

### Mock mode (`CMDR_MOCK_LICENSE`)

For local development, mock the license status:

```bash
CMDR_MOCK_LICENSE=commercial pnpm tauri dev
```

**Possible values:**

| Value                | Description                                   | Title bar                  |
|----------------------|-----------------------------------------------|----------------------------|
| `personal`           | No license - personal use only (no reminder)  | "Cmdr – Personal use only" |
| `personal_reminder`  | Personal license (shows commercial reminder)  | "Cmdr – Personal use only" |
| `supporter`          | Supporter license (no reminder)               | "Cmdr – Personal"          |
| `supporter_reminder` | Supporter license (shows commercial reminder) | "Cmdr – Personal"          |
| `commercial`         | Commercial subscription license               | "Cmdr"                     |
| `perpetual`          | Commercial perpetual license                  | "Cmdr"                     |
| `expired`            | Expired commercial license (shows modal)      | "Cmdr – Personal use only" |
| `expired_no_modal`   | Expired license (modal already dismissed)     | "Cmdr – Personal use only" |

**Notes:**

- Only works in debug builds (`pnpm tauri dev`)
- When set, server validation is skipped entirely
