# Admin endpoints

Read-only aggregations the private analytics dashboard calls, all behind `ADMIN_API_TOKEN`. `admin.ts` holds the
per-metric endpoints (`/admin/stats`, `/admin/downloads`, `/admin/active-users`, `/admin/update-activity`,
`/admin/crashes`, `/admin/heartbeat-dau`, `/admin/config-shape`, `/admin/feedback`, `/admin/error-reports`); `funnel.ts`
holds `/admin/funnel`, the one-call per-UTC-day acquisition funnel.

## Must-knows

- **Bearer tokens compare with `constantTimeEqual` (`../licensing/paddle.ts`), ❌ never `===`.** `verifyAdminAuth`
  (`../types.ts`) is the one gate; `/admin/generate` is the odd one out and lives in `../licensing/`.
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
- **`/admin/funnel?days=N` clamps to 1..90** and always includes today as a partial day.
- **UA-family aggregation reads `resolveUaFamily` (`../user-agent.ts`)**, ❌ never a fresh classification of the raw UA:
  the stored `ua_family` outlives the `user_agent` the retention sweep clears at 90 days.

Per-endpoint shapes, the funnel's column derivations (D7, newInstalls, Listmonk signups), and its test strategy:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
