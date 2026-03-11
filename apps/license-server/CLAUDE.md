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
| `src/device-tracking.ts`                    | Device set helpers: prune stale devices, alert threshold      |
| `src/license.test.ts`, `src/paddle.test.ts` | Vitest tests                                                  |
| `src/device-tracking.test.ts`               | Tests for device tracking helpers                             |
| `scripts/generate-keys.js`                  | Ed25519 key pair generation (run once at setup)               |
| `scripts/setup-cf-infra.sh`                 | Cloudflare KV namespace provisioning                          |

## Routes

| Method | Path                       | Auth         | Purpose                                            |
| ------ | -------------------------- | ------------ | -------------------------------------------------- |
| GET    | `/`                        | —            | Health check                                       |
| POST   | `/webhook/paddle`          | HMAC sig     | Purchase completed → generate & email key(s)       |
| POST   | `/activate`                | —            | Exchange short code → full cryptographic key       |
| POST   | `/validate`                | —            | Check subscription status via Paddle API           |
| POST   | `/admin/generate`          | Bearer token | Manual key generation (customer service / testing) |
| GET    | `/download/:version/:arch` | —            | Log download to Analytics Engine, 302 → GitHub     |

## Environments

Sandbox (dev) and live (prod) are **completely separated**. They share the same codebase but have different Paddle
accounts, API keys, price IDs, webhook secrets, and notification destinations. There is no cross-environment routing.

`PADDLE_ENVIRONMENT` (in `wrangler.toml` and overridable as a wrangler secret) controls which Paddle API base URL and
API key the server uses. Set to `"sandbox"` by default (from `wrangler.toml`). The deployed worker overrides it to
`"live"` via a wrangler secret.

### Configuration

| Secret / var                       | `.dev.vars` (local dev)          | Wrangler secret (deployed worker) |
| ---------------------------------- | -------------------------------- | --------------------------------- |
| `PADDLE_ENVIRONMENT`               | `"sandbox"` (from wrangler.toml) | `"live"`                          |
| `PADDLE_WEBHOOK_SECRET_SANDBOX`    | Sandbox secret                   | Sandbox secret (for safety)       |
| `PADDLE_WEBHOOK_SECRET_LIVE`       | —                                | Live secret                       |
| `PADDLE_API_KEY_SANDBOX`           | Sandbox API key                  | —                                 |
| `PADDLE_API_KEY_LIVE`              | —                                | Live API key                      |
| `PRICE_ID_COMMERCIAL_SUBSCRIPTION` | Sandbox price ID                 | Live price ID                     |
| `PRICE_ID_COMMERCIAL_PERPETUAL`    | Sandbox price ID                 | Live price ID                     |
| `ED25519_PRIVATE_KEY`              | Private key hex                  | Same private key hex              |
| `RESEND_API_KEY`                   | Resend key                       | Same Resend key                   |

**Paddle dashboards**: [sandbox](https://sandbox-vendors.paddle.com) | [live](https://vendors.paddle.com)

### Webhook verification

`verifyPaddleWebhookMulti` tries both `PADDLE_WEBHOOK_SECRET_LIVE` and `PADDLE_WEBHOOK_SECRET_SANDBOX` when verifying
incoming webhooks. This is a safety net — in practice, the sandbox dashboard sends webhooks only to the sandbox
destination (ngrok for local dev), and the live dashboard sends only to the live destination (`license.getcmdr.com`).

## Data flow

```
Paddle webhook → HMAC verify (tries both live + sandbox secrets)
  → idempotency check (KV key: "transaction:{id}", 7-day TTL)
  → Paddle API: fetch customer details
  → per seat: generateLicenseKey() → generateShortCode() → KV.put(code, {fullKey, orgName})
  → sendLicenseEmail() via Resend
  → KV.put(idempotencyKey, "processed")

App activation: POST /activate → KV.get(shortCode) → return fullKey

Subscription validation: POST /validate → Paddle API transactions + subscriptions
  → HTTP 200 + ValidationResponse on success or invalid transaction (Paddle 404)
  → HTTP 502 + { error: "upstream_error" } if Paddle API unreachable or returns server error
  → if deviceId present: track device in KV (devices:{seatTransactionId}), log to Analytics Engine
  → if device count >= 6 and not recently alerted: send alert email to legal@getcmdr.com

Download redirect: GET /download/:version/:arch → log to Analytics Engine → 302 to GitHub Releases
```

## Key patterns

**Short code format:** `CMDR-XXXX-XXXX-XXXX` using 31 unambiguous chars (excludes 0/O/1/I/L). Rejection sampling avoids
modulo bias (max unbiased byte = `256 - (256 % 31)`).

**License key format:** `base64(JSON payload).base64(Ed25519 signature)`. Payload contains: email, transactionId,
issuedAt, type, organizationName.

**License types:** `commercial_subscription` | `commercial_perpetual`

**Idempotency:** 7-day KV entry per transaction. If email throws after KV writes but before the idempotency key is set,
Paddle's retry re-generates and re-sends — intentional design.

**Price ID → license type mapping:** `getLicenseTypeFromPriceId()` in `paddle-api.ts` maps Paddle price IDs (from
`PRICE_ID_*` env vars) to license types. Unknown price IDs fall back to `commercial_subscription` for backwards
compatibility.

**Security:** Admin bearer token compared with `constantTimeEqual` (XOR-accumulate, timing-safe). All secrets are
Cloudflare secrets (`wrangler secret put`), never in `wrangler.toml`.

**No database:** All state lives in Cloudflare KV. Short codes never expire (perpetual licenses last forever);
subscription validity is checked live via Paddle API.

**Validation error granularity:** `/validate` distinguishes "Paddle says invalid" (HTTP 200 + `status: "invalid"`) from
"Paddle is unreachable" (HTTP 502 + `{ error: "upstream_error" }`). `paddle-api.ts` throws `PaddleApiError` on
network/5xx errors and returns `null` on 404 (transaction not found). This lets the desktop app fall back to cached
status on transient Paddle outages instead of overwriting a valid "active" cache with "invalid."

**Download tracking:** Uses Cloudflare Analytics Engine (binding: `DOWNLOADS`, dataset: `cmdr_downloads`).
`writeDataPoint` is fire-and-forget. Data schema: indexes=[version], blobs=[version, arch, country, continent],
doubles=[1]. Query via CF Analytics Engine SQL API.

**Device tracking (fair use):** On each `/validate` call with a `deviceId`, the server tracks the device in KV
(`devices:{seatTransactionId}`) and logs to Analytics Engine (binding: `DEVICE_COUNTS`, dataset: `cmdr_device_counts`).
Devices older than 90 days are pruned on each write. If 6+ devices are active and no alert was sent in the past 30 days,
an internal email is sent to `legal@getcmdr.com` via Resend. Device tracking is fire-and-forget and never affects the
validation response. The KV value stores a `DeviceSet` with device hashes mapped to last-seen timestamps plus an
optional `lastAlertedAt`. See `docs/specs/fair-use-device-tracking-plan.md` for the full plan.

## Local development

### Run locally

```sh
pnpm dev          # starts wrangler dev server on :8787
pnpm test         # vitest unit tests
```

### Expose locally via ngrok (for Paddle sandbox webhooks)

```bash
ngrok http 8787 --url unsickerly-acclivitous-lala.ngrok-free.dev
```

The ngrok domain is stable across restarts. The Paddle sandbox notification destination already points to
`https://unsickerly-acclivitous-lala.ngrok-free.dev/webhook/paddle`.

### Generate a test license key

For quick local testing of crypto verification and the activation UI, use `/admin/generate`. It accepts the Paddle
sandbox webhook secret as the bearer token:

```bash
curl -X POST http://localhost:8787/admin/generate \
  -H "Authorization: Bearer $(grep PADDLE_WEBHOOK_SECRET_SANDBOX apps/license-server/.dev.vars | cut -d= -f2-)" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","type":"commercial_subscription","organizationName":"Test Corp"}'
```

Returns `code` (short code like `CMDR-ABCD-EFGH-1234`) and `type`. Change `type` to `commercial_perpetual` for a
perpetual license. These keys use synthetic transaction IDs (`manual-*`), so they won't pass server validation via
`/validate` — they're for offline crypto + UI testing only.

For end-to-end testing including `/validate`, use the Paddle sandbox checkout flow (see
[README.md](README.md#testing-paddle-checkout)).

### Testing Paddle checkout (sandbox)

See [README.md](README.md#testing-paddle-checkout) — requires setting up a Paddle client-side token and a default
payment link in the sandbox dashboard. This is an interactive, human-driven flow.

## Deployment

```bash
cd apps/license-server && npx wrangler deploy
```

Deployed to `license.getcmdr.com` via Cloudflare custom domain (declared in `wrangler.toml` `[[routes]]`). Fallback URL:
`cmdr-license-server.veszelovszki.workers.dev`.

### Troubleshooting deployment

- **522 on `license.getcmdr.com`**: Custom domain isn't routing to the Worker. Check `npx wrangler deploy` output shows
  `license.getcmdr.com (custom domain)`. The `[[routes]]` block in `wrangler.toml` may be missing, or a DNS record is
  blocking it.
- **"externally managed DNS records"**: Delete the manual DNS record via CF API/dashboard, then redeploy.
- **"kv bindings require kv write perms"**: API token missing "Workers KV Storage: Edit". Update at
  https://dash.cloudflare.com/profile/api-tokens.
- **Workers.dev works but custom domain doesn't**: Domain binding failed. Check error in deploy output.

## Business rules

**Commercial prices use `external` tax_mode.** Commercial customers pay tax on top of the listed price. This is
configured per-price in the Paddle dashboard (both sandbox and live).

## Key decisions

**Decision**: `PADDLE_ENVIRONMENT` env var controls sandbox vs live routing, rather than inferring from transaction IDs.
**Why**: Both sandbox and live transactions use the same `txn_` prefix — there's no reliable way to detect the
environment from a transaction ID. An explicit env var is unambiguous. `wrangler.toml` defaults to `"sandbox"` for local
dev; the deployed worker overrides to `"live"` via a wrangler secret.

**Decision**: Price IDs stored as env vars (`PRICE_ID_*`) rather than hardcoded. **Why**: Sandbox and live Paddle
accounts have different price IDs for the same products. Env vars let each environment use its own IDs without code
changes. `.dev.vars` has sandbox IDs; wrangler secrets have live IDs.

## Gotchas

**Gotcha**: Paddle preserves `custom_data` key casing exactly as passed in from checkout. **Why**: The checkout passes
`organizationName` (camelCase), and both webhook payloads and API responses return it in camelCase. The code must use
`organizationName`, not `organization_name`.

**Gotcha**: `verifyPaddleWebhookMulti` tries both webhook secrets even though environments are separated. **Why**:
Safety net. If a sandbox webhook somehow reaches the production endpoint (or vice versa), it still verifies rather than
silently failing. Costs one extra HMAC check on mismatch.

## Dependencies

Runtime: `hono`, `@noble/ed25519`, `resend` Dev: `wrangler`, `vitest`, `typescript`, `eslint`, `prettier`

**Decision**: Paddle as Merchant of Record (not Stripe, Gumroad, LemonSqueezy, or Polar).
**Why**: All-inclusive pricing (5% + $0.50, no hidden non-US or EU payout fees), aggregate monthly payouts (one invoice for accountant instead of per-transaction), handles global VAT/GST calculation and remittance, established reputation (Sketch, etc.). On a $29 sale: $1.95 fee → $27.05 net. At 30k sales, saves ~$7k/year vs LemonSqueezy. Stripe was rejected because solo-dev handling VAT in 27+ EU countries is impractical (Stripe is a payment processor, not an MoR).

**Decision**: BSL 1.1 license with free personal use (supersedes earlier AGPL + trial model).
**Why**: The AGPL + trial model felt pushy for hobbyists (trial countdown, nagware). BSL gives friction-free personal use (no nags), clear commercial terms (businesses know they must pay), and simpler enforcement (title bar shows license type, honor system beats trial timers). Source converts to AGPL-3.0 after 3 years per release.

See also: `apps/desktop/src/lib/licensing/CLAUDE.md` — full frontend licensing feature overview
