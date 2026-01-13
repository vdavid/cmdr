# License server

Cloudflare Worker that handles Paddle webhooks and generates Ed25519-signed license keys for Cmdr.

## Overview

This server receives purchase notifications from Paddle, generates cryptographically signed license keys, and emails
them to customers via Resend.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Purchase flow                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. User clicks "Buy" on getcmdr.com                                │
│           ↓                                                          │
│  2. Redirect to Paddle checkout (paddle.com/checkout/...)           │
│           ↓                                                          │
│  3. User pays → Paddle sends webhook to this server                 │
│           ↓                                                          │
│  4. Server generates Ed25519-signed license key                     │
│           ↓                                                          │
│  5. Server emails license key to user via Resend                    │
│           ↓                                                          │
│  6. User enters key in Cmdr app                                     │
│           ↓                                                          │
│  7. App validates signature locally (no server call needed)         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Setup

1. `pnpm install` to install dependencies
2. Gen Ed25519 pair: `pnpm run generate-keys` → `keys/public.key` (use in step 2) and `keys/private.key` (use in step 8)
3. Copy public key to `PUBLIC_KEY_HEX` in [`verification.rs`](../desktop/src-tauri/src/licensing/verification.rs)).
4. Resend: Sign up for / log in to resend.com, and create API key [here](https://resend.com/api-keys) or during
   onboarding.
5. Resend: Go to https://resend.com/domains, and add getcmdr.com. Let it adds its DNS records to Cloudflare.
6. Paddle: (first time only) Create a Paddle account at https://paddle.com.
7. Paddle: (first time only) Also create a Paddle sandbox account at https://sandbox-vendors.paddle.com/.
8. Paddle (sandbox): Go to https://sandbox-vendors.paddle.com/products-v2, click New product, and crate "Cmdr" or sg,
   "Standard digital goods" cat, some description.
9. Paddle (sandbox): Click "New price", and add $29+tax, one-time purchase, rest random.
10. Paddle (sandbox): Go to https://sandbox-vendors.paddle.com/notifications-v2, click New destination, add the webhook
    URL `https://cmdr-license-server.veszelovszki.workers.dev/webhook/paddle`, and tick event `transaction.completed`.
11. Paddle (sandbox): Click "..." → Edit destination → copy "Secret key". (Looks like `pdl_ntfset_01keh5q...`)
12. TODO: Paddle live!
13. Cloudflare: (first time only) `npx wrangler login` to log in to Cloudflare.
14. Cloudflare: Set secrets (supports both live and sandbox simultaneously):
    - `npx wrangler secret put PADDLE_WEBHOOK_SECRET_SANDBOX` - From sandbox webhook (step 11)
    - `npx wrangler secret put PADDLE_WEBHOOK_SECRET_LIVE` - From live webhook (once approved)
    - `npx wrangler secret put ED25519_PRIVATE_KEY` - From `keys/private.key`
    - `npx wrangler secret put RESEND_API_KEY` - From resend.com
15. Safest to save `keys/private.key` in a secure store at this point and delete it from the file system.
16. Cloudflare: Deploy worker: `pnpm run deploy`. Should output `https://cmdr-license-server.veszelovszki.workers.dev`.
17. Go to Sandbox/Notifications at https://sandbox-vendors.paddle.com/notifications-v2

## Local development

- Create `.dev.vars` with your secrets:
    ```
    PADDLE_WEBHOOK_SECRET_SANDBOX=pdl_ntfset_xxx
    PADDLE_WEBHOOK_SECRET_LIVE=pdl_ntfset_yyy
    ED25519_PRIVATE_KEY=your_private_key_hex
    RESEND_API_KEY=re_xxxxx
    ```
- Run `pnpm run dev`.
- Test it with `4000 0566 5566 5556` / CVC: `100`, or one of the other test cards from
  https://developer.paddle.com/concepts/payment-methods/credit-debit-card#test-payment-details.

## Testing Paddle checkout

Test the full purchase flow through Paddle's sandbox. **This only works with sandbox credentials.**

### Prerequisites

1. **Set a default payment link** in Paddle sandbox:
    - Go to https://sandbox-vendors.paddle.com/checkout-settings
    - Under "Default payment link", enter `http://localhost:3333` (or any URL)
    - Save

2. **Create a client-side token**:
    - Go to https://sandbox-vendors.paddle.com/authentication-v2
    - Click "Client-side tokens" tab
    - Create a new token (will start with `test_`)

### Run the test

```bash
# Get values from Paddle sandbox dashboard
PADDLE_CLIENT_TOKEN=test_xxx PADDLE_PRICE_ID=pri_xxx pnpm test:checkout
```

Then open http://localhost:3333 and click "Buy Cmdr".

### Troubleshooting

| Error                            | Fix                                                  |
| -------------------------------- | ---------------------------------------------------- |
| "Something went wrong"           | Set default payment link in Paddle Checkout settings |
| Token doesn't start with `test_` | Use sandbox token from sandbox-vendors.paddle.com    |
| "Invalid price"                  | Ensure price ID is from the same sandbox account     |

## Endpoints

| Method | Path              | Description                                        |
| ------ | ----------------- | -------------------------------------------------- |
| `GET`  | `/`               | Health check                                       |
| `POST` | `/webhook/paddle` | Paddle webhook (generates and emails license)      |
| `POST` | `/validate`       | Validate license key (returns subscription status) |
| `POST` | `/admin/generate` | Manual license generation (requires auth header)   |

## Architecture decisions

- See [ADR 014: Payment provider choice](../../docs/artifacts/adr/014-payment-provider-paddle.md) for why Paddle
- See [ADR 016: License model](../../docs/artifacts/adr/016-license-model-bsl.md) for the BSL license approach
- See [Licensing feature docs](../../docs/features/licensing.md) for the full feature overview
