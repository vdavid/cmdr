# Error report feature — execution log

Companion to [`error-report-plan.md`](error-report-plan.md). Records what shipped, in what order, and what an operator
needs to do before the feature is live in production.

## Date executed

2026-04-23 / 2026-04-24 (overnight).

## Phases shipped

| Phase | Commit      | Summary                                                                                                                                                            |
| ----- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1     | `1d719f36`  | Shared `redact/` module with snapshot-tested log corpus; crash reporter migrated to consume it.                                                                    |
| 2     | `f3dbf514`  | `tauri-plugin-log` file target raised to DEBUG, `KeepSome(N)` rotation driven by new `advanced.maxLogStorageMb` setting (default 200 MB, `0` = disabled).          |
| 2.1   | `8e24ee13`  | Tightened noise: special-file skip logs dropped from WARN to DEBUG so the bumped file target doesn't add user-visible noise.                                       |
| 3     | `1a2ea1c0`  | `POST /error-report` endpoint on `api-server`: multipart upload to R2, `tryEvict` in `waitUntil`, Discord notification with 7-day presigned URL, daily cron sweep. |
| 4     | `6d904aa6`  | Flow A: `error_reporter/` module + `prepare_error_report_preview` / `send_error_report` commands, `ErrorReportDialog.svelte` preview, Help menu item.              |
| 5     | `51b6102a`  | Flow B: debounced `auto_dispatcher` (60 s + ±10 s jitter), `log_error!` macro at user-visible call sites, `updates.errorReports` opt-in setting, auto-send toast.  |
| 6a    | `dd53b4fd`  | Housekeeping: oxfmt sweep on two files left unstaged by Phase 4.                                                                                                   |
| 6b    | `837abe49`  | Documentation pass: CLAUDE.mds, `architecture.md`, `security.md`, `CHANGELOG.md`.                                                                                  |
| 6c    | (this file) | Spec + execution log added to `docs/specs/`.                                                                                                                       |

## Test counts

Approximate net additions across phases:

- **Rust:** `redact/tests.rs` ~430 lines, `error_reporter/tests.rs` 192 lines, `error_reporter/auto_dispatcher_tests.rs`
  155 lines, `logging/tests.rs` 131 lines, plus ~20 lines added to `crash_reporter/tests.rs` for redactor migration.
  Total Rust unit suite at execution: **1369 tests passing**, plus **26 integration tests**.
- **Frontend:** 5 new vitest files (`error-report-flow.test.ts`, `auto-send-toast.test.ts`,
  `tauri-commands/error-reporter.test.ts`, plus three a11y tests), totaling ~500 LoC of tests.
- **API server:** 3 new vitest files (`discord.test.ts`, `error-report.test.ts`, `error-report-eviction.test.ts`) plus
  ~120 lines added to `scheduled.test.ts` for `handleDailyEvictionSweep`.

All `./scripts/check.sh` checks green at execution time (42 checks, ~2 m 14 s wall clock).

## Divergences from plan

- **Phase 1 — redactor scope.** MTP device names redacted in log targets but not in arbitrary message bodies. The plan
  listed device-name redaction as a generic pattern; in practice the only stable signal is the log target prefix.
  Rationale: false positives in message bodies were higher than expected for short device-name strings. Listed as a
  follow-up below.
- **Phase 2 — verbose toggle becomes a Rust-side no-op when file logging is on.** `tauri-plugin-log` doesn't support
  per-target level filtering, so raising the file target to DEBUG also raises the global floor. The frontend (LogTape)
  toggle still works; the Rust toggle is only visible when log storage is disabled. Documented in
  `apps/desktop/src/lib/logging/CLAUDE.md` and `docs/tooling/logging.md`.
- **Phase 3 — admin re-mint endpoint deferred.** Plan listed `GET /admin/error-report/:id/url` as part of Phase 3.
  Skipped because the 7-day presigned URL on the Discord embed covers the maintainer use case; re-mint becomes
  worthwhile only if access to `#error-reports` widens. ~50 LOC to add later.
- **Phase 4 — Save-bundle-to-disk debug option.** Implemented as planned, gated on `cfg!(debug_assertions)`. No
  surprises.
- **Phase 5 — `log_error!` migration scope.** Migrated only the call sites that already produce user-visible error
  toasts (not every `log::error!`). Strict superset of the plan's "literal interpretation" — this is intentional and
  matches the plan's stated bias toward better signal.

## Operator action required before deploying

Do these in order. None are reversible; do them when you're ready to flip the feature on.

1. **Rotate the Discord webhook.** A previous webhook URL was leaked in chat. At https://discord.com/, go to the Cmdr
   server → right-click `#error-reports` → **Edit Channel** → **Integrations** → **Webhooks** → click the existing
   webhook → **Delete Webhook**, then **New Webhook** named "Cmdr error reports". Click **Copy Webhook URL**. Keep that
   URL handy for step 5.
2. **Provision Cloudflare infrastructure.** From `apps/api-server/`:
   ```sh
   ./scripts/setup-cf-infra.sh
   ```
   This creates the `cmdr-error-reports` R2 bucket, the `ERROR_REPORT_META` KV namespace (the script prints the KV
   namespace ID — copy it for step 3), and applies the 90-day R2 lifecycle rule.
3. **Update `apps/api-server/wrangler.toml`** — replace `REPLACE_WITH_KV_ID` with the printed KV namespace ID. Commit
   the change.
4. **Generate an R2 S3-compat access key** in the Cloudflare dashboard with read access to `cmdr-error-reports`. Keep
   the access key ID and secret handy for step 5.
5. **Set wrangler secrets** (run from anywhere in the repo):
   ```sh
   pnpm --filter @cmdr/api-server exec wrangler secret put DISCORD_WEBHOOK_URL
   pnpm --filter @cmdr/api-server exec wrangler secret put R2_ACCOUNT_ID
   pnpm --filter @cmdr/api-server exec wrangler secret put R2_ACCESS_KEY_ID
   pnpm --filter @cmdr/api-server exec wrangler secret put R2_SECRET_ACCESS_KEY
   ```
6. **Deploy:**
   ```sh
   pnpm --filter @cmdr/api-server exec wrangler deploy
   ```
7. **Smoke-test.** From a dev build of Cmdr (or after a release build): **Help > Send error report…** → preview → send.
   Verify a notification lands in `#error-reports` and the download link works.

## Outstanding follow-ups (non-blocking)

- **Phase 1: MTP device names in message bodies.** Currently only redacted from log targets. ~5-line addition (extra
  pattern entry + corpus update) if real reports show device-name leaks in the message body.
- **Phase 2: verbose-logging toggle is UX-confusing.** It's a Rust-side no-op when file logging is on; the label doesn't
  communicate this. Consider renaming or restyling — for example, splitting into two switches (frontend verbose / Rust
  file-target level), or adding inline help text in the Logging section.
- **Phase 3: admin re-mint endpoint.** Add if Discord channel access widens beyond the maintainer.
