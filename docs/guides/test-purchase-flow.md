# Testing the purchase flow (sandbox)

End-to-end test of the full buy-activate flow using Paddle sandbox. Covers website checkout, webhook delivery, license
key generation, and activation in the desktop app.

## Prerequisites (one-time)

- Paddle sandbox default payment link set to `http://localhost:4321`
  ([checkout settings](https://sandbox-vendors.paddle.com/checkout-settings))
- Paddle sandbox client-side token (starts with `test_`) in `apps/website/.env` as `PUBLIC_PADDLE_CLIENT_TOKEN`
  ([create one](https://sandbox-vendors.paddle.com/authentication-v2))
- Sandbox price IDs in `apps/website/.env` (`PUBLIC_PADDLE_PRICE_ID_*`) and `apps/license-server/.dev.vars`
  (`PRICE_ID_*`)
- ngrok installed (`brew install ngrok`) with auth token configured

See [license server README](../../apps/license-server/README.md) and
[website .env.example](../../apps/website/.env.example) for full setup.

## Start the services

Three terminals:

```bash
# 1. License server
cd apps/license-server && pnpm dev

# 2. ngrok tunnel (exposes license server for Paddle webhooks)
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

1. Open http://localhost:4321/pricing/
2. Click a buy button (for example, "Buy commercial license")
3. For commercial tiers, enter an organization name and email in the modal
4. In the Paddle checkout overlay, use test card `4000 0566 5566 5556`, CVC `100`, any future expiry
5. Complete the purchase

More test cards: https://developer.paddle.com/concepts/payment-methods/credit-debit-card#test-payment-details

## Verify the webhook

After checkout completes, the ngrok terminal should show a `POST /webhook/paddle` request, and the license server
terminal should log the key generation. The test email address receives a license key via Resend.

If the webhook doesn't arrive, check the Paddle sandbox
[notification log](https://sandbox-vendors.paddle.com/notifications).

## Activate in the desktop app

1. Open the desktop app (dev mode)
2. Open Settings (or About) and enter the license key or short code from the email
3. The app verifies the Ed25519 signature locally, then validates with the license server

For quicker activation testing without the full purchase flow, generate a test key directly:

```bash
curl -X POST http://localhost:8787/admin/generate \
  -H "Authorization: Bearer $(grep PADDLE_WEBHOOK_SECRET_SANDBOX apps/license-server/.dev.vars | cut -d= -f2-)" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","type":"commercial_subscription","organizationName":"Test Corp"}'
```

Note: keys from `/admin/generate` use synthetic transaction IDs and won't pass server validation via `/validate` — they
work for offline crypto and UI testing only.

## Detailed docs

- [License server CLAUDE.md](../../apps/license-server/CLAUDE.md) — environments, webhook flow, local dev
- [License server README](../../apps/license-server/README.md) — first-time setup, standalone checkout playground
- [Desktop licensing CLAUDE.md](../../apps/desktop/src/lib/licensing/CLAUDE.md) — activation flow, license types
- [ngrok tooling](../tooling/ngrok.md) — tunnel setup
