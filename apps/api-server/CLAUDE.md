# API server

Cloudflare Worker (Hono) backend for Cmdr: licensing (Paddle webhooks, Ed25519 keys, activation codes in KV),
telemetry (crash reports, downloads, update checks, heartbeats in D1), admin endpoints, and cron notifications.
Deployed at `api.getcmdr.com` (`license.getcmdr.com` is a permanent alias for existing app versions).

## Module map

- `src/index.ts`: Hono app assembly + scheduled handler wiring. `src/types.ts`: shared `Bindings`, constants, auth.
- Route modules: `licensing.ts`, `admin.ts`, `telemetry.ts` (crash/heartbeat/download/update-check), `likes.ts`,
  `error-report.ts`, `beta-signup.ts`, `feedback.ts`. Cron in `scheduled.ts`; eviction in `error-report-eviction.ts`.
- Crypto/Paddle: `license.ts` (short code + key gen), `paddle.ts` (HMAC verify), `paddle-api.ts`, `email.ts`,
  `discord.ts`, `device-tracking.ts`. Tests sit beside their module.

The full route table, env-var/binding tables, data flows, runbooks, and decisions are in DETAILS.md.

## Must-knows

- **Sandbox and live are completely separated** (different Paddle accounts, keys, price IDs, webhook secrets, Discord
  targets). `PADDLE_ENVIRONMENT` picks the routing; never infer it from transaction IDs (both use the `txn_` prefix).
- **Secrets are Cloudflare secrets** (`wrangler secret put`), never in `wrangler.toml`. `/admin/stats` uses a dedicated
  `ADMIN_API_TOKEN`, separate from the Paddle webhook secret used by `/admin/generate`. Admin auth compares with
  `constantTimeEqual` (timing-safe); don't swap in `===`.
- **The `anal_` analytics id and the `diag_` diagnostics id must never co-occur on one request.** Heartbeats carry
  `anal_`; crash/error reports carry `diag_`; `/beta-signup` and `/feedback` carry NO install id of any kind. This keeps
  the analytics stream unjoinable to any identity. Guarded by `crash-report.test.ts` and `beta-signup.test.ts`. Don't
  add an install id to beta-signup or feedback, and reject any `anal_`-shaped `diagId` (400).
- **`/beta-signup` must stay double-opt-in:** never send `preconfirm_subscriptions` to Listmonk, and the 409
  add-to-list path MUST call `POST /api/subscribers/{id}/optin` (the list-add endpoint does not send the confirmation
  mail on its own). Every outcome returns an identical empty 204, so the response can't be used for email enumeration.
- **Validators for optional fields from the Rust client must tolerate both `null` and `undefined`** (serde
  `Option::None` serializes as JSON `null`, and `skip_serializing_if` is banned on the IPC surface). Pattern:
  `value !== undefined && value !== null && <shape check>`. A `!== undefined`-only check drops upgrade-window reports.
- **`/feedback` D1 write is AWAITED** (soft 502 on failure so the app retries); the other telemetry writes
  (`crash-report`, `heartbeat`, `download`, `update-check`) are fire-and-forget via `waitUntil`. Don't flip either.
- **Apply D1 migrations before deploying** schema changes: `wrangler d1 migrations apply cmdr-telemetry`. The
  `heartbeat` `config_json` column is one verbatim JSON blob on purpose (new settings absorb without a migration); don't
  split it into per-field columns.
- **The default export uses the object form** (`{ fetch, scheduled }`); cron support breaks without it. `app` is also a
  named export so tests can `app.request()`.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
