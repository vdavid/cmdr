# Admin endpoints

Aggregations the private analytics dashboard calls, all behind `ADMIN_API_TOKEN`. `admin.ts` holds the per-metric
endpoints (`/admin/stats`, `/admin/downloads`, `/admin/active-users`, `/admin/update-activity`, `/admin/crashes`,
`/admin/heartbeat-dau`, `/admin/config-shape`, `/admin/feedback`, `/admin/error-reports`) plus the one write,
`PUT /admin/feedback/:id/answered`; `funnel.ts` holds `/admin/funnel`, the one-call per-UTC-day acquisition funnel (its
contract and pure assembly in `funnel-assemble.ts`), and `funnel-countries.ts` its per-country totals,
`/admin/funnel/countries`.

## Must-knows

- **Bearer tokens compare with `constantTimeEqual` (`../licensing/paddle.ts`), ❌ never `===`.** `verifyAdminAuth`
  (`../admin-auth.ts`, awaited) is the one gate, including for `/admin/generate`, `/admin/revoke`, and
  `/admin/licenses`, which take the same token but live in `../licensing/` because they're licensing operations. The
  dashboard's Licenses page reads the last of those: `../licensing/DETAILS.md` § The licenses listing.
- **In every response, `null` means "unknown" and `0` means a real zero.** The dashboard renders `null` as a dash, so
  collapsing the two invents data: an unreachable Listmonk, or a D7 cohort younger than eight days, is NOT zero.
- **An aggregate wins over the live query, and both are unioned.** `/admin/downloads` prefers the
  `downloads_daily_unique` rollup and falls back to counting live rows for days still inside the retention window (same
  pattern `/admin/update-activity` uses over `daily_active_users`). Query only the live table and every pre-sweep unique
  count reads as zero.
- **`/admin/config-shape` counts each install's LATEST heartbeat only, and keeps `app_version` on every row.** The
  config shape is sparse, so an absent key means "on the default" OR "the setting didn't exist in that build"; only the
  dashboard's per-version defaults manifest separates those, and it needs the version to do it. ❌ Never sum the
  versions away here.
- **`/admin/funnel?days=N` takes 1..400** and always includes today as a partial day. Its fields only ever get ADDED:
  the dashboard must keep working against an older Worker.
- **Per-install facts read `install_summary`, ❌ never a `GROUP BY anal_id` over `heartbeat`**: that scans every beat
  ever, and D1 bills rows read. Three call sites keep the summary in step (`/heartbeat`, both sweeps); a new path that
  deletes beats must delete summary rows too. DETAILS § Install summary.
- **An install's country, for any per-country number, is `first_beat_country`, ❌ never `install_summary.country`**: the
  latter fills in on a later beat, so only surviving installs gain one and a country's retention reads too high.
- **`FunnelDay.byCountry` must sum to the day**: `assembleFunnel` builds the day from its slices, and every query groups
  by country rather than running once per country. DETAILS § Per-day funnel.
- **UA-family aggregation reads `resolveUaFamily` (`../user-agent.ts`)**, ❌ never a fresh classification of the raw UA:
  the stored `ua_family` outlives the `user_agent` the retention sweep clears at 90 days.

Per-endpoint shapes, the funnel's column derivations (D7, newInstalls, Listmonk signups), and its test strategy:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
