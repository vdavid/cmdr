# License server

Cloudflare Worker (Hono) that handles the full license lifecycle: verifies Paddle webhooks, generates Ed25519-signed
license keys, stores short activation codes in KV, and emails keys via Resend.

## Key files

| File                                        | Purpose                                                       |
| ------------------------------------------- | ------------------------------------------------------------- |
| `src/index.ts`                              | Hono app: all routes, webhook processing, admin endpoint      |
| `src/license.ts`                            | Short code + license key generation, `LicenseType` enum       |
| `src/paddle.ts`                             | HMAC-SHA256 webhook verification, `constantTimeEqual`         |
| `src/paddle-api.ts`                         | Paddle REST client: transaction/subscription/customer fetch   |
| `src/email.ts`                              | Resend email delivery (HTML + plain text, multi-seat support) |
| `src/license.test.ts`, `src/paddle.test.ts` | Vitest tests                                                  |
| `scripts/generate-keys.js`                  | Ed25519 key pair generation (run once at setup)               |
| `scripts/setup-cf-infra.sh`                 | Cloudflare KV namespace provisioning                          |

## Routes

| Method | Path              | Auth         | Purpose                                            |
| ------ | ----------------- | ------------ | -------------------------------------------------- |
| GET    | `/`               | —            | Health check                                       |
| POST   | `/webhook/paddle` | HMAC sig     | Purchase completed → generate & email key(s)       |
| POST   | `/activate`       | —            | Exchange short code → full cryptographic key       |
| POST   | `/validate`       | —            | Check subscription status via Paddle API           |
| POST   | `/admin/generate` | Bearer token | Manual key generation (customer service / testing) |

## Data flow

```
Paddle webhook → HMAC verify (live + sandbox secrets)
  → idempotency check (KV key: "transaction:{id}", 7-day TTL)
  → Paddle API: fetch customer details
  → per seat: generateLicenseKey() → generateShortCode() → KV.put(code, {fullKey, orgName})
  → sendLicenseEmail() via Resend
  → KV.put(idempotencyKey, "processed")

App activation: POST /activate → KV.get(shortCode) → return fullKey

Subscription validation: POST /validate → Paddle API transactions + subscriptions
```

## Key patterns

**Short code format:** `CMDR-XXXX-XXXX-XXXX` using 31 unambiguous chars (excludes 0/O/1/I/L). Rejection sampling avoids
modulo bias (max unbiased byte = `256 - (256 % 31)`).

**License key format:** `base64(JSON payload).base64(Ed25519 signature)`. Payload contains: email, transactionId,
issuedAt, type, organizationName.

**License types:** `supporter` | `commercial_subscription` | `commercial_perpetual`

**Idempotency:** 7-day KV entry per transaction. If email throws after KV writes but before the idempotency key is set,
Paddle's retry re-generates and re-sends — intentional design.

**Dual environment:** Both live and sandbox webhook secrets coexist; `verifyPaddleWebhookMulti` tries both. Transaction
ID prefix (`txn_`) used to route Paddle API calls to sandbox vs live.

**Security:** Admin bearer token compared with `constantTimeEqual` (XOR-accumulate, timing-safe). All secrets are
Cloudflare secrets (`wrangler secret put`), never in `wrangler.toml`.

**No database:** All state lives in Cloudflare KV. Short codes never expire (perpetual licenses last forever);
subscription validity is checked live via Paddle API.

## Dependencies

Runtime: `hono`, `@noble/ed25519`, `resend` Dev: `wrangler`, `vitest`, `typescript`, `eslint`, `prettier`

## Running / deploying

```sh
pnpm dev          # local dev via wrangler
pnpm cf:deploy    # deploy to Cloudflare
pnpm test         # vitest unit tests
```
