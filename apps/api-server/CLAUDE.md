# API server

Cloudflare Worker (Hono) backend for Cmdr: licensing (Paddle webhooks, Ed25519 keys, activation codes in KV), telemetry
(crash reports, downloads, update checks, heartbeats in D1), admin endpoints, and cron notifications. Deployed at
`api.getcmdr.com` (`license.getcmdr.com` is a permanent alias for existing app versions). Routes, env/binding tables,
data flows, runbooks, and decisions: [DETAILS.md](DETAILS.md).

## Module map

- `src/index.ts`: Hono app assembly + scheduled-handler wiring. `src/types.ts`: shared `Bindings`, constants, auth.
- Route modules: `licensing.ts`, `admin.ts`, `telemetry.ts` (crash/heartbeat/download/update-check), `likes.ts`,
  `error-report.ts`, `beta-signup.ts`, `feedback.ts`, `funnel.ts`, `link-codes.ts` (`?r=` codes). Cron in
  `scheduled.ts`; eviction in `error-report-eviction.ts`.
- Crypto/Paddle: `license.ts`, `paddle.ts` (HMAC verify), `paddle-api.ts`, `email.ts`, `discord.ts`,
  `device-tracking.ts`. Tests sit beside their module.

## Must-knows

- **Sandbox and live are fully separated** (different Paddle accounts, keys, price IDs, webhook secrets, Discord
  targets). `PADDLE_ENVIRONMENT` routes; never infer it from transaction IDs (both use the `txn_` prefix).
- **Secrets are Cloudflare secrets** (`wrangler secret put`), never in `wrangler.toml`. `/admin/stats` uses a dedicated
  `ADMIN_API_TOKEN`, separate from the Paddle webhook secret used by `/admin/generate`. Admin auth compares with
  `constantTimeEqual` (timing-safe); don't swap in `===`.
- **The `anal_` analytics id and the `diag_` diagnostics id must never co-occur on one request.** Heartbeats carry
  `anal_`; crash/error reports carry `diag_`; `/beta-signup` and `/feedback` carry NO install id. This keeps the
  analytics stream unjoinable to any identity. Don't add an install id to beta-signup or feedback, and reject any
  `anal_`-shaped `diagId` (400). Guarded by `crash-report.test.ts` and `beta-signup.test.ts`.
- **`/beta-signup` must stay double-opt-in:** never send `preconfirm_subscriptions` to Listmonk, and the 409 add-to-list
  path MUST call `POST /api/subscribers/{id}/optin` (the list-add endpoint doesn't send the confirmation mail itself).
  Every outcome returns an identical empty 204, blocking email enumeration.
- **Validators for optional fields from the Rust client must tolerate both `null` and `undefined`** (serde
  `Option::None` serializes as JSON `null`, and `skip_serializing_if` is banned on the IPC surface). Use
  `value !== undefined && value !== null && <shape check>`; a `!== undefined`-only check drops upgrade-window reports.
- **`/feedback` D1 write is AWAITED** (soft 502 on failure so the app retries); the other telemetry writes
  (`crash-report`, `heartbeat`, `download`, `update-check`) are fire-and-forget via `waitUntil`. Don't flip either.
- **`/download` is conditional and tagged:** it skips the D1 write for bot/unfurler User-Agents (still serves the 302),
  stores a daily-hashed IP for same-day dedup, and tags `source` (homebrew/website/other). Keep Homebrew exempt from the
  bot filter (it downloads via curl) and keep the `?src=website` param on the website button, or installs misclassify.
  DETAILS § Download tracking.
- **Apply D1 migrations before deploying** schema changes: `wrangler d1 migrations apply cmdr-telemetry`. The
  `heartbeat` `config_json` column is one verbatim JSON blob on purpose (new settings absorb without a migration); don't
  split it into per-field columns.
- **The default export uses the object form** (`{ fetch, scheduled }`); cron breaks without it. `app` is also a named
  export so tests can `app.request()`.
- **Charset is the cross-repo attribution contract** (`docs/architecture.md` § Acquisition analytics): `sanitizeRef`
  (download `ref`, `telemetry.ts`) keeps `[a-z0-9._:-]` (colon included); `sanitizeUtmValue` (link codes,
  `link-codes.ts`) keeps `[a-z0-9._-]` (no colon). Both lowercase and cap length (ref 120, code key 1..64). The website
  and blog client-side sanitizers MUST normalize identically.
- **`/admin/funnel` returns `FunnelDay[]`, one per UTC day; in every column `null` = unknown, `0` = a real zero.**
  `downloadsByRef` buckets a NULL ref under `(none)`. D7 retention: for a cohort whose first heartbeat was on day X, an
  install is retained if it has any heartbeat in `[X+7d, X+8d)`; cohorts younger than 8 days report `null`, not 0.
  (DETAILS § funnel.)

Read [DETAILS.md](DETAILS.md) before any non-trivial work here: editing, planning, reorganizing, or advising.
