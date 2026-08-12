# API server

Cloudflare Worker (Hono): licensing (Paddle, Ed25519 keys, KV activation codes), telemetry (D1), admin, cron. Lives at
`api.getcmdr.com`; `license.getcmdr.com` is a permanent alias for shipped app versions.

## Module map

`index.ts` (Hono assembly + cron wiring), `types.ts` (`Bindings`, auth, `enforceIpRateLimit`). Route modules are named
after their path; the non-obvious ones are `telemetry.ts` (crash/heartbeat/download/update-check), `link-codes.ts`
(`?r=` codes), `scheduled.ts` (cron). Crypto and notify: `license.ts`, `paddle{,-api}.ts`, `email.ts`, `discord.ts`.
Per-file inventory: DETAILS § Key files.

## Must-knows

- **Sandbox and live never mix** (accounts, keys, price IDs, secrets, Discord targets): `PADDLE_ENVIRONMENT` routes. ❌
  Never infer it from a transaction id, both use `txn_`.
- **Secrets go in via `wrangler secret put`, ❌ never `wrangler.toml`**; admin auth compares with `constantTimeEqual`,
  ❌ never `===`.
- **One purchase yields ONE set of license codes.** `/webhook/paddle` claims the D1 `license_issuance` row before any
  side effect, stores the codes before emailing, and never expires the row. DETAILS § Fulfillment.
- **Email through `sendViaResend` (`email.ts`), ❌ never `resend.emails.send`**: Resend reports a failed send in its
  RESPONSE rather than throwing, so a raw call makes a lost license email read as a fulfilled purchase.
- **The `anal_` analytics id and the `diag_` diagnostics id never co-occur on a request**, and `/beta-signup` +
  `/feedback` carry neither: that's what keeps analytics unjoinable to an identity.
- **`/beta-signup` stays double-opt-in**: ❌ no `preconfirm_subscriptions`, and the 409 path MUST call
  `POST /api/subscribers/{id}/optin`. Every outcome returns an identical empty 204, blocking enumeration.
- **Optional Rust-client fields arrive as `null` OR `undefined`** (serde `Option::None` → JSON `null`): a
  `!== undefined`-only validator silently drops reports.
- **`top_function` is the only crash-grouping key and must skip the panic machinery** (`extractTopFunction`), else every
  panic groups under `install_panic_hook`.
- **Only `/feedback` AWAITS its D1 write** (soft 502, so the app retries); crash-report, heartbeat, download, and
  update-check are fire-and-forget `waitUntil`. ❌ Don't flip either.
- **Eviction spares bundles under `EVICTION_MIN_AGE_DAYS` (60) and is all-or-nothing**, else unauthenticated
  `/error-report` becomes a delete primitive.
- **`/error-report` reads bodies via `readCappedBody`, ❌ never `c.req.parseBody()`**: `content-length` is advisory, so
  the parser buffers up to 100 MB in a 128 MB isolate.
- **Hash every stored IP through `types.ts::hashCallerIp` with the `IP_HASH_PEPPER` secret.** The salts are public (a
  UTC day, a post slug), so the pepper alone makes the hash one-way (IPv4 brute-forces in seconds). ❌ Never a second
  scheme, ❌ never an IP-derived value no query reads.
- **What we store is a promise**: `apps/website/src/pages/privacy-policy.astro` lists every table's columns and
  retention here. Change either side and the other has to follow. DETAILS § Data retention.
- **Unauthenticated writes bound their own namespace**: `/likes/:slug` validates the slug before touching KV, and every
  intake route gates on `enforceIpRateLimit`.
- **Deploy rails**: D1 migrations first (`wrangler d1 migrations apply cmdr-telemetry`); the default export stays the
  object form (`{ fetch, scheduled }`) or cron breaks (`app` is also named-exported for tests).
- **Attribution charset is a cross-repo contract**: `sanitizeRef` keeps `[a-z0-9._:-]`, `sanitizeUtmValue` drops the
  colon, and the website and blog sanitizers MUST match. `docs/architecture.md` § Acquisition analytics.

Routes (including `/download`'s bot filter and `/admin/funnel`'s `null` = unknown), data flows, runbooks, and decisions:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
