# Testing the purchase flow (sandbox)

End-to-end test of the full buy-activate flow using Paddle sandbox. Covers website checkout, webhook delivery, license
key generation, and activation in the desktop app.

## Prerequisites (one-time)

- Paddle sandbox default payment link set to `http://localhost:4829`
  ([checkout settings](https://sandbox-vendors.paddle.com/checkout-settings))
- Paddle sandbox client-side token (starts with `test_`) in `apps/website/.env` as `PUBLIC_PADDLE_CLIENT_TOKEN`
  ([create one](https://sandbox-vendors.paddle.com/authentication-v2))
- Sandbox price IDs in `apps/website/.env` (`PUBLIC_PADDLE_PRICE_ID_*`) and `apps/api-server/.dev.vars` (`PRICE_ID_*`)
- ngrok installed (`brew install ngrok`) with auth token configured

See [API server README](../../apps/api-server/README.md) and [website .env.example](../../apps/website/.env.example) for
full setup.

## Start the services

Three terminals:

```bash
# 1. API server
cd apps/api-server && pnpm dev

# 2. ngrok tunnel (exposes API server for Paddle webhooks)
ngrok http 8787 --url unsickerly-acclivitous-lala.ngrok-free.dev

# 3. Website
cd apps/website && pnpm dev
```

Optionally, start the desktop app too if you want to test activation:

```bash
# 4. Desktop app
pnpm dev
```

## Buy a license

1. Open http://localhost:4829/pricing/
2. Click a buy button (for example, "Buy commercial license")
3. For commercial tiers, enter an organization name and email in the modal
4. In the Paddle checkout overlay, use test card `4000 0566 5566 5556`, CVC `100`, any future expiry
5. Complete the purchase

More test cards: https://developer.paddle.com/concepts/payment-methods/credit-debit-card#test-payment-details

## Verify the webhook

After checkout completes, the ngrok terminal should show a `POST /webhook/paddle` request, and the API server terminal
should log the key generation. The test email address receives a license key via Resend.

If the webhook doesn't arrive, check the Paddle sandbox
[notification log](https://sandbox-vendors.paddle.com/notifications).

## Activate in the desktop app

1. Open the desktop app (dev mode)
2. Open Settings (or About) and enter the license key or short code from the email
3. The app verifies the Ed25519 signature locally, then validates with the API server

For quicker activation testing without the full purchase flow, mint a key directly:

```bash
node apps/api-server/scripts/mint-license.js --email test@example.com --org "Test Corp" \
  --note "local testing" --api http://localhost:8787
```

These keys pass `/validate` too, resolved from the local `license_issuance` ledger rather than from Paddle, so run
`wrangler d1 migrations apply cmdr-telemetry --local` once first. Flags, revocation, and the production runbook:
`apps/api-server/src/licensing/DETAILS.md` § Manual licenses.

## Refund it

The sandbox notification destination needs `adjustment.created` and `adjustment.updated` beside `transaction.completed`.
Refund the transaction in full from the sandbox dashboard: Paddle approves sandbox refunds within ten minutes, and the
`adjustment.updated` that follows revokes the license. The app drops to Personal at its next check, and the dashboard's
Licenses page shows the row as revoked. Rules for partial refunds and chargebacks:
`apps/api-server/src/licensing/DETAILS.md` § Refunds.

## Detailed docs

- [API server CLAUDE.md](../../apps/api-server/CLAUDE.md): environments, webhook flow, local dev
- [API server README](../../apps/api-server/README.md): first-time setup, standalone checkout playground
- [Desktop licensing CLAUDE.md](../../apps/desktop/src/lib/licensing/CLAUDE.md): activation flow, license types
- ngrok generic tooling doc: tunnel setup
