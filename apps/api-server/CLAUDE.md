# API server

Cloudflare Worker (Hono): licensing (Paddle, Ed25519 keys, KV activation codes), telemetry (D1), admin, cron. Lives at
`api.getcmdr.com`; `license.getcmdr.com` is a permanent alias for shipped app versions.

## Module map

- `index.ts` (Hono assembly + cron wiring), `types.ts` (`Bindings`, auth, `enforceIpRateLimit`).
- Route modules are named after their path. Non-obvious: `telemetry.ts` (crash/heartbeat/download/update-check),
  `link-codes.ts` (`?r=` codes), `scheduled.ts` (cron). Crypto and notify: `license.ts`, `paddle{,-api}.ts`, `email.ts`,
  `discord.ts`, `device-tracking.ts`.

## Must-knows

- **Sandbox and live are fully separate** (accounts, keys, price IDs, secrets, Discord targets); `PADDLE_ENVIRONMENT`
  routes. ❌ Never infer it from a transaction id: both use `txn_`.
- **Secrets are Cloudflare secrets** (`wrangler secret put`), never `wrangler.toml`. Admin auth uses
  `constantTimeEqual`, ❌ never `===`; `/admin/stats` takes `ADMIN_API_TOKEN`, `/admin/generate` the Paddle secret.
- **One purchase yields ONE set of license codes.** `/webhook/paddle` claims the transaction in D1 (`license_issuance`)
  before any side effect, stores the codes before emailing, marks `emailed_at` after, and never expires the row. A
  redelivery re-sends the stored codes; a claim still in flight gets a 503 so Paddle retries. DETAILS § Fulfillment.
- **Resend reports a failed send in its RESPONSE, it doesn't throw.** Send through `sendViaResend` (`email.ts`), ❌
  never `resend.emails.send` directly, or a lost license email reads as a fulfilled purchase.
- **The `anal_` analytics id and the `diag_` diagnostics id must never co-occur on a request** (400 an `anal_`-shaped
  `diagId`), and `/beta-signup` + `/feedback` carry neither: that's what keeps analytics unjoinable to an identity.
- **`/beta-signup` stays double-opt-in**: ❌ no `preconfirm_subscriptions`, and the 409 path MUST call
  `POST /api/subscribers/{id}/optin`. Every outcome returns an identical empty 204, blocking enumeration.
- **Optional Rust-client fields arrive as `null` OR `undefined`** (serde `Option::None` → JSON `null`); a
  `!== undefined`-only validator silently drops reports.
- **`top_function` is the only crash-grouping key and must skip the panic machinery** (`extractTopFunction`), else every
  panic groups under `install_panic_hook`. `panic_message` is client-redacted; truncate at 2,000 chars rather than
  400ing.
- **Only `/feedback`'s D1 write is AWAITED** (soft 502, so the app retries); crash-report, heartbeat, download, and
  update-check are fire-and-forget `waitUntil`. Don't flip either.
- **Eviction spares bundles under `EVICTION_MIN_AGE_DAYS` (60) and is all-or-nothing** (pauses intake, resumes at the
  LOW watermark). Drop either and unauthenticated `/error-report` becomes a delete primitive.
- **`/error-report` bodies go through `readCappedBody`, ❌ never `c.req.parseBody()`**: `content-length` is advisory, so
  the parser buffers up to 100 MB in a 128 MB isolate.
- **`/download` skips the D1 write for bot User-Agents** but still 302s. Keep Homebrew exempt (it uses curl) and
  `?src=website` on the site button. DETAILS § Download tracking.
- **Deploy rails**: apply D1 migrations first (`wrangler d1 migrations apply cmdr-telemetry`); the heartbeat
  `config_json` blob absorbs new settings without one. The default export must stay the object form
  (`{ fetch, scheduled }`) or cron breaks; `app` is also a named export for tests.
- **Attribution charset is a cross-repo contract**: `sanitizeRef` keeps `[a-z0-9._:-]`, `sanitizeUtmValue` drops the
  colon, and the website and blog sanitizers MUST match. `docs/architecture.md` § Acquisition analytics.
- **In `/admin/funnel` output, `null` = unknown and `0` = a real zero.** DETAILS § funnel.

Routes, data flows, runbooks, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
