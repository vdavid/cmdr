# Admin endpoints details

Pull-tier docs for `src/admin/`. Must-know invariants live in `CLAUDE.md`; the tables these queries read are documented
where they're written (`../telemetry/DETAILS.md`, `../licensing/DETAILS.md`), and the retention windows that shape them
in `../../DETAILS.md` § Data retention.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Files

- **`admin.ts`**: `/admin/stats` (activation + device counts), `/admin/downloads` (by day/version/arch/country/source,
  raw `count` plus deduped `uniqueCount`), `/admin/active-users`, `/admin/update-activity` (per-day distinct
  update-enabled installs, the retained aggregate ∪ today's raw), `/admin/crashes` (by day/crash site/signal),
  `/admin/heartbeat-dau`, `/admin/feedback` (full text + reply-to email, newest first), `/admin/error-reports`
  (per-bundle R2 metadata via `list` + custom metadata, newest first).
- **`funnel.ts`**: `/admin/funnel`, plus the pure `buildDateList` / `assembleFunnel` / `aggregateUaFamilies` helpers.
- Tests: `admin-stats.test.ts`, `admin-endpoints.test.ts`, `funnel.test.ts` (route auth/validation plus the pure date
  math, zero-fill, and D7-knowability rules).

Consumer: the private SvelteKit dashboard in `apps/analytics-dashboard/`.

## Auth

Every route here takes `Authorization: Bearer <ADMIN_API_TOKEN>` through `verifyAdminAuth` (`../types.ts`), which
compares with the timing-safe `constantTimeEqual`. `/admin/generate` is deliberately NOT here: it's a licensing
operation and takes the Paddle webhook secret instead (`../licensing/`).

**Gotcha:** `verifyAdminAuth` uses a manual type annotation for `c` instead of Hono's `Context`. Using
`Context<{ Bindings: Bindings }>` would mean importing Hono's internal generic types and threading them through; the
manual shape `{ env: Bindings; req: { header: ... } }` is simpler and avoids coupling to Hono internals.

## Per-day funnel (`GET /admin/funnel`)

One admin endpoint the dashboard uses to render its top "Daily funnel" table in a single call, instead of stitching
per-metric endpoints together. Param `?days=N` (default 30, clamped to 1..90, else 400). Returns `FunnelDay[]`, oldest
UTC day first, **including today** (a partial day). Every column is bucketed by UTC day (`date()` in D1 is UTC; Listmonk
timestamps are normalized to UTC here), so a "day" means the same window across all columns. `null` (not 0) means
"unknown", which the dashboard renders as a dash.

Per-day columns and how each is derived:

- `downloads` + `downloadsBySource` (`{ website, homebrew, other }`): `COUNT(*)` of `downloads` rows by
  `COALESCE(source, 'other')`. Bots already filtered at write time; rows before migration 0008 have NULL source →
  `other`.
- `downloadsByRef` (`Record<ref, count>`): the same rows grouped by `COALESCE(ref, '(none)')`, so the dashboard can
  attribute installs to a first-touch channel. NULL ref (Homebrew, direct links, return visits in a later session, and
  rows before migration 0009) buckets under `"(none)"`. An empty object means no downloads that day. The `ref` is
  already sanitized at write time, so the grouping is on the stored value as-is.
- `downloadsByReferer`: the same rows grouped by the stored `Referer` host, the breakdown that illuminates the large
  `(none)` `ref` bucket.
- `downloadsByUaFamily` (`{ human, bot, unknown }`) + `humanInstalls`: per-row family via `resolveUaFamily`
  (`../user-agent.ts`), which prefers the stored `ua_family` and falls back to classifying the raw UA for pre-`0013`
  rows. `humanInstalls = human + unknown`: it drops only the provably-impossible downloads and keeps every ambiguous
  one, so it never overclaims. The classification rules and why `human` is NOT a clean count: `../telemetry/DETAILS.md`
  § Download tracking.
- `newInstalls`: count of `anal_id`s whose **first-ever** heartbeat (`MIN(created_at)` over the whole `heartbeat` table,
  no window filter on the inner query) fell on that UTC day. So an install that first beat months ago never counts as
  "new" inside the window.
- `dau`: `COUNT(DISTINCT anal_id)` beating that day (true DAU, same definition as `/admin/heartbeat-dau`).
- `d7Retention` (0..1 fraction) + `d7Retained` (raw count): **D7 definition** — for a cohort whose first heartbeat was
  on day X, an install is "D7 retained" if it has ANY heartbeat in the half-open window `[X+7d, X+8d)` (exactly the 7th
  day after install). `d7Retained` is the distinct count of such installs; `d7Retention = d7Retained / newInstalls(X)`.
  Both are `null` for cohorts younger than 8 days (the window hasn't fully passed, so it's genuinely unknown, not 0). An
  old cohort with installs but no retained beats is `0`, not `null`.
- `newsletterSignups`: Listmonk subscribers (newsletter list `LISTMONK_NEWSLETTER_LIST_ID`, default 3, **plus** beta
  list `LISTMONK_BETA_LIST_ID`) whose `created_at` falls on that UTC day, via one read-only
  `GET /api/subscribers?query=...` filtered by `created_at >= sinceDate`, paginated, then bucketed in code. Caveat:
  `created_at` is the subscriber's creation time, not the per-list join time, so someone who joins a second list later
  is counted only on their original signup day (fine for a coarse acquisition signal). Best-effort: when Listmonk is
  unconfigured (URL/user/token missing) OR the query throws, signups are `null` for every day, never 0 — so the
  dashboard distinguishes "no signups" from "couldn't ask". The list ids MUST be TOML integers in `[vars]` (the resolver
  checks `typeof === 'number'`); a string drops that list from the count.

`buildDateList` and `assembleFunnel` are pure and exported so the date math, zero-fill, and D7-knowability logic are
unit-tested without a live D1 (`funnel.test.ts`); the SQL semantics are verified against a real local D1 with
`../../scripts/seed-funnel-local.sql` (hand-computed expectations are in that file's header).

## Rollup-versus-live union

Two endpoints read a summary table that the retention sweep or the daily cron fills, unioned with live rows for the days
the summary doesn't cover yet:

- `/admin/downloads` prefers `downloads_daily_unique` (migration `0014`, written by the retention sweep BEFORE it clears
  `hashed_ip`) and falls back to `COUNT(DISTINCT hashed_ip)` over live rows inside the 90-day window.
- `/admin/update-activity` prefers `daily_active_users` (written by the daily aggregation job) and falls back to raw
  `update_checks` for the seven days before pruning.

Query only the live table and every day past its window silently reads as zero, with no way back. The ordering guarantee
that makes this work (roll up before clearing, cutoffs snapped to midnight UTC) is in `../../DETAILS.md` § Data
retention.
