# API server

Cloudflare Worker (Hono): licensing (Paddle, Ed25519 keys, KV activation codes), telemetry (D1), website endpoints,
admin aggregations, cron. Lives at `api.getcmdr.com`; `license.getcmdr.com` is a permanent alias for shipped app
versions.

## Module map

Four areas own their own code, tests, and `CLAUDE.md` + `DETAILS.md`; read an area's docs before working in it.

- `src/licensing/` — the Paddle webhook, `/activate`, `/validate`, `/admin/generate`.
- `src/telemetry/` — crash reports, heartbeats, downloads, update checks, error reports, feedback.
- `src/website/` — `/beta-signup`, `/likes/:slug`, the `?r=` link codes.
- `src/admin/` — the dashboard's read-only aggregations, including `/admin/funnel`.

Root holds only the assembly and the shared leaves: `index.ts` (Hono mounting + cron wiring), `types.ts` (`Bindings`,
`verifyAdminAuth`, `enforceIpRateLimit`, `hashCallerIp`), `email.ts`, `discord.ts`, `scheduled.ts` (cron),
`user-agent.ts`. ❌ Areas depend on root leaves, never on each other.

## Must-knows

- **Secrets go in via `wrangler secret put`, ❌ never `wrangler.toml`.**
- **Email through `sendViaResend` (`email.ts`), ❌ never `resend.emails.send`**: Resend reports a failed send in its
  RESPONSE rather than throwing, so a raw call reads every failure as success (for a license mail, that means the buyer
  pays and gets nothing).
- **Hash every stored IP through `types.ts::hashCallerIp` with the `IP_HASH_PEPPER` secret.** The salts are public (a
  UTC day for telemetry, a post slug for likes), so the pepper alone makes the hash one-way (IPv4 brute-forces in
  seconds) and the privacy policy's "we don't store your IP address" true. ❌ Never a second scheme, ❌ never an
  IP-derived value no query reads.
- **What we store is a promise**: `apps/website/src/pages/privacy-policy.astro` lists every table's columns and
  retention. Change either side and the other has to follow, in the same commit. DETAILS § Data retention.
- **Rate limits are per data center, not global** (`enforceIpRateLimit` is the one gate every intake route calls), so
  they bound a single abusive client, never a distributed flood. `/error-report` carries a global ceiling on top.
- **Deploy rails**: apply D1 migrations first (`wrangler d1 migrations apply cmdr-telemetry`); the default export must
  stay the object form (`{ fetch, scheduled }`) or cron breaks (`app` is also named-exported for tests).

Routes, configuration and secrets, bindings, cron, retention, and the deploy and local-dev runbooks: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
