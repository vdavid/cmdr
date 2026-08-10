# API server

Cloudflare Worker (Hono) backend for Cmdr: licensing (Paddle webhooks, Ed25519 keys, activation codes in KV), telemetry
(crash reports, downloads, update checks, heartbeats in D1), admin endpoints, and cron notifications. Deployed at
`api.getcmdr.com`; `license.getcmdr.com` is a permanent alias for existing app versions. Routes, data flows, runbooks,
and decisions: `DETAILS.md`.

## Module map

- `src/index.ts`: Hono app assembly + scheduled-handler wiring. `src/types.ts`: shared `Bindings`, constants, auth,
  `enforceIpRateLimit`.
- Route modules: `licensing.ts`, `admin.ts`, `telemetry.ts` (crash/heartbeat/download/update-check), `likes.ts`,
  `error-report.ts`, `beta-signup.ts`, `feedback.ts`, `funnel.ts`, `link-codes.ts` (`?r=` codes). Cron in
  `scheduled.ts`; eviction in `error-report-eviction.ts`; error-report admission in `error-report-intake.ts`.
- Crypto/Paddle: `license.ts`, `paddle.ts` (HMAC verify), `paddle-api.ts`, `email.ts`, `discord.ts`,
  `device-tracking.ts`.

## Must-knows

- **Sandbox and live are fully separate** (different Paddle accounts, keys, price IDs, webhook secrets, Discord
  targets). `PADDLE_ENVIRONMENT` routes; never infer it from transaction IDs (both use `txn_`).
- **Secrets are Cloudflare secrets** (`wrangler secret put`), never in `wrangler.toml`. `/admin/stats` uses a dedicated
  `ADMIN_API_TOKEN`, separate from the Paddle webhook secret used by `/admin/generate`. Admin auth uses
  `constantTimeEqual`; never `===`.
- **The `anal_` analytics id and the `diag_` diagnostics id must never co-occur on one request.** Heartbeats carry
  `anal_`; crash/error reports carry `diag_`; `/beta-signup` and `/feedback` carry NO install id, keeping the analytics
  stream unjoinable to any identity. Reject any `anal_`-shaped `diagId` (400). Guarded by `crash-report.test.ts` and
  `beta-signup.test.ts`.
- **`/beta-signup` must stay double-opt-in:** never send `preconfirm_subscriptions` to Listmonk, and the 409 add-to-list
  path MUST call `POST /api/subscribers/{id}/optin` (list-add sends no confirmation mail itself). Every outcome returns
  an identical empty 204, blocking enumeration.
- **Validators for optional Rust-client fields must tolerate both `null` and `undefined`** (serde `Option::None`
  serializes as JSON `null`). Use `value !== undefined && value !== null && <shape check>`; a `!== undefined`-only check
  drops upgrade-window reports.
- **`top_function` is the only crash-grouping key and must skip the panic machinery.** `extractTopFunction`
  (`telemetry.ts`) drops `crash_reporter` / `std::panicking` / unwrap-helper frames before taking the first `cmdr::`
  frame; without it every panic groups under `install_panic_hook::{{closure}}`. DETAILS § top_function.
- **`panic_message` arrives already redacted and capped by the client.** The server caps again at 2,000 chars,
  truncating rather than 400ing (losing a whole report over a fat message is worse). No un-redacted free text here.
- **`/feedback` D1 write is AWAITED** (soft 502 so the app retries); the other telemetry writes (`crash-report`,
  `heartbeat`, `download`, `update-check`) are fire-and-forget via `waitUntil`. Don't flip either.
- **`/download` is conditional and tagged:** it skips the D1 write for bot/unfurler User-Agents (still serves the 302),
  stores a daily-hashed IP for same-day dedup, and tags `source`. Keep Homebrew exempt from the bot filter (it uses
  curl) and `?src=website` on the website button, or installs misclassify. `:version` also takes `latest`, resolved to a
  real version before the redirect and the write, so no row reads `latest`. DETAILS § Download tracking.
- **Eviction never touches bundles younger than `EVICTION_MIN_AGE_DAYS` (60) and is all-or-nothing**: it pauses intake
  instead of half-deleting, and resumes only at the LOW watermark. Drop either property and unauthenticated
  `/error-report` becomes a delete primitive aimed at real reports. DETAILS § age floor.
- **`/error-report` bodies go through `readCappedBody`, never `c.req.parseBody()`**: `content-length` is advisory, so
  the parser would otherwise buffer up to 100 MB in a 128 MB isolate.
- **Apply D1 migrations before deploying** schema changes: `wrangler d1 migrations apply cmdr-telemetry`. The
  `heartbeat` `config_json` column is one verbatim JSON blob on purpose (new settings absorb without a migration).
- **The default export uses the object form** (`{ fetch, scheduled }`); cron breaks without it. `app` is also exported
  by name for `app.request()` in tests.
- **Charset is the cross-repo attribution contract** (`docs/architecture.md` § Acquisition analytics): `sanitizeRef`
  (download `ref`, `telemetry.ts`) keeps `[a-z0-9._:-]` (colon included); `sanitizeUtmValue` (`link-codes.ts`) keeps
  `[a-z0-9._-]` (no colon). Both lowercase and cap length (ref 120, code key 1..64); the website and blog sanitizers
  MUST normalize identically.
- **`/admin/funnel` returns `FunnelDay[]`, one per UTC day; in every column `null` = unknown, `0` = a real zero.**
  `downloadsByRef` buckets a NULL ref under `(none)`; cohorts younger than 8 days report `null` D7 retention, not 0.
  (DETAILS § funnel.)

Read `DETAILS.md` before any non-trivial work here: editing, planning, reorganizing, or advising.
