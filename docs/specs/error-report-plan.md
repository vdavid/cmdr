# Error report feature

Add a way for testers and users to ship Cmdr diagnostic logs to us when something goes wrong. Two flows:

- **Flow A (user-initiated)**: a "Send error report" menu item and a button on existing error toasts. The user sees a
  preview, clicks send, gets a short reference ID like `ERR-8F3A2`.
- **Flow B (auto-send on error)**: if opted in, when a user-visible error fires, ship the most recent log tail
  automatically. Toast: "Error encountered, error report sent [View] [Change settings]", auto-dismiss after 10 s.

Bundles ship to a Cloudflare R2 bucket via the existing `api.getcmdr.com` Worker; a Discord webhook posts a one-line
notification to `#error-reports` so we can act on incoming reports without polling.

## Why

Right now the only diagnostics path is "ask the tester to find `~/Library/Logs/com.veszelovszki.cmdr/`, zip it, send it
to me." That works once; it doesn't scale to even ten testers, and it relies on the user knowing what to send and what
to redact. We need a one-click pipeline before launch.

The crash reporter already covers panics. This feature covers everything else: MTP weirdness, network glitches, indexing
oddities, generic "this didn't work."

## Design decisions

### Logs stay on disk only, at DEBUG level, capped by a setting

Currently `tauri-plugin-log` rotates at 50 MB with `RotationStrategy::KeepAll` at INFO level. Change to:

- File target level: **DEBUG** (terminal level unchanged in dev; devs use `RUST_LOG` to override).
- Cap: configurable via new setting `advanced.maxLogStorageMb`, default **200 MB**, range 0–5000.
- `0` is a hard disable: drop the `Folder` target from the plugin builder entirely. No logs on disk, no error report
  bundles possible.
- Rotation: keep N files where `N = ceil(cap_mb / 50)`. Use `RotationStrategy::KeepSome(N)` if the pinned plugin version
  exposes it; otherwise add a small post-rotation pruner module. Either way, ship a pruner. Defensive belt and braces.

**Why disk-only, not an in-RAM ring buffer?** Disk is essentially free; RAM is perceptually expensive on 8 GB machines.
The user-visible benefit is identical for both flows. Skip the complexity.

**Why DEBUG always, not gated on `developer.verboseLogging`?** Error reports need debug context regardless of the
toggle. The verbose toggle continues to control in-RAM/terminal gating (`log::set_max_level`), the file target now
captures debug independently. Document this in `apps/desktop/src/lib/logging/CLAUDE.md`.

### Shared, path-shape-preserving redactor

Extend the existing crash reporter sanitizer (`apps/desktop/src-tauri/src/crash_reporter/mod.rs:157-174`) into a
standalone `redact` module. Both crash reporter and error reporter consume it.

**Path-shape preservation**: `/Users/john/Documents/budget.pdf` → `$HOME/Documents/<file>.pdf`. Keep extension and
parent dir, redact only the user-identifying parts. Allowlist of "safe" parent dir names (`Documents`, `Downloads`,
`Desktop`, `Library`, `src`, `Pictures`, `Movies`, `Music`, `Public`, `AppData`, `Application Support`); anything else
(e.g., `/Users/john/SecretProjectName/...`) collapses parent to `<dir>`.

Patterns to cover: Unix/Linux/Windows home paths, `/Volumes/<label>`, `/media/<label>`, SMB UNC URIs, bare `*.local`
hostnames, MTP device names, IPv4/IPv6, email addresses, URL userinfo.

**Test corpus is mandatory.** A redactor miss ships PII. Land a `tests/fixtures/log-corpus.txt` with ~150 synthesized
log lines, snapshot-assert the redacted output, plus negative tests for things that look path-ish but aren't
(`Cargo.toml`, `cmdr_lib::network::smb_client`, `0.1.2-alpha`), idempotency check, and a histogram of replacement counts
so coverage regressions show up as numeric diffs in CI.

### Two flows, A first; no opt-in for A, opt-in for B

Flow A: clicking the button **is** the consent. No setting. The preview dialog shows manifest + first 5 / last 20
redacted log lines so the user can see what they're sending.

Flow B: fires without the user's per-event consent, so it's gated by `updates.errorReports` (default off). Debounce
errors within 60 s into a single report (with ±10 s jitter to avoid lock-step reporting under global outages). On send,
toast as described above; "View" opens the same preview dialog as Flow A.

### Trigger Flow B at user-visible error sites, not via a global log tap

Tapping `log::Log` to fire on every `log::error!` is fragile (tauri-plugin-log already owns the global logger) and noisy
(`smb2`, `nusb` warnings would trigger reports we don't want). Instead, add a `log_error!` macro that does
`log::error!(...)` plus `error_reporter::auto_dispatcher::on_error_logged(...)`, and migrate select call sites: the same
set that already produces user-visible error toasts. Panics still go through the existing crash reporter.

This is a strict superset of the literal interpretation: we only auto-report errors the developer flagged as
user-impacting. Better signal, less noise.

### Server: extend the existing api-server, not a new Worker

Add `POST /error-report` to `apps/api-server/`. Reasons: shared bindings (R2, KV), shared deploy pipeline, shared admin
auth, shared Discord webhook. The current cron handler is already a multi-job dispatcher, and adding a fourth "daily
eviction sweep" job is the cheapest path.

### R2 layout + 8/6 GB eviction with a KV lock, on the upload path

Bucket: `cmdr-error-reports`. Key: `error-reports/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`.

Eviction runs **on every upload, in `event.waitUntil(...)`** so the client response isn't blocked:

1. Increment a KV counter `total_bytes`.
2. If counter > 8 GB:
   - Check KV flag `eviction_in_progress` (TTL 60 s). If set, skip.
   - Otherwise set the flag, `LIST` R2 by prefix, delete oldest until total ≤ 6 GB, reset counter to the recomputed
     ground truth, post an eviction notification to Discord.

KV consistency is loose; R2 deletes are idempotent. Worst case two evictors overlap and both try to delete the same
oldest objects. No harm.

A **daily cron** (`handleDailyEvictionSweep`) recomputes total from R2 ground truth and evicts if > 8 GB. Catches drift
from KV inconsistency or a Worker dying mid-eviction.

A **90-day R2 lifecycle rule** is the third safety layer.

### Long-TTL presigned URLs (no re-mint endpoint)

Discord message embeds a long-TTL presigned R2 GET URL (7-day TTL, R2's max for presigned URLs). Only the maintainer has
access to the Cmdr Discord server, and the `#error-reports` channel is not shared, so the convenience of a
click-to-download link outweighs the theoretical risk of URL leakage. If that changes, flip to short-TTL + admin re-mint
endpoint later (cost: ~50 LOC).

### Reports are anonymous

No license key, no email, no device ID. The short ID `ERR-XXXXX` (5 chars from the same unambiguous alphabet as license
short codes) is the only correlation handle. Users include it in their bug report when they reach out.

## Implementation

### Phase 0: Investigations

No code lands. Outputs feed Phase 2 and Phase 3.

- Confirm what `RotationStrategy` variants the pinned `tauri-plugin-log` version exposes (`KeepSome(N)`?
  `KeepLast(N)`?). Read `~/.cargo/registry/.../tauri-plugin-log-*/src/lib.rs` against `Cargo.lock`. Decide: native
  keep-N or custom pruner.
- Provision `cmdr-error-reports` R2 bucket + `ERROR_REPORT_META` KV namespace. Add to `scripts/setup-cf-infra.sh` so
  it's reproducible.
- Decide auth model for the Discord deep link (re-mint endpoint vs long-TTL; recommend re-mint, see decisions above).
- Decide whether eviction notifications go to a separate Discord channel (`#cmdr-ops`?) or share `#error-reports`.
  Default: share.

### Phase 1: Redaction module + crash reporter migration

Land the redactor _before_ flipping logs to DEBUG, so the corpus is proven before PII volume rises.

**Files:**

- New: `apps/desktop/src-tauri/src/redact/mod.rs`, `tests.rs`, `CLAUDE.md`.
- New: `apps/desktop/src-tauri/tests/fixtures/log-corpus.txt`, `tests/fixtures/log-corpus.redacted.txt`.
- Modified: `apps/desktop/src-tauri/src/lib.rs` (add `mod redact;`).
- Modified: `apps/desktop/src-tauri/src/crash_reporter/mod.rs` (delegate `sanitize_panic_message` to
  `redact::redact_panic_message`; keep wrapper one cycle for test compat).

**Order:** redactor + tests first, CI green; then migrate crash reporter, CI green; verify a synthesized crash JSON
diffs cleanly between old and new sanitizer.

### Phase 2: Logging config + settings UI

- Modified: `apps/desktop/src-tauri/src/lib.rs`: read `max_log_storage_mb` from settings before building
  `tauri-plugin-log`. If `Some(0)`: drop the `Folder` target. Else: file level → DEBUG, rotation → keep-N.
- New: `apps/desktop/src-tauri/src/logging/mod.rs`: pruner task (10-min tick + startup), `list_recent_log_files` helper
  (used by Phase 4 bundle builder), resolved log dir via `OnceLock<PathBuf>`.
- New setting `advanced.maxLogStorageMb` in `apps/desktop/src/lib/settings/settings-registry.ts` (integer, default 200,
  min 0, max 5000). Section: Advanced. Description notes that 0 = "disables log storage; error reports cannot be sent"
  and that hard-disable / re-enable requires restart (in-RAM cap updates live; rotation strategy doesn't).
- Render in `AdvancedSection.svelte` via `SettingNumberInput` with "MB" suffix.
- Wire applier in `settings-applier.ts` → new Tauri command `set_max_log_storage_mb` in `commands/settings.rs`.
- Update `apps/desktop/src-tauri/src/settings/loader.rs` with the new field.

### Phase 3: Server side

In `apps/api-server/`:

- New: `src/error-report.ts`: `POST /error-report` route (multipart: `bundle` zip + `meta` JSON), validates ≤10 MB,
  generates short ID with R2 HEAD collision check (3 retries), writes to R2, returns `{ id }` immediately, schedules
  KV/eviction/Discord work via `c.executionCtx.waitUntil(...)`.
- New: `src/error-report-eviction.ts`: `incrementTotalBytes`, `tryEvict`, `recomputeTotal`. Pure functions; importable
  from cron.
- New: `src/discord.ts`: `postErrorReportNotification`, `postEvictionNotification`. Single retry on 429 `Retry-After`,
  then `console.error` and drop.
- Modified: `src/admin.ts`: add `GET /admin/error-report/:id/url` (re-mint 6-hour presigned URL).
- Modified: `src/scheduled.ts`: add `handleDailyEvictionSweep(env)`.
- Modified: `src/index.ts`: mount the new route, register the new cron job.
- Modified: `src/types.ts`: extend `Bindings` with `ERROR_REPORTS_BUCKET: R2Bucket`, `ERROR_REPORT_META: KVNamespace`,
  `DISCORD_WEBHOOK_URL: string`.
- Modified: `wrangler.toml`: `[[r2_buckets]]` + `[[kv_namespaces]]` bindings.
- Modified: `scripts/setup-cf-infra.sh`: provision bucket, KV namespace, R2 90-day lifecycle rule.
- New tests: `error-report.test.ts`, `error-report-eviction.test.ts`, `discord.test.ts`. Extend `scheduled.test.ts`.
- Discord webhook how-to is documented in `apps/api-server/CLAUDE.md` § Discord webhooks. Set the secret with:
  ```sh
  pnpm --filter @cmdr/api-server exec wrangler secret put DISCORD_WEBHOOK_URL
  ```
  (also for `--env staging` if separate envs exist by then).

### Phase 4: Flow A (user-initiated)

- New: `apps/desktop/src-tauri/src/error_reporter/mod.rs` (+ `tests.rs`, `CLAUDE.md`): `build_bundle`,
  `generate_short_id`, `upload`, `cap_bundle_to_mb`. Bundle: `manifest.json` + `logs/cmdr.log` + rotated siblings,
  redacted line-by-line. Skip upload in dev/CI (mirror crash reporter logic).
- New: `apps/desktop/src-tauri/src/commands/error_reporter.rs`: `prepare_error_report_preview`,
  `send_prepared_error_report`. Two commands so the preview is deterministic without shipping MB through IPC twice.
- New: `apps/desktop/src/lib/error-reporter/`: `ErrorReportDialog.svelte` (preview), `ErrorReportToastContent.svelte`,
  `error-report-flow.ts` (single entry point used by both menu and toast button), `CLAUDE.md`.
- New: `apps/desktop/src/lib/tauri-commands/error-reporter.ts`: typed wrappers + `PreviewPayload` interface.
- Modified: command registry / dispatch: add `help.sendErrorReport`, route to `openErrorReportDialog()`.
- Modified: `apps/desktop/src-tauri/src/menu/`: add "Send error report…" under Help menu.
- Modified: existing error-toast component: add inline secondary action that calls
  `openErrorReportDialog(toastMessage)`.

### Phase 5: Flow B (auto-send on error)

Builds entirely on Phase 4's bundle builder.

- New: `apps/desktop/src-tauri/src/error_reporter/auto_dispatcher.rs`: debouncer (60 s + ±10 s jitter), bundle build,
  upload, emit Tauri event for the toast.
- New macro `log_error!` in a shared place, used at user-visible error call sites. Migrate incrementally; don't touch
  every existing `log::error!`.
- New setting `updates.errorReports` (boolean, default false) in `settings-registry.ts`, `loader.rs`,
  `settings-applier.ts`, `commands/settings.rs`.
- New: `apps/desktop/src/lib/error-reporter/auto-send-toast.svelte.ts`: listens for the event, renders the toast.

### Phase 6: Documentation pass

One PR. Update:

- `apps/desktop/src/lib/logging/CLAUDE.md` + `docs/tooling/logging.md`: DEBUG default for file target, keep-N pruner,
  200 MB default, 0 = disabled, interaction with verbose toggle.
- `apps/desktop/src-tauri/src/settings/CLAUDE.md` + `apps/desktop/src/lib/settings/CLAUDE.md`: list the two new
  settings.
- `apps/desktop/src-tauri/src/commands/CLAUDE.md`: add `error_reporter.rs` row.
- `apps/api-server/CLAUDE.md`: extend the routes table, the secrets table (already has the row), the cron section, and
  the data-flow block. Already has the Discord webhook how-to.
- `docs/architecture.md`: link the new subsystem.
- `docs/security.md`: privacy posture (consent model A vs opt-in B, redactor's role, retention).
- `CHANGELOG.md`: user-facing entry.

## Decisions (previously open)

1. **Discord deep links: long-TTL (7 days).** Only the maintainer has access to the Discord server; convenience wins.
   Re-mint endpoint deferred.
2. **Eviction notifications: same `#error-reports` channel** as new reports.
3. **User note cap: 100 000 chars.** Soft character counter appears once the note exceeds 50 000 chars so the user sees
   they're writing a lot. Server still enforces the 10 MB total payload cap as the hard limit.
4. **Debug-only "Save bundle to disk" option: ship it.** Gated on `cfg!(debug_assertions)`; writes the bundle to the
   app's data dir with a clear filename so we can inspect what's about to be sent when developing the redactor.

## Cross-cutting reminders

- Tracing target gotcha: `cmdr_lib` not `cmdr`. Redactor negative tests must include `cmdr_lib::network::smb_client`
  shape so we don't treat module paths as filesystem paths.
- No `Co-Authored-By` lines in commits (`.claude/rules/git-conventions.md`).
- Sentence case for all UI strings.
- Frontend log target prefix is `FE:` (set by `batch_fe_logs`). Redactor doesn't need to know; it operates on the
  message body, not the target.
