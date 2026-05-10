# API server

Cloudflare Worker (Hono) that serves as the backend API for Cmdr. Handles licensing (Paddle webhooks, Ed25519 key
generation, activation codes in KV), telemetry (crash reports, downloads, update checks in D1), admin endpoints, and
cron-based notifications. Deployed at `api.getcmdr.com` (`license.getcmdr.com` remains as a permanent alias for existing
app versions).

## Key files

| File                                        | Purpose                                                                             |
| ------------------------------------------- | ----------------------------------------------------------------------------------- |
| `src/index.ts`                              | Hono app assembly: mounts route modules, wires scheduled handler                    |
| `src/types.ts`                              | Shared types (`Bindings`), constants, and helpers (auth, validation)                |
| `src/licensing.ts`                          | Routes: `/activate`, `/validate`, `/webhook/paddle`, `/admin/generate`              |
| `src/admin.ts`                              | Routes: `/admin/stats`, `/admin/downloads`, `/admin/active-users`, `/admin/crashes` |
| `src/telemetry.ts`                          | Routes: `/crash-report`, `/update-check/:version`, `/download/:version/:arch`       |
| `src/likes.ts`                              | Routes: `/likes/:slug` (GET, POST, DELETE, OPTIONS)                                 |
| `src/error-report.ts`                       | Route: `POST /error-report` (multipart upload to R2, Discord notify)                |
| `src/error-report-eviction.ts`              | Eviction logic: 8/6 GB watermarks, KV lock, recompute helper                        |
| `src/discord.ts`                            | Discord webhook client (single-retry on 429, drop-on-failure)                       |
| `src/scheduled.ts`                          | Cron handler functions (crash notifications, aggregation, DB size, eviction)        |
| `src/license.ts`                            | Short code + license key generation, `LicenseType` enum                             |
| `src/paddle.ts`                             | HMAC-SHA256 webhook verification, `constantTimeEqual`                               |
| `src/paddle-api.ts`                         | Paddle REST client: transaction/subscription/customer fetch                         |
| `src/email.ts`                              | Resend email delivery (HTML + plain text, multi-seat support)                       |
| `src/device-tracking.ts`                    | Device set helpers: prune stale devices, alert threshold                            |
| `src/license.test.ts`, `src/paddle.test.ts` | Vitest tests                                                                        |
| `src/device-tracking.test.ts`               | Tests for device tracking helpers                                                   |
| `src/admin-stats.test.ts`                   | Tests for `/admin/stats` endpoint and activation counter                            |
| `src/admin-endpoints.test.ts`               | Tests for `/admin/downloads`, `/admin/active-users`, `/admin/crashes`               |
| `src/crash-report.test.ts`                  | Tests for `POST /crash-report` endpoint                                             |
| `src/download-and-update-check.test.ts`     | Tests for download redirect and update check routes                                 |
| `src/scheduled.test.ts`                     | Tests for cron handler (crash notifications, aggregation)                           |
| `scripts/generate-keys.js`                  | Ed25519 key pair generation (run once at setup)                                     |
| `scripts/setup-cf-infra.sh`                 | Cloudflare KV namespace provisioning                                                |

## Routes

| Method | Path                       | Auth         | Purpose                                                   |
| ------ | -------------------------- | ------------ | --------------------------------------------------------- |
| GET    | `/`                        | —            | Health check                                              |
| POST   | `/webhook/paddle`          | HMAC sig     | Purchase completed → generate & email key(s)              |
| POST   | `/activate`                | —            | Exchange short code → full cryptographic key              |
| POST   | `/validate`                | —            | Check subscription status via Paddle API                  |
| POST   | `/admin/generate`          | Bearer token | Manual key generation (customer service / testing)        |
| GET    | `/admin/stats`             | Bearer token | Activation count + device count (for analytics dashboard) |
| GET    | `/admin/downloads`         | Bearer token | Aggregated download data by day/version/arch/country      |
| GET    | `/admin/active-users`      | Bearer token | Aggregated daily active users by version/arch             |
| GET    | `/admin/crashes`           | Bearer token | Aggregated crash data by day/crash site/signal            |
| GET    | `/download/:version/:arch` | —            | Log download to D1, 302 → GitHub                          |
| POST   | `/crash-report`            | —            | Ingest crash report to D1                                 |
| POST   | `/error-report`            | —            | Multipart upload (zip + meta) → R2, Discord notify        |
| GET    | `/update-check/:version`   | —            | Log update check to D1 (deduped), 302 → latest.json       |

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
| `CRASH_NOTIFICATION_EMAIL`         | `veszelovszki@gmail.com`         | Recipient email for crash alerts  |
| `DISCORD_WEBHOOK_URL`              | Same webhook URL                 | Discord webhook for error reports |
| `R2_ACCOUNT_ID`                    | Same account ID                  | For minting presigned R2 URLs     |
| `R2_ACCESS_KEY_ID`                 | Same access key                  | R2 S3-compat access key (read OK) |
| `R2_SECRET_ACCESS_KEY`             | Same secret                      | Paired secret for R2 access key   |

**R2/KV bindings** (declared in `wrangler.toml`, provisioned via `./scripts/setup-cf-infra.sh`):

| Binding                | Type         | Purpose                                                                    |
| ---------------------- | ------------ | -------------------------------------------------------------------------- |
| `ERROR_REPORTS_BUCKET` | R2 bucket    | Stores error report zip bundles (`cmdr-error-reports`, 90-day TTL)         |
| `ERROR_REPORT_META`    | KV namespace | `total_bytes` counter + `eviction_in_progress` lock for the eviction logic |

**Paddle dashboards**: [sandbox](https://sandbox-vendors.paddle.com) | [live](https://vendors.paddle.com)

### Discord webhooks

`DISCORD_WEBHOOK_URL` posts notifications to the `#error-reports` channel of the **Cmdr** Discord server. The URL is the
secret — anyone holding it can post to that channel — so it lives only as a wrangler secret, never in the repo.

**To create or rotate the webhook:**

1. Open the Cmdr Discord server → right-click `#error-reports` → **Edit Channel** → **Integrations** → **Webhooks**.
2. To rotate: click the existing webhook → **Delete Webhook**, then **New Webhook**. To create fresh: just **New
   Webhook**. Name it "Cmdr error reports".
3. Click **Copy Webhook URL**. URL shape: `https://discord.com/api/webhooks/<id>/<token>`.
4. Store it as a wrangler secret (run from anywhere in the repo):
   ```sh
   pnpm --filter @cmdr/api-server exec wrangler secret put DISCORD_WEBHOOK_URL
   ```
5. Smoke-test it landed correctly:
   ```sh
   curl -H "Content-Type: application/json" -d '{"content":"webhook test"}' "<webhook-url>"
   ```

Rate limit: 30 messages/min per webhook. The Worker should retry once on `Retry-After`, then drop with a `console.error`
— we don't run our own queue infra for an internal channel.

### R2 presigned URLs (for error-report download links)

The error-report Worker mints 7-day presigned GET URLs for the zip bundles in R2 and embeds them in Discord
notifications. R2 bindings can't presign on their own, so the Worker uses the S3-compatible API via `aws4fetch` and
three secrets: `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`.

Current values: stored in David's password store (Bitwarden). The secrets also live as Cloudflare Worker secrets
(`wrangler secret list` to confirm).

**To create (or rotate) the R2 access key:**

1. https://dash.cloudflare.com → **R2 Object Storage** → **Manage R2 API Tokens** (top right).
2. **Create API Token**. Name: `cmdr-error-reports-presign`.
3. Permission: **Object Read** (read-only is enough — writes go through the R2 binding, not the S3 key).
4. Scope: **Apply to specific buckets only** → `cmdr-error-reports`.
5. TTL: forever (or match your rotation policy).
6. Click **Create API Token**. The token page shows THREE values that are displayed ONCE:
   - **Access Key ID** → `R2_ACCESS_KEY_ID`
   - **Secret Access Key** → `R2_SECRET_ACCESS_KEY`
   - **Account ID** (also shown in the dashboard top-right / R2 URL) → `R2_ACCOUNT_ID`
7. Save all three into Bitwarden before leaving the page.
8. Set the three as wrangler secrets:
   ```sh
   pnpm --filter @cmdr/api-server exec wrangler secret put R2_ACCOUNT_ID
   pnpm --filter @cmdr/api-server exec wrangler secret put R2_ACCESS_KEY_ID
   pnpm --filter @cmdr/api-server exec wrangler secret put R2_SECRET_ACCESS_KEY
   ```
9. To rotate: create a fresh token first, set the new secrets, deploy, then delete the old token from the R2 API Tokens
   page.

**Gotcha when deploying**: if your shell has `CLOUDFLARE_API_TOKEN` set, `wrangler deploy` uses that instead of the
interactive OAuth login. The token must have the `Workers R2 Storage: Edit` permission or the deploy fails with
`Authentication error [code: 10000]` on the R2 bucket precheck. Fix at https://dash.cloudflare.com/profile/api-tokens.
One-shot workaround without editing the token:
`CLOUDFLARE_API_TOKEN= pnpm --filter @cmdr/api-server exec wrangler deploy` (empties the env var for that command, falls
back to the OAuth login).

### Webhook verification

`verifyPaddleWebhookMulti` tries both `PADDLE_WEBHOOK_SECRET_LIVE` and `PADDLE_WEBHOOK_SECRET_SANDBOX` when verifying
incoming webhooks. This is a safety net — in practice, the sandbox dashboard sends webhooks only to the sandbox
destination (ngrok for local dev), and the live dashboard sends only to the live destination (`api.getcmdr.com`).

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

Download redirect: GET /download/:version/:arch → write to D1 (fire-and-forget) → 302 to GitHub Releases

Crash report: POST /crash-report → validate payload (size + required fields) → hash IP with daily salt → write to D1 (fire-and-forget via waitUntil) → 204

Update check proxy: GET /update-check/:version → hash IP with daily salt → INSERT OR IGNORE into D1 (fire-and-forget) → 302 to latest.json

Cron (every 12h): scheduled handler runs three jobs:
  1. Crash notifications: query un-notified crash_reports → group by top_function → mark notified → email summary
  2. Daily aggregation (00:00 UTC only): aggregate update_checks → daily_active_users, prune raw data older than 7 days
  3. DB size check (00:00 UTC only): query pragma_page_count/pragma_page_size → email alert if over 100 MB
```

## Cron handler

A single `scheduled` handler runs every 12 hours (`0 */12 * * *`). It runs three independent jobs, each in its own
try-catch so one failure doesn't block the others:

1. **Crash notifications** (every invocation): queries `crash_reports WHERE notified_at IS NULL`, sorted newest-first,
   marks rows as notified, then sends an email via Resend with one row per crash report (When, Env, ID, Site, Signal,
   Version). Marks before sending to prefer missed notifications over duplicates. Pre-fix-\* this grouped by
   `top_function`; the per-row layout is easier to scan and includes the user-visible `CRASH-XXXXX` id. Requires
   `CRASH_NOTIFICATION_EMAIL` and `RESEND_API_KEY`.

2. **Daily aggregation** (00:00 UTC only): aggregates yesterday's `update_checks` into `daily_active_users` via
   `INSERT OR IGNORE ... GROUP BY`, then prunes raw update checks older than 7 days. Idempotent via existence check.

3. **DB size check** (00:00 UTC only): queries D1 pragma for total database size. Sends an alert email if over 100 MB.

4. **Daily eviction sweep** (00:00 UTC only): `handleDailyEvictionSweep` recomputes `total_bytes` from R2 ground truth
   (the per-upload KV counter is racy and drifts), then triggers `tryEvict` if still over 8 GB. Idempotent. Catches
   drift from concurrent uploads or a Worker dying mid-eviction.

The default export uses the object form (`{ fetch, scheduled }`) required for cron support. The Hono `app` is also
exported as a named export so tests can use `app.request()`.

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
Cloudflare secrets (`wrangler secret put`), never in `wrangler.toml`. `/admin/stats` uses a dedicated `ADMIN_API_TOKEN`
secret, separate from the Paddle webhook secrets used by `/admin/generate`.

**Activation counter:** `/activate` increments a KV counter at `_meta:activation_count` on each successful activation.
Read by `/admin/stats`. The counter starts from zero when deployed — initialize via the CF API if historical count is
needed.

**D1 for telemetry:** Crash reports, downloads, and update checks are stored in D1 (binding: `TELEMETRY_DB`, database:
`cmdr-telemetry`). Migrations live in `migrations/`. Apply with `wrangler d1 migrations apply cmdr-telemetry` before
deploying changes that add new tables. The only remaining Analytics Engine dataset is `DEVICE_COUNTS` for fair-use
monitoring. All other state (license codes, activation counter, device sets) lives in Cloudflare KV. Short codes never
expire (perpetual licenses last forever); subscription validity is checked live via Paddle API.

**Validation error granularity:** `/validate` distinguishes "Paddle says invalid" (HTTP 200 + `status: "invalid"`) from
"Paddle is unreachable" (HTTP 502 + `{ error: "upstream_error" }`). `paddle-api.ts` throws `PaddleApiError` on
network/5xx errors and returns `null` on 404 (transaction not found). This lets the desktop app fall back to cached
status on transient Paddle outages instead of overwriting a valid "active" cache with "invalid."

**Download tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `downloads`). One row per download event with
app_version, arch, country, and continent. D1 write is fire-and-forget via `waitUntil` + `.catch(() => {})`.

**Update check tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `update_checks`). Counts active users (free +
licensed) by proxying update checks through `GET /update-check/:version`. Each unique (date, hashed_ip, app_version,
arch) combo gets one row — `INSERT OR IGNORE` with a UNIQUE constraint handles deduplication for free. IP is hashed with
SHA-256 + daily salt for deduplication without storing PII. D1 write is fire-and-forget via `waitUntil` +
`.catch(() => {})`. The cron handler aggregates raw data into the `daily_active_users` summary table daily.

**Crash report tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `crash_reports`). Receives crash reports from the
desktop app via `POST /crash-report`. Columns: hashed_ip, app_version, os_version, arch, signal, top_function,
backtrace, build_mode (`'release'` / `'debug'`, nullable for legacy rows), short_id (`CRASH-XXXXX`, nullable for legacy
rows). IP is hashed with SHA-256 + daily salt (same pattern as update checks). Validates payload size (max 64 KB),
required fields, and the shape of optional fields before writing. D1 write is fire-and-forget via `waitUntil` +
`.catch(() => {})`. No authentication required.

**Device tracking (fair use):** On each `/validate` call with a `deviceId`, the server tracks the device in KV
(`devices:{seatTransactionId}`) and logs to Analytics Engine (binding: `DEVICE_COUNTS`, dataset: `cmdr_device_counts`).
Devices older than 90 days are pruned on each write. If 6+ devices are active and no alert was sent in the past 30 days,
an internal email is sent to `legal@getcmdr.com` via Resend. Device tracking is fire-and-forget and never affects the
validation response. The KV value stores a `DeviceSet` with device hashes mapped to last-seen timestamps plus an
optional `lastAlertedAt`. Device tracking is per seat — each seat in a multi-seat purchase has its own transaction ID
and its own 6-device allowance.

**Update check proxy:** `GET /update-check/:version` routes update checks through the worker to count all users (free +
licensed). Without this, there's no signal for how many people actually run the app — Umami only tracks website visitors
and download tracking only captures installs.

**Error report R2 key shape:** `error-reports/{prod|dev}/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`. The env segment (`prod`
for release builds, `dev` for debug builds, inferred from `meta.buildMode`) keeps dev-run reports out of the production
sort order. Legacy keys (`error-reports/{yyyy-mm-dd}/...` — pre-env-prefix) still exist; eviction reads the date segment
via `extractDateSegment` which handles both shapes. The 90-day R2 lifecycle drains the legacy shape naturally — there's
no migration.

**Error report eviction (8/6 GB watermarks + lifecycle):** Three layers keep the bucket bounded.

1. **On-upload eviction**: every `POST /error-report` schedules `tryEvict` in `waitUntil(...)`. If `total_bytes` (KV) >
   8 GB and `eviction_in_progress` (KV, 60-s TTL lock) isn't set, lists R2 objects under `error-reports/`, sorts
   oldest-first by the embedded `yyyy-mm-dd` segment (via `extractDateSegment`, which handles both new and legacy key
   shapes) then by `uploaded`, deletes until ≤ 6 GB, then resets the counter to the recomputed ground truth.
2. **Daily cron sweep**: corrects KV drift by recomputing from R2 and re-running `tryEvict`.
3. **R2 lifecycle rule**: 90-day expiration applied at provisioning time via `scripts/setup-cf-infra.sh`.

The KV counter is approximate (read-then-write, no atomic increment — same as `_meta:activation_count`). Both the daily
sweep and post-eviction recompute correct it. R2 deletes are idempotent; concurrent evictors deleting the same oldest
object cause no harm.

**Error report Discord notifications:** Every upload triggers a Discord embed with a 7-day presigned R2 GET URL. Uses
the R2 S3-compatible API via `aws4fetch` (`AwsClient.sign` with `signQuery: true` + `X-Amz-Expires`). 7 days is R2's max
for presigned URLs. Convenience of click-to-download outweighs leak risk because only the maintainer accesses the
`#error-reports` channel.

**Short ID generation:** `generateShortId(prefix, len)` in `license.ts` produces IDs like `ERR-A2345` from the same
unambiguous alphabet (`23456789ABCDEFGHJKMNPQRSTUVWXYZ`) as license short codes. Rejection sampling avoids modulo bias.
The error report route does NOT regenerate the id server-side — it validates the client-supplied `meta.id` against the
shape `^ERR-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$` and uses it as-is. On the astronomically rare R2 key collision (same
id + same date + UUID clash), the route retries with a fresh UUID — never a fresh id — so the user-visible id from the
preview dialog stays stable through to the toast.

## Local development

### Run locally

```sh
pnpm dev          # starts wrangler dev server on :8787
pnpm test         # vitest unit tests
```

### Run wrangler from anywhere in the repo

`wrangler` is a local devDependency, not global. From inside `apps/api-server/` use `npx wrangler …`. From the repo root
(no `cd` needed), use the pnpm filter form:

```sh
pnpm --filter @cmdr/api-server exec wrangler secret put DISCORD_WEBHOOK_URL
pnpm --filter @cmdr/api-server exec wrangler deploy
```

Both forms resolve the same local `wrangler` binary.

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
  -H "Authorization: Bearer $(grep PADDLE_WEBHOOK_SECRET_SANDBOX apps/api-server/.dev.vars | cut -d= -f2-)" \
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
cd apps/api-server
npx wrangler d1 migrations apply cmdr-telemetry  # apply any new D1 migrations first
npx wrangler deploy
```

Deployed to `api.getcmdr.com` via Cloudflare custom domain (declared in `wrangler.toml` `[[routes]]`).
`license.getcmdr.com` is a permanent alias for existing app versions. Fallback URL:
`cmdr-license-server.veszelovszki.workers.dev`. The cron trigger (`0 */12 * * *`) is declared in `wrangler.toml` under
`[triggers]` and is deployed automatically with `wrangler deploy`.

### Troubleshooting deployment

- **522 on `api.getcmdr.com`**: Custom domain isn't routing to the Worker. Check `npx wrangler deploy` output shows
  `api.getcmdr.com (custom domain)`. The `[[routes]]` block in `wrangler.toml` may be missing, or a DNS record is
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

**Decision**: No hard enforcement of device limits — the server never rejects a validation because of device count.
**Why**: Suspension is a manual decision after human review. The goal is to detect obvious key sharing (one key on 6+
devices), not to restrict legitimate power users. Alert threshold is 6 because 3-4 Macs is normal, 5 is plausible, 6 is
hard to explain as one person. The threshold is not published in the ToS to avoid gaming.

## Gotchas

**Gotcha**: `verifyAdminAuth` uses a manual type annotation for `c` instead of Hono's `Context` type. **Why**: Using
`Context<{ Bindings: Bindings }>` would require importing Hono's internal generic types and threading them through. The
manual shape `{ env: Bindings; req: { header: ... } }` is simpler and avoids coupling to Hono internals.

**Gotcha**: Paddle preserves `custom_data` key casing exactly as passed in from checkout. **Why**: The checkout passes
`organizationName` (camelCase), and both webhook payloads and API responses return it in camelCase. The code must use
`organizationName`, not `organization_name`.

**Gotcha**: `verifyPaddleWebhookMulti` tries both webhook secrets even though environments are separated. **Why**:
Safety net. If a sandbox webhook somehow reaches the production endpoint (or vice versa), it still verifies rather than
silently failing. Costs one extra HMAC check on mismatch.

**Gotcha**: The activation counter (`_meta:activation_count` in KV) uses read-then-write, which has a race condition
under concurrent `/activate` requests. **Why**: KV doesn't support atomic increment. The counter is approximate — if
exact counts matter, query the CF API to list KV keys, or switch to Durable Objects / D1.

**Gotcha**: The `/download/:version/:arch` redirect maps `x86_64` → `x64` in the filename. **Why**: `tauri-action` names
the Intel DMG `Cmdr_<ver>_x64.dmg`, but the rest of the codebase (URL path, D1 telemetry, website data attrs, Rust
target triple, `uname -m`) consistently uses `x86_64`. Mapping at the boundary keeps everything else canonical. Same
convention is already used in `.github/workflows/release.yml` when reading DMG sizes for `latest.json`.

## Dependencies

Runtime: `hono`, `@noble/ed25519`, `resend` Dev: `wrangler`, `vitest`, `typescript`, `eslint`, `prettier`

**Decision**: Paddle as Merchant of Record (not Stripe, Gumroad, LemonSqueezy, or Polar). **Why**: All-inclusive pricing
(5% + $0.50, no hidden non-US or EU payout fees), aggregate monthly payouts (one invoice for accountant instead of
per-transaction), handles global VAT/GST calculation and remittance, established reputation (Sketch, etc.). On a $29
sale: $1.95 fee → $27.05 net. At 30k sales, saves ~$7k/year vs LemonSqueezy. Stripe was rejected because solo-dev
handling VAT in 27+ EU countries is impractical (Stripe is a payment processor, not an MoR).

**Decision**: BSL 1.1 license with free personal use (supersedes earlier AGPL + trial model). **Why**: The AGPL + trial
model felt pushy for hobbyists (trial countdown, nagware). BSL gives friction-free personal use (no nags), clear
commercial terms (businesses know they must pay), and simpler enforcement (title bar shows license type, honor system
beats trial timers). Source converts to AGPL-3.0 after 3 years per release.

See also: `apps/desktop/src/lib/licensing/CLAUDE.md` — full frontend licensing feature overview
