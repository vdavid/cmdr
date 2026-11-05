# API server

Cloudflare Worker (Hono): licensing (Paddle, Ed25519 keys, KV activation codes), telemetry (D1), website endpoints,
admin aggregations, cron. Lives at `api.getcmdr.com`; `license.getcmdr.com` is a permanent alias for shipped app
versions.

## Module map

Five areas own their code, tests, and `C+D.md`; read an area's docs before working in it.

- `src/licensing/` — the Paddle webhook, `/activate`, `/validate`, the `/admin/` license routes, the daily backup.
- `src/telemetry/` — crash reports, heartbeats, downloads, update checks, error reports, feedback.
- `src/website/` — `/beta-signup`, `/newsletter-signup`, `/subprocessor-updates-signup`, `/likes/:slug`, `?r=` codes.
- `src/admin/` — the dashboard's read-only aggregations, including `/admin/funnel`.
- `src/s3-prices/` — `/s3-prices/v1`, the app's S3 price table.

Root holds the assembly and the shared leaves (`index.ts`, `types.ts`, `admin-auth.ts`, `email/`, `ops-events.ts`, the
`report-*.ts` + `github-issues.ts` + `morning-brief.ts` board, `project-board.ts`, `scheduled.ts`, `cron-health.ts`,
`ci-*.ts`, `user-agent.ts`; DETAILS § Root files). ❌ Areas depend on root leaves, never each other.

`email/` is one module per audience: `send.ts` (the SMTP2Go door), `layout.ts` (HTML chrome), then `crash.ts`,
`feedback.ts`, `error-report.ts`, `ops-alerts.ts`, `license.ts`, `sale.ts`. Import the specific one; no barrel.

## Must-knows

- **Secrets go in via `wrangler secret put`, ❌ never `wrangler.toml`.** A required one also joins the deploy workflow's
  `required` list, put BEFORE landing. DETAILS § Deployment.
- **Email through `sendEmail` (`src/email/send.ts`), ❌ never a raw `fetch` to SMTP2Go**: it answers 200 for a failed
  send, so a raw call reads failure as success (a buyer pays, gets no license).
- **Hash every stored IP through `types.ts::hashCallerIp` with `IP_HASH_PEPPER`.** The salts are public (a UTC day, a
  post slug), so the pepper alone makes the hash one-way and the policy's "we don't store your IP address" true. ❌
  Never a second scheme, ❌ never an IP-derived value no query reads.
- **What we store is a promise**: `apps/website/src/pages/privacy-policy.astro` lists every column and retention. Change
  either side and the other follows, same commit. DETAILS § Data retention.
- **Every report becomes a card in a PRIVATE repo**, filed by the cron from a D1 outbox (`report-outbox.ts`), never at
  intake. Privacy is re-checked per write, failing closed. ❌ No note, reply-to, or bundle link in an issue title or
  body: personal data goes only in the `expires=`-stamped comment. ❌ Never widen `GITHUB_PROJECT_TOKEN` past `project`.
  DETAILS §§ The reports repo, The backlog board, Card ratings.
- **Rate limits are per data center** (`enforceIpRateLimit`): they bound one abusive client, never a distributed flood.
  `/error-report` adds a global ceiling.
- **A heartbeat install id is not proof of a person.** A fresh data dir mints a fresh `anal_` id, which left the table
  6x over-counted. `handleSyntheticHeartbeatSweep` deletes ids that never persisted a setting and have been quiet a
  week; ❌ never shorten that grace period, a brand-new real user looks identical for their first minutes. DETAILS §
  Synthetic heartbeats.
- **A cron job's failure has to leave the Worker.** `runCronJob` (`index.ts`) writes an ops event for the digest, and
  the tick reports to healthchecks.io. ❌ Never let `console.error` be a job's only failure path: that's how a dead
  credential ran silently for weeks. DETAILS § Cron alarms.
- **D1 rejects SQL that SQLite accepts, and mocked tests can't see it**: a dialect rejection (`SQLITE_AUTH` on
  `pragma_*` functions) passes CI and throws daily in production. Run a new query shape through
  `wrangler d1 execute --remote` once. DETAILS § Cron jobs, job 4.
- **Deploy rails**: apply D1 migrations first (`wrangler d1 migrations apply TELEMETRY_DB`); the default export stays
  the object form (`{ fetch, scheduled }`) or cron breaks (`app` is also named-exported for tests).

Routes, secrets, bindings, Worker types, cron, retention, and runbooks: `DETAILS.md`. Read it before any non-trivial
work or advice here.
