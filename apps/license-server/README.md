# License server

Cloudflare Worker that handles Paddle webhooks and generates Ed25519-signed license keys for Cmdr.

For architecture, data flow, environments, and dev instructions, see [CLAUDE.md](CLAUDE.md).

## Purchase flow

```
User clicks "Buy" on getcmdr.com
  → Paddle checkout
  → User pays → Paddle sends webhook to this server
  → Server generates Ed25519-signed license key
  → Server emails license key to user via Resend
  → User enters key in Cmdr app
  → App validates signature locally (no server call needed)
```

## First-time setup

These steps only need to be done once, by a human. After this, agents can handle dev and deployment via the instructions
in [CLAUDE.md](CLAUDE.md).

1. `pnpm install`
2. Generate Ed25519 key pair: `pnpm run generate-keys` → `keys/public.key` + `keys/private.key`
3. Copy public key hex to `PUBLIC_KEY_HEX` in [`verification.rs`](../desktop/src-tauri/src/licensing/verification.rs)
4. **Resend**: Create API key at https://resend.com/api-keys. Add `getcmdr.com` domain at https://resend.com/domains
   (adds DNS records to Cloudflare automatically).
5. **Paddle**: Create accounts at https://paddle.com (live) and https://sandbox-vendors.paddle.com (sandbox).
6. **Paddle (both environments)**: Create product "Cmdr" (standard tax category), then three prices:
    - Personal supporter: $10, one-time
    - Commercial subscription: $59/year
    - Commercial perpetual: $199, one-time
7. **Paddle (both environments)**: Create notification destination → webhook URL, subscribe to `transaction.completed`.
    - Sandbox: `https://unsickerly-acclivitous-lala.ngrok-free.dev/webhook/paddle` (for local dev via ngrok)
    - Live: `https://license.getcmdr.com/webhook/paddle`
8. **Cloudflare**: Set `CLOUDFLARE_API_TOKEN` — see [cloudflare.md](../../docs/tooling/cloudflare.md#api-token).
9. **Wrangler secrets** (deployed worker — live values):
    ```
    npx wrangler secret put PADDLE_WEBHOOK_SECRET_LIVE
    npx wrangler secret put PADDLE_WEBHOOK_SECRET_SANDBOX
    npx wrangler secret put PADDLE_API_KEY_LIVE
    npx wrangler secret put PADDLE_API_KEY_SANDBOX
    npx wrangler secret put ED25519_PRIVATE_KEY
    npx wrangler secret put RESEND_API_KEY
    npx wrangler secret put PADDLE_ENVIRONMENT              # "live"
    npx wrangler secret put PRICE_ID_SUPPORTER              # live price ID
    npx wrangler secret put PRICE_ID_COMMERCIAL_SUBSCRIPTION # live price ID
    npx wrangler secret put PRICE_ID_COMMERCIAL_PERPETUAL   # live price ID
    ```
10. **`.dev.vars`** (local dev — sandbox values): see [CLAUDE.md](CLAUDE.md#configuration) for the full table.
11. Save `keys/private.key` in a secure store, then delete it from the filesystem.
12. Deploy: `cd apps/license-server && npx wrangler deploy`

## Testing Paddle checkout

Test the full purchase flow through Paddle's sandbox. Only works with sandbox credentials.

### Prerequisites

1. Set a **default payment link** in Paddle sandbox: https://sandbox-vendors.paddle.com/checkout-settings → enter
   `http://localhost:3333` → save.
2. Create a **client-side token**: https://sandbox-vendors.paddle.com/authentication-v2 → "Client-side tokens" tab →
   create (starts with `test_`).

### Run the test

```bash
PADDLE_CLIENT_TOKEN=test_xxx PADDLE_PRICE_ID=pri_xxx pnpm test:checkout
```

Open http://localhost:3333 and click "Buy Cmdr". Use test card `4000 0566 5566 5556` / CVC `100`. More test cards:
https://developer.paddle.com/concepts/payment-methods/credit-debit-card#test-payment-details

### Troubleshooting

| Error                            | Fix                                                  |
| -------------------------------- | ---------------------------------------------------- |
| "Something went wrong"           | Set default payment link in Paddle checkout settings |
| Token doesn't start with `test_` | Use sandbox token from sandbox-vendors.paddle.com    |
| "Invalid price"                  | Ensure price ID is from the same sandbox account     |

## Architecture decisions

- [ADR 014: Payment provider choice](../../docs/adr/014-payment-provider-paddle.md)
- [ADR 016: License model](../../docs/adr/016-license-model-bsl.md)
- [CLAUDE.md](CLAUDE.md) — full technical reference
