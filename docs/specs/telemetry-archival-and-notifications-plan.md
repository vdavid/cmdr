# Telemetry persistence and crash notifications

## Intention

Cloudflare Analytics Engine retains data for only 90 days, which means we lose historical trends for downloads, active
users, and crash data. The crash reporter has no notification mechanism — reports land silently. And the worker is
misleadingly named "license server" despite handling telemetry, downloads, and crash reports.

This plan addresses all three:
1. **D1 as primary store** — replace AE with D1 for crash reports, downloads, and update checks. Permanent retention.
2. **Crash notifications** — email alerts via Resend when new crashes arrive.
3. **Rename** — `license.getcmdr.com` → `api.getcmdr.com` to honestly reflect the worker's scope.

**Design philosophy:** D1 becomes the single source of truth for telemetry data. AE is removed for crash reports,
downloads, and update checks — nobody was reading crash data from AE (the dashboard never integrated it), and the
dashboard can easily switch to D1 for downloads and update checks. The only remaining AE dataset is `DEVICE_COUNTS`
(fair-use monitoring), where 90-day retention is fine and the data doesn't need long-term archival.

## Milestone 1: D1 database and crash report migration

**Intention:** Create a D1 database, write crash reports directly to D1 (not AE), and remove the `CRASH_REPORTS` AE
binding. D1 is the sole store for crash data from this point on.

### Infrastructure

- Create D1 database: `wrangler d1 create cmdr-telemetry`
- Add D1 binding to `wrangler.toml`:
  ```toml
  [[d1_databases]]
  binding = "TELEMETRY_DB"
  database_name = "cmdr-telemetry"
  database_id = "<from wrangler output>"
  ```
- Remove the `CRASH_REPORTS` AE binding from `wrangler.toml`
- Create initial migration (`apps/license-server/migrations/0001_crash_reports.sql`):
  ```sql
  CREATE TABLE crash_reports (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
      notified_at TEXT,
      hashed_ip TEXT NOT NULL,
      app_version TEXT NOT NULL,
      os_version TEXT NOT NULL,
      arch TEXT NOT NULL,
      signal TEXT NOT NULL,
      top_function TEXT NOT NULL,
      backtrace TEXT NOT NULL
  );

  CREATE INDEX idx_crash_reports_notified ON crash_reports(notified_at);
  CREATE INDEX idx_crash_reports_created ON crash_reports(created_at);
  ```
- Apply: `wrangler d1 migrations apply cmdr-telemetry`

### Code changes

- **`src/index.ts` — Bindings type:** add `TELEMETRY_DB: D1Database`, remove `CRASH_REPORTS: AnalyticsEngineDataset`
- **`POST /crash-report` route:** replace the AE `writeDataPoint` with a D1 insert. The D1 write should be
  fire-and-forget (via `c.executionCtx.waitUntil`) so it doesn't slow down the response.
  ```typescript
  // Write to D1 (fire-and-forget)
  c.executionCtx.waitUntil(
      c.env.TELEMETRY_DB.prepare(
          `INSERT INTO crash_reports (hashed_ip, app_version, os_version, arch, signal, top_function, backtrace)
           VALUES (?, ?, ?, ?, ?, ?, ?)`
      ).bind(hashedIp, report.appVersion, report.osVersion, report.arch, report.signal, topFunction, backtraceTruncated)
       .run()
       .catch(() => {}) // Don't let D1 failure block the response
  )
  ```
  **Gotcha:** `c.executionCtx` is unavailable in Hono's test harness (`app.request()`). Wrap the `waitUntil` call in a
  try-catch or check for `executionCtx` existence, same pattern as any existing `waitUntil` usage in the codebase. Tests
  must provide a mock `executionCtx` with a `waitUntil` that collects promises for assertion.

### Tests

- Update `crash-report.test.ts`:
  - Replace mock AE `CRASH_REPORTS` with mock D1 `TELEMETRY_DB` in `createBindings` (mock `prepare().bind().run()`)
  - Add mock `executionCtx` with `waitUntil`
  - Update test: valid crash report inserts into D1 with correct values (no more AE assertion)
  - Add test: D1 failure doesn't affect the 204 response (mock `.run()` to reject)
- Update `admin-stats.test.ts`: add `TELEMETRY_DB` mock to `baseBindings`, remove `CRASH_REPORTS` AE mock
- Run `./scripts/check.sh --check vitest-license-server`

### Docs

- Update `apps/license-server/CLAUDE.md`:
  - Add D1 binding to the overview
  - Update "No database" key pattern to mention D1 for telemetry
  - Update `POST /crash-report` data flow: "writes to D1" (not AE)
  - Remove `CRASH_REPORTS` from AE bindings list

---

## Milestone 2: Downloads and update checks migration to D1

**Intention:** Move downloads and update check tracking from AE to D1. Downloads get one row per event (low volume).
Update checks use a dedup table with a UNIQUE constraint — one D1 write per request, deduplication handled by SQLite.
A cron aggregates raw update checks into a `daily_active_users` summary table.

### Infrastructure

- Remove `DOWNLOADS` and `UPDATE_CHECKS` AE bindings from `wrangler.toml`
- New migration (`apps/license-server/migrations/0002_downloads_and_update_checks.sql`):
  ```sql
  CREATE TABLE downloads (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
      app_version TEXT NOT NULL,
      arch TEXT NOT NULL,
      country TEXT NOT NULL,
      continent TEXT NOT NULL
  );

  CREATE INDEX idx_downloads_created ON downloads(created_at);

  -- Raw update checks with UNIQUE constraint for per-day deduplication.
  -- Each unique (date, hashed_ip, version, arch) combo = one row.
  -- INSERT OR IGNORE handles duplicates at zero cost.
  CREATE TABLE update_checks (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      date TEXT NOT NULL,
      hashed_ip TEXT NOT NULL,
      app_version TEXT NOT NULL,
      arch TEXT NOT NULL,
      UNIQUE(date, hashed_ip, app_version, arch)
  );

  CREATE INDEX idx_update_checks_date ON update_checks(date);

  -- Aggregated daily active users, computed from update_checks by the cron.
  CREATE TABLE daily_active_users (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      date TEXT NOT NULL,
      app_version TEXT NOT NULL,
      arch TEXT NOT NULL,
      unique_users INTEGER NOT NULL,
      UNIQUE(date, app_version, arch)
  );

  CREATE INDEX idx_dau_date ON daily_active_users(date);
  ```

### Code changes

- **`src/index.ts` — Bindings type:** remove `DOWNLOADS: AnalyticsEngineDataset` and
  `UPDATE_CHECKS: AnalyticsEngineDataset`
- **`GET /download/:version/:arch` route:** replace AE `writeDataPoint` with a D1 insert, fire-and-forget via
  `waitUntil` + `.catch(() => {})`.
- **`GET /update-check/:version` route:** replace AE `writeDataPoint` with a D1 upsert:
  ```typescript
  const today = new Date().toISOString().slice(0, 10) // YYYY-MM-DD

  c.executionCtx.waitUntil(
      c.env.TELEMETRY_DB.prepare(
          `INSERT OR IGNORE INTO update_checks (date, hashed_ip, app_version, arch) VALUES (?, ?, ?, ?)`
      ).bind(today, hashedIp, version, arch)
       .run()
       .catch(() => {})
  )
  ```
  `INSERT OR IGNORE` means duplicate (date, hashedIp, version, arch) tuples are silently dropped — deduplication for
  free via the UNIQUE constraint.

### Tests

- Update existing download and update-check tests: replace AE mocks with D1 mocks
- Update `admin-stats.test.ts`: remove `DOWNLOADS` and `UPDATE_CHECKS` AE mocks from `baseBindings`
- Add test: download route inserts into D1 `downloads` table with correct values
- Add test: update-check route inserts into D1 `update_checks` table
- Add test: duplicate update check in same day is silently ignored (INSERT OR IGNORE)
- Add test: D1 failure doesn't affect the 302 redirects
- Both download and update-check routes need the same `executionCtx` mock pattern described in milestone 1's gotcha
- Run `./scripts/check.sh --check vitest-license-server`

### Docs

- Update `apps/license-server/CLAUDE.md`:
  - Remove `DOWNLOADS` and `UPDATE_CHECKS` from AE bindings
  - Update download tracking and update check tracking sections to reference D1
  - Note remaining AE dataset: only `DEVICE_COUNTS` for fair-use monitoring

---

## Milestone 3: Cron handler (crash notifications + daily aggregation)

**Intention:** A single `scheduled` handler that runs every 12 hours. It does three jobs: (1) email crash notifications
for any un-notified crashes, (2) once per day, aggregate yesterday's raw update checks into the `daily_active_users`
table and prune old raw data, and (3) once per day, check DB size and notify if it's growing large.

### Why one handler

CF free tier allows 5 cron triggers per worker. Using one trigger with branching logic inside is simpler and leaves room
for future crons. The 12h interval covers all jobs: crash notifications run every time, daily jobs run only on the
midnight UTC invocation.

### Infrastructure

- Add cron trigger to `wrangler.toml`:
  ```toml
  [triggers]
  crons = ["0 */12 * * *"]
  ```
- Add secret:
  - `CRASH_NOTIFICATION_EMAIL` (secret) — recipient address (your email). Separate from `SUPPORT_EMAIL` so it can be
    changed independently.
- Setup:
  ```bash
  wrangler secret put CRASH_NOTIFICATION_EMAIL
  ```
  Also add to `.dev.vars` for local testing.

### Code changes

- **`src/index.ts` — Bindings type:** add `CRASH_NOTIFICATION_EMAIL?: string`
- **New export: `scheduled` handler** (Hono doesn't directly support `scheduled`, so export it alongside the app).
  **Gotcha:** The current code does `export default app`. Changing to the object form below is required for cron
  support. Use `app.fetch.bind(app)` defensively — zero cost, avoids breakage if Hono's method isn't pre-bound.

  Keep `app` as a **named export** (`export { app }`) so tests can use `app.request()`. **All test files** that
  currently do `import app from './index'` must be updated to `import { app } from './index'` — there are at least 2
  test files (`crash-report.test.ts`, `admin-stats.test.ts`) that import the default export.
  ```typescript
  export { app }

  export default {
      fetch: app.fetch.bind(app),
      async scheduled(event: ScheduledEvent, env: Bindings, ctx: ExecutionContext) {
          try {
              await handleCrashNotifications(env)
          } catch (e) {
              console.error('Crash notifications failed:', e)
          }

          // Daily jobs: only run on the 00:00 UTC invocation
          const hour = new Date(event.scheduledTime).getUTCHours()
          if (hour === 0) {
              try {
                  await handleDailyAggregation(env)
              } catch (e) {
                  console.error('Daily aggregation failed:', e)
              }

              try {
                  await handleDbSizeCheck(env)
              } catch (e) {
                  console.error('DB size check failed:', e)
              }
          }
      },
  }
  ```
  **Why try-catch per job:** If one job throws (for example, a D1 query failure), it must not prevent the others from
  running. Each job is independent and should fail independently.

#### Crash notifications (`handleCrashNotifications`)

1. Early-return if `CRASH_NOTIFICATION_EMAIL` or `RESEND_API_KEY` is not set (both are needed to send email — the cron
   runs without them, just skips notifications).
2. Query D1: `SELECT id, app_version, os_version, arch, signal, top_function, created_at FROM crash_reports WHERE notified_at IS NULL`
3. If zero rows, return early
4. Group by `top_function` to build a summary (count per crash site)
5. Collect the IDs of all un-notified rows
6. Mark as notified **before** sending: generate dynamic placeholders (`?, ?, ?...` — one per ID) and run
   `UPDATE crash_reports SET notified_at = ? WHERE id IN (?, ?, ...)` with `.bind(now, ...ids)`.
   **Why before, not after:** If the email succeeds but the UPDATE fails, the next cron sends a duplicate email. Marking
   first accepts the small risk of a missed notification (if the email fails after marking) over duplicate emails.
   Missed notifications self-heal: if a crash pattern persists, new crashes generate fresh un-notified rows.
7. Send email via Resend to `CRASH_NOTIFICATION_EMAIL`:
   - From: `Cmdr Crash Alerts <noreply@getcmdr.com>`
   - Subject: `Cmdr: {count} new crash {count === 1 ? 'report' : 'reports'}` — dynamically pluralized per style guide
   - Body: HTML table with crash site, count, versions affected, most recent timestamp. Same inline-style HTML email
     pattern as `sendDeviceCountAlert` in `email.ts`.

**Idempotency:** The `notified_at` column makes this naturally idempotent. If the cron runs twice (CF guarantees
at-least-once), the second run finds zero un-notified rows and does nothing.

**Gotcha:** D1 (SQLite) doesn't support binding an array to a single `?` placeholder. Generate N placeholders
dynamically: `` `WHERE id IN (${ids.map(() => '?').join(', ')})` `` and spread the IDs into `.bind(now, ...ids)`.
If the batch is large (unlikely for crash reports), chunk into groups of 100 to stay within SQLite's variable limit.

#### Daily aggregation (`handleDailyAggregation`)

1. Compute yesterday's date: `YYYY-MM-DD`
2. Check D1: `SELECT 1 FROM daily_active_users WHERE date = ? LIMIT 1` — skip if already aggregated (idempotent)
3. Aggregate from the raw `update_checks` table:
   ```sql
   INSERT OR IGNORE INTO daily_active_users (date, app_version, arch, unique_users)
   SELECT date, app_version, arch, COUNT(*) AS unique_users
   FROM update_checks
   WHERE date = ?
   GROUP BY date, app_version, arch
   ```
   `COUNT(*)` works because the UNIQUE constraint on `update_checks` already deduplicated per IP — each row represents
   one unique user for that day+version+arch combo.
4. Prune raw update checks older than 7 days:
   ```sql
   DELETE FROM update_checks WHERE date < date('now', '-7 days')
   ```
   Raw data is only needed until it's aggregated. 7 days gives a buffer in case the cron misses a day.

#### DB size check (`handleDbSizeCheck`)

1. Query D1: `SELECT page_count * page_size AS total_size FROM pragma_page_count, pragma_page_size`
2. If `total_size` exceeds a threshold (for example, 100 MB — D1 free tier max is 5 GB, so this gives plenty of warning), send
   an email via Resend:
   - Subject: `Cmdr: telemetry DB is {sizeMB} MB`
   - Body: table row counts for each table (`SELECT COUNT(*) FROM crash_reports`, etc.)
3. To avoid daily spam: store last alert date in a small `cron_state` table (or just accept one email per day as
   tolerable — the threshold should rarely be hit).

### New file: `src/email.ts` additions

Add `sendCrashNotificationEmail` and `sendDbSizeAlert` functions alongside the existing email functions. Same pattern as
`sendDeviceCountAlert`: accepts params, constructs inline-styled HTML, sends via Resend.

### Tests

- Unit test `handleCrashNotifications`: mock D1 returning un-notified rows, verify Resend is called with correct
  subject/body, verify rows are marked as notified
- Unit test `handleCrashNotifications` with zero rows: verify no email is sent
- Unit test `handleDailyAggregation`: mock D1 with raw update_checks rows, verify aggregation insert and pruning
- Unit test `handleDailyAggregation` idempotency: mock D1 returning existing row, verify aggregation is skipped
- Unit test `handleDbSizeCheck`: mock pragma queries, verify email sent when over threshold
- Run `./scripts/check.sh --check vitest-license-server`

### Docs

- Update `apps/license-server/CLAUDE.md`:
  - Add "Cron handler" section describing the three jobs
  - Add `CRASH_NOTIFICATION_EMAIL` to the secrets table
  - Add cron trigger to the deployment section

---

## Milestone 4: Dashboard migration

**Intention:** Update the analytics dashboard to read from D1 instead of AE for downloads and active users. The
dashboard currently queries AE directly via the CF Analytics Engine SQL API. After this milestone, it queries D1 via
new read endpoints on the worker.

### Code changes (worker)

Add read-only endpoints to the worker, authenticated with the existing `ADMIN_API_TOKEN`:

- **`GET /admin/downloads`** — accepts `?range=24h|7d|30d|all`, queries D1 `downloads` table, returns JSON array of
  `{ date, version, arch, country, count }` grouped by day.
- **`GET /admin/active-users`** — accepts `?range=7d|30d|90d|all`, queries D1 `daily_active_users` table, returns JSON
  array of `{ date, version, arch, unique_users }`.
- **`GET /admin/crashes`** — accepts `?range=7d|30d|90d|all`, queries D1 `crash_reports` table, returns JSON array of
  `{ date, top_function, signal, count, versions }` grouped by crash site. This endpoint also serves the future
  "Stability" dashboard section.

### Code changes (dashboard)

- Update `src/lib/server/sources/cloudflare.ts`: replace AE SQL API queries with `fetch()` calls to the new worker
  endpoints. Auth via `LICENSE_SERVER_ADMIN_TOKEN` (already available in dashboard env).
- The `/admin/active-users` endpoint returns per-arch data, but the current dashboard view aggregates across arches
  (showing total unique users per version). Either aggregate client-side in the dashboard, or add a `?group_by=version`
  query param to the endpoint that omits the arch breakdown. Client-side aggregation is simpler.
- Remove `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` from the dashboard env vars — `cloudflare.ts` is their only
  consumer. Remove from `.env.example`, CF Pages secrets, and the env vars table in `CLAUDE.md`.

### Tests

- Add tests for the new admin endpoints (valid ranges, auth, empty results)
- Verify dashboard still renders correctly (manual test — the dashboard has no automated tests)
- Run `./scripts/check.sh`

### Docs

- Update `apps/analytics-dashboard/CLAUDE.md`:
  - Update data sources table: `cloudflare.ts` now queries worker endpoints, not AE SQL API
  - Remove the `cmdr_crash_reports` "not yet integrated" note — crash data is now available via `/admin/crashes`
- Update `apps/license-server/CLAUDE.md`:
  - Add the new admin endpoints to the routes table

---

## Milestone 5: Rename to `api.getcmdr.com`

**Intention:** The worker handles licensing, telemetry, crash reports, downloads, update checks, and admin endpoints.
"License server" is misleading. Rename to reflect reality.

### Infrastructure

**Critical gotcha:** Changing the `name` in `wrangler.toml` doesn't rename the existing worker — it creates a **new**
worker. The old `cmdr-license-server` continues running with stale code. KV bindings, AE datasets, D1 database, and
secrets don't carry over automatically.

1. **Keep the worker name as `cmdr-license-server`** in `wrangler.toml` — don't change it. The CF worker name is an
   internal identifier, not user-facing. Renaming it creates migration pain with zero user benefit.
2. Add DNS: CNAME `api.getcmdr.com` pointing to the zone (same pattern as `license.getcmdr.com`)
3. Add a second route in `wrangler.toml`:
   ```toml
   [[routes]]
   pattern = "api.getcmdr.com/*"
   zone_name = "getcmdr.com"
   ```
4. **Keep the existing `license.getcmdr.com/*` route** — it must continue working for all existing app versions that
   hardcode `license.getcmdr.com`. This is not temporary; it stays forever.
5. Deploy and verify both domains serve the same worker

### Code changes

- **Desktop app — `src-tauri/src/commands/crash_reporter.rs`:** change `CRASH_REPORT_URL` to
  `https://api.getcmdr.com/crash-report`
- **Desktop app — any other hardcoded `license.getcmdr.com` references:** update to `api.getcmdr.com`. Search for
  `license.getcmdr.com` across the entire codebase.
- **Health check route:** update the response service name from `"cmdr-license-server"` to `"cmdr-api-server"`

### Directory rename

- Rename `apps/license-server/` → `apps/api-server/`
- Update all references:
  - `pnpm-workspace.yaml` or `package.json` workspace entries
  - GitHub Actions workflows (`.github/workflows/`)
  - `AGENTS.md` references
  - `docs/architecture.md`
  - Any import paths or script references
- **Do NOT break git history** — use `git mv` for the rename

### Tests

- Verify all existing tests pass after the rename
- Run `./scripts/check.sh`

### Docs

- Rename `apps/api-server/CLAUDE.md` (was `apps/license-server/CLAUDE.md`):
  - Update title and description
  - Update deployment commands and URLs
  - Note that `license.getcmdr.com` remains as an alias
- Update `AGENTS.md`: all license-server references → api-server
- Update `docs/architecture.md`: update the "Other apps" table
- Update `docs/tooling/cloudflare.md` if it references the worker name
- Update privacy policy § data retention: update crash report retention (now permanent until manual pruning), add
  downloads and aggregated active user data.

---

## Execution notes

- **Milestones 1–3 are sequential** — each builds on the previous.
- **Milestone 4 (dashboard migration) depends on milestones 1–2** (needs D1 data and admin endpoints) but is
  independent of milestone 3 (cron). The dashboard doesn't need to work continuously — a brief gap during migration is
  fine.
- **Milestone 5 (rename) is independent** of all other milestones. Doing it last avoids dealing with the rename while
  making functional changes.
- After each milestone, run `./scripts/check.sh` to verify nothing is broken.
- No parallelism needed — clean sequential execution.
- **D1 migrations must be applied before each deploy** that adds new tables. `wrangler deploy` does not auto-apply
  migrations. Run `wrangler d1 migrations apply cmdr-telemetry` before `npx wrangler deploy`. Update the deployment
  section in `CLAUDE.md` to include this step.

## Decisions

**Decision: D1 as primary store, not dual-write with AE.**
**Why:** Nobody reads crash data from AE (the dashboard never integrated it). The dashboard can easily switch from AE to
D1 for downloads and update checks. Dual-writing adds complexity (two write paths, two sets of mocks in tests, two
failure modes) for zero benefit. D1 as the single source of truth is simpler and gives permanent retention.

**Decision: Keep `DEVICE_COUNTS` as the only remaining AE dataset.**
**Why:** Device count tracking is operational data for fair-use monitoring. 90-day retention is plenty — if someone had
too many devices 6 months ago, it doesn't matter. The volume is low and it doesn't need long-term archival.

**Decision: D1 writes are fire-and-forget (waitUntil + catch).**
**Why:** Crash report ingestion, download redirects, and update check proxying must never fail or slow down because of
a telemetry write. If D1 is temporarily unavailable, we lose that data point. This is acceptable — D1 outages are rare
and short, and a few missing telemetry rows don't affect business decisions.

**Decision: `INSERT OR IGNORE` with UNIQUE constraint for update check deduplication, not in-memory counting or
AE `COUNT(DISTINCT)`.**
**Why:** The UNIQUE constraint on `(date, hashed_ip, app_version, arch)` makes deduplication free — SQLite silently
drops duplicates. One D1 write per request is well within the free tier (100K writes/day). Expected volume: the Tauri
updater checks on launch, so roughly one request per active user per day. Even at 1,000 daily active users, that's
~1,000 D1 writes/day — 1% of the free tier limit. No AE SQL API token needed, no cross-service query, no cron
dependency for the primary data path.

**Decision: Aggregate `update_checks` → `daily_active_users` via cron, prune raw data after 7 days.**
**Why:** The raw `update_checks` table grows by one row per unique user per day. Aggregation compresses this to ~5 rows
per day (one per version+arch combo). Pruning after 7 days keeps the table small while giving a buffer in case the cron
misses a run. The `daily_active_users` table is the long-term record and stays forever.

**Decision: DB size notification instead of automated pruning.**
**Why:** Automated pruning requires choosing retention periods upfront. At Cmdr's current scale, the D1 database will
take years to reach even 100 MB. A size alert lets you make a human decision about what to prune when it actually
matters, rather than pre-committing to retention policies that may be wrong.

**Decision: `notified_at` column for idempotent crash notifications, not time-window queries.**
**Why:** Time-window queries (`WHERE created_at > now - 12h`) can miss crashes if the cron is delayed, or double-notify
if it runs early. Marking rows as notified is idempotent — the cron can run any number of times without duplicating
notifications or skipping crashes.

**Decision: One cron trigger (every 12h) for all jobs, not separate triggers.**
**Why:** CF free tier allows 5 cron triggers per worker. Using one trigger with internal branching (crash notifications
every run, daily jobs on the midnight run) is simpler and leaves headroom for future crons.

**Decision: Keep `license.getcmdr.com` as a permanent alias after the rename.**
**Why:** Every shipped app version hardcodes `license.getcmdr.com` for crash reports, update checks, and license
validation. Breaking this URL would break every existing installation. The old route costs nothing to maintain (one
extra line in `wrangler.toml`).

**Decision: Resend for crash notifications (not Discord/Slack webhooks).**
**Why:** Already in the stack (used for license emails and device alerts). No new dependency, no new account, same
`RESEND_API_KEY`. Email is more reliable for low-frequency alerts than webhook URLs that can silently break.

**Decision: Rename directory to `api-server`, not just `server`.**
**Why:** `server` is too generic in a monorepo. `api-server` clearly communicates "this is the API backend" without
being as narrow as `license-server`. It matches the `api.getcmdr.com` domain.

**Decision: Keep the CF worker name as `cmdr-license-server`, only rename the directory and domain.**
**Why:** Changing `name` in `wrangler.toml` creates a new worker rather than renaming the existing one. All bindings
(KV, AE, D1), secrets, and configuration would need to be manually recreated. The worker name is an internal CF
identifier — not worth the migration pain. The directory rename (`apps/api-server/`) and domain (`api.getcmdr.com`)
are what developers and users see.
