# Admin endpoints details

Pull-tier docs for `src/admin/`. Must-know invariants live in `CLAUDE.md`; the tables these queries read are documented
where they're written (`../telemetry/DETAILS.md`, `../licensing/DETAILS.md`), and the retention windows that shape them
in `../../DETAILS.md` § Data retention.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Files

- **`admin.ts`**: `/admin/stats` (activation + device counts), `/admin/downloads` (by day/version/arch/country/source,
  raw `count` plus deduped `uniqueCount`), `/admin/active-users`, `/admin/update-activity` (per-day distinct
  update-enabled installs, the retained aggregate ∪ today's raw), `/admin/crashes` (by day/crash site/signal),
  `/admin/heartbeat-dau` (§ below), `/admin/config-shape` (settings values per app version, § below), `/admin/feedback`
  (full text, reply-to email, `answeredAt`, and the linked logs' `errorReportId`, newest first),
  `PUT /admin/feedback/:id/answered` (§ below), `/admin/error-reports` (per-bundle R2 metadata via `list` + custom
  metadata, newest first; `.amend.json` sidecars share the prefix and are skipped, since an amendment is part of its
  report rather than a second one).
- **`funnel.ts`**: `/admin/funnel`, its queries (`queryFunnelRows`), and the pure `buildDateList` / `shiftDate` /
  `parseDays` helpers. **`funnel-assemble.ts`**: the contract (`FunnelDay`, `CountryDay`), the row sets the queries
  return (`FunnelRows`), `normalizeCountry`, and the pure `assembleFunnel` (`funnel.ts` re-exports the public names).
  **`funnel-countries.ts`**: `/admin/funnel/countries` (`queryCountryRows` + the pure `assembleCountries`), reusing
  those helpers.
- Tests: `admin-stats.test.ts`, `admin-endpoints.test.ts`, `funnel.test.ts` (route auth/validation plus the pure date
  math, zero-fill, channel, D7/D28-knowability, retention-so-far, and `byCountry` rules), `funnel-sql.test.ts` (both
  funnels' SQL on real SQLite, seeded through the `/heartbeat` INSERTs, including the country slices summing to the
  day), `heartbeat-dau-sql.test.ts` (the DAU and app-hours query on real SQLite), `feedback-answered.test.ts` (the
  feedback list and the mark-answered toggle through the app on real SQLite).

Consumer: the private SvelteKit dashboard in `apps/analytics-dashboard/`.

## Heartbeat DAU and app hours

`/admin/heartbeat-dau?range=7d|30d|90d|all` returns one row per UTC day: `{ date, dau, appHours }`.

- `dau`: `COUNT(DISTINCT anal_id)` that beat that day.
- `appHours`: `SUM(COALESCE(uptime_seconds, 3600)) / 3600.0`, how long the app ran that day summed over installs. A beat
  reports the runtime it accounts for; a beat from a client older than `uptime_seconds` stores NULL and counts as one
  hour, because those clients beat hourly. That's what lets rows from before and after the three-hour cadence join in
  one series. ❌ Don't go back to counting rows: a row count measures the beat cadence, so it drops by two thirds the
  day a release changes it, with no change in use.

The whole beat counts on the day it ARRIVED, so runtime that spans midnight lands on the later day. At a three-hour
cadence that shifts a little runtime across a day boundary and changes no weekly or monthly total.

## Marking feedback answered (`PUT /admin/feedback/:id/answered`)

David replies to feedback from his own mailbox, which this Worker never sees, so a reply leaves no trace here. This
toggle is the only signal, and the dashboard's "Awaiting reply" count (reply-to email AND `answered_at IS NULL`) reads
it. Body `{ "answered": true }` stamps `feedback.answered_at` (migration `0025`), `false` clears it. Marking an
already-answered row keeps its first timestamp (`COALESCE`), so a double click doesn't move the date. Returns
`{ id, answeredAt }`, 404 for an unknown id, 400 for a bad id or a non-boolean flag.

By hand, with the admin token:

```sh
curl -X PUT https://api.getcmdr.com/admin/feedback/<id>/answered \
  -H "Authorization: Bearer $(secret CMDR_ADMIN_API_TOKEN)" -H 'Content-Type: application/json' \
  -d '{"answered": true}'
```

## Auth

Every route here takes `Authorization: Bearer <ADMIN_API_TOKEN>` through `verifyAdminAuth` (`../admin-auth.ts`), which
rate-limits, compares with the timing-safe `constantTimeEqual`, and records a rejection (`../../DETAILS.md` § Admin
gate). `/admin/generate`, `/admin/revoke`, and `/admin/licenses` take the same token but live in `../licensing/`: they
mint, kill, and list licenses rather than aggregate telemetry, so they belong with the money code. The last is what the
dashboard's Licenses page calls (every ledger row with a computed state, plus activation codes in KV that no row
explains): `../licensing/DETAILS.md` § The licenses listing.

## Per-day funnel (`GET /admin/funnel`)

One admin endpoint the dashboard uses to render its top "Daily funnel" table in a single call, instead of stitching
per-metric endpoints together. Params: `?days=N` (default 30, an integer 1..400, else 400) and an optional `?country=CC`
(ISO-2, case-insensitive, else 400). Returns `FunnelDay[]`, oldest UTC day first, **including today** (a partial day).
Every column is bucketed by UTC day (`date()` in D1 is UTC; Listmonk timestamps are normalized to UTC here), so a "day"
means the same window across all columns. `null` (not 0) means "unknown", which the dashboard renders as a dash.

**The contract only grows.** Every field below stays; new ones are additive, and the dashboard tells an older Worker
apart by a field it lacks (an old Worker ignores `country` and answers the global rows, so the dashboard must not
present those as a country's).

**With `country`**, the download columns filter on `downloads.country` (the download's own request), while
`newInstalls*`, `dau`, and every retention field filter on the country of the install's first beat
(`install_summary.first_beat_country`, § Install summary; `dau` counts the beats of installs from that country).
`newsletterSignups` is `null` for every day: Listmonk has no country, and Listmonk isn't asked.

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
- `newInstalls` + `newInstallsByChannel` (`{ homebrew, direct, unknown }`, summing to `newInstalls`): installs whose
  **first-ever** beat (`install_summary.first_beat_at`) fell on that UTC day, so an install that first beat months ago
  never counts as "new" inside the window. The channel is the install's first non-NULL `install_channel`; NULL (clients
  older than the field) and any value the server doesn't know land in `unknown`.
- `dau`: `COUNT(DISTINCT anal_id)` beating that day (true DAU, same definition as `/admin/heartbeat-dau`).
- `d7Retention` (0..1 fraction) + `d7Retained` (raw count): **D7 definition** — for a cohort whose first heartbeat was
  on day X, an install is "D7 retained" if it has ANY heartbeat in the half-open window `[X+7d, X+8d)` (exactly the 7th
  day after install). `d7Retained` is the distinct count of such installs; `d7Retention = d7Retained / newInstalls(X)`.
  Both are `null` for cohorts younger than 8 days (the window hasn't fully passed, so it's genuinely unknown, not 0). An
  old cohort with installs but no retained beats is `0`, not `null`.
- `retentionSoFar` (`{ day, retained } | null`): the dashboard's grey "tentative" D7 point. For a cohort 2–7 days old
  with installs, `day` is the latest fully passed day index n = age − 1 (1..6) and `retained` the cohort installs with a
  beat in `[X+n, X+n+1)`, the D7 definition with n in place of 7. `null` for younger cohorts, for D7-knowable ones, and
  for a day with no installs.
- `d28Retention` + `d28Retained` + `d28SoFar`: the same three with 28 in place of 7. D28 is known once the cohort is at
  least 29 days old; `d28SoFar` covers cohorts 8–28 days old (`day` 7..27), so it starts exactly where D7 becomes known.
  One pure helper (`horizonRetention`) derives both horizons.
- **How `queryRetention` reads it**: three windows per install, UNIONed: `[X+7, X+8)`, `[X+28, X+29)`, and yesterday. A
  cohort n+1 days old has fully passed day n exactly at yesterday, so that one window answers both "so far" fields for
  every cohort 2–28 days old, and each install's lookup touches about three days of beats rather than its first four
  weeks. The windows can overlap (yesterday is day seven for an eight-day-old cohort), hence `COUNT(DISTINCT)`. The
  offsets it returns are 7, 28, and age − 1, nothing else.
- `byCountry` (`Record<country, CountryDay>`): the day sliced per country, for the dashboard's instant country filter.
  Keys are ISO-2 or `"(unknown)"` (`normalizeCountry`), and a country with nothing that day is absent. `CountryDay` is
  `{ serverDownloads, humanInstalls, downloadsBySource, newInstalls, newInstallsByChannel, d7Retained, d28Retained, retentionSoFar, d28SoFar }`,
  each with the top-level field's meaning and null rules; rates are left to the dashboard (sum the counts, then divide).
  A download counts under its own country, an install under its first beat's. **The slices sum to the day by
  construction**: the download, install, and retention queries are grouped by country too, and `assembleFunnel` builds
  the day's own numbers by summing the slices (`funnel-sql.test.ts` asserts the invariant). That grouping is what keeps
  it at the same seven queries; ❌ never a query per country.
- `newsletterSignups`: Listmonk subscribers (newsletter list `LISTMONK_NEWSLETTER_LIST_ID`, default 3, **plus** beta
  list `LISTMONK_BETA_LIST_ID`) whose `created_at` falls on that UTC day, via one read-only
  `GET /api/subscribers?query=...` filtered by `created_at >= sinceDate`, paginated, then bucketed in code. Caveat:
  `created_at` is the subscriber's creation time, not the per-list join time, so someone who joins a second list later
  is counted only on their original signup day (fine for a coarse acquisition signal). Best-effort: when Listmonk is
  unconfigured (URL/user/token missing) OR the query throws, signups are `null` for every day, never 0 — so the
  dashboard distinguishes "no signups" from "couldn't ask". The list ids MUST be TOML integers in `[vars]` (the resolver
  checks `typeof === 'number'`); a string drops that list from the count.

`buildDateList` and `assembleFunnel` are pure and exported so the date math, zero-fill, and D7/D28-knowability logic are
unit-tested without a live D1 (`funnel.test.ts`); the SQL is proved on real SQLite by `funnel-sql.test.ts`, and can be
eyeballed against a local D1 with `../../scripts/seed-funnel-local.sql` (hand-computed expectations are in that file's
header). The upsert, `julianday` offsets, and the three-window UNION retention query ran clean on a local D1 (wrangler
4.151.0, a probe script, 2026-11-10); the remote D1 can't run them until migrations 0031 and 0032 are applied.

## Per-country funnel (`GET /admin/funnel/countries`)

`?days=N` (same bounds as `/admin/funnel`) and an optional `?end=YYYY-MM-DD` (inclusive, no later than today; a
malformed or future one is a 400): the N UTC days ending `end`, or ending today (partial) without it. That's how the
dashboard's back/forward steps reach a past window. The `X-Window: since..end` response header names the days answered;
an older worker ignores `end`, and the missing header is how the dashboard tells. Returns
`[{ country, serverDownloads, humanInstalls, newInstalls, d7Cohort, d7Retained, d28Cohort, d28Retained }]` over the
whole window:

- `serverDownloads` / `humanInstalls`: `downloads` rows by their own `country`, `humanInstalls` dropping the `bot`
  family exactly as `FunnelDay.humanInstalls` does.
- `newInstalls`: installs whose first beat fell in the window, by `install_summary.first_beat_country`.
- `d7Cohort` / `d7Retained`: of those, the installs in cohorts at least eight days old (first beat before today minus
  seven, and inside the window), and the ones among them retained at D7. The dashboard divides the two; a young cohort
  counts in `newInstalls` only.
- `d28Cohort` / `d28Retained`: the same for D28, cohorts at least 29 days old (first beat before today minus 28). One
  query reads both retention windows.
- `country` is ISO-2, or `"(unknown)"` for anything else: a NULL install country, and the `unknown` / `XX` / `T1` values
  `downloads.country` can hold (`normalizeCountry`).
- Sorted by `newInstalls`, then `serverDownloads`, both descending; on a full tie `(unknown)` goes last, then by code.

## Install summary

`install_summary` (migration `0031`) is one row per install: `anal_id`, `first_beat_at`, the install's first non-NULL
`country` and `install_channel`, and `first_beat_country` (migration `0032`), the country of the first beat itself.

**Decision: per-country install facts read `first_beat_country`, ❌ never `country`.** An install from before the field
gains a `country` on its next beat, but only installs still in use ever beat again, so a country built on `country`
counts its survivors and not its churned installs: its installs look lower and its retention higher than they are. The
first-beat country is either known at install time or never, so every install counts under a real country or
`"(unknown)"` regardless of whether it stuck around. `/heartbeat` writes it on the insert only; every install from
before `0032` backfills NULL (`"(unknown)"`), even one whose `country` filled in later. That's every per-country number
in `/admin/funnel` (`?country=`, `byCountry`) and `/admin/funnel/countries`; downloads keep `downloads.country`, which
has always been recorded with the request. `country` stays for whatever doesn't need the cohort to be honest.

**Decision: maintain it at ingest rather than aggregate at query time.** Why:

- D1 bills rows READ, and every per-install fact (first beat, country, channel) needs a `GROUP BY anal_id` over the
  whole `heartbeat` table, which is every beat ever (about eight a day per active install, kept two years). The funnel
  did that twice per load, the new columns would add more, and `/admin/funnel/countries` three. The summary is one row
  per install (a few hundred), with `first_beat_at` indexed.
- "First non-NULL" is awkward at query time (window functions or correlated subqueries per load) and trivial at write
  time.
- The write cost is one upsert per beat in the beat's own `batch`, so beat and summary commit together. Its `WHERE`
  makes the common case (nothing new to fill) a no-op rather than a row write.
- It also stays right where the beats don't: once retention deletes an install's two-year-old first beat, `MIN()` over
  `heartbeat` would move its "install day" forward, while `first_beat_at` keeps the real one.

Keeping it in step is three call sites: `/heartbeat` (`upsertInstallSummarySql`, `../telemetry/heartbeat.ts`), the
synthetic sweep (`deleteSyntheticInstallSummariesSql`, BEFORE the beats go, since the classifier reads `heartbeat`), and
the retention sweep (`deleteOrphanInstallSummariesSql`, AFTER the heartbeat delete). A SQL trigger would make it one,
but `CREATE TRIGGER` bodies are known to fail in D1 migrations (`workers-sdk` issue 4998, "incomplete input" on remote),
so the logic stays in code where tests reach it. A raw `INSERT INTO heartbeat` (a seed script, a test) leaves the
summary behind; build it with the migration's backfill `SELECT`. `/heartbeat` binds `created_at` explicitly, so the
summary's `first_beat_at` is the beat row's `created_at` to the second.

The retention join reads `idx_heartbeat_anal_created` (`anal_id, created_at`), so each cohort install's lookup touches
its first week of beats only; that index replaced `idx_heartbeat_anal`, whose every use it covers.

## Config shape (`GET /admin/config-shape`)

Two queries over one CTE that reduces the window to each install's LATEST heartbeat
(`ROW_NUMBER() OVER (PARTITION BY anal_id ORDER BY created_at DESC, id DESC)`), so an install that beat hourly for a
week weighs exactly as much as one that ran for an afternoon, and a setting changed mid-window reads at its current
value. `config_json IS NOT NULL` filters inside the CTE, keeping `json_each` off a NULL from a release that predates the
config shape.

- `installs`: `[{ appVersion, installs }]`, the per-version denominator.
- `values`: `[{ appVersion, key, type, value, installs }]`, one row per distinct value of each key.

`type` is SQLite's `json_each.type` (`true` / `false` / `integer` / `real` / `text`) and travels beside `value` because
D1 flattens a JSON boolean to `1` / `0`; the dashboard rebuilds the real type from the pair.

**Why `appVersion` can't be summed away.** The config shape is sparse (the app persists only settings a user explicitly
CHANGED), so a key absent from an install's row means "on the default" OR "the setting didn't exist in that build". Only
the per-version defaults manifest on the dashboard separates those, and it needs the version to do it. Collapsing
versions here would make every adoption number silently wrong for older installs.

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
