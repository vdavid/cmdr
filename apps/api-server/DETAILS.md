# API server details

Pull-tier docs for `apps/api-server/`: what's app-wide (routes, configuration, bindings, cron, retention, deployment).
Must-know invariants live in `CLAUDE.md`. Per-area depth lives with the code:

- `src/licensing/DETAILS.md` — fulfillment, Paddle, device tracking, the sandbox runbooks.
- `src/telemetry/DETAILS.md` — crash/heartbeat/download/update-check payloads, error-report eviction and intake.
- `src/website/DETAILS.md` — Listmonk signup, blog likes, `?r=` link codes.
- `src/admin/DETAILS.md` — the dashboard endpoints and the funnel's column derivations.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Root files

- **`index.ts`**: Hono app assembly — mounts each area's route modules, wires the scheduled handler.
- **`types.ts`**: `Bindings`, shared constants, and the helpers every area calls: `verifyAdminAuth`,
  `enforceIpRateLimit`, `callerIp`, `hashCallerIp`, `isValidEmail`, `redactEmail`, `activationCountKey`.
- **`email.ts`**: Resend delivery (HTML + plain text, multi-seat), behind the single `sendViaResend` wrapper.
- **`discord.ts`**: Discord webhook client (single retry on 429, drop-on-failure).
- **`scheduled.ts`**: the cron jobs (crash notifications, daily aggregation, DB size, retention sweep, eviction sweep).
  Tests split by axis: `crash-notification-email.test.ts` covers the one job whose output is a document (query → row →
  rendered HTML and subject), `scheduled.test.ts` the DB-writing jobs, with the D1/env fakes in `cron-test-helpers.ts`.
- **`user-agent.ts`**: `classifyUaFamily` / `resolveUaFamily`, shared by the download write path and the funnel read
  path so neither area imports the other.
- **`scripts/generate-keys.js`**: Ed25519 key pair generation (run once at setup).
- **`scripts/setup-cf-infra.sh`**: Cloudflare KV namespace provisioning + the R2 lifecycle rule.

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
| GET     | `/likes/:slug`             | none          | Blog-post like count + whether this caller already liked it                                        |
| POST    | `/likes/:slug`             | IP rate-limit | Like a blog post (idempotent per caller pseudonym)                                                 |
| DELETE  | `/likes/:slug`             | IP rate-limit | Unlike a blog post                                                                                 |
| OPTIONS | `/likes/:slug`             | none          | CORS preflight (204), getcmdr.com origins only                                                     |
| GET     | `/r-codes.json`            | none          | Public `?r=<code>` → UTM map (note stripped), edge-cached 5 min, `Access-Control-Allow-Origin: *`  |
| OPTIONS | `/r-codes.json`            | none          | CORS preflight (204)                                                                               |
| GET     | `/admin/r-codes`           | Bearer token  | Full code map including admin `note`                                                               |
| PUT     | `/admin/r-codes/:code`     | Bearer token  | Upsert a code: `{ utm_source, utm_medium?, note? }` (utm values sanitized; code charset validated) |
| DELETE  | `/admin/r-codes/:code`     | Bearer token  | Remove a code from the map                                                                         |

## Environments

Sandbox (dev) and live (prod) are **completely separated**. They share the same codebase but have different Paddle
accounts, API keys, price IDs, webhook secrets, and notification destinations. There is no cross-environment routing.
`PADDLE_ENVIRONMENT` (in `wrangler.toml`, overridable as a wrangler secret) selects the Paddle API base URL and key; it
defaults to `"sandbox"`, and the deployed worker overrides it to `"live"`. Rationale: `src/licensing/DETAILS.md` §
Decisions.

### Configuration

| Secret / var                       | `.dev.vars` (local dev)          | Wrangler secret (deployed worker)  |
| ---------------------------------- | -------------------------------- | ---------------------------------- |
| `PADDLE_ENVIRONMENT`               | `"sandbox"` (from wrangler.toml) | `"live"`                           |
| `PADDLE_WEBHOOK_SECRET_SANDBOX`    | Sandbox secret                   | Sandbox secret (for safety)        |
| `PADDLE_WEBHOOK_SECRET_LIVE`       | n/a                              | Live secret                        |
| `PADDLE_API_KEY_SANDBOX`           | Sandbox API key                  | n/a                                |
| `PADDLE_API_KEY_LIVE`              | n/a                              | Live API key                       |
| `PRICE_ID_COMMERCIAL_SUBSCRIPTION` | Sandbox price ID                 | Live price ID                      |
| `PRICE_ID_COMMERCIAL_PERPETUAL`    | Sandbox price ID                 | Live price ID                      |
| `ED25519_PRIVATE_KEY`              | DEV private key hex              | PRODUCTION private key hex         |
| `RESEND_API_KEY`                   | Resend key                       | Same Resend key                    |
| `CRASH_NOTIFICATION_EMAIL`         | `david@getcmdr.com`              | Recipient email for crash alerts   |
| `DISCORD_WEBHOOK_URL`              | Same webhook URL                 | Discord webhook for error reports  |
| `DISCORD_BETA_SIGNUP_WEBHOOK_URL`  | Optional (falls back)            | Optional `#beta-signups` webhook   |
| `R2_ACCOUNT_ID`                    | Same account ID                  | For minting presigned R2 URLs      |
| `R2_ACCESS_KEY_ID`                 | Same access key                  | R2 S3-compat access key (read OK)  |
| `R2_SECRET_ACCESS_KEY`             | Same secret                      | Paired secret for R2 access key    |
| `LISTMONK_API_URL`                 | `https://mail.getcmdr.com`       | Same base URL                      |
| `LISTMONK_API_USER`                | Listmonk API user                | Same (least-privilege at deploy)   |
| `LISTMONK_API_TOKEN`               | Listmonk API token               | Same (least-privilege at deploy)   |
| `LISTMONK_BETA_LIST_ID`            | Beta-list numeric id             | Same id                            |
| `IP_HASH_PEPPER`                   | Any random string                | Makes every stored IP hash one-way |

`ED25519_PRIVATE_KEY` is two different keys, unlike the rows marked "Same". The production signer can mint a license
every shipped build accepts, so it exists only as a wrangler secret; `.dev.vars` gets its own pair, and the desktop app
verifies against whichever public key matches its build mode. Full rationale and the rotation caveat:
`apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signing keys.

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
| `LIKES_LIMITER`        | Rate limit   | Gates `POST`/`DELETE /likes/:slug` at 20 req/min/IP (bounds unauthenticated KV growth) |
| `BLOG_LIKES`           | KV namespace | One key per post (`likes:<slug>`) holding the count and the caller pseudonyms          |

**Rate limits are per data center, not global.** Cloudflare's rate-limit bindings count per colo
([docs](https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/)), so each one bounds a single
abusive client and not a distributed flood. `enforceIpRateLimit` (`types.ts`) is the single gate every route calls;
`/error-report` carries a global ceiling on top (`src/telemetry/DETAILS.md` § Intake admission), because it's the one
where a flood is expensive.

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

The error-report route mints 7-day presigned GET URLs for the zip bundles in R2 and embeds them in Discord
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

## Storage

**D1 for telemetry and fulfillment:** crash reports, downloads, update checks, heartbeats, feedback, and the
`license_issuance` record all live in D1 (binding `TELEMETRY_DB`, database `cmdr-telemetry`). Migrations live in
`migrations/` (latest: `0015_crash_app_fate.sql`, the nullable `app_fate` column the crash email ranks rows by;
`0014_downloads_daily_unique.sql` is the distinct-downloader rollup the retention sweep writes;
`0013_minimize_stored_identifiers.sql` adds `downloads.ua_family` and erases the crash-table IP hashes;
`0012_license_issuance.sql` is the fulfillment record; `0011_crash_panic_message.sql` adds the nullable `panic_message`
column; `0007_feedback.sql` adds the `feedback` table; `0006_crash_diag_email.sql` adds the nullable `diag_id` + `email`
columns; `0005_heartbeat.sql` adds the `heartbeat` table). Apply with `wrangler d1 migrations apply cmdr-telemetry`
before deploying changes that add tables or columns.

`license_issuance` is the one money-critical table in an otherwise telemetry-shaped database: it shares the binding
because a second D1 buys nothing at a few hundred rows a year, and nothing prunes it (the daily aggregation job only
touches `update_checks`). The only remaining Analytics Engine dataset is `DEVICE_COUNTS` for fair-use monitoring. All
other state (license codes, activation counter, device sets, link codes, blog likes) lives in Cloudflare KV. Short codes
never expire (perpetual licenses last forever); subscription validity is checked live via the Paddle API.

**Workers types entrypoint:** `tsconfig.json` pins `@cloudflare/workers-types/2023-07-01`, not the package root (which
resolves to the 2021-11-03 snapshot). The root snapshot predates `R2ListOptions.include` and `R2Object.customMetadata`,
which `/admin/error-reports` needs. The dated entrypoint matches the runtime better anyway (`compatibility_date` is
2025-01-01); don't revert it to the bare package name.

## Cron handler

A single `scheduled` handler runs every 3 hours (`0 */3 * * *`). It runs its jobs in independent try-catch blocks so one
failure doesn't block the others:

1. **Crash notifications** (every invocation): queries `crash_reports WHERE notified_at IS NULL`, sorted newest-first,
   marks rows as notified, then sends an email via Resend with one row per crash report (When, Env, Fate, ID, Site,
   Signal, Version, Reply to) plus a full-width sub-row carrying the redacted `panic_message` (an em-dash when the row
   has none). Marks before sending to prefer missed notifications over duplicates. Requires `CRASH_NOTIFICATION_EMAIL`
   and `RESEND_API_KEY`.
   - **Fate is the severity ranking**, and the reason the email is worth reading row by row: `crashed` (red) for
     `app_fate = 'ended'`, `kept running` (amber) for `'keptRunning'`, `?` (gray) for a NULL or `'unconfirmed'` row that
     claims nothing. Two rows that both read `signal: panic` are otherwise indistinguishable, though one killed the app
     and one didn't. The subject carries it too, since that's all you see without opening: the plain count when nothing
     survived, `, the app kept running` when every report did, and `(N kept running)` for a mix. Only survivors are
     counted there — a NULL fate is never tallied as a crash.
2. **Daily aggregation** (00:00 UTC only): aggregates yesterday's `update_checks` into `daily_active_users` via
   `INSERT OR IGNORE ... GROUP BY`, then prunes raw update checks older than 7 days. Idempotent via existence check.
3. **DB size check** (00:00 UTC only): queries the D1 pragma for total database size, alerting by email over 100 MB.
4. **Retention sweep** (00:00 UTC only): `handleRetentionSweep` enforces the per-table retention promises below.
5. **Daily eviction sweep** (00:00 UTC only): `handleDailyEvictionSweep` recomputes `total_bytes` from R2 ground truth
   (the per-upload KV counter is racy and drifts), clears `intake_paused` if the bucket is back under the LOW watermark,
   then triggers `tryEvict` if still over 8 GB. Idempotent, and it catches drift from concurrent uploads or a Worker
   dying mid-eviction.

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
  `daily_active_users`.
- **`crash_reports`**: `email` (the reply-to a beta tester attached) and `diag_id` cleared after 90 days; the technical
  row is kept indefinitely for long-standing stability work. `hashed_ip` is no longer written at all (migration `0013`
  erased the historical values) because nothing ever read it.
- **`feedback`**: the optional reply-to `email` cleared after two years; the message text stays.
- **`heartbeat`**: rows DELETED after two years. Two years covers every window the dashboard computes (DAU, new
  installs, D7 retention) with room to spare.
- **Error report bundles**: 90-day R2 lifecycle, plus capacity-driven eviction that never touches anything under 60 days
  (`src/telemetry/DETAILS.md` § Eviction). Not part of this sweep.

Two invariants the sweep must keep, both pinned by tests in `src/scheduled.test.ts`:

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

## Local development

```sh
pnpm dev          # starts wrangler dev server on :8787
pnpm test         # vitest unit tests
```

**Run wrangler from anywhere in the repo.** `wrangler` is a local devDependency, not global. From inside
`apps/api-server/` use `npx wrangler …`; from the repo root (no `cd` needed) use the pnpm filter form:

```sh
pnpm --filter @cmdr/api-server exec wrangler secret put DISCORD_WEBHOOK_URL
pnpm --filter @cmdr/api-server exec wrangler deploy
```

Both forms resolve the same local `wrangler` binary. Paddle-specific local setup (ngrok for sandbox webhooks, minting a
test license key): `src/licensing/DETAILS.md` § Sandbox runbooks.

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
rows written before it (90 days for `downloads`, seven days for `update_checks`). The blog-like pseudonyms are the
exception to that self-healing, since KV has no retention sweep: `src/website/DETAILS.md` § Blog likes.

Deployed to `api.getcmdr.com` via a Cloudflare custom domain (declared in `wrangler.toml` `[[routes]]`).
`license.getcmdr.com` is a permanent alias for existing app versions. Fallback URL:
`cmdr-license-server.veszelovszki.workers.dev`. The cron trigger (`0 */3 * * *`) is declared in `wrangler.toml` under
`[triggers]` and deploys automatically with `wrangler deploy`.

### Troubleshooting deployment

- **522 on `api.getcmdr.com`**: Custom domain isn't routing to the Worker. Check `npx wrangler deploy` output shows
  `api.getcmdr.com (custom domain)`. The `[[routes]]` block in `wrangler.toml` may be missing, or a DNS record is
  blocking it.
- **"externally managed DNS records"**: Delete the manual DNS record via CF API/dashboard, then redeploy.
- **"kv bindings require kv write perms"**: API token missing "Workers KV Storage: Edit". Update at
  https://dash.cloudflare.com/profile/api-tokens.
- **Workers.dev works but custom domain doesn't**: Domain binding failed. Check the error in the deploy output.

## Decisions

**BSL 1.1 with free personal use** (supersedes the earlier AGPL + trial model). The AGPL + trial model felt pushy for
hobbyists (trial countdown, nagware). BSL gives friction-free personal use (no nags), clear commercial terms (businesses
know they must pay), and simpler enforcement (the title bar shows license type, honor system beats trial timers). Source
converts to AGPL-3.0 after 3 years per release.

Payment-provider, pricing, and device-limit decisions live with the code that implements them:
`src/licensing/DETAILS.md` § Decisions.

## Dependencies

Runtime: `hono`, `@noble/ed25519`, `resend`, `aws4fetch`. Dev: `wrangler`, `vitest`, `typescript`, `eslint`, `prettier`.

See also: `apps/desktop/src/lib/licensing/CLAUDE.md` (the frontend licensing feature).
