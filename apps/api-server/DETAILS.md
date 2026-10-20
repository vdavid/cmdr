# API server details

Pull-tier docs for `apps/api-server/`: what's app-wide (routes, configuration, bindings, cron, retention, deployment).
Must-know invariants live in `CLAUDE.md`. Per-area depth lives with the code:

- `src/licensing/DETAILS.md` — fulfillment, Paddle, device tracking, the sandbox runbooks.
- `src/telemetry/DETAILS.md` — crash/heartbeat/download/update-check payloads, error-report eviction and intake.
- `src/website/DETAILS.md` — Listmonk signup, blog likes, `?r=` link codes.
- `src/admin/DETAILS.md` — the dashboard endpoints and the funnel's column derivations.
- `src/s3-prices/DETAILS.md` — the S3 price table's response headers and caching.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Root files

- **`index.ts`**: Hono app assembly — mounts each area's route modules, wires the scheduled handler.
- **`admin-auth.ts`**: `verifyAdminAuth`, the one admin gate every `/admin/` route calls first (§ Admin gate).
- **`types.ts`**: `Bindings`, shared constants, and the helpers every area calls: `enforceIpRateLimit`, `callerIp`,
  `hashCallerIp`, `isValidEmail`, `hasEmailShape` / `emailShapePattern` (the one loose reply-to check, shared by crash
  reports, feedback, error reports, and amendments), `isAbsent`, `readCappedBody`, `scheduleBackground`, `redactEmail`,
  `activationCountKey`, `formatBytes`. The last four live here rather than beside one route precisely so a second route
  can't reimplement them: a hand-rolled body read is how a size cap becomes decorative.
- **`email/`**: Resend delivery, split one module per audience so a sender is read and changed without the others in
  view.
  - **`send.ts`**: the only door out. `sendViaResend` (every sender goes through it), `humanReportRecipient` (the one
    place the `FEEDBACK_NOTIFICATION_EMAIL ?? CRASH_NOTIFICATION_EMAIL` rule lives), and `sendEmailPathProbe` (the daily
    liveness check, § Cron handler job 9).
  - **`layout.ts`**: the HTML vocabulary. `escapeHtml`, `documentShell` (the `<!DOCTYPE>`…`<body>` wrapper),
    `bodyStyle`, the card constants, `replyToLine`, `boardLine` (an entry's link to its card), and the table cell styles
    (`CELL_STYLE`, `headCellStyle`, `TABLE_STYLE`). Everything is inline style with explicit hex: mail clients strip
    `<style>` blocks and their `prefers-color-scheme` support is a coin flip, so the pages commit to one light palette.
  - **`digest.ts`**: the three-hour report digest, the one email reports produce (§ Cron handler job 2). It composes the
    renderers in **`crash.ts`** (a table: machine facts scanned down a column), **`error-report.ts`** (report and
    amendment cards), and **`feedback.ts`** (cards: prose would shred in a table column). Then **`ops-alerts.ts`** (DB
    size, device count) and **`license.ts`** (key delivery).
  - **Decision: two visual families, deliberately.** The digest renders on `#1f2937` over an explicit white ground; the
    ops alerts (DB size, device count) render tables on `#333` over the client's default ground. `bodyStyle` takes the
    color and width rather than hiding the difference, because converging them changes what lands in a human inbox and
    so belongs to a copy-and-design pass, not a refactor.
  - `license.ts` exports its two copy builders (`getLicenseDescription`, `getIntroText`) so the three validity sentences
    and the two openers are pinned by `license.test.ts` rather than reviewed by eye. `src/licensing/DETAILS.md` § Manual
    licenses says why a dated license must never read as renewing.
  - **Decision: `license.ts` shares only `escapeHtml`.** It is the one email a customer receives, the one with a
    plain-text alternative (it carries a key the reader has to copy, so it must survive an HTML-refusing client), and
    the one styled by a `<head>` `<style>` block with class names. Forcing it onto the ops chrome would trade a real
    difference for a false symmetry.
  - There is no barrel file: importers name the module they need (`./email/digest`, `../email/ops-alerts`), so an import
    says which family of mail a route touches.
  - **Every sender has a real-runtime lane.** `email-runtime.test.ts` drives them through the actual Worker and asserts
    the Resend payload off the wire; `src/licensing/webhook-runtime.test.ts` does `sendLicenseEmail`. Adding a sender
    means adding it there too, or it ships untested against the runtime it deploys to. § Test runtimes.
- **`ops-events.ts`**: the operational notices (cron failures, eviction, CSP violations, signups) as D1 rows the digest
  mails. § Ops events.
- **`cron-health.ts`**: `pingCronHealth`, the healthchecks.io dead-man's switch for the cron tick. § Cron alarms.
- **`report-outbox.ts`**: the cron jobs that file every pending crash, error report, feedback message, and amendment as
  a card on the reports board. § The reports repo.
- **`report-cards.ts`**: what goes ON a card (titles, bodies, labels, the expiring personal comment); pure, no network.
  **`github-issues.ts`** is the GitHub transport (the privacy probe, filing, comments, labels, listing, the sweep).
- **`report-ratings.ts`**: the cron job that copies the triage agent's `Rating:` line onto a report card's Severity,
  Impact, and Size fields on the backlog board. § Card ratings.
- **`report-digest.ts`**: the cron job that mails the three-hour digest and stamps what it sent. § Cron handler.
- **`morning-brief.ts`**: the 06:30 Stockholm brief: who's waiting on a reply, unprocessed cards, the last day's
  arrivals, and the tripwires. § Cron handler job 10.
- The five are tested against a REAL SQLite built from the migrations (`sqlite-d1-test-helper.ts`), because the risk of
  an outbox is a row a WHERE clause quietly skips: `report-outbox.test.ts`, `report-digest.test.ts`,
  `report-retention.test.ts`, `morning-brief.test.ts`, `report-ratings.test.ts`.
- **`scheduled.ts`**: the other cron jobs (license backup, daily aggregation, DB size, retention sweep, eviction sweep,
  personal-comment sweep). `scheduled.test.ts` covers the DB-writing jobs, `cron-alarm.test.ts` the handler's alarm
  wiring, and `email-path-probe.test.ts` the daily liveness check, with the D1/env fakes in `cron-test-helpers.ts`.
- **`user-agent.ts`**: `classifyUaFamily` / `resolveUaFamily`, shared by the download write path and the funnel read
  path so neither area imports the other.
- **`scripts/generate-keys.js`**: Ed25519 key pair generation (run once at setup).
- **`scripts/setup-cf-infra.sh`**: Cloudflare KV namespace provisioning + the R2 lifecycle rule.
- **`scripts/mint-license.js`** / **`scripts/revoke-license.js`**: hand out a license, or take one back. They share
  `scripts/admin-api.js` (token + request). Runbook: `src/licensing/DETAILS.md` § Manual licenses.

## Routes

| Method  | Path                           | Auth          | Purpose                                                                                            |
| ------- | ------------------------------ | ------------- | -------------------------------------------------------------------------------------------------- |
| GET     | `/`                            | none          | Health check                                                                                       |
| POST    | `/webhook/paddle`              | HMAC sig      | Purchase completed → generate & email key(s); full refund or chargeback → revoke them              |
| POST    | `/activate`                    | none          | Exchange short code → full cryptographic key                                                       |
| POST    | `/validate`                    | none          | Check subscription status via Paddle API                                                           |
| POST    | `/admin/generate`              | Bearer token  | Mint a hand-issued license (evaluation, partner, thank-you, support recovery); writes the ledger   |
| POST    | `/admin/revoke`                | Bearer token  | Kill a hand-issued license by short code or manual transaction id                                  |
| GET     | `/admin/licenses`              | Bearer token  | Every license in the ledger with a computed state, plus KV codes no row explains (and the reverse) |
| PUT     | `/admin/licenses/:id/note`     | Bearer token  | Rewrite one row's note, either source. A hand-issued license can't be left without one             |
| GET     | `/admin/stats`                 | Bearer token  | Activation count + device count (for analytics dashboard)                                          |
| GET     | `/admin/downloads`             | Bearer token  | Aggregated downloads by day/version/arch/country/source, with raw `count` + deduped `uniqueCount`  |
| GET     | `/admin/active-users`          | Bearer token  | Aggregated daily active users by version/arch                                                      |
| GET     | `/admin/update-activity`       | Bearer token  | Per-day distinct update-enabled installs by version (retained aggregate ∪ today's raw)             |
| GET     | `/admin/crashes`               | Bearer token  | Aggregated crash data by day/crash site/signal                                                     |
| GET     | `/admin/heartbeat-dau`         | Bearer token  | Per-day DAU (distinct `anal_id`) + app hours (`uptime_seconds`, an hour per legacy beat)           |
| GET     | `/admin/funnel`                | Bearer token  | Per-UTC-day acquisition funnel for the last N days (downloads, installs, DAU, D7, signups)         |
| GET     | `/admin/feedback`              | Bearer token  | In-app feedback rows from D1 (full text, reply-to email, `answeredAt`), newest first               |
| PUT     | `/admin/feedback/:id/answered` | Bearer token  | Mark one feedback row answered (`{ answered: true }`) or back to awaiting a reply (`false`)        |
| GET     | `/admin/error-reports`         | Bearer token  | Per-bundle error-report metadata from the R2 prod prefix (`list` + custom metadata), newest first  |
| GET     | `/download/:version/:arch`     | none          | Log download to D1 (bots skipped, source + `ref` tagged), 302 → GitHub; `:version` takes `latest`  |
| POST    | `/crash-report`                | IP rate-limit | Ingest crash report to D1                                                                          |
| POST    | `/heartbeat`                   | IP rate-limit | Usage heartbeat + relayed feature events (anonymous `anal_id`) to D1                               |
| POST    | `/error-report`                | IP rate-limit | Multipart upload (zip + meta) → R2 + an outbox row. Also gated by the global intake budget         |
| POST    | `/error-report/:id/amend`      | amend key     | Add a note or reply-to address to a report already in R2 (`.amend.json` sidecar + email)           |
| POST    | `/beta-signup`                 | IP rate-limit | Subscribe a contact email to the Listmonk beta list (NO install id)                                |
| POST    | `/newsletter-signup`           | IP rate-limit | getcmdr.com's newsletter form → the Listmonk newsletter list (CORS: getcmdr.com)                   |
| POST    | `/feedback`                    | IP rate-limit | Ingest in-app feedback to D1 (the outbox row)                                                      |
| GET     | `/update-check/:version`       | none          | Log update check to D1 (deduped), 302 → latest.json                                                |
| GET     | `/likes/:slug`                 | none          | Blog-post like count + whether this caller already liked it                                        |
| POST    | `/likes/:slug`                 | IP rate-limit | Like a blog post (idempotent per caller pseudonym)                                                 |
| DELETE  | `/likes/:slug`                 | IP rate-limit | Unlike a blog post                                                                                 |
| OPTIONS | `/likes/:slug`                 | none          | CORS preflight (204), getcmdr.com origins only                                                     |
| POST    | `/csp-report`                  | IP rate-limit | getcmdr.com CSP violation reports; our own breakage raises an ops alert once a day per origin      |
| OPTIONS | `/csp-report`                  | none          | CORS preflight (204) for Reporting API batches, getcmdr.com origins only                           |
| GET     | `/r-codes.json`                | none          | Public `?r=<code>` → UTM map (note stripped), edge-cached 5 min, `Access-Control-Allow-Origin: *`  |
| OPTIONS | `/r-codes.json`                | none          | CORS preflight (204)                                                                               |
| GET     | `/s3-prices/v1`                | none          | The app's S3 list-price table (JSON), `max-age=3600` + `ETag`; byte-identical to the crate's copy  |
| GET     | `/admin/r-codes`               | Bearer token  | Full code map including admin `note`                                                               |
| PUT     | `/admin/r-codes/:code`         | Bearer token  | Upsert a code: `{ utm_source, utm_medium?, note? }` (utm values sanitized; code charset validated) |
| DELETE  | `/admin/r-codes/:code`         | Bearer token  | Remove a code from the map                                                                         |

## Environments

Sandbox (dev) and live (prod) are **completely separated**. They share the same codebase but have different Paddle
accounts, API keys, price IDs, webhook secrets, and notification destinations. There is no cross-environment routing.
`PADDLE_ENVIRONMENT` (in `wrangler.toml`, overridable as a wrangler secret) selects the Paddle API base URL and key; it
defaults to `"sandbox"`, and the deployed worker overrides it to `"live"`. Rationale: `src/licensing/DETAILS.md` §
Decisions.

### Configuration

| Secret / var                       | `.dev.vars` (local dev)          | Wrangler secret (deployed worker)   |
| ---------------------------------- | -------------------------------- | ----------------------------------- |
| `PADDLE_ENVIRONMENT`               | `"sandbox"` (from wrangler.toml) | `"live"`                            |
| `PADDLE_WEBHOOK_SECRET_SANDBOX`    | Sandbox secret                   | Sandbox secret (for safety)         |
| `PADDLE_WEBHOOK_SECRET_LIVE`       | n/a                              | Live secret                         |
| `PADDLE_API_KEY_SANDBOX`           | Sandbox API key                  | n/a                                 |
| `PADDLE_API_KEY_LIVE`              | n/a                              | Live API key                        |
| `PRICE_ID_COMMERCIAL_SUBSCRIPTION` | Sandbox price ID                 | Live price ID                       |
| `PRICE_ID_COMMERCIAL_PERPETUAL`    | Sandbox price ID                 | Live price ID                       |
| `ED25519_PRIVATE_KEY`              | DEV private key hex              | PRODUCTION private key hex          |
| `RESEND_API_KEY`                   | Resend key                       | Same Resend key                     |
| `CRASH_NOTIFICATION_EMAIL`         | `david@getcmdr.com`              | Report digest + ops alert recipient |
| `FEEDBACK_NOTIFICATION_EMAIL`      | unset (falls back)               | Optional report digest recipient    |
| `R2_ACCOUNT_ID`                    | Same account ID                  | For minting presigned R2 URLs       |
| `R2_ACCESS_KEY_ID`                 | Same access key                  | R2 S3-compat access key (read OK)   |
| `R2_SECRET_ACCESS_KEY`             | Same secret                      | Paired secret for R2 access key     |
| `LISTMONK_API_URL`                 | `https://mail.getcmdr.com`       | Same base URL                       |
| `LISTMONK_API_USER`                | Listmonk API user                | Funnel only (signups need none)     |
| `LISTMONK_API_TOKEN`               | Listmonk API token               | Funnel only (signups need none)     |
| `LISTMONK_*_LIST_ID` / `_UUID`     | Beta + newsletter list ids/UUIDs | Same ([vars], not secrets)          |
| `IP_HASH_PEPPER`                   | Any random string                | Makes every stored IP hash one-way  |
| `HEALTHCHECKS_PING_URL`            | unset (skips the ping)           | healthchecks.io cron ping URL       |

`ED25519_PRIVATE_KEY` is two different keys, unlike the rows marked "Same". The production signer can mint a license
every shipped build accepts, so it exists only as a wrangler secret; `.dev.vars` gets its own pair, and the desktop app
verifies against whichever public key matches its build mode. Full rationale and the rotation caveat:
`apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signing keys.

**R2/KV bindings** (declared in `wrangler.toml`, provisioned via `./scripts/setup-cf-infra.sh`):

| Binding                      | Type         | Purpose                                                                                   |
| ---------------------------- | ------------ | ----------------------------------------------------------------------------------------- |
| `ERROR_REPORTS_BUCKET`       | R2 bucket    | Stores error report zip bundles (`cmdr-error-reports`, 90-day TTL)                        |
| `ERROR_REPORT_META`          | KV namespace | Eviction bookkeeping + intake admission counters (key list below)                         |
| `LINK_CODES`                 | KV namespace | One key (`codes`) holds the whole `?r=<code>` → UTM map (see the note below)              |
| `HEARTBEAT_LIMITER`          | Rate limit   | Gates `POST /heartbeat` at 12 req/min/IP (`[[ratelimits]]`, type `RateLimit`)             |
| `SIGNUP_LIMITER`             | Rate limit   | Gates both signup routes at 5 req/min/IP, one shared window (signups are rare)            |
| `FEEDBACK_LIMITER`           | Rate limit   | Gates `POST /feedback` at 5 req/min/IP (real feedback is rare; spam loops aren't)         |
| `ERROR_REPORT_LIMITER`       | Rate limit   | Gates `POST /error-report` at 3 req/min/IP (tightest: each request stores up to 10 MB)    |
| `ERROR_REPORT_AMEND_LIMITER` | Rate limit   | Gates `POST /error-report/:id/amend` at 10 req/min/IP (a note, not a bundle)              |
| `CRASH_REPORT_LIMITER`       | Rate limit   | Gates `POST /crash-report` at 10 req/min/IP (a crashing app flushes a small burst)        |
| `LIKES_LIMITER`              | Rate limit   | Gates `POST`/`DELETE /likes/:slug` at 20 req/min/IP (bounds unauthenticated KV growth)    |
| `BLOG_LIKES`                 | KV namespace | One key per post (`likes:<slug>`) holding the count and the caller pseudonyms             |
| `CSP_REPORT_LIMITER`         | Rate limit   | Gates `POST /csp-report` at 30 req/min/IP (a violating page load sends a handful)         |
| `ACTIVATE_LIMITER`           | Rate limit   | Gates `POST /activate` at 10 req/min/IP (keeps license-code guessing slow)                |
| `VALIDATE_LIMITER`           | Rate limit   | Gates `POST /validate` at 60 req/min/IP (loose for company NATs; a 429 keeps the cache)   |
| `CSP_ALERTS`                 | KV namespace | CSP alert dedupe: `csp:<directive>:<blocked origin>`, 24 h TTL, nothing about the visitor |

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
- `budget_alert:{yyyy-mm-dd}`: claimed by the one caller that records the day's "budget exhausted" alert. 48-h TTL.
- `report:{ERR-XXXXX}`: `{ env, date, key, amendKeyHash }` for one uploaded report, so an id is addressable without
  walking the bucket, and `/error-report/:id/amend` has something to authenticate against. 90-day TTL, matching the R2
  lifecycle. Only the credential's SHA-256 is here, never the credential. `src/telemetry/DETAILS.md` § The report index.

`LINK_CODES` detail: the one `codes` key maps `?r=<code>` → `{ utm_source, utm_medium?, note? }` (id
`6dbba67c8ece475daf3e8c0406d242c9`). Created with `wrangler kv namespace create LINK_CODES`; no preview id (matches the
other namespaces).

**Paddle dashboards**: [sandbox](https://sandbox-vendors.paddle.com) | [live](https://vendors.paddle.com)

### Ops events

`ops-events.ts` writes the operational notices to the D1 `ops_events` table: `cron-failure`, `board-add-failed`,
`board-rating-failed`, `csp-violation`, `eviction`, `eviction-blocked`, `intake-rejected`, `admin-auth-rejected`,
`webhook-signature-rejected`, and the two signup kinds. The three-hour digest mails the alerts (§ Cron handler job 2)
and leaves the signups for the morning brief to count. Nothing else watches a channel: there is no Discord integration.

- **Rejected requests are counted, not logged one by one** (`countOpsEvent`): a repeat adds to the unnotified row with
  the same kind and message (`occurrences`, migration `0024`), so a burst is one digest line and one small row, and the
  digest's stamp stays a short id list. The message names the route PATTERN or the webhook source, never the raw path,
  the caller, or what they sent, so the set of messages is fixed by the code.
- **Thresholds per kind** (`report-digest.ts::ALERT_THRESHOLDS`): a rejected admin request alerts on the first one (the
  only admin is David, so the expected count is zero), and bad webhook signatures alert once one window holds a few,
  since a provider retry can produce one now and then. Rows below their threshold are stamped without an email, which
  closes the window on them.

- **`recordOpsEvent` never throws**: it sits inside the paths whose failures it reports, so a broken D1 costs that
  alert, never the caller. A cron failure has healthchecks.io as its second, independent channel (§ Cron alarms).
- **No personal data, ever**: no address, no note, no IP. A signup event is the list and the time; a CSP event carries
  URLs with their query strings stripped. Pinned by `listmonk-signup.test.ts` and `csp-report.test.ts`.
- Rare alerts are deduped where they could flood: CSP once a day per `(directive, blocked origin)` (`CSP_ALERTS` KV),
  the intake budget once a day (`budget_alert:` key).

### Admin gate

`admin-auth.ts::verifyAdminAuth` is the one check in front of every `/admin/` route, wherever the route lives (`admin/`,
`licensing/`, `website/link-codes.ts`): ❌ no route compares the token itself. In order, it answers 500 when
`ADMIN_API_TOKEN` isn't set, 429 when the caller's IP is over `ADMIN_LIMITER` (checked before the token, per data center
like every limiter here), and 401 when the `Authorization` header doesn't carry the token (compared with
`constantTimeEqual`). A 401 counts an `admin-auth-rejected` ops event against the route pattern (§ Ops events).
`admin-auth.test.ts` walks `app.routes`, so a new `/admin/` route that skips the gate fails there.

Bad webhook signatures (`/webhook/paddle`, `/webhook/github`) count a `webhook-signature-rejected` event the same way.

### R2 presigned URLs (for error-report download links)

The cron mints 24-hour presigned GET URLs for the zip bundles in R2 for the digest and the card's personal comment. R2
bindings can't presign on their own, so the Worker uses the S3-compatible API via `aws4fetch` and three secrets:
`R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`.

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

**D1 for telemetry and fulfillment:** crash reports, downloads, update checks, heartbeats, relayed feature events,
feedback, and the `license_issuance` record all live in D1 (binding `TELEMETRY_DB`, database `cmdr-telemetry`).
Migrations live in `migrations/` (latest: `0026_feedback_error_report_id.sql`, the logs a feedback message came with, §
The reports repo; `0023_card_ratings.sql`, the triage comments the rating job has applied or flagged, § Card ratings;
`0022_report_outbox.sql`, the report outbox: `error_reports`, `error_report_amendments`, `ops_events`, and
`issue_number` on crashes and feedback, § The reports repo; `0021_license_adjustments.sql` is the refund and chargeback
record `/webhook/paddle` keeps beside the ledger (`src/licensing/DETAILS.md` § Refunds);
`0020_heartbeat_uptime_and_events.sql` is `heartbeat.uptime_seconds` and the `analytics_event` table `/heartbeat` relays
feature events into; `0017_manual_licenses.sql` is the `source` / `organization_name` / `expires_at` / `revoked_at` /
`note` columns that turn `license_issuance` into the ledger for hand-issued licenses as well as purchases;
`0016_feedback_notified_at.sql` is the `feedback.notified_at` column the report digest reads, which also stamps the
pre-existing rows so the first tick doesn't mail the backlog; `0015_crash_app_fate.sql` adds the nullable `app_fate`
column the crash email ranks rows by; `0014_downloads_daily_unique.sql` is the distinct-downloader rollup the retention
sweep writes; `0013_minimize_stored_identifiers.sql` adds `downloads.ua_family` and erases the crash-table IP hashes;
`0012_license_issuance.sql` is the fulfillment record; `0011_crash_panic_message.sql` adds the nullable `panic_message`
column; `0007_feedback.sql` adds the `feedback` table; `0006_crash_diag_email.sql` adds the nullable `diag_id` + `email`
columns; `0005_heartbeat.sql` adds the `heartbeat` table). Apply with `wrangler d1 migrations apply cmdr-telemetry`
before deploying changes that add tables or columns.

`license_issuance` is the one money-critical table in an otherwise telemetry-shaped database: it shares the binding
because a second D1 buys nothing at a few hundred rows a year, and nothing prunes it (the daily aggregation job only
touches `update_checks`). It records both kinds of license: a Paddle purchase (`source = 'paddle'`, where the row is a
fulfillment marker and Paddle holds the truth) and a hand-issued one (`source = 'manual'`, where the row IS the license
and `/validate` answers from it). `src/licensing/DETAILS.md` § Manual licenses. The only remaining Analytics Engine
dataset is `DEVICE_COUNTS` for fair-use monitoring. All other state (license codes, activation counter, device sets,
link codes, blog likes) lives in Cloudflare KV. Short codes never expire (perpetual licenses last forever); subscription
validity is checked live via the Paddle API.

**Workers types:** there's no `@cloudflare/workers-types` dependency. `wrangler types` generates
`worker-configuration.d.ts` from `wrangler.toml`, and `tsconfig.json` includes it. Two things come out of that file: the
runtime globals (`KVNamespace`, `D1Database`, `R2Bucket`, `RateLimit`, and the rest) at this Worker's
`compatibility_date`, and an `Env` interface derived from the bindings and `[vars]`. Deriving from the compatibility
date is the point: the published package tracks the newest runtime, so it types APIs a Worker pinned to 2025-01-01 can't
call.

The file is gitignored, and three things write it: the `prepare` script (so a plain `pnpm install` leaves it in place),
the `typecheck` script humans run standalone, and the `api-server-worker-types` check the ESLint and typecheck lanes
depend on. It's regenerated rather than compared, so it can't go stale. `Bindings` in `src/types.ts` stays hand-written
and is what the code uses: it carries the secrets `wrangler.toml` can't know about, and marks the optional ones the
routes degrade over.

Rerun `pnpm --filter @cmdr/api-server types:gen` after editing bindings or vars in `wrangler.toml`.

## Cron handler

A single `scheduled` handler serves two triggers: every 3 hours (`0 */3 * * *`, jobs 1–9) and the morning one
(`30 4,5 * * *`, job 10 only, dispatched on `event.cron`). Every job goes through `runCronJob`, which isolates one job's
failure from the rest and raises the alarm for it (§ Cron alarms):

1. **The board** (every invocation, first): `fileCrashCards`, `fileErrorReportCards`, `fileFeedbackCards`, then
   `postAmendmentsToBoard` file every pending report as a card. § The reports repo. Then `applyCardRatings` sets the
   Severity, Impact, and Size the triage agent wrote on each card it triaged (§ Card ratings).
2. **Report digest** (every invocation, right after the board, so it can link the cards just filed):
   `handleReportDigest` (`report-digest.ts`) sends ZERO OR ONE email covering every release-build crash, error report
   (hand-written and auto-sent), amendment, and feedback message whose `notified_at` is NULL, then stamps exactly those
   rows. Rendering is `src/email/digest.ts`, composing `crash.ts` (a table), `error-report.ts` and `feedback.ts`
   (cards). Recipient is `FEEDBACK_NOTIFICATION_EMAIL ?? CRASH_NOTIFICATION_EMAIL` (`david@getcmdr.com`).
   - **The subject IS the alert**: `Cmdr digest: 1 crash, 2 errors, 1 note added, 4 feedback in the past 3h`, naming
     only the kinds that arrived. A phone notification shows nothing else, so it's built for "is anything urgent?".
   - **One email for all four kinds, on purpose**: per-kind emails at intake trained David to ignore them, and an
     instant mail per report meant a flood of auto-sends would bury the one that needs an answer. Three hours is fine
     because the board, not the inbox, is where a report waits.
   - **Stamps `notified_at` AFTER the send.** `sendViaResend` throws on a rejected send, so a failure leaves every row
     NULL and the next tick retries in a bigger digest. Stamping first drops the alert permanently, and the credential
     failure that would cause it is exactly when reports matter most. The cost is a duplicate when a send succeeds and
     an `UPDATE` then fails; it's bounded by the cron interval, which a silent drop is not. ❌ Don't stamp before the
     send.
   - **Fate is the crash severity ranking**: `crashed` (red) for `app_fate = 'ended'`, `kept running` (amber) for
     `'keptRunning'`, `?` (gray) for a NULL or `'unconfirmed'` row that claims nothing. The subject names the mix
     (`2 crashes (1 kept running)`); a NULL fate is never tallied as a survivor.
   - **`replyTo` is set only when exactly one distinct address appears in the whole digest**, which makes answering that
     person a plain reply. With several there's no single right answer, so the per-entry `mailto:` links carry them.
   - Every entry links its card, or says it isn't on the board yet. Debug builds are left out entirely (and never
     stamped, which costs nothing).
   - **Logs sent with feedback show on the feedback entry** (id, size, download link, or "hadn't arrived yet"), and the
     report isn't listed again under error reports, though it's stamped with the rest. Logs that arrive after their
     feedback was already mailed come on their own in a later digest.
3. **License backup** (00:00 UTC only, and FIRST in the daily block, so a snapshot precedes anything that mutates):
   `handleLicenseBackup` writes every license and every stored key into one R2 object. What's in it and how a restore
   would read it: `src/licensing/DETAILS.md` § License backups.
4. **Daily aggregation** (00:00 UTC only): aggregates yesterday's `update_checks` into `daily_active_users` via
   `INSERT OR IGNORE ... GROUP BY`, then prunes raw update checks older than 7 days. Idempotent via existence check.
5. **DB size check** (00:00 UTC only): alerts by email over 100 MB, with the per-table row counts that say where the
   size went. The size is `meta.size_after`, which D1 stamps on the result of every statement, so a throwaway `SELECT 1`
   reads it and the under-threshold path costs one query; the row counts only run past the threshold.
   - ❌ **Never read the size from `pragma_page_count` / `pragma_page_size`.** They're SQLite's obvious answer and D1
     rejects the table-valued form with `SQLITE_AUTH` (verified against the live DB via `wrangler d1 execute --remote`,
     2026-09-04). This job shipped that way on 2026-03-24 and threw every day until the cron alarms surfaced it on
     2026-09-03, while its tests passed throughout: a mocked D1 answers whatever it's told, so no test here can see a
     dialect rejection. `scheduled.test.ts` asserts no statement names a pragma, which is the closest anchor available.
6. **Retention sweep** (00:00 UTC only): `handleRetentionSweep` enforces the per-table retention promises below.
7. **Synthetic heartbeat sweep** (00:00 UTC only): `handleSyntheticHeartbeatSweep` deletes the beats of installs that
   were never a person. See § Synthetic heartbeats below.
8. **Daily eviction sweep** (00:00 UTC only): `handleDailyEvictionSweep` recomputes `total_bytes` from R2 ground truth
   (the per-upload KV counter is racy and drifts), clears `intake_paused` if the bucket is back under the LOW watermark,
   then triggers `tryEvict` if still over 8 GB. Idempotent, and it catches drift from concurrent uploads or a Worker
   dying mid-eviction.
9. **Email path probe** (00:00 UTC only): `handleEmailPathProbe` sends one throwaway email through Resend so a dead
   `RESEND_API_KEY` surfaces within a day instead of at the moment a report digest or a buyer's license key needs it.
   Recipient is `delivered@resend.dev`, Resend's simulator address, so nobody receives it.
   - **It has to be a real send.** The key is scoped to sending only, so `GET /domains`, `/api-keys`, and `/emails` all
     return 401 however healthy the key is (verified against the live key, 2026-09-02); a check built on them would
     alarm constantly. ❌ Don't widen the key to "Full access" to make them work: it would test the wrong capability (a
     readable domain list doesn't prove sending works) and give a leaked key the power to delete our sending domain and
     mint its own keys.
   - Covers the Worker's Resend API path only. Listmonk authenticates to the same service over SMTP with a **separate**
     credential, so one can be dead while the other works. `src/website/DETAILS.md` owns the signup path.

10. **Morning brief** (the morning trigger, on the tick that is 06:xx in Stockholm): `handleMorningBrief`
    (`morning-brief.ts`, rendered by `src/email/morning-brief.ts`) sends one email every day, quiet days included, an
    hour after the triage agent runs. Subject:
    `Cmdr morning: 2 waiting on a reply, 1 unprocessed; in the last day 1 crash, 2 errors`.
    - **Why two UTC times**: Cloudflare crons are UTC and Stockholm moves between UTC+1 and UTC+2, so the trigger fires
      at 04:30 AND 05:30 UTC and `isMorningBriefTick` keeps the one that is 06:30 local. `morning-brief.test.ts` pins
      both seasons and that `MORNING_CRON` matches `wrangler.toml`.
    - **What it reads**: open `needs-reply` and `unprocessed` cards (`listOpenCards`), and the last 24 hours of release
      rows (crashes, error reports by kind, amendments, feedback, signups, ops alerts).
    - **The tripwires** ("Look closer"): reports still owed a card after six hours (filing is failing), yesterday's prod
      bundles in R2 outnumbering yesterday's `error_reports` rows (a failed outbox insert, which only logs), and a board
      GitHub wouldn't list. These are what make "nothing is lost" checkable rather than hoped for.
    - Its healthcheck report is failure-only: the check's period is the three-hour tick, and a success ping from the
      morning trigger would only blur it.

The default export uses the object form (`{ fetch, scheduled }`) required for cron support. The Hono `app` is also
exported as a named export so tests can use `app.request()`.

## Cron alarms

The cron jobs are the whole notification pipeline: the board, the report digest, retention, eviction. When one of them
stops working, nothing downstream complains, because "no crash email" and "no crashes" look identical. Three layers
cover that, and they cover different failures.

1. **An ops event, for a tick that ran and threw.** `runCronJob` (`index.ts`) catches, then writes a `cron-failure` ops
   event naming the job, the tick's `scheduledTime`, and the stringified error, so the next digest says what broke.
   `recordOpsEvent` never throws, so a broken D1 costs this layer and nothing else. It can't be the only layer: D1 and
   Resend are both things that can be broken here, and an alarm routed through the broken path can't fire.
2. **healthchecks.io, for a tick that never ran AND for one that threw** (`cron-health.ts`, `HEALTHCHECKS_PING_URL`).
   The switch pings after the last job, so a removed cron trigger, a bad deploy, or a Worker dying before the first job
   all surface when the expected ping doesn't arrive. A tick with failures pings `/fail` instead, which trips the check
   immediately and names the jobs in the body. healthchecks.io emails through its own infrastructure, not Resend, so
   this is the layer that still works when D1 or the Resend key is dead.
3. **Workers logs, for reading the stack afterwards.** `[observability] enabled = true` in `wrangler.toml`. Without it
   `console.error` is written nowhere and is visible only to a live `wrangler tail`, which is how a job could throw
   every three hours indefinitely and leave no trace. Layers 1 and 2 wake someone; this is what they read.

Both secrets are optional, and the cron behaves exactly as before when they're unset, so local dev and tests need no
setup. `HEALTHCHECKS_PING_URL` is a capability URL (anyone holding it can report the check healthy), hence a secret.

**To create or rotate the healthchecks.io check:**

1. At [healthchecks.io](https://healthchecks.io), add a check named "Cmdr cron". Period 3 hours, grace 1 hour: the
   Worker's own schedule plus room for a slow tick.
2. Copy its ping URL (shape `https://hc-ping.com/<uuid>`) and store it:
   ```sh
   pnpm --filter @cmdr/api-server exec wrangler secret put HEALTHCHECKS_PING_URL
   ```
3. Confirm the alert route on the check is an address that doesn't depend on Cmdr's own Resend account. The whole point
   of this layer is surviving a broken Cmdr email path.
4. Smoke-test both endpoints, then let the next real tick clear the check:
   ```sh
   curl -fsS -X POST -d 'manual test' "<ping-url>/fail" && curl -fsS -X POST -d 'manual test' "<ping-url>"
   ```

Tests: `cron-alarm.test.ts` drives the real handler with a D1 that throws and asserts both channels fire, that a later
job still runs after an earlier one throws, and that a dead alarm channel can't take the cron down.

## Data retention

Every window below is enforced in code by `handleRetentionSweep` (`scheduled.ts`, daily at 00:00 UTC) and stated to
users in `apps/website/src/pages/privacy-policy.astro` § "How long we keep your data". The two have to move together:
the policy is a promise about these tables, so changing a window, a column, or a table here means editing that page in
the same commit.

The sweep's default shape is **clear the identifying columns, keep the row**. Counts, version breakdowns, and crash
triage value live in the other columns, and there's no privacy reason to lose them. Only `heartbeat` and
`analytics_event` delete rows, because their stable `anal_id` IS the identifying data.

- **`downloads`**: `hashed_ip` and `user_agent` cleared after 90 days; the row (version, arch, country, continent,
  source, ref, referer, `ua_family`) is kept indefinitely. 90 days is what the two columns are FOR: same-day dedup needs
  one day, and re-tuning `classifyUaFamily` against real UA strings needs a recent sample.
- **`update_checks`**: raw rows deleted after seven days by `handleDailyAggregation`, after rolling into
  `daily_active_users`.
- **`crash_reports`**: `email` (the reply-to a beta tester attached) and `diag_id` cleared after 90 days; the technical
  row is kept indefinitely for long-standing stability work. `hashed_ip` is no longer written at all (migration `0013`
  erased the historical values) because nothing ever read it.
- **`feedback`**: the optional reply-to `email` cleared after two years; the message text stays.
- **`error_reports`** and **`error_report_amendments`**: rows DELETED at the bundle's 90 days, because they carry the
  same note and reply-to the bundle does. An amendment is aged by its report's `report_date`, never its own
  `created_at`. Pinned against a real SQLite by `report-retention.test.ts`.
- **`license_issuance`**: never swept. A fulfillment record with an end date is how a late redelivery mints a second set
  of licenses, and a manual row IS the license, so deleting it would revoke one by accident. It holds the buyer's or
  recipient's email and, for a hand-issued license, a free-text `note` naming who it's for; the privacy policy covers
  both under the license sections.
- **`card_ratings`**: never swept. GitHub comment and issue numbers, the applied `Rating:` line, and timestamps; no
  personal data, so the privacy policy has nothing to promise about it.
- **`license_adjustments`**: never swept, for the same audit reason; it's how we know a license was revoked by a refund.
  It holds no personal data (ids, amount, Paddle's reason), only purchase details the policy already covers.
- **`heartbeat`**: rows DELETED after two years. Two years covers every window the dashboard computes (DAU, new
  installs, D7 retention) with room to spare.
- **`analytics_event`**: rows DELETED after two years, the same "desktop usage stats" promise as `heartbeat`. Aged by
  `received_at` (our clock), never `occurred_at`: that's the client's clock, and a wrong one would keep a row forever.
- **Error report bundles**: 90-day R2 lifecycle (`error-reports/` prefix only, rule details in
  `src/telemetry/DETAILS.md` § Eviction), plus capacity-driven eviction that never touches anything under 60 days
  (`src/telemetry/DETAILS.md` § Eviction). Not part of this sweep. The same 90 days covers every reply-to address an
  error report can carry (`meta.email` inside the bundle zip, and any address in the `.amend.json` sidecar) and the
  `report:{id}` KV index entry, whose TTL is set to match. That is the same window `crash_reports.email` gets, reached
  by expiry rather than by a sweep: the R2 objects have no columns to clear, so the lifecycle IS the enforcement. ❌
  Never raise the index TTL past the lifecycle: an entry outliving its bundle is both a dangling pointer and an address
  kept past what the policy promises.

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

## The backlog board

Two repos feed the Cmdr backlog project (`https://github.com/users/vdavid/projects/2`), by two different routes, and the
split is forced by how GitHub scopes project access.

- **`vdavid/cmdr` (PUBLIC)** → `POST /webhook/github` (`webhook-github.ts` + `project-board.ts`). An `issues`/`opened`
  delivery adds the issue and sets Status to Triage.
- **`vdavid/cmdr-reports` (PRIVATE)** → the project's own built-in "Auto-add to project" workflow, configured in
  GitHub's UI.

**Why not one mechanism for both.** The free plan allows exactly ONE auto-add workflow, so one repo has to be served
some other way. It has to be the public one: adding a PRIVATE repo's issue to a USER-OWNED project requires a classic
PAT with `repo` scope (full read/write over every repo the account owns), because a `project`-scoped token cannot
resolve a private issue's node at all (verified against the live API, 2026-09-19:
`Could not resolve to a node with the global id`). A fine-grained PAT can carry `Projects: Read and write`, but only as
an ORGANIZATION permission, so it does nothing for a user-owned board. ❌ Never widen `GITHUB_PROJECT_TOKEN` to `repo`
to collapse these two paths: it would put a credential with full access to every repo next to the one thing this
subsystem exists to keep narrow. The real fix, if the split ever becomes a problem, is moving the project to an
organization, where a fine-grained token scoped to that org's projects plus read on one repo does everything.

**What the token can do**: `project` scope alone reads and rewrites every board the account can reach, and the REST
project-items endpoint returns the bodies of private issues on them, `cmdr-reports` cards included (verified
2026-10-14). It reads no repository contents. That reach is why it lives only here, in a Worker secret, and never on the
triage box, whose agent reads stranger-written text (§ Card ratings).

**Failures are loud.** A classic PAT expires, and the failure mode is issues quietly not arriving. `addIssueToBoard`
returns false on every failure and the route records a `board-add-failed` ops alert, naming the issue and pointing at
the token and the Worker logs. The status update is deliberately NOT part of that answer: an item in the wrong column is
cosmetic, an item that never arrived is not.

**A refused add isn't a failure until the board says so.** Agents file with `gh issue create --project`, which races the
webhook, and GitHub refuses the second of two concurrent adds with `Content already exists in this project` (verified in
the Worker logs, 2026-10-20: six false `board-add-failed` alerts in one filing burst). So on a refused add,
`addIssueToBoard` reads the issue's `projectItems` and counts it as added when the board is among them, leaving Status
to whoever won. The check is structural (board membership), never the error message.

**The signature is the only thing that refuses.** Everything the route understands answers 204, including ignored
events, because a 4xx on a normal delivery is a red cross in GitHub's webhook UI forever. Renovate and Dependabot are
skipped by author: Renovate's long-lived Dependency Dashboard issue carries no labels, so a label filter cannot catch
it.

## The reports repo

Every crash, error report, feedback message, and amendment becomes a card in the PRIVATE repo named by
`GITHUB_ISSUES_REPO` (`vdavid/cmdr-reports`), so triage happens on one board. `report-cards.ts` decides what goes on a
card, `github-issues.ts` writes it, and `report-outbox.ts` decides when. The token is the `GITHUB_ISSUES_TOKEN` secret:
a fine-grained PAT scoped to that ONE repo with Issues read+write. With either unset the integration is off and nothing
is filed.

**The outbox.** Intake only writes a D1 row (`crash_reports`, `error_reports`, `feedback`, `error_report_amendments`;
migration `0022_report_outbox.sql`). `issue_number IS NULL` means a card is still owed, `0` means never (rows that
predate the outbox), and a positive number is the card. Every three-hour tick runs `fileCrashCards`,
`fileErrorReportCards`, `fileFeedbackCards`, then `postAmendmentsToBoard`, each oldest first.

- **Why the cron files, not intake**: a GitHub outage, an expired token, or a tripped cap leaves the row pending and a
  later tick files it. Filing at intake was fire-and-forget, so any of those dropped the card for good.
- **Why a job THROWS after a failed filing** (having tried the rest): `runCronJob` turns it into the alarm. "Cards
  aren't reaching the board" must never be quiet. A tripped cap is not a failure; the rest wait for tomorrow.
- **Why `error_reports` holds the note and the reply-to**: the cron can't read them back out of a zip, and a card's
  personal comment needs them. The retention sweep deletes the row at the bundle's 90 days.

**What earns a card.** Everything from a `release` build, hand-written and auto-sent alike. An auto-send flood is what
the daily cap absorbs: 20 cards per source per UTC day (`gh_issue_count:{source}:{date}` keys in `ERROR_REPORT_META`),
because `kind` and `buildMode` come from the client and a mislabelling build must cost a bounded amount. The morning
triage agent marks duplicates. Debug builds are our own E2E traffic and never get a card; the queries filter them
(`COALESCE(build_mode, 'release') != 'debug'`, since a NULL is an old release client).

**Labels.** The source (`crash-report`, `error-report`, `in-app-feedback`), `unprocessed` on every new card, and
`needs-reply` when the person attached an address (an amendment that brings one adds it). The triage agent
(`scripts/report-triage/CLAUDE.md`) takes `unprocessed` off once it has researched and renamed a card.

**Titles are technical** (`Error report ERR-XXXXX: hand-written on 0.52.0 (macOS 27.0)`,
`Crash CRASH-XXXXX in cmdr::x (SIGABRT) on …`, `Feedback #14 on …`) until the triage agent writes a better one. ❌ Never
put a note or a feedback message in a title: GitHub keeps every earlier title in the issue's timeline, so it could never
be deleted.

**The two invariants.**

1. **The repo's privacy is re-checked before every write.** `isRepoPrivate` asks GitHub and `fileIssue` refuses unless
   the answer is an explicit `private: true`. It fails CLOSED: a network error, a non-200, or a missing field all mean
   "don't write". That is what makes a wrong `GITHUB_ISSUES_REPO` a disabled integration rather than a disclosure.
   Verified against the live public `vdavid/cmdr` during development: the probe refused and nothing was posted.
2. **Personal data lives only in a comment that expires.** The issue BODY carries technical facts that name nobody and
   is kept as long as the bug is. Anything a person wrote or attached goes in one comment prefixed
   `<!-- cmdr:personal-data expires=YYYY-MM-DD -->`, and `handlePersonalCommentSweep` deletes whatever is due.
   **Deleting the comment rather than editing the body is the whole point**: an edit leaves the original text in
   GitHub's revision history, so it would not be a deletion.

The stamp travels with the comment, so one mechanism serves every promise: an error report's note and reply-to and a
crash's reply-to carry 90 days, a feedback reply-to carries two years. ❗ Each counts from when the person SENT it
(`expiryAfter(created_at, days)`), never from when the cron filed the card, which can be days later. Feedback's MESSAGE
is the exception that proves the split, and it lives in the issue body, because the policy keeps feedback text so it can
be acted on; only the address expires. A crash's technical facts (panic message, macOS exception, top frames) also go in
the body: the client redacts them and the policy keeps them indefinitely.

**Feedback with logs is ONE card.** A feedback row's `error_report_id` names the logs the sender included, which the app
uploads as an ordinary error report right after the feedback (`src/telemetry/DETAILS.md` § In-app feedback).

- `fileErrorReportCards` holds a report back while a release feedback row naming it still owes a card, and
  `fileFeedbackCards` files the feedback with the report's technical facts (id, size, R2 key) in the body and its
  download link in the personal comment, then stamps the report's `issue_number` and KV pointer with the feedback card.
  So the report never files its own card, and an amendment to it lands on the feedback card.
- The personal comment expires with the reply-to's two years when there is one, else at the logs' 90 days.
- Logs that hadn't arrived when the feedback card was filed: the card says so, and when they do arrive they get their
  own card with a `Sent with` line pointing at the feedback card. A debug feedback row never gets a card, so it never
  holds a report back.

**Amendments land on the card they amend.** `postAmendmentsToBoard` runs after the report jobs, so a note added minutes
after its upload lands as a comment on the card filed just before it.

- The report's card exists: comment on it (`recordAmendmentOnBoard` with the outbox's `issueNumber`).
- The report is in `error_reports` but its card is still owed: the amendment waits. Filing an "amended" card now would
  make the report's own card a duplicate a tick later.
- The report predates the outbox: `recordAmendmentOnBoard` asks the `gh_issue:{ERR-XXXXX}` KV pointer, and when there is
  none, the amendment FILES the card (`filedFromAmendment`: the title says `note added later`). Its technical facts come
  from the bundle's own `customMetadata`, read with one `head` (`readReportFacts` in `error-report-amend.ts`). No
  bundle, no card: one that can't say what report it is about helps nobody.

The KV pointer is its OWN key rather than a field on the `report:{id}` index, because that entry is written before the
200 and carries the amend credential's hash: adding to it would be a read-modify-write against an eventually-consistent
store with the credential as the thing at risk. `fileErrorReportCards` still writes it, for amendments arriving via that
third path.

❗ **An amendment's comment carries the ORIGINAL report's expiry, never its own 90 days** (from the row's
`report_date`). A report amended on day 80 would otherwise keep a note on the board until day 170, outliving both its
bundle and the policy's "anything you added to the report afterwards". `recordAmendmentOnBoard` also re-runs the privacy
probe rather than trusting that the repo was private when the card was filed: months can pass between an upload and its
amendment, and the answer is allowed to have changed.

**Untrusted text is always fenced** (`fencedBlock`, with a fence longer than any backtick run inside it), so a note
cannot become a heading, an `@mention` that would notify a stranger, or a cross-repo reference. Notes are truncated well
under GitHub's 65,536-character ceiling; the full text is in the bundle manifest or the D1 row either way.

The personal-comment sweep THROWS on failure, so `runCronJob` raises the alarm. A sweep that quietly stopped would leave
personal data past its promise, which is exactly the failure that has to be loud.

## Card ratings

The morning triage agent (`scripts/report-triage/CLAUDE.md`) rates each report card it triages, but it holds no board
token: the only token that can write a user-owned board is a classic `project` PAT, which reaches every board the
account can (private issue bodies included), and the agent reads stranger-written text. So it ends its one triage
comment with a line, and `applyCardRatings` (`report-ratings.ts`) copies that onto the board with
`GITHUB_PROJECT_TOKEN`. The board fields stay the only home of the values; there are no severity or size labels.

- **The grammar** (the module comment is canonical): the comment's last non-blank line, and its only rating-like one, is
  exactly `Rating: Severity=<Critical|High|Medium|Low|none> Impact=<High|Medium|Low> Size=<XS|S|M|L|XL>`. Fixed order,
  single spaces, case-sensitive. `Severity=none` clears the field. Anything else is rejected, never guessed at; a
  mangled attempt (`**Rating**:`, `rating:`) counts as rejected, not as absent.
- **Which comments count**: authored by `GITHUB_TRIAGE_AUTHOR` (a var, `vdavid`) AND starting with `board.sh`'s
  `<!-- cmdr:triage -->` marker. The agent's fine-grained token is David's, so it posts as him, and so does the Worker's
  issues token, whose personal-data comments carry a stranger's note: the marker is what keeps a `Rating:` line typed
  into a note from counting. David's own comments count only with the marker. Strangers can't comment on the private
  repo. Triage comments created before `RATING_LINES_SINCE` (2026-10-15) predate the line and are skipped.
- **Finding the card**: issue comments edited in the last 14 days (`sort=updated&since=`), then, only when something is
  due, the board's layout over GraphQL (field and option ids by NAME, so a renamed option fails loudly) and its
  `repo:vdavid/cmdr-reports` items over REST, matched on issue number and repository URL. REST because GraphQL shows a
  private issue's item as `content: null` to a `project`-only token. All three fields go in one mutation, ids as
  variables.
- **Idempotent**: `card_ratings` (migration `0023`) keeps the line last applied per comment id. A rerun is a no-op that
  makes no board calls at all; an edit that changes the line re-applies, any other edit doesn't (so a field David
  changed by hand stays changed), and a new rated comment applies on its own.
- **Loud**: a comment with no line, a rejected line, an option the board lacks, or a card not on the board yet becomes a
  `board-rating-failed` ops event, at most once a UTC day per comment (or sooner when the reason changes), and retries
  every tick until 14 days after the comment's last edit. A refused mutation or a failed listing throws, so `runCronJob`
  raises `cron-failure` and healthchecks.io.
- **Bounded**: five comment pages and 25 ratings per tick, 20 pages of board items; the rest wait a tick.
- **Cadence**: every three-hour tick, after the amendments and before the digest (so a failure from this tick is in it).
  The agent runs at 05:30 Stockholm, so a rating lands on the next tick, 06:00 UTC: about 1.5–2.5 hours later.

## Synthetic heartbeats

`handleSyntheticHeartbeatSweep` deletes every beat AND every relayed feature event belonging to an install that has
NEVER persisted a setting and has been silent for `syntheticHeartbeatGraceDays` (seven days). It's a data-INTEGRITY
sweep, not a retention one, so it sits outside `handleRetentionSweep` and outside the privacy-policy contract (it only
ever deletes earlier than promised).

**What it corrects.** A fresh data dir mints a fresh `anal_` install id, so every instance Cmdr's own tooling launches
(E2E shards, i18n captures, marketing shots) registered as a brand-new user, on every launch. Through the beta that put
1,786 phantom installs in the table against 303 real ones: installs over-counted 6x, and roughly 24 phantom daily
actives a day on `/admin/heartbeat-dau`, which is what the analytics dashboard plots. The app-side fix is the real one
(`apps/desktop/src-tauri/src/analytics/DETAILS.md` § "Why an isolated instance must never send"); this sweep is what
makes the STORED history honest, since the Worker can't tell a robot from a person at intake, only across an install's
history.

**The classifier** (`syntheticInstallIdsSql`, shared by `deleteSyntheticEventsSql` and `deleteSyntheticHeartbeatsSql`),
decided per install rather than per row:

- Never sent a beat whose `config_json` contains `"_schemaVersion"`. The frontend settings store stamps that key on
  every save, and the heartbeat's config snapshot carries every number-valued key, so one such beat proves a person
  changed something. Independently validated: filtering the table to ids that ever carried it gives 303 installs, and
  PostHog, a wholly separate pipeline, reported 302 distinct desktop installs over the same window.
- AND its LAST beat is older than the cutoff.

One qualifying beat vouches for that id's whole history, so a real user's launch beats from before their first settings
save survive. `instr`, never `LIKE`: SQLite's `_` is a single-character wildcard, so `LIKE '%"_schemaVersion"%'` would
let an unrelated `"xschemaVersion"` key vouch for a synthetic install.

**Events go first.** The classifier reads `heartbeat`, so deleting the beats first would leave that install's events
with nothing marking them synthetic, until retention. In this order a failure between the two statements is harmless:
tomorrow's run finds the same beats.

**Why the grace period is the delicate part.** A genuinely brand-new user has no `settings.json` either, so for their
first minutes they look exactly like a test shard. Deleting on the classifier alone would erase real first-day installs.
Measured over the beta, 293 of 303 real installs stamped `_schemaVersion` within their first HOUR and exactly one took
longer than 48 hours, so a week is far past the observed spread while still converging. ❌ Don't shorten it to make the
numbers settle sooner.

The predicate is proved against a real SQLite (`node:sqlite`, no dependency) in `src/synthetic-heartbeats.test.ts`; the
statement-shape mock in `src/scheduled.test.ts` can only prove which statements ran.

## Test runtimes

The deployed Worker runs on workerd with `compatibility_date = "2025-01-01"` and NO `compatibility_flags`, so there is
no `nodejs_compat`: `Buffer`, `process`, and every `node:` module are absent in production. `vitest.config.ts` therefore
defines two projects, and a test's runtime is a choice:

- **`node`** (default, `environment: 'node'`) runs everything not listed below. Fast, and it has the whole Node standard
  library, which is exactly why it can't prove anything about production's runtime.
- **`workerd`** (`vitest.workerd.config.ts`, `@cloudflare/vitest-plugin`) runs the files in its exported `workerdTests`
  list inside workerd, under this Worker's own `wrangler.toml`. Add a file to that list to move it across;
  `vitest.config.ts` excludes it from the node project automatically.

**The `workerd` project can't catch a Node-only global either.** The pool needs Node APIs to run Vitest itself inside
workerd, so it force-enables `nodejs_compat_v2` on the test worker whatever the config says: `Buffer` and `process`
exist there (verified with a probe test on `@cloudflare/vitest-pool-workers` 0.22.0, the plugin's name before its rename
to `@cloudflare/vitest-plugin`, 2026-09-16). It buys real bindings and real Worker semantics, ❌ never runtime-parity
proof.

**Wrangler's `createTestHarness` is the lane that proves parity**, and the only one. It builds `src/index.ts` and runs
it in workerd under the real `wrangler.toml`, with test-only secrets passed in, so a Node API anywhere along an
exercised route throws the way it throws in production. Three files use it, and they cover every path that leaves the
Worker for a third party:

- `src/licensing/production-runtime.test.ts` — minting through `/admin/generate`, manual validation, revocation.
- `src/licensing/webhook-runtime.test.ts` — `/webhook/paddle` including `sendLicenseEmail`, the route the `Buffer` bug
  actually broke.
- `src/email/email-runtime.test.ts` — the other senders in `email/`: the report digest (a report and its amendment
  through `/error-report` and `/error-report/:id/amend`, plus a crash and feedback, mailed on one cron tick), the daily
  path probe, the device count alert (over `/validate`), and the DB size alert.

Bring another risky path under it by adding a request rather than by widening the pool project. ❌ Never hand the
harness its own compatibility settings.

**A cron job is reachable**: `server.getWorker().scheduled({ scheduledTime, cron })` dispatches a real scheduled event,
and `scheduledTime` decides which jobs run (the daily ones are gated on UTC hour 0). `runCronJob` swallows a job's
failure, so the tick still reports `outcome: 'ok'` — assert on what reached the wire, and carry `server.getLogs()` into
the failure message, or a job that threw looks like a job that had nothing to do.

**The DB size alert costs a 100 MB database.** Its only trigger is `meta.size_after` past the threshold, which the
harness's D1 reports truthfully, so the test writes real ballast (~3 s, in the harness's temp directory, gone at
`server.close()`) and runs last because the weight stays for the rest of the file. Production code doesn't get a
threshold seam just to make that cheaper.

**Stub an upstream at the socket, never in the source.** The harness points the Worker's global `fetch` at this process
(`outboundService: (request) => globalThis.fetch(request.url, request)`), so replacing `globalThis.fetch` in a test
intercepts every outbound request the Worker makes with the route, the SDK, and the runtime untouched. That is how
`webhook-runtime.test.ts` answers for Paddle and Resend; `applyD1Migrations('TELEMETRY_DB')` on the worker handle gives
it the real schema. Two gotchas (verified on wrangler 4.107.1, 2026-09-16): the intercepted request arrives as the INIT
argument, not the input, and it's undici's internal `Request` class, so `init instanceof Request` reads false in the
test's realm and silently yields an empty body. Duck-type on `text()`. Requests the harness itself makes don't pass
through this stub, so an unstubbed host can safely fail loudly instead of reaching the internet.

**How the gap shipped**: `bytesToBase64` called `Buffer.from()`, so every license mint threw in production from the
first deploy, while a node-project test suite stayed green over it.

❌ **Don't count on the build to catch a `node:` import either.** It answers with a WARNING and deploys anyway ("The
package `node:fs` wasn't found on the file system but is built into node… Your Worker may throw errors at runtime unless
you enable the `nodejs_compat` compatibility flag"), which scrolls past in CI output like any other warning (verified on
wrangler 4.107.1 with a `node:fs` import in `src/`, 2026-09-16). The `no-restricted-imports` ban below is what actually
stops one.

**The lint ban is the cheap half of that defense** (`eslint.config.js`, `no-restricted-globals` +
`no-restricted-imports` over `src/**/*.ts`). It refuses `Buffer`, `process`, `global`, `__dirname`, `__filename`,
`require`, `module`, `exports`, `setImmediate`, `clearImmediate`, every `node:*` specifier, and the same modules spelled
bare, each with a message naming the missing `nodejs_compat`. It covers TESTS as well as production code, because both
test projects hand a test the Node standard library and a test written against it proves nothing about production. A
file that drives the Worker FROM Node (`production-runtime.test.ts` reading migrations, `synthetic-heartbeats.test.ts`
opening `node:sqlite`) opts out per line with `eslint-disable-next-line no-restricted-imports -- <reason>`; there is no
allowlist to keep in sync, so a new workerd test is banned by default. `scripts/` and `test/` are Node programs that
never reach the Worker, so they stay out of scope. What lint structurally can't see is a DEPENDENCY reaching for a Node
global on a path we exercise, which is what the `createTestHarness` lanes above are for.

❌ **Don't add a third layer that scans the built bundle**, and don't re-derive the question from scratch. It sounds
like the way to reach dependency code no test exercises; measured against the real bundle
(`wrangler deploy --dry-run --outdir`, 429 KB, 2026-09-16) it reports 14 Node references and not one of them is a bug:

- `Buffer` × 3 — one live call, `resend`'s `forwardWrapped` (forwarding a raw `.eml`), which nothing here calls; the
  other two are a string literal and a `constructor.name` comparison inside svix's webhook verifier.
- `process` × 10 — every one inside `resend`'s `typeof process !== 'undefined' && process.env` config lookups, which is
  the standard way a library reads an optional env and is correct in workerd.
- `node:*` imports, `__dirname`, `__filename`, `setImmediate`, `require` — zero. The single `global` is in a comment.

To report nothing today it would have to parse JS rather than match text (to skip strings, comments, and property
names), understand `typeof` guards, and do reachability analysis from the entry point, because "references a Node
global" is not the claim worth failing on. Short of that it needs an allowlist that re-churns on every `resend` bump,
and an allowlist of unreachable symbols is exactly the contract that rots into a rubber stamp. The runtime lanes answer
the same question by executing the paths instead of approximating them.

```sh
pnpm dev          # starts wrangler dev server on :8787
pnpm test         # vitest unit tests, node + workerd projects
```

**Run wrangler from anywhere in the repo.** `wrangler` is a local devDependency, not global. From inside
`apps/api-server/` use `npx wrangler …`; from the repo root (no `cd` needed) use the pnpm filter form:

```sh
pnpm --filter @cmdr/api-server exec wrangler secret put HEALTHCHECKS_PING_URL
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

Runtime: `hono`, `@noble/ed25519`, `resend`, `aws4fetch`. Dev: `wrangler`, `vitest`, `typescript`, `eslint`.

See also: `apps/desktop/src/lib/licensing/CLAUDE.md` (the frontend licensing feature).
