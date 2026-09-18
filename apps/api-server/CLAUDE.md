# API server

Cloudflare Worker (Hono): licensing (Paddle, Ed25519 keys, KV activation codes), telemetry (D1), website endpoints,
admin aggregations, cron. Lives at `api.getcmdr.com`; `license.getcmdr.com` is a permanent alias for shipped app
versions.

## Module map

Four areas own their code, tests, and `C+D.md`; read an area's docs before working in it.

- `src/licensing/` — the Paddle webhook, `/activate`, `/validate`, the `/admin/` license routes, the daily backup.
- `src/telemetry/` — crash reports, heartbeats, downloads, update checks, error reports, feedback.
- `src/website/` — `/beta-signup`, `/likes/:slug`, the `?r=` link codes.
- `src/admin/` — the dashboard's read-only aggregations, including `/admin/funnel`.

Root holds the assembly and the shared leaves (`index.ts`, `types.ts`, `email/`, `discord.ts`, `github-issues.ts`,
`project-board.ts` + `webhook-github.ts`, `scheduled.ts`, `cron-health.ts`, `user-agent.ts`; DETAILS § Root files). ❌
Areas depend on root leaves, never each other.

`email/` is one module per audience: `send.ts` (the Resend door), `layout.ts` (HTML chrome), then `crash.ts`,
`feedback.ts`, `error-report.ts`, `ops-alerts.ts`, `license.ts`. Import the specific one; no barrel.

## Must-knows

- **Secrets go in via `wrangler secret put`, ❌ never `wrangler.toml`.**
- **Email through `sendViaResend` (`src/email/send.ts`), ❌ never `resend.emails.send`**: Resend reports a failed send
  in its RESPONSE rather than throwing, so a raw call reads every failure as success (for a license mail, that means the
  buyer pays and gets nothing).
- **Hash every stored IP through `types.ts::hashCallerIp` with `IP_HASH_PEPPER`.** The salts are public (a UTC day, a
  post slug), so the pepper alone makes the hash one-way and the policy's "we don't store your IP address" true. ❌
  Never a second scheme, ❌ never an IP-derived value no query reads.
- **What we store is a promise**: `apps/website/src/pages/privacy-policy.astro` lists every column and retention. Change
  either side and the other follows, same commit. DETAILS § Data retention.
- **Reports and feedback also become issues in a PRIVATE repo** (`github-issues.ts`), whose privacy is re-checked before
  every write, failing closed. ❌ Never put a note, reply-to, or bundle link in an issue title or body; personal data
  goes only in the `expires=`-stamped comment. ❌ Never widen `GITHUB_PROJECT_TOKEN` past `project` scope to get that
  repo onto the backlog board either. DETAILS §§ The reports repo, The backlog board.
- **Rate limits are per data center, not global** (`enforceIpRateLimit` gates every intake route), so they bound one
  abusive client, never a distributed flood. `/error-report` carries a global ceiling on top.
- **A heartbeat install id is not proof of a person.** A fresh data dir mints a fresh `anal_` id, which left the table
  6x over-counted. `handleSyntheticHeartbeatSweep` deletes ids that never persisted a setting and have been quiet a
  week; ❌ never shorten that grace period, a brand-new real user looks identical for their first minutes. DETAILS §
  Synthetic heartbeats.
- **A cron job's failure has to leave the Worker.** Every job goes through `runCronJob` (`index.ts`), which alerts
  Discord, and the tick reports to healthchecks.io. ❌ Never let `console.error` be a job's only failure path: that's
  how a dead credential ran silently for weeks. DETAILS § Cron alarms.
- **D1 rejects SQL that SQLite accepts, and mocked tests can't see it**: a dialect rejection (`SQLITE_AUTH` on
  `pragma_*` functions) passes CI and throws daily in production. Run a new query shape through
  `wrangler d1 execute --remote` once. DETAILS § Cron jobs, job 4.
- **Deploy rails**: apply D1 migrations first (`wrangler d1 migrations apply cmdr-telemetry`); the default export stays
  the object form (`{ fetch, scheduled }`) or cron breaks (`app` is also named-exported for tests).

Routes, secrets, bindings, Worker types (`wrangler types`, gitignored), cron, retention, the reports repo, test
runtimes, and the runbooks: `DETAILS.md`. Read it before any non-trivial work here.
