# API server details

Pull-tier docs for `apps/api-server/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in `CLAUDE.md`.

## Key files

- **`src/index.ts`**: Hono app assembly: mounts route modules, wires scheduled handler
- **`src/types.ts`**: Shared types (`Bindings`), constants, and helpers (auth, validation)
- **`src/licensing.ts`**: Routes: `/activate`, `/validate`, `/webhook/paddle`, `/admin/generate`
- **`src/license-issuance.ts`**: The durable fulfillment record behind `/webhook/paddle` (D1 table `license_issuance`):
  claim, take-over, code storage, delivery marking, and the pure `classifyIssuance`
- **`src/admin.ts`**: Routes: `/admin/stats`, `/admin/downloads`, `/admin/active-users`, `/admin/update-activity`,
  `/admin/crashes`, `/admin/heartbeat-dau`, `/admin/feedback`, `/admin/error-reports`
- **`src/funnel.ts`**: Route `/admin/funnel`: per-UTC-day acquisition funnel (downloads, new installs, DAU, D7
  retention, Listmonk signups). Pure `buildDateList`/`assembleFunnel` helpers are unit-tested
- **`src/telemetry.ts`**: Routes: `/crash-report`, `/heartbeat`, `/update-check/:version`, `/download/:version/:arch`
- **`src/likes.ts`**: Routes: `/likes/:slug` (GET, POST, DELETE, OPTIONS)
- **`src/error-report.ts`**: Route: `POST /error-report` (multipart upload to R2, Discord notify)
- **`src/beta-signup.ts`**: Route: `POST /beta-signup` (email-only Listmonk double-opt-in subscribe; NO install id)
- **`src/feedback.ts`**: Route: `POST /feedback` (in-app feedback → D1 + Discord notify)
- **`src/link-codes.ts`**: Routes: `GET /r-codes.json` (public, edge-cached) + `/admin/r-codes` CRUD. `?r=<code>` → UTM
  map in KV (`LINK_CODES`). Pure `sanitizeUtmValue`/`isValidCode` are unit-tested
- **`src/link-codes.test.ts`**: Tests for `/r-codes.json` (public map, CORS, cache), the admin CRUD (auth, upsert,
  delete), and the validators
- **`src/error-report-eviction.ts`**: Eviction logic: 8/6 GB watermarks, 60-day age floor, KV lock, recompute helper
- **`src/error-report-intake.ts`**: Admission control for `POST /error-report`: daily byte budget, intake pause flag,
  once-a-day alert claims, notification fan-out cap
- **`src/error-report-intake.test.ts`**: Tests for the budget, the pause switch, and both claim counters
- **`src/discord.ts`**: Discord webhook client (single-retry on 429, drop-on-failure)
- **`src/scheduled.ts`**: Cron handler functions (crash notifications, aggregation, DB size, eviction)
- **`src/license.ts`**: Short code + license key generation, `LicenseType` enum
- **`src/paddle.ts`**: HMAC-SHA256 webhook verification, `constantTimeEqual`
- **`src/paddle-api.ts`**: Paddle REST client: transaction/subscription/customer fetch
- **`src/email.ts`**: Resend email delivery (HTML + plain text, multi-seat support)
- **`src/device-tracking.ts`**: Device set helpers: prune stale devices, alert threshold
- **`src/license.test.ts`, `src/paddle.test.ts`**: Vitest tests
- **`src/webhook-paddle.test.ts`**: Tests for `POST /webhook/paddle`: first delivery, duplicate, retry after a failed
  email, concurrent delivery, and a Resend rejection
- **`src/license-issuance.test.ts`**: Tests for the pure `classifyIssuance` state rules
- **`src/device-tracking.test.ts`**: Tests for device tracking helpers
- **`src/admin-stats.test.ts`**: Tests for `/admin/stats` endpoint and activation counter
- **`src/admin-endpoints.test.ts`**: Tests for `/admin/downloads`, `/admin/active-users`, `/admin/update-activity`,
  `/admin/crashes`, `/admin/heartbeat-dau`, `/admin/feedback`, `/admin/error-reports`
- **`src/funnel.test.ts`**: Tests for `/admin/funnel`: route auth/validation, pure `buildDateList`/`assembleFunnel`
  (date math, zero-fill, D7-knowability)
- **`src/crash-report.test.ts`**: Tests for `POST /crash-report` endpoint
- **`src/heartbeat.test.ts`**: Tests for `POST /heartbeat` (validation, config round-trip, rate limit)
- **`src/beta-signup.test.ts`**: Tests for `POST /beta-signup` (Listmonk call, no-install-id invariant, soft failure,
  rate limit)
- **`src/feedback.test.ts`**: Tests for `POST /feedback` (validation, caps, D1 row, Discord ping, rate limit)
- **`src/download-and-update-check.test.ts`**: Tests for download redirect and update check routes
- **`src/scheduled.test.ts`**: Tests for cron handler (crash notifications, aggregation)
- **`scripts/generate-keys.js`**: Ed25519 key pair generation (run once at setup)
- **`scripts/setup-cf-infra.sh`**: Cloudflare KV namespace provisioning

## Routes

| Method  | Path                       | Auth          | Purpose                                                                                            |
| ------- | -------------------------- | ------------- | -------------------------------------------------------------------------------------------------- |
| GET     | `/`                        | none          | Health check                                                                                       |
| POST    | `/webhook/paddle`          | HMAC sig      | Purchase completed → generate & email key(s)                                                       |
| POST    | `/activate`                | none          | Exchange short code → full cryptographic key                                                       |
| POST    | `/validate`                | none          | Check subscription status via Paddle API                                                           |
| POST    | `/admin/generate`          | Bearer token  | Manual key generation (customer service / testing)                                                 |
| GET     | `/admin/stats`             | Bearer token  | Activation count + device count (for analytics dashboard)                                          |
| GET     | `/admin/downloads`         | Bearer token  | Aggregated downloads by day/version/arch/country/source, with raw `count` + deduped `uniqueCount`  |
| GET     | `/admin/active-users`      | Bearer token  | Aggregated daily active users by version/arch                                                      |
| GET     | `/admin/update-activity`   | Bearer token  | Per-day distinct update-enabled installs by version (retained aggregate ∪ today's raw)             |
| GET     | `/admin/crashes`           | Bearer token  | Aggregated crash data by day/crash site/signal                                                     |
| GET     | `/admin/heartbeat-dau`     | Bearer token  | Per-day DAU (distinct `anal_id`) + beats from `heartbeat`                                          |
| GET     | `/admin/funnel`            | Bearer token  | Per-UTC-day acquisition funnel for the last N days (downloads, installs, DAU, D7, signups)         |
| GET     | `/admin/feedback`          | Bearer token  | In-app feedback rows from D1 (full text + reply-to email), newest first                            |
| GET     | `/admin/error-reports`     | Bearer token  | Per-bundle error-report metadata from the R2 prod prefix (`list` + custom metadata), newest first  |
| GET     | `/download/:version/:arch` | none          | Log download to D1 (bots skipped, source + `ref` tagged), 302 → GitHub; `:version` takes `latest`  |
| POST    | `/crash-report`            | IP rate-limit | Ingest crash report to D1                                                                          |
| POST    | `/heartbeat`               | IP rate-limit | Ingest a usage heartbeat (anonymous `anal_id`) to D1                                               |
| POST    | `/error-report`            | IP rate-limit | Multipart upload (zip + meta) → R2, Discord notify. Also gated by the global intake budget         |
| POST    | `/beta-signup`             | IP rate-limit | Subscribe a contact email to the Listmonk beta list (NO install id)                                |
| POST    | `/feedback`                | IP rate-limit | Ingest in-app feedback to D1, Discord notify                                                       |
| GET     | `/update-check/:version`   | none          | Log update check to D1 (deduped), 302 → latest.json                                                |
| GET     | `/r-codes.json`            | none          | Public `?r=<code>` → UTM map (note stripped), edge-cached 5 min, `Access-Control-Allow-Origin: *`  |
| OPTIONS | `/r-codes.json`            | none          | CORS preflight (204)                                                                               |
| GET     | `/admin/r-codes`           | Bearer token  | Full code map including admin `note`                                                               |
| PUT     | `/admin/r-codes/:code`     | Bearer token  | Upsert a code: `{ utm_source, utm_medium?, note? }` (utm values sanitized; code charset validated) |
| DELETE  | `/admin/r-codes/:code`     | Bearer token  | Remove a code from the map                                                                         |

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
| `PADDLE_WEBHOOK_SECRET_LIVE`       | n/a                              | Live secret                       |
| `PADDLE_API_KEY_SANDBOX`           | Sandbox API key                  | n/a                               |
| `PADDLE_API_KEY_LIVE`              | n/a                              | Live API key                      |
| `PRICE_ID_COMMERCIAL_SUBSCRIPTION` | Sandbox price ID                 | Live price ID                     |
| `PRICE_ID_COMMERCIAL_PERPETUAL`    | Sandbox price ID                 | Live price ID                     |
| `ED25519_PRIVATE_KEY`              | Private key hex                  | Same private key hex              |
| `RESEND_API_KEY`                   | Resend key                       | Same Resend key                   |
| `CRASH_NOTIFICATION_EMAIL`         | `david@getcmdr.com`              | Recipient email for crash alerts  |
| `DISCORD_WEBHOOK_URL`              | Same webhook URL                 | Discord webhook for error reports |
| `DISCORD_BETA_SIGNUP_WEBHOOK_URL`  | Optional (falls back)            | Optional `#beta-signups` webhook  |
| `R2_ACCOUNT_ID`                    | Same account ID                  | For minting presigned R2 URLs     |
| `R2_ACCESS_KEY_ID`                 | Same access key                  | R2 S3-compat access key (read OK) |
| `R2_SECRET_ACCESS_KEY`             | Same secret                      | Paired secret for R2 access key   |
| `LISTMONK_API_URL`                 | `https://mail.getcmdr.com`       | Same base URL                     |
| `LISTMONK_API_USER`                | Listmonk API user                | Same (least-privilege at deploy)  |
| `LISTMONK_API_TOKEN`               | Listmonk API token               | Same (least-privilege at deploy)  |
| `LISTMONK_BETA_LIST_ID`            | Beta-list numeric id             | Same id                           |

**R2/KV bindings** (declared in `wrangler.toml`, provisioned via `./scripts/setup-cf-infra.sh`):

| Binding                | Type         | Purpose                                                                                |
| ---------------------- | ------------ | -------------------------------------------------------------------------------------- |
| `ERROR_REPORTS_BUCKET` | R2 bucket    | Stores error report zip bundles (`cmdr-error-reports`, 90-day TTL)                     |
| `ERROR_REPORT_META`    | KV namespace | Eviction bookkeeping + intake admission counters (key list below)                      |
| `LINK_CODES`           | KV namespace | One key (`codes`) holds the whole `?r=<code>` → UTM map (see the note below)           |
| `HEARTBEAT_LIMITER`    | Rate limit   | Gates `POST /heartbeat` at 12 req/min/IP (`[[ratelimits]]`, type `RateLimit`)          |
| `BETA_SIGNUP_LIMITER`  | Rate limit   | Gates `POST /beta-signup` at 5 req/min/IP (signups are rare; tighter than heartbeat)   |
| `FEEDBACK_LIMITER`     | Rate limit   | Gates `POST /feedback` at 5 req/min/IP (real feedback is rare; spam loops aren't)      |
| `ERROR_REPORT_LIMITER` | Rate limit   | Gates `POST /error-report` at 3 req/min/IP (tightest: each request stores up to 10 MB) |
| `CRASH_REPORT_LIMITER` | Rate limit   | Gates `POST /crash-report` at 10 req/min/IP (a crashing app flushes a small burst)     |

**Rate limits are per data center, not global.** Cloudflare's rate-limit bindings count per colo
([docs](https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/)), so each one bounds a single
abusive client and not a distributed flood. `enforceIpRateLimit` (`types.ts`) is the single gate every route calls;
`/error-report` carries a global ceiling on top (below), because it's the one where a flood is expensive.

`ERROR_REPORT_META` keys:

- `total_bytes`: running bucket size. Approximate (racy read-then-write), corrected by the daily sweep.
- `eviction_in_progress`: 60-s TTL lock preventing concurrent eviction.
- `bytes_today:{yyyy-mm-dd}`: accepted bundle bytes for the day, against `DAILY_INTAKE_BUDGET_BYTES`. 48-h TTL.
- `intake_paused`: kill switch. Present = `/error-report` returns 503.
- `budget_alert:{yyyy-mm-dd}`: claimed by the one caller that sends the day's "budget exhausted" ping. 48-h TTL.
- `notify_count:{yyyy-mm-dd}`: per-upload Discord pings sent today, against `DAILY_NOTIFICATION_CAP`. 48-h TTL.

`LINK_CODES` detail: the one `codes` key maps `?r=<code>` → `{ utm_source, utm_medium?, note? }` (id
`6dbba67c8ece475daf3e8c0406d242c9`). Created with `wrangler kv namespace create LINK_CODES`; no preview id (matches the
other namespaces).

**Paddle dashboards**: [sandbox](https://sandbox-vendors.paddle.com) | [live](https://vendors.paddle.com)

### Discord webhooks

`DISCORD_WEBHOOK_URL` posts notifications to the `#error-reports` channel of the **Cmdr** Discord server. The URL is the
secret (anyone holding it can post to that channel), so it lives only as a wrangler secret, never in the repo.

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
We don't run our own queue infra for an internal channel.

**Optional dedicated webhooks (`#beta-signups`, `#feedback`):** `POST /beta-signup` posts to
`DISCORD_BETA_SIGNUP_WEBHOOK_URL` and `POST /feedback` to `DISCORD_FEEDBACK_WEBHOOK_URL`. Both fall back to
`DISCORD_WEBHOOK_URL` when unset, so the feature works before the dedicated channel exists (pings just land in
`#error-reports`). To split beta-signup pings into their own channel:

1. Create the channel `#beta-signups` in the Cmdr Discord server.
2. Right-click `#beta-signups` → **Edit Channel** → **Integrations** → **Webhooks** → **New Webhook**. Name it "Cmdr
   beta signups". **Copy Webhook URL** (shape `https://discord.com/api/webhooks/<id>/<token>`).
3. Store it as a wrangler secret:
   ```sh
   pnpm --filter @cmdr/api-server exec wrangler secret put DISCORD_BETA_SIGNUP_WEBHOOK_URL
   ```
4. Smoke-test it landed:
   ```sh
   curl -H "Content-Type: application/json" -d '{"content":"beta-signups webhook test"}' "<webhook-url>"
   ```

### R2 presigned URLs (for error-report download links)

The error-report Worker mints 7-day presigned GET URLs for the zip bundles in R2 and embeds them in Discord
notifications. R2 bindings can't presign on their own, so the Worker uses the S3-compatible API via `aws4fetch` and
three secrets: `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`.

Current values: stored in David's password store (Bitwarden). The secrets also live as Cloudflare Worker secrets
(`wrangler secret list` to confirm).

**To create (or rotate) the R2 access key:**

1. https://dash.cloudflare.com → **R2 Object Storage** → **Manage R2 API Tokens** (top right).
2. **Create API Token**. Name: `cmdr-error-reports-presign`.
3. Permission: **Object Read** (read-only is enough; writes go through the R2 binding, not the S3 key).
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
incoming webhooks. This is a safety net; in practice, the sandbox dashboard sends webhooks only to the sandbox
destination (ngrok for local dev), and the live dashboard sends only to the live destination (`api.getcmdr.com`).

## Data flow

```
Paddle webhook → HMAC verify (tries both live + sandbox secrets)
  → claim the transaction (D1 license_issuance, conditional INSERT; see "Fulfillment" below)
  → Paddle API: fetch customer details
  → per seat: generateLicenseKey() → generateShortCode() → KV.put(code, {fullKey, orgName})
  → store the codes on the row (short_codes, issued_at)
  → sendLicenseEmail() via Resend
  → mark the row delivered (emailed_at)

App activation: POST /activate → KV.get(shortCode) → return fullKey

Subscription validation: POST /validate → Paddle API transactions + subscriptions
  → HTTP 200 + ValidationResponse on success or invalid transaction (Paddle 404)
  → HTTP 502 + { error: "upstream_error" } if Paddle API unreachable or returns server error
  → if deviceId present: track device in KV (devices:{seatTransactionId}), log to Analytics Engine
  → if device count >= 6 and not recently alerted: send alert email to legal@getcmdr.com

Download redirect: GET /download/:version/:arch → write to D1 (fire-and-forget) → 302 to GitHub Releases

Crash report: POST /crash-report → rate-limit by IP (CRASH_REPORT_LIMITER, 429 if over) → validate payload (size + required fields + optional diagId/email shape) → hash IP with daily salt → write to D1 incl. nullable diag_id + email (fire-and-forget via waitUntil) → 204

Error report: POST /error-report → rate-limit by IP (ERROR_REPORT_LIMITER, 429 if over) → global intake gates (intake_paused, then the day's byte budget; 503 + Retry-After if either trips, plus one Discord ping the day the budget runs out) → read the body under MAX_BODY_BYTES, cancelling past it (413) → parse multipart, validate bundle + meta (400/413) → stream the bundle to R2 under error-reports/{prod|dev}/{date}/{id}-{uuid}.zip → in waitUntil: bump total_bytes, charge the daily budget, tryEvict, then a Discord embed (capped at DAILY_NOTIFICATION_CAP/day) → 200 {id}

Heartbeat: POST /heartbeat → rate-limit by IP (HEARTBEAT_LIMITER, 429 if over) → validate payload (size + required fields + analId/version shape + config-size cap) → write to D1 heartbeat (fire-and-forget via waitUntil), no IP stored → 204

Beta signup: POST /beta-signup → rate-limit by IP (BETA_SIGNUP_LIMITER, 429 if over) → read ONLY the email (no install id) → validate shape → Listmonk POST /api/subscribers (list = LISTMONK_BETA_LIST_ID, subscriber status "enabled", NO preconfirm = double opt-in) → on 2xx: Discord ping (waitUntil, DISCORD_BETA_SIGNUP_WEBHOOK_URL, falls back to DISCORD_WEBHOOK_URL) → on 409 (existing): GET /api/subscribers lookup; if NOT on the beta list, PUT /api/subscribers/lists (action add, status unconfirmed) + POST /api/subscribers/{id}/optin to send the confirmation mail, then ping; if already on the beta list, silent 204, no ping → always an empty 204 (new, added, and already-subscribed are indistinguishable; no enumeration), soft 502 on Listmonk error

Feedback: POST /feedback → rate-limit by IP (FEEDBACK_LIMITER, 429 if over) → validate shape (required feedback text ≤ 100k code points + appVersion/osVersion, optional email/buildMode) → AWAITED D1 write to `feedback` (failure → soft 502 so the app offers a retry) → Discord ping in waitUntil (DISCORD_FEEDBACK_WEBHOOK_URL, falls back to DISCORD_WEBHOOK_URL) → 204

Update check proxy: GET /update-check/:version → hash IP with daily salt → INSERT OR IGNORE into D1 (fire-and-forget) → 302 to latest.json

Cron (every 3h): scheduled handler runs three jobs:
  1. Crash notifications: query un-notified crash_reports → mark notified → email one row per report
  2. Daily aggregation (00:00 UTC only): aggregate update_checks → daily_active_users, prune raw data older than 7 days
  3. DB size check (00:00 UTC only): query pragma_page_count/pragma_page_size → email alert if over 100 MB
```

## Cron handler

A single `scheduled` handler runs every 3 hours (`0 */3 * * *`). It runs three independent jobs, each in its own
try-catch so one failure doesn't block the others:

1. **Crash notifications** (every invocation): queries `crash_reports WHERE notified_at IS NULL`, sorted newest-first,
   marks rows as notified, then sends an email via Resend with one row per crash report (When, Env, ID, Site, Signal,
   Version, Reply to) plus a full-width sub-row carrying the redacted `panic_message` (an em-dash when the row has
   none). Marks before sending to prefer missed notifications over duplicates. The per-row layout is easy to scan and
   includes the user-visible `CRASH-XXXXX` id. Requires `CRASH_NOTIFICATION_EMAIL` and `RESEND_API_KEY`.

2. **Daily aggregation** (00:00 UTC only): aggregates yesterday's `update_checks` into `daily_active_users` via
   `INSERT OR IGNORE ... GROUP BY`, then prunes raw update checks older than 7 days. Idempotent via existence check.

3. **DB size check** (00:00 UTC only): queries D1 pragma for total database size. Sends an alert email if over 100 MB.

4. **Retention sweep** (00:00 UTC only): `handleRetentionSweep` enforces the per-table retention promises. See § Data
   retention below for the windows and the reasoning.

5. **Daily eviction sweep** (00:00 UTC only): `handleDailyEvictionSweep` recomputes `total_bytes` from R2 ground truth
   (the per-upload KV counter is racy and drifts), clears `intake_paused` if the bucket is back under the LOW watermark,
   then triggers `tryEvict` if still over 8 GB. Idempotent. Catches drift from concurrent uploads or a Worker dying
   mid-eviction.

The default export uses the object form (`{ fetch, scheduled }`) required for cron support. The Hono `app` is also
exported as a named export so tests can use `app.request()`.

## Data retention

Every window below is enforced in code by `handleRetentionSweep` (`scheduled.ts`, daily at 00:00 UTC) and stated to
users in `apps/website/src/pages/privacy-policy.astro` § "How long we keep your data". The two have to move together:
the policy is a promise about these tables, so changing a window, a column, or a table here means editing that page in
the same commit.

The sweep's default shape is **clear the identifying columns, keep the row**. Counts, version breakdowns, and crash
triage value live in the other columns, and there's no privacy reason to lose them. Only `heartbeat` deletes rows,
because its stable `anal_id` IS the identifying data.

- **`downloads`**: `hashed_ip` and `user_agent` cleared after 90 days; the row (version, arch, country, continent,
  source, ref, referer, `ua_family`) is kept indefinitely. 90 days is what the two columns are FOR: same-day dedup needs
  one day, and re-tuning `classifyUaFamily` against real UA strings needs a recent sample.
- **`update_checks`**: raw rows deleted after seven days by `handleDailyAggregation`, after rolling into
  `daily_active_users`. Unchanged, and already the model the rest of this follows.
- **`crash_reports`**: `email` (the reply-to a beta tester attached) and `diag_id` cleared after 90 days; the technical
  row is kept indefinitely for long-standing stability work. `hashed_ip` is no longer written at all (migration `0013`
  erased the historical values) because nothing ever read it.
- **`feedback`**: the optional reply-to `email` cleared after two years; the message text stays.
- **`heartbeat`**: rows DELETED after two years. Two years covers every window the dashboard computes (DAU, new
  installs, D7 retention) with room to spare.
- **Error report bundles**: 90-day R2 lifecycle, plus capacity-driven eviction that never touches anything under 60 days
  (`error-report-eviction.ts`). Not part of this sweep.

Two invariants the sweep must keep, both pinned by tests in `scheduled.test.ts`:

- **Roll up before clearing.** `downloads_daily_unique` (migration `0014`) captures each day's
  `COUNT(DISTINCT hashed_ip)` per `/admin/downloads` grouping BEFORE the clear erases the hashes. Reverse the order and
  every historical unique count silently becomes zero, with no way back. `/admin/downloads` then prefers the rollup and
  falls back to the live count for days still inside the window, the same union pattern `/admin/update-activity` uses
  over `daily_active_users`.
- **Cutoffs snap to midnight UTC**, so a day is always swept whole. A mid-day cutoff would roll up half a day's distinct
  count, clear that half, and leave the admin query preferring the partial number for that date. `created_at` is
  compared directly (never wrapped in `date()`) so the indexes stay usable; the two `created_at` formats in these tables
  (`T`/`Z` versus a space) only differ within a second of the boundary, which midnight-snapping puts out of reach.

Every statement is idempotent: each `WHERE` excludes what it already cleared, so re-running after an outage is free.

## Key patterns

**Short code format:** `CMDR-XXXX-XXXX-XXXX` using 31 unambiguous chars (excludes 0/O/1/I/L). Rejection sampling avoids
modulo bias (max unbiased byte = `256 - (256 % 31)`).

**License key format:** `base64(JSON payload).base64(Ed25519 signature)`. Payload contains: email, transactionId,
issuedAt, type, organizationName.

**License types:** `commercial_subscription` | `commercial_perpetual`

**Fulfillment (exactly-once issuance, at-least-once delivery):** a purchase must yield ONE set of license codes, but the
email carrying them is safe to repeat. `license-issuance.ts` keeps those apart, on the D1 table `license_issuance`
(migration `0012`), one row per Paddle transaction:

1. **Claim**: `INSERT ... ON CONFLICT(transaction_id) DO NOTHING RETURNING transaction_id`, before any side effect. Two
   concurrent deliveries race on one primary key and SQLite hands the row to exactly one of them. This is the whole
   atomicity guarantee, and the reason the record lives in D1 rather than KV: KV has no conditional write, and its reads
   are eventually consistent (a redelivery within the propagation window would read a stale "not processed yet").
2. **Mint**: one signed license per seat, each stored in KV under its short code, then `short_codes` + `issued_at` on
   the row. Storing before sending is what makes a later redelivery reuse the SAME codes.
3. **Deliver**: `sendLicenseEmail`, then `emailed_at`. Only now is the purchase fulfilled.

A delivery that loses the claim reads the row and classifies it (pure `classifyIssuance`, unit-tested):

- `delivered` (`emailed_at` set) → 200 `already_processed`, forever.
- `in_flight` (claimed under `issuanceStaleAfterMs`, 5 min) → **503**, so Paddle redelivers instead of us running a
  second issuance beside the first. Live retries are 60 attempts over 3 days (20 in the first hour), so a transient 503
  costs minutes, and the buyer's licenses are never at stake.
- `resend` (stale claim, codes stored) → take over and re-send those codes. A duplicate email is the worst case.
- `remint` (stale claim, no codes: the delivery died before minting) → take over and mint. Any codes a dead attempt
  wrote to KV before failing are orphaned, which is harmless: nobody has seen them.

Take-over is `UPDATE ... WHERE claimed_at = <the value we read>`, so when two deliveries both find a stale claim, only
one wins and the other gets the 503.

**Rows never expire.** "This purchase was fulfilled" has no useful end date, and an expiring marker is exactly how a
late redelivery or a replayed webhook mints a second set of usable perpetual licenses. The table also doubles as the
support/audit trail (who got which codes, when).

**Decision, why not the Paddle `event_id` as the key:** one purchase must yield one set of licenses however many events
carry it, so the transaction id is the unit of fulfillment. `event_id` is stored on the row for debugging only.

**Gotcha: Resend reports failures in its response, it doesn't throw.** `resend.emails.send()` returns `{ data, error }`
(network failures included), so an unchecked `await` reads every failure as success, marks the purchase delivered, and
stops Paddle retrying: the buyer pays and gets nothing. `sendViaResend` (`email.ts`) is the single wrapper that turns an
`error` into a thrown one; all four senders go through it. Don't call `emails.send` directly.

**Known gap: no webhook timestamp tolerance.** `verifyPaddleWebhook` signs over `ts:body` but doesn't reject an old
`ts`, so a captured webhook stays replayable forever. The fulfillment row is what actually blocks the damage (a replay
finds `emailed_at` and does nothing). Paddle recommends a five-second window, but their docs don't say whether a retry
is re-signed with a fresh `ts` or replays the original signature, and rejecting legitimate retries would lose a
delivery, which is worse than the replay. So: log the observed `now - ts` on live deliveries first (including one forced
retry), then enable rejection with a tolerance the data supports.

**Price ID → license type mapping:** `getLicenseTypeFromPriceId()` in `paddle-api.ts` maps Paddle price IDs (from
`PRICE_ID_*` env vars) to license types. Unknown price IDs fall back to `commercial_subscription` for backwards
compatibility.

**Security:** Admin bearer token compared with `constantTimeEqual` (XOR-accumulate, timing-safe). All secrets are
Cloudflare secrets (`wrangler secret put`), never in `wrangler.toml`. `/admin/stats` uses a dedicated `ADMIN_API_TOKEN`
secret, separate from the Paddle webhook secrets used by `/admin/generate`.

**Workers types entrypoint:** `tsconfig.json` pins `@cloudflare/workers-types/2023-07-01`, not the package root (which
resolves to the 2021-11-03 snapshot). The root snapshot predates `R2ListOptions.include` and `R2Object.customMetadata`,
which `/admin/error-reports` needs to read bundle metadata via `bucket.list`. The dated entrypoint matches the runtime
better anyway (`compatibility_date` is 2025-01-01); don't revert it to the bare package name.

**Activation counter:** `/activate` increments a KV counter at `_meta:activation_count` on each successful activation.
Read by `/admin/stats`. The counter starts from zero when deployed; initialize via the CF API if historical count is
needed.

**D1 for telemetry:** Crash reports, downloads, update checks, and heartbeats are stored in D1 (binding: `TELEMETRY_DB`,
database: `cmdr-telemetry`). Migrations live in `migrations/` (latest: `0014_downloads_daily_unique.sql`, the
distinct-downloader rollup the retention sweep writes; `0013_minimize_stored_identifiers.sql` adds `downloads.ua_family`
and erases the crash-table IP hashes; `0012_license_issuance.sql` is the fulfillment
record above; `0011_crash_panic_message.sql` adds the nullable `panic_message` column to `crash_reports`;
`0007_feedback.sql` adds the `feedback` table; `0006_crash_diag_email.sql` adds the nullable `diag_id` + `email`
columns; `0005_heartbeat.sql` adds the `heartbeat` table). Apply with `wrangler d1 migrations apply cmdr-telemetry`
before deploying changes that add new tables or columns. `license_issuance` is the one money-critical table in an
otherwise telemetry-shaped database: it shares the binding because a second D1 buys nothing at a few hundred rows a
year, and nothing prunes it (the daily aggregation job only touches `update_checks`). The only remaining Analytics
Engine dataset is `DEVICE_COUNTS` for fair-use monitoring. All other state (license codes, activation counter, device
sets) lives in Cloudflare KV. Short codes never expire (perpetual licenses last forever); subscription validity is
checked live via Paddle API.

**Validation error granularity:** `/validate` distinguishes "Paddle says invalid" (HTTP 200 + `status: "invalid"`) from
"Paddle is unreachable" (HTTP 502 + `{ error: "upstream_error" }`). `paddle-api.ts` throws `PaddleApiError` on
network/5xx errors and returns `null` on 404 (transaction not found). This lets the desktop app fall back to cached
status on transient Paddle outages instead of overwriting a valid "active" cache with "invalid."

**Download tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `downloads`). One row per download event with
`app_version`, `arch`, `country`, `continent`, `hashed_ip`, `source`, `ref`, `referer`, and `user_agent`. D1 write is
fire-and-forget via `waitUntil` + `.catch(() => {})`. Three things make the count meaningful as an install signal
(migration `0008`):

- **`latest` is a valid `:version`,** for links we can't edit per release (app directories, the README, blog posts, a
  chat message from last year). `resolveLatestVersion` reads `getcmdr.com/latest.json` (the same manifest the in-app
  updater reads, so `latest` can never name a version the updater doesn't know), falling back to GitHub's
  `releases/latest` API for the window where the website is down or mid-deploy. Both answers are validated against
  `versionPattern` before they reach a redirect URL or a D1 row, and both fetches are edge-cached for five minutes, so a
  download burst costs one origin fetch. D1 stores the RESOLVED version, never `latest`, which keeps the per-version
  breakdown intact. When neither source answers, the handler 302s to the GitHub releases page (a human can pick a build
  there) and writes NO row: a guessed version would corrupt the counts. `getcmdr.com/download/latest/<arch>` is the
  public face of this, an nginx redirect in `apps/website/nginx.conf`.
- **Bot/unfurler hits are dropped:** link-preview bots (Discord, Slack, etc.) and crawlers fetch the URL and would
  inflate the count, so a User-Agent denylist skips the D1 write (the 302 is still served). A missing UA is treated as a
  bot too. Homebrew downloads via curl, which would match the `curl` rule, so Homebrew is explicitly exempted.
- **`hashed_ip` enables same-day dedup:** `SHA-256(IP_HASH_PEPPER + IP + daily salt)`, the same scheme as
  `update_checks`. We keep one row per request (raw count is `COUNT(*)`); the dashboard derives distinct same-day
  downloaders with `COUNT(DISTINCT hashed_ip)`. The two ingredients do different jobs and both are load-bearing: the
  daily salt stops the value linking a visitor across days, and the pepper (a Cloudflare secret) is what makes it
  one-way at all. A date-only salt is public and predictable, so the 2^32 IPv4 candidates brute-force on a GPU in
  seconds; such a hash IS the address, and storing one would contradict the privacy policy. A missing pepper still
  hashes (losing download counts is worse) and logs a warning. Rotating the pepper re-anonymizes every older row and
  costs only one day of dedup accuracy.
- **`source` tags origin:** `homebrew` (Homebrew cask, by User-Agent), `website` (getcmdr.com button, which sends
  `?src=website`), or `other` (links shared elsewhere). In-app auto-updates never appear here: they fetch the tarball
  straight from GitHub, not this endpoint.

- **`ref` tags the first-touch channel** (migration `0009`): where a website visitor originally arrived from (a UTM
  source/campaign, or an external referrer hostname), so the dashboard can attribute installs to a channel ("HN drove N
  downloads, Reddit drove M"). The website computes it client-side from URL state only (no localStorage/cookie, to stay
  banner-free) and forwards it as `?ref=`. The handler never trusts that input: `sanitizeRef` lowercases, drops anything
  outside `[a-z0-9._:-]`, and caps at 120 chars, mirroring the website's normalization. Absent or sanitizes-to-empty →
  stored NULL (not `''`). Homebrew, direct links, and return visits in a later session carry no ref and stay NULL. The
  charset rule is the trust boundary — keep client and server in sync if either changes.

- **`referer` and `user_agent` capture the hit's own HTTP metadata** (migration `0010`), the first-party signal that
  illuminates the large `(none)` `ref` bucket. The website button sends `?ref=`, but a DIRECT hit to `/download` (a link
  to `api.getcmdr.com` shared on AlternativeTo, a directory, GitHub, Reddit, a forum) carries no `ref` yet arrives with
  a `Referer` header naming the page that linked it. Unlike `ref`, this is NOT client-supplied attribution (it's the raw
  request header), so there's no website-side sanitizer to keep in sync. `sanitizeRefererHost` keeps the HOST only
  (never path or query, so a referring page's query string can't leak in), lowercases, strips a leading `www.`, drops
  anything outside `[a-z0-9.-]`, and caps at 120 chars; absent/unparseable/empty → NULL. `user_agent` is the raw UA
  capped at 400 chars (separates a human browser from `curl`/Homebrew/CI inside the `other` bucket). Both sit beside the
  daily-rotating `hashed_ip`, so neither adds a cross-day identifier. The dashboard rolls `referer` up into the
  "Download referrers" breakdown (`funnel.ts` `downloadsByReferer`), parallel to the `ref`-based "Channels".

- **User-Agent family classification + `humanInstalls`:** the raw download count over-reads as an install signal because
  a large share of `/download` hits are scrapers and non-macOS clients. Cmdr is macOS-only, which is the whole basis: a
  Windows/Android/Linux/X11 client fetching the `.dmg` literally cannot install it. So at query time `/admin/funnel`
  classifies each stored `user_agent` with the pure, unit-tested `classifyUaFamily` (`funnel.ts`) into one of three
  families and returns a per-day `downloadsByUaFamily { human, bot, unknown }` plus a `humanInstalls` count:
  - **`human`** (a possible install, checked first so a Mac-claiming UA is never excluded): UA contains `Macintosh` or
    `Mac OS` (a Mac browser), `Homebrew`, or `curl`/`wget` (cask and manual CLI installs).
  - **`bot` / impossible install** (the one high-confidence exclusion): UA contains `Windows`, `Android`, `Linux`, or
    `X11` — a non-macOS client, so not a real install.
  - **`unknown`**: anything else, including a NULL UA on rows captured before `user_agent` existed. We can't tell, so it
    is NEVER excluded.
  - **`humanInstalls` = `human + unknown`** (downloads minus the `bot` ones). Deliberately conservative: it drops only
    the provably-impossible downloads and keeps every ambiguous one, so it never overclaims. Crucially, the scraper
    spoofs Mac browser UAs too (lots of `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)`, often from China), so those
    land in `human` and `human` is NOT a clean count — only the `bot` exclusion is high-confidence. We do not exclude by
    country. The dashboard surfaces this as the "Downloads by client" panel and the "Human installs" headline next to
    the raw download count (`funnel.ts` `aggregateUaFamilies`).
  - **Where the family is computed:** at WRITE time into `downloads.ua_family` (migration `0013`), so the signal
    outlives the raw `user_agent` that the retention sweep clears after 90 days. The read side calls `resolveUaFamily`,
    which prefers the stored value and falls back to `classifyUaFamily` on the raw UA for pre-`0013` rows. A row with
    neither (a pre-`0013` row whose UA the sweep cleared) lands in `unknown`, which is never excluded.
    `classifyUaFamily` stays the single pure definition of the rules; the sweep's 90-day window is what still lets us
    re-tune it against real UAs.

**Update check tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `update_checks`). Counts active users (free +
licensed) by proxying update checks through `GET /update-check/:version`. Each unique (date, hashed_ip, app_version,
arch) combo gets one row (`INSERT OR IGNORE` with a UNIQUE constraint handles deduplication for free). The IP goes
through the same peppered `hashCallerIp` as `/download`, so nothing recoverable is stored. D1 write is fire-and-forget
via `waitUntil` + `.catch(() => {})`. The cron handler aggregates raw data into the `daily_active_users` summary table
daily and prunes the raw rows at seven days.

**`top_function` derivation (`extractTopFunction`, `telemetry.ts`):** the grouping key is the topmost backtrace frame
that is real application code. Frames belonging to the panic machinery are skipped first (`crash_reporter`,
`std::panicking`, `core::panicking`, `rust_begin_unwind`, `std::backtrace` / `std::sys::backtrace`,
`core::str::slice_error_fail`, and the `core::option` / `core::result` unwrap-and-expect helpers), then the first
`cmdr::` / `cmdr_lib::` frame wins. Without the skip list every panic grouped under
`cmdr_lib::crash_reporter::install_panic_hook::{{closure}}` (the hook is itself app code and always the first `cmdr`
frame), which collapsed 15 of 17 real reports into one bucket and made the nightly email useless for telling unrelated
bugs apart. Non-app frames are never the key even when they are the immediate cause, since `tokio::task::spawn::spawn`
or `core::str::slice_error_fail` would group unrelated bugs by a shared library call; a backtrace with no app frame
stays `'unknown'`. Pinned by the real-backtrace cases in `crash-report.test.ts` § "top_function derivation".

**Crash report tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `crash_reports`). Receives crash reports from the
desktop app via `POST /crash-report`. Columns: `hashed_ip`, `app_version`, `os_version`, `arch`, `signal`,
`top_function`, `backtrace`, `build_mode` (`'release'` / `'debug'`, nullable for legacy rows), `short_id`
(`CRASH-XXXXX`, nullable for legacy rows), `diag_id` (`diag_<uuid>`, nullable), `email` (nullable), `panic_message`
(nullable: signal crashes carry no panic payload, and legacy rows predate the column). IP is hashed with SHA-256 + daily
salt (same pattern as update checks). Validates payload size (max 64 KB), required fields, and the shape of optional
fields before writing. `diagId` must match `^diag_[0-9a-f-]{36}$` (a malformed value, including any `anal_`-prefixed
value, is rejected 400); `email` is loosely shape-checked. `diag_id` and `email` are nullable and stay NULL for reports
without an attached email. The `diag_` id is deliberately separate from the `anal_` analytics id (which is NEVER on a
crash report), so a voluntarily-attached email can't be joined to the analytics stream (guarded by
`crash-report.test.ts`). The email is surfaced in the crash-notification email (a "Reply to" column, see `scheduled.ts`
/ `email.ts`) so the maintainer can reply. D1 write is fire-and-forget via `waitUntil` + `.catch(() => {})`. No
authentication required.

**Heartbeat tracking:** Uses D1 (binding: `TELEMETRY_DB`, table: `heartbeat`). The desktop app posts one beat at launch
and hourly via `POST /heartbeat` for true daily-active tracking during the open beta. Identity is the random
`anal_<uuid>` analytics id (regex `^anal_[0-9a-f-]{36}$`); the IP is used only to key the rate limiter and is never
stored. Required fields: `analId`, `appVersion` (semver), `osVersion`, `arch`. Optional: `buildMode`
(`'release'`/`'debug'`, nullable) and `config`, an arbitrary object stored verbatim as the `config_json` column. The
config is a single JSON blob, not per-field columns, so new settings auto-absorb without a migration: DAU/engagement
queries never touch it (richer config-shape filtering lives in PostHog person properties). Caps: 32 KB whole body, 16 KB
config blob. No UNIQUE/dedup constraint: every beat is kept forever (engagement = beats/day), and DAU
(`COUNT(DISTINCT anal_id)`) is computed at query time by `GET /admin/heartbeat-dau`. The `anal_` id is the analytics
identity and is **never** attached to a crash or error report (those carry a separate `diag_` id), so the analytics
stream stays unjoinable to any identity. D1 write is fire-and-forget via `waitUntil` + `.catch(() => {})`.

**Per-day funnel (`GET /admin/funnel`, `src/funnel.ts`):** One admin endpoint the analytics dashboard uses to render its
top "Daily funnel" table in a single call, instead of stitching per-metric endpoints together. Auth is the shared
`ADMIN_API_TOKEN` (`verifyAdminAuth`). Param `?days=N` (default 30, clamped to 1..90, else 400). Returns `FunnelDay[]`,
oldest UTC day first, **including today** (a partial day). Every column is bucketed by UTC day (`date()` in D1 is UTC;
Listmonk timestamps are normalized to UTC here), so a "day" means the same window across all columns. `null` (not 0)
means "unknown", which the dashboard renders as a dash.

Per-day columns and how each is derived:

- `downloads` + `downloadsBySource` (`{ website, homebrew, other }`): `COUNT(*)` of `downloads` rows by
  `COALESCE(source, 'other')`. Bots already filtered at write time; rows before migration 0008 have NULL source →
  `other`.
- `downloadsByRef` (`Record<ref, count>`): the same `downloads` rows grouped by `COALESCE(ref, '(none)')`, so the
  dashboard can attribute installs to a first-touch channel. NULL ref (Homebrew, direct links, return visits in a later
  session, and rows before migration 0009) buckets under `"(none)"`. An empty object means no downloads that day. The
  `ref` is already sanitized at write time, so the grouping is on the stored value as-is.
- `newInstalls`: count of `anal_id`s whose **first-ever** heartbeat (`MIN(created_at)` over the whole `heartbeat` table,
  no window filter on the inner query) fell on that UTC day. So an install that first beat months ago never counts as
  "new" inside the window.
- `dau`: `COUNT(DISTINCT anal_id)` beating that day (true DAU, same definition as `/admin/heartbeat-dau`).
- `d7Retention` (0..1 fraction) + `d7Retained` (raw count): **D7 definition** — for a cohort whose first heartbeat was
  on day X, an install is "D7 retained" if it has ANY heartbeat in the half-open window `[X+7d, X+8d)` (exactly the 7th
  day after install). `d7Retained` is the distinct count of such installs; `d7Retention = d7Retained / newInstalls(X)`.
  Both are `null` for cohorts younger than 8 days (the `[X+7, X+8)` window hasn't fully passed, so it's genuinely
  unknown, not 0). An old cohort with installs but no retained beats is `0`, not `null`.
- `newsletterSignups`: Listmonk subscribers (newsletter list `LISTMONK_NEWSLETTER_LIST_ID`, default 3, **plus** beta
  list `LISTMONK_BETA_LIST_ID`) whose `created_at` falls on that UTC day, via one read-only
  `GET /api/subscribers?query=...` filtered by `created_at >= sinceDate`, paginated, then bucketed in code. Caveat:
  `created_at` is the subscriber's creation time, not the per-list join time, so someone who joins a second list later
  is counted only on their original signup day (fine for a coarse acquisition signal). Best-effort: when Listmonk is
  unconfigured (URL/user/token missing) OR the query throws, signups are `null` for every day, never 0 — so the
  dashboard distinguishes "no signups" from "couldn't ask". The list ids MUST be TOML integers in `[vars]` (the resolver
  checks `typeof === 'number'`); a string drops that list from the count.

`buildDateList` and `assembleFunnel` are pure and exported so the date math, zero-fill, and D7-knowability logic are
unit-tested without a live D1 (`funnel.test.ts`); the SQL semantics are verified against a real local D1 with
`scripts/seed-funnel-local.sql` (hand-computed expectations are in that file's header).

**Heartbeat rate limiting:** `POST /heartbeat` is gated by the Workers rate-limit binding `HEARTBEAT_LIMITER`
(`[[ratelimits]]` in `wrangler.toml`, type `RateLimit`, `.limit({ key })` → `{ success }`), keyed by `cf-connecting-ip`
at 12 req/min/IP (`period` must be 10 or 60). Legit traffic is ~1 beat/hour/install, so the cap stops a bloat-spam loop
without touching real users; over the limit returns 429 before any parsing or D1 write. The binding is typed optional so
tests and incomplete envs can omit it (the gate is then a no-op).

**Beta signup (decoupled, contact-only):** `POST /beta-signup` is the contact channel for early testers. It reads ONLY
the `email` from the body and subscribes it to the double-opt-in Listmonk list `LISTMONK_BETA_LIST_ID`
(`POST https://mail.getcmdr.com/api/subscribers`, `Authorization: token <LISTMONK_API_USER>:<LISTMONK_API_TOKEN>`,
subscriber `status: "enabled"` — the subscriber-status enum only accepts enabled/disabled/blocklisted, while
`"unconfirmed"` is the per-LIST subscription status — and deliberately NO `preconfirm_subscriptions` so Listmonk sends
its own confirmation email, which blocks prank signups for someone else's address). The privacy invariant is the whole
point: the request carries NO install id of any kind (no `anal_`, no `diag_`), so the email and the analytics ids never
co-occur on our servers and the analytics stream stays unjoinable to any identity (guarded by `beta-signup.test.ts`,
including the outbound Discord payload).

On a Listmonk network/5xx failure it returns a soft 502 the desktop app surfaces as a gentle "try again" (NOT
fire-and-forget: we want the user to know it didn't land). Missing Listmonk config returns 500. The list id is a
wrangler `[var]` (not a secret); see `docs/tooling/listmonk.md`.

**409 add-to-list recovery:** a 409 ("subscriber already exists" — for example they're on the newsletter list) used to
map straight to 204, which left that person OFF the beta list. Now a 409 triggers a lookup
(`GET /api/subscribers?query=subscribers.email='<addr>'`); if they're not yet on the beta list, the route adds it
(`PUT /api/subscribers/lists`, `action: "add"`, `status: "unconfirmed"`) and then explicitly sends the opt-in mail
(`POST /api/subscribers/{id}/optin`). The optin call is REQUIRED: the list-add endpoint does NOT send the confirmation
email on its own (verified against Listmonk's `ManageSubscriberLists` handler), so without it consent would be silently
implied. A subscriber already on the beta list is a quiet re-signup: no list change, no mail, no ping. Every outcome
returns the identical empty 204, so the response never reveals whether the address existed (no enumeration).

**Discord ping:** a successful signup pings Discord (`DISCORD_BETA_SIGNUP_WEBHOOK_URL`, falling back to
`DISCORD_WEBHOOK_URL` so it works before the `#beta-signups` channel exists) in `waitUntil` after the 204 ships,
drop-on-failure (the 204 never waits on Discord). The ping fires ONLY when a beta subscription was newly established (a
fresh 2xx, or the 409 add-to-list path), NEVER on a Listmonk failure and NEVER on a plain already-on-list 409. The embed
carries the email (full, same precedent as the feedback reply-to) and the signup time, and states the honest consent
status ("unconfirmed — Listmonk sent the confirmation email" for both paths). It carries no install id, by construction.

**In-app feedback:** `POST /feedback` is the open-beta "Send feedback" channel. JSON body: required `feedback` text
(trimmed, 1–100 000 Unicode code points; the cap matches the desktop dialog and the Rust validator) plus `appVersion` /
`osVersion`, optional reply-to `email` (loose shape check) and `buildMode`. Body capped at 512 KB. The D1 `feedback`
table is the durable sink, so unlike the other telemetry writes this one is AWAITED: a D1 failure returns a soft 502 the
desktop app surfaces as a gentle retry. The Discord ping (truncated preview, `[DEV]`/`[PROD]` title prefix from
`buildMode`) rides `waitUntil` after the 204; it prefers `DISCORD_FEEDBACK_WEBHOOK_URL` and falls back to
`DISCORD_WEBHOOK_URL` so feedback works with no new secret. No install id of any kind is read or stored, so feedback
can't be joined to the analytics stream. Rate-limited at 5/min/IP via `FEEDBACK_LIMITER` (IP never stored).

**Device tracking (fair use):** On each `/validate` call with a `deviceId`, the server tracks the device in KV
(`devices:{seatTransactionId}`) and logs to Analytics Engine (binding: `DEVICE_COUNTS`, dataset: `cmdr_device_counts`).
Devices older than 90 days are pruned on each write. If 6+ devices are active and no alert was sent in the past 30 days,
an internal email is sent to `legal@getcmdr.com` via Resend. Device tracking is fire-and-forget and never affects the
validation response. The KV value stores a `DeviceSet` with device hashes mapped to last-seen timestamps plus an
optional `lastAlertedAt`. Device tracking is per seat: each seat in a multi-seat purchase has its own transaction ID and
its own 6-device allowance.

**Update check proxy:** `GET /update-check/:version` routes update checks through the worker to count all users (free +
licensed). Without this, there's no signal for how many people actually run the app (Umami only tracks website visitors
and download tracking only captures installs).

**Error report R2 key shape:** `error-reports/{prod|dev}/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`. The env segment (`prod`
for release builds, `dev` for debug builds, inferred from `meta.buildMode`) keeps dev-run reports out of the production
sort order. Legacy keys (`error-reports/{yyyy-mm-dd}/...`, pre-env-prefix) still exist; eviction reads the date segment
via `extractDateSegment` which handles both shapes. The 90-day R2 lifecycle drains the legacy shape naturally. No
migration needed.

**Error report eviction (8/6 GB watermarks + 60-day age floor + lifecycle):** Three layers keep the bucket bounded.

1. **On-upload eviction**: every `POST /error-report` schedules `tryEvict` in `waitUntil(...)`. If `total_bytes` (KV) >
   8 GB and `eviction_in_progress` (KV, 60-s TTL lock) isn't set, lists R2 objects under `error-reports/`, keeps only
   those older than `EVICTION_MIN_AGE_DAYS`, sorts oldest-first by the embedded `yyyy-mm-dd` segment (via
   `extractDateSegment`, which handles both new and legacy key shapes) then by `uploaded`, deletes until ≤ 6 GB, then
   resets the counter to the recomputed ground truth.
2. **Daily cron sweep**: corrects KV drift by recomputing from R2, lifts an intake pause once the bucket is back under
   the LOW watermark, and re-runs `tryEvict`.
3. **R2 lifecycle rule**: 90-day expiration applied at provisioning time via `scripts/setup-cf-infra.sh`.

The KV counter is approximate (read-then-write, no atomic increment; same as `_meta:activation_count`). Both the daily
sweep and post-eviction recompute correct it. R2 deletes are idempotent; concurrent evictors deleting the same oldest
object cause no harm.

**Why the age floor exists (`EVICTION_MIN_AGE_DAYS`, 60 days):** `/error-report` is unauthenticated, so without a floor
anyone able to push the bucket past 8 GB turns eviction into a delete primitive aimed at the oldest (most likely
genuine) reports. Eviction's real job is only to pull the 90-day lifecycle forward under space pressure, so what it
deletes should already be near its natural end; under normal growth there is plenty of 60-day-old material, and a 30-day
eviction window remains before the lifecycle takes over.

Eviction is therefore **all-or-nothing**: when the eligible bundles can't free enough on their own, `tryEvict` deletes
NOTHING, sets `intake_paused`, and returns `{ outcome: 'paused' }` so the caller alerts Discord. Half-evicting would
destroy real reports AND leave the bucket over its watermark. A flood of fresh junk finds nothing eligible and costs
zero deletions; reaching eligibility would take 60 days of sustained flooding, alerting daily along the way.

A pause reads as one of two things: a flood filled the bucket with fresh bundles, or real traffic outgrew the watermarks
(raise them, or shorten the lifecycle). The daily sweep clears the flag on its own once the bucket is back under 6 GB;
resuming at the high watermark instead would reopen intake straight into the level that paused it.

**Error report intake admission (`error-report-intake.ts`):** the global ceiling the per-colo rate limiter can't give.
Both gates run before the body is read, so a rejected upload costs no parsing and no storage.

- **Daily byte budget** (`DAILY_INTAKE_BUDGET_BYTES`, 2 GB/UTC day): past it, `/error-report` returns 503 +
  `Retry-After` for the rest of the day and pings Discord ONCE (`budget_alert:{date}` claim). Legitimate traffic is
  orders of magnitude below this, so the ping is as much the point as the rejection. It also means filling the 8 GB
  watermark takes days of flooding, alerting each day.
- **Intake pause** (`intake_paused`): 503 while set. Written by eviction (above), cleared by the daily sweep, and
  settable by hand for an incident (`wrangler kv key put --binding ERROR_REPORT_META intake_paused 1`).
- **Notification cap** (`DAILY_NOTIFICATION_CAP`, 50/day): per-upload Discord embeds stop after 50, with one notice
  saying so, then silence until tomorrow. A webhook takes 30 messages/min, and a channel that goes quiet without
  explanation reads as "no reports". Bundles remain in R2 and in `GET /admin/error-reports` regardless. Eviction and
  budget alerts are NOT capped.

Every counter here is a racy read-then-write (KV has no atomic increment), so a concurrent burst can overshoot by
roughly the in-flight amount. Deliberate: these are coarse circuit breakers, and the 10 MB bundle cap keeps a single
overshoot small.

**Error report body cap:** `content-length` is advisory (a chunked upload declares no length; a declared one can lie),
so `readReportUpload` reads the body itself through `readCappedBody` and cancels past `MAX_BODY_BYTES` (bundle cap + 1
MB for the `meta` part and multipart framing, sized against the client's 100,000-char `userNote` limit). Without it the
multipart parser would buffer up to Cloudflare's 100 MB request limit inside a 128 MB isolate. Over-cap returns null
rather than throwing, so 413 stays distinguishable from a malformed-multipart 400 without matching on parser text.

**Error report Discord notifications:** Every upload triggers a Discord embed with a 7-day presigned R2 GET URL. Uses
the R2 S3-compatible API via `aws4fetch` (`AwsClient.sign` with `signQuery: true` + `X-Amz-Expires`). 7 days is R2's max
for presigned URLs. Convenience of click-to-download outweighs leak risk because only the maintainer accesses the
`#error-reports` channel.

**Short ID generation:** `generateShortId(prefix, len)` in `license.ts` produces IDs like `ERR-A2345` from the same
unambiguous alphabet (`23456789ABCDEFGHJKMNPQRSTUVWXYZ`) as license short codes. Rejection sampling avoids modulo bias.
The error report route does NOT regenerate the id server-side. It validates the client-supplied `meta.id` against the
shape `^ERR-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$` and uses it as-is. On the astronomically rare R2 key collision (same
id + same date + UUID clash), the route retries with a fresh UUID (never a fresh id), so the user-visible id from the
preview dialog stays stable through to the toast.

**Link codes (`?r=` tracking links, `src/link-codes.ts`):** Short, inconspicuous `?r=<code>` links (for example
`getcmdr.com/?r=rmc`) expand to UTM params client-side on getcmdr.com and David's blog. The code → meaning map lets
David invent a new code without a code change or deploy.

- **KV model:** the WHOLE map lives under ONE key (`codes`) in the `LINK_CODES` namespace, as JSON
  `{ "<code>": { "utm_source": "...", "utm_medium": "...", "note": "..." }, ... }`. The map is tiny (a handful of
  channels), so one blob keeps the public endpoint a single KV get and writes a trivial read-modify-write of one value.
  Key-per-code would buy nothing here.
- **Public endpoint `GET /r-codes.json`:** returns the map with the admin-only `note` stripped (source + medium only),
  `Access-Control-Allow-Origin: *` (public non-sensitive config, fetched cross-origin from both getcmdr.com and the blog
  at veszelovszki.com), and `Cache-Control: public, max-age=300`. The 5-minute edge cache keeps blog page loads off KV;
  a new code is live within the TTL. CORS preflight is `OPTIONS` → 204.
- **Admin CRUD (`/admin/r-codes*`, Bearer `ADMIN_API_TOKEN`):** `GET` lists the full map (with notes);
  `PUT /admin/r-codes/:code` upserts; `DELETE /admin/r-codes/:code` removes. The path `:code` must match `[a-z0-9._-]`,
  1..64 chars (`isValidCode`), else 400. `utm_source` is required and `utm_medium`/`note` are optional; UTM values run
  through `sanitizeUtmValue` (lowercase, drop outside `[a-z0-9._-]`, cap 120) — a source that sanitizes to empty is
  rejected 400. `note` is capped at 500 chars and never leaves the admin endpoint.
- **Charset is the contract:** `sanitizeUtmValue` mirrors the blogs' client-side sanitizer and the `/download` `ref`
  rule, so a stored value and a client pass-through value normalize identically. Keep them in sync if any changes.

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
`/validate` (offline crypto + UI testing only).

For end-to-end testing including `/validate`, use the Paddle sandbox checkout flow (see
[testing Paddle checkout](README.md#testing-paddle-checkout)).

### Testing Paddle checkout (sandbox)

See [testing Paddle checkout](README.md#testing-paddle-checkout). Requires setting up a Paddle client-side token and a
default payment link in the sandbox dashboard. This is an interactive, human-driven flow.

## Deployment

```bash
cd apps/api-server
npx wrangler secret put IP_HASH_PEPPER            # one-time, before the first deploy of the peppered hashing
npx wrangler d1 migrations apply cmdr-telemetry   # apply any new D1 migrations first
npx wrangler deploy
```

`IP_HASH_PEPPER` is a one-time setup step (`openssl rand -hex 32`), not a per-deploy one, but until it exists every
stored IP hash is brute-forceable. The Worker warns in the log rather than failing, so check for
`IP_HASH_PEPPER is not set` after the first deploy. Once it's set, the next retention sweep clears the weakly-hashed
rows written before it (90 days for `downloads`, seven days for `update_checks`).

Deployed to `api.getcmdr.com` via Cloudflare custom domain (declared in `wrangler.toml` `[[routes]]`).
`license.getcmdr.com` is a permanent alias for existing app versions. Fallback URL:
`cmdr-license-server.veszelovszki.workers.dev`. The cron trigger (`0 */3 * * *`) is declared in `wrangler.toml` under
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
**Why**: Both sandbox and live transactions use the same `txn_` prefix, so there's no reliable way to detect the
environment from a transaction ID. An explicit env var is unambiguous. `wrangler.toml` defaults to `"sandbox"` for local
dev; the deployed worker overrides to `"live"` via a wrangler secret.

**Decision**: Price IDs stored as env vars (`PRICE_ID_*`) rather than hardcoded. **Why**: Sandbox and live Paddle
accounts have different price IDs for the same products. Env vars let each environment use its own IDs without code
changes. `.dev.vars` has sandbox IDs; wrangler secrets have live IDs.

**Decision**: No hard enforcement of device limits; the server never rejects a validation because of device count.
**Why**: Suspension is a manual decision after human review. The goal is to detect obvious key sharing (one key on 6+
devices), not to restrict legitimate power users. Alert threshold is 6 because 3-4 Macs is normal, 5 is plausible, 6 is
hard to explain as one person. The threshold is not published in the ToS to avoid gaming.

**Decision**: Paddle as Merchant of Record (not Stripe, Gumroad, LemonSqueezy, or Polar). **Why**: All-inclusive pricing
(5% +
$0.50, no hidden non-US or EU payout fees), aggregate monthly payouts (one invoice for accountant instead of
per-transaction), handles global VAT/GST calculation and remittance, established reputation (Sketch, etc.). On a $29
sale: $1.95 fee → $27.05 net. At 30k sales, saves ~$7k/year vs LemonSqueezy. Stripe was rejected because solo-dev
handling VAT in 27+ EU countries is impractical (Stripe is a payment processor, not an MoR).

**Decision**: BSL 1.1 license with free personal use (supersedes earlier AGPL + trial model). **Why**: The AGPL + trial
model felt pushy for hobbyists (trial countdown, nagware). BSL gives friction-free personal use (no nags), clear
commercial terms (businesses know they must pay), and simpler enforcement (title bar shows license type, honor system
beats trial timers). Source converts to AGPL-3.0 after 3 years per release.

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
under concurrent `/activate` requests. **Why**: KV doesn't support atomic increment. The counter is approximate; if
exact counts matter, query the CF API to list KV keys, or switch to Durable Objects / D1.

**Gotcha**: The `/download/:version/:arch` redirect maps `x86_64` → `x64` in the filename. **Why**: `tauri-action` names
the Intel DMG `Cmdr_<ver>_x64.dmg`, but the rest of the codebase (URL path, D1 telemetry, website data attrs, Rust
target triple, `uname -m`) consistently uses `x86_64`. Mapping at the boundary keeps everything else canonical. Same
convention is already used in `.github/workflows/release.yml` when reading DMG sizes for `latest.json`.

**Gotcha**: Validators for optional fields posted from the Rust desktop client must tolerate **both `null` and
`undefined`**, not just `undefined`. **Why**: serde `Option::None` serializes as JSON `null`, not as an absent key.
`#[serde(skip_serializing_if = "Option::is_none")]` would omit the key but is rejected by `specta`'s unified mode (the
struct is part of a Tauri command surface). An old crash file read by a new client surfaces missing fields as `None`,
the client posts `"buildMode": null`, and a `!== undefined`-only check rejects it, losing exactly the upgrade-window
reports we want to keep. Pattern: `value !== undefined && value !== null && <shape check>`. See `telemetry.ts`
`validateCrashReportShape` for the canonical form.

## Dependencies

Runtime: `hono`, `@noble/ed25519`, `resend` Dev: `wrangler`, `vitest`, `typescript`, `eslint`, `prettier`

See also: `apps/desktop/src/lib/licensing/CLAUDE.md` (full frontend licensing feature overview)
