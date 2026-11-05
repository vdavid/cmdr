# API server

Cloudflare Worker that serves as the backend for Cmdr: licensing, telemetry, crash reports, downloads, and admin
endpoints.

For architecture, data flow, environments, and dev instructions, see [CLAUDE.md](CLAUDE.md).

## Purchase flow

```
User clicks "Buy" on getcmdr.com
  → Paddle checkout
  → User pays → Paddle sends webhook to this server
  → Server generates Ed25519-signed license key
  → Server emails license key to user via SMTP2Go (EU endpoint)
  → User enters key in Cmdr app
  → App validates signature locally (no server call needed)
```

## First-time setup

These steps only need to be done once, by a human. After this, agents can handle dev and deployment via the instructions
in [CLAUDE.md](CLAUDE.md).

1. `pnpm install`
2. Generate **two** Ed25519 key pairs with `pnpm run generate-keys`, one for production and one for local dev. They stay
   separate for the same reason sandbox and live Paddle keys do: the production private key can mint a valid license for
   every shipped build, so it belongs in Cloudflare and nowhere else, least of all a dev config file.
3. Copy each public key hex into its arm of `PUBLIC_KEY_HEX` in
   [`verification.rs`](../desktop/src-tauri/src/licensing/verification.rs): the dev key under `debug_assertions`, the
   production key under `not(debug_assertions)`. That mirrors `LICENSE_SERVER_URL`, so a build trusts exactly the server
   it talks to. The private halves go to `.dev.vars` (dev, step 10) and `wrangler secret put` (production, step 9).
4. **SMTP2Go**: Verify `getcmdr.com` as a sender domain (its DNS records go in Cloudflare), then create an API key in
   the dashboard allowed ONLY `/email/send` and `/stats/email_cycle`. A broader key can delete the sending domain;
   [DETAILS.md](DETAILS.md#configuration) says why.
5. **Paddle**: Create accounts at https://paddle.com (live) and https://sandbox-vendors.paddle.com (sandbox).
6. **Paddle (both environments)**: Create product "Cmdr" (standard tax category), then one price:
   - Commercial license: $59, one-time, `tax_mode: external` so VAT is added on top. Point both
     `PRICE_ID_COMMERCIAL_PERPETUAL` (here) and `PUBLIC_PADDLE_PRICE_ID_COMMERCIAL` (the website) at it. The server-side
     name still says "perpetual" because that's the license type the webhook issues; an unmapped price ID falls back to
     `commercial_subscription`, which would hand the buyer a license that expires. The ids that exist today, and which
     retired prices stay mapped, are listed in `docs/business/pricing.md`.
7. **Paddle (both environments)**: Create notification destination → webhook URL, subscribe to `transaction.completed`.
   - Sandbox: `https://unsickerly-acclivitous-lala.ngrok-free.dev/webhook/paddle` (for local dev via ngrok)
   - Live: `https://api.getcmdr.com/webhook/paddle`
8. **Cloudflare**: Set `CLOUDFLARE_API_TOKEN` (see [cloudflare.md](../../docs/tooling/cloudflare.md#api-token)).
9. **Wrangler secrets** (deployed worker, live values):
   ```
   npx wrangler secret put PADDLE_WEBHOOK_SECRET_LIVE
   npx wrangler secret put PADDLE_WEBHOOK_SECRET_SANDBOX
   npx wrangler secret put PADDLE_API_KEY_LIVE
   npx wrangler secret put PADDLE_API_KEY_SANDBOX
   npx wrangler secret put ED25519_PRIVATE_KEY
   npx wrangler secret put SMTP2GO_API_KEY
   npx wrangler secret put PADDLE_ENVIRONMENT              # "live"
   npx wrangler secret put PRICE_ID_COMMERCIAL_PERPETUAL   # live price ID of the $59 one-time price
   npx wrangler secret put PRICE_ID_COMMERCIAL_SUBSCRIPTION # retired $59/year price; stays mapped, one live customer
   npx wrangler secret put HEALTHCHECKS_PING_URL           # optional: cron dead-man's switch
   ```
10. **`.dev.vars`** (local dev, sandbox values): see [CLAUDE.md](CLAUDE.md#configuration) for the full table.
11. Save `keys/private.key` in a secure store, then delete it from the filesystem.
12. Deploy: `cd apps/api-server && npx wrangler deploy`

## Testing Paddle checkout

Test the full purchase flow through Paddle's sandbox. Only works with sandbox credentials.

### Prerequisites

1. The **default payment link** in Paddle sandbox should be `http://localhost:4829` (the local website). Live uses
   `https://getcmdr.com`. Check at https://sandbox-vendors.paddle.com/checkout-settings.
2. Create a **client-side token**: https://sandbox-vendors.paddle.com/authentication-v2 → "Client-side tokens" tab →
   create (starts with `test_`).

### Run the test

Start the local website (`pnpm dev` in `apps/website`) and the local API server (`pnpm dev` in `apps/api-server` \+
ngrok). Then use the buy buttons on http://localhost:4829/pricing/.

Use test card `4000 0566 5566 5556` / CVC `100`. More test cards:
https://developer.paddle.com/concepts/payment-methods/credit-debit-card#test-payment-details

**Standalone checkout playground:** There's also a minimal test page at `pnpm test:checkout` (port 3333). To use it,
temporarily change the default payment link to `http://localhost:3333` in the sandbox checkout settings.

### Troubleshooting

| Error                            | Fix                                                      |
| -------------------------------- | -------------------------------------------------------- |
| "Something went wrong"           | Check default payment link matches your localhost origin |
| Token doesn't start with `test_` | Use sandbox token from sandbox-vendors.paddle.com        |
| "Invalid price"                  | Ensure price ID is from the same sandbox account         |

## Architecture decisions

- Payment provider: Paddle (Merchant of Record). See [CLAUDE.md](CLAUDE.md) for rationale.
- License model: BSL 1.1 with free personal use. See [CLAUDE.md](CLAUDE.md).
- [CLAUDE.md](CLAUDE.md): full technical reference
