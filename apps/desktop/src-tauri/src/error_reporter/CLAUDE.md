# Error reporter

Builds a privacy-redacted zip bundle of recent log files plus a JSON manifest, then (in
prod) ships it to `POST /error-report` on the api server. Used by the user-initiated
"Send error report" flow in Phase 4 and the auto-send flow in Phase 5.

## Convention

**Use `log_error!` at all error-level sites in the desktop crate.** If a failure is
recoverable, expected, or not user-impacting, downgrade to `log::warn!` — don't reach
for `log::error!` to dodge the dispatcher. The error-level threshold IS the auto-report
threshold. The `scripts/check.sh` `log-error-macro` check enforces this and will fail
on any new raw `log::error!` site outside the macro definition itself.

## What we send

Bundle layout:

```
manifest.json          # see BundleManifest
logs/<filename>        # one entry per recent log file in the log dir
logs/<next-filename>   # ...
```

Manifest fields (`BundleManifest`):

- `id`: short ID (`ERR-XXXXX`) generated client-side. **Server may regenerate** — UI
  always shows the server's response ID, never the local one.
- `kind`: `"user"` or `"auto"` (Phase 5).
- `appVersion`, `osVersion`, `arch`: build/platform identifiers.
- `activeSettings`: settings snapshot via the `ResolvedSettings` struct — every field
  resolved to its effective value (`null` is never shipped). Includes `indexingEnabled`,
  `aiProvider`, `mcpEnabled`, `mcpPort`, `verboseLogging`, `maxLogStorageMb`,
  `errorReportsEnabled`, `crashReportsEnabled`. Defaults are duplicated from
  `apps/desktop/src/lib/settings/settings-registry.ts` — see `ResolvedSettings::from_settings`.
- `logLevels`: `LogLevelSnapshot` with `stdoutDefault` (startup level), `stdoutCurrent`
  (live atomic), `fileChain` (always `"debug"`), and `stdoutModuleOverrides` (noise
  suppression + `RUST_LOG` directives in insertion order). Lets a triager tell whether
  the absence of a debug line means "didn't happen" or "filtered out."
- `lastUserAction` (optional): last command dispatched through
  `handleCommandExecute` (`{commandId, at}`). Populated by the FE via
  `record_user_action`; `None` if the user hadn't acted yet at error time.
- `userNote` (optional): user-supplied free text. Trimmed; capped at 100 000 chars by the
  Tauri command layer.
- `generatedAt`: ISO 8601 UTC timestamp.

Distinct from `crash_reporter::ActiveSettings`: that struct is the on-disk crash file
format and stays `Option<bool>`-shaped for backward compatibility with crash files
written by older app versions. Manifests are built fresh per bundle and don't have
that constraint, so the resolved shape lives in `error_reporter::ResolvedSettings`.

Every log line is run through [`crate::redact::redact_line`](../redact/CLAUDE.md) before
it hits the zip. The redactor handles file paths, hostnames, IPs, emails, URL userinfo,
SMB URIs, and UNC paths. See the redact module for the full pattern table.

## What we never send

- License keys, transaction IDs, device IDs.
- Raw file paths, volume names, SMB credentials.
- Settings registry content beyond the four feature flags above.
- Anything outside the log dir — no app data files, no settings.json.

## Files

| File                       | Purpose                                                                                                       |
| -------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `mod.rs`                   | `build_bundle`, `build_zip`, `generate_short_id`, `upload`, `cap_bundle_to_mb`, `save_bundle_to_disk` (debug); also exports the `log_error!` macro |
| `tests.rs`                 | Unit tests: zip structure, redaction, ID format/uniqueness, capping                                           |
| `auto_dispatcher.rs`       | Flow B: opt-in auto-send on user-visible errors (60 s ± 10 s debounce, 1 MB tail, no retry on failure)        |
| `auto_dispatcher_tests.rs` | Unit tests: debounce, opt-in flag, first-call wins, jitter band, crash-loop interaction                       |

## Two-command frontend split (rationale)

The Tauri commands layer exposes **two** commands:

- `prepare_error_report_preview` returns the manifest + first/last sample lines + total
  size. Used by the dialog to render the preview.
- `send_error_report` re-builds the bundle and uploads it.

Why two commands instead of building once and caching across IPC: the bundle is megabytes
of compressed bytes. Holding it in a Tauri-side `OnceLock` between IPC calls would couple
state across two unrelated commands and risk leaking memory if the user dismisses the
dialog. Re-building is cheap (the heavy work is reading + redacting log lines, which
runs on the blocking pool either way), and the inputs are deterministic enough that the
preview hash matches what'll be uploaded.

## Dev/CI bypass

[`upload`] checks `cfg!(debug_assertions) || std::env::var("CI").is_ok()`. In either
case it returns the locally-generated ID without calling the network. Mirrors the crash
reporter's bypass — same reasoning (no production data pollution from dev runs).

The dialog has an extra "Save bundle to disk (debug)" button in dev that calls
`save_error_report_to_disk` instead, writing the zip to the app data dir for inspection.

## Bundle scope and cap

`build_bundle` takes a `BundleScope`:

- `BundleScope::Last24Hours` — Flow A. Files whose mtime falls outside the last 24 h are
  skipped entirely. Capped at 1 MB compressed via `cap_bundle_to_mb`. (Lowered from 10 MB
  after QA: 10 MB compressed = ~190 MB uncompressed, way more than triage needs. The
  server still enforces its own 10 MB ceiling — we apply the smaller client-side cap so
  the user's upload is fast and the bundle's tail stays useful.)
- `BundleScope::Window { first_error_at }` — Flow B. The window is
  `[first_error_at - 30 min, now]`. Files whose mtime is older than the lower bound are
  skipped; surviving files are line-filtered by parsing the leading ISO-8601 timestamp
  the file chain writes (see `logging::dispatch::file_timestamp`). Capped at 1 MB.

`cap_bundle_to_mb` trims log content from the **head** of the newest file (line by line)
rather than dropping whole files. Always preserves `manifest.json` verbatim and the last
50 lines of the newest file (even if it pushes the cap by ~10%) — better to ship 1.1 MB
of useful tail than 0 useful lines, which is what the pre-fix-6 implementation did.

## Flow B (auto-send on error)

Opt-in via the `updates.errorReports` setting (default off — Flow B sends data without
per-event consent, so the consent has to be up front). When enabled:

1. The `log_error!` macro routes select call sites through
   `auto_dispatcher::on_error_logged(category, message)` in addition to the normal
   `log::error!` emit.
2. The first error in a 60 s window captures `(category, first_message, error_count = 1)`
   and schedules a flush at `now + 60 s ± 10 s of jitter`. Subsequent errors in the same
   window only bump the counter — the first-call metadata is kept verbatim.
3. When the timer fires: build a bundle (`BundleKind::Auto`, user note carries the count
   + first-error preview), trim to a 1 MB tail via `cap_bundle_to_mb`, upload, emit
   `error-report-auto-sent` with the server-issued ID. The frontend listens for that
   event and shows a confirmation toast (see `apps/desktop/src/lib/error-reporter/`).

### Why jitter?

Without jitter, a global outage (DNS, an upstream API) triggers thousands of users to
auto-send at the same `now + 60 s` instant. The ±10 s uniform spread costs nothing on
the client and smears the load over a 20 s window server-side.

### Why no retry on upload failure?

We're already debounced at 60 s. If the network's flaky, the user will hit other errors
soon and the next debounce window will fire normally. Retrying inside a single dispatch
risks flooding the server during real outages, and the user still has Flow A as a manual
safety net.

### Crash-loop interaction (read this!)

If the app exits inside the 60 s debounce window — for example, during a panic — the
spawned flush task is dropped before it fires. **The auto-dispatcher does not flush on
shutdown**, by design.

This is fine because:

- **Panics** route through `crash_reporter`, which writes a JSON file synchronously and
  uploads it on the next launch. That covers the "app died" case end-to-end.
- **Soft errors** that don't kill the app are exactly what the auto-dispatcher exists
  for, and the next `log_error!` call after the next launch will start a fresh window.

If a future scenario shows we're losing important reports here, the simplest fix is to
add a debug-only "flush now" command and let panic hooks call it. Don't add a queue or
on-disk persistence layer — the manual flow is the safety net (matches the FE log
bridge's `beforeunload` semantics: best-effort, no durability guarantees).

### `log_error!` convention

Use `log_error!` instead of `log::error!` at user-visible failure sites — anything that
already produces a user toast or that an end user would describe as "this didn't work."
Skip noisy library-level errors (`smb2`, `nusb`, etc.); the goal is signal, not coverage.

The macro forwards to `log::error!` unconditionally, then calls
`auto_dispatcher::on_error_logged(target, message)` which bails out on a single atomic
load when the opt-in flag is off.

The current set of migrated call sites is small and deliberate; expand it as we discover
new user-visible errors. Do not bulk-migrate.

### Backtrace capture

Every `log_error!` call captures a backtrace via `Backtrace::force_capture()` and emits
it as a **separate debug-level record** under `cmdr_lib::error_reporter::backtrace`.
The fern dispatch tree pins the file chain at Debug regardless of `RUST_LOG`/verbose, so
the backtrace always lands in the log file (and therefore in error report bundles). The
stdout chain's Info default drops it on the floor, keeping the terminal clean. The
error-level message stays a single readable line — pre-fix-* code emitted backtrace as
continuation lines on the error record itself, which spammed the terminal even when no
report was being built. The bundle's per-line window-trim passes through lines without a
parseable leading ISO-8601 timestamp, so the backtrace continuation lines survive into
Flow B reports intact. The redactor scrubs build-machine paths embedded in the symbol
metadata via the same `redact_line` pass every other log line gets.

The auto-dispatcher's `first_message` (which becomes the manifest's `userNote`) sees
only the user-supplied message — the trace stays in the log file. So bundle manifests
stay terse, and triage gets the call site without us having to wire stack-capture into
each error site individually.

`force_capture` ignores `RUST_BACKTRACE`. This is intentional — error report bundles
need stack context regardless of the user's env.

### State snapshot at error time

`auto_dispatcher::on_error_logged` (called from every `log_error!`) also reads the
`cmdr://state` MCP resource and emits the YAML as a debug-level record under
`cmdr_lib::error_reporter::state_snapshot`. Throttled to one per 30 s so an error storm
doesn't fill the file. **Always runs** (regardless of the Flow B opt-in) so manual
"Send error report" bundles built minutes after a failure still have a snapshot.
File-only via the same per-output filtering as the backtrace.

### AppHandle wiring

The macro can't thread an `AppHandle` through every call site, so
`auto_dispatcher::set_app_handle(handle)` stashes one in a `OnceLock` at app startup
(called from `lib.rs::setup` right after `crash_reporter::init`). If an error fires
before the handle is wired, the debounce window opens normally but the flush task isn't
spawned (no handle to hand to `tauri::async_runtime::spawn`). The state carries a
`flush_spawned` flag for exactly this reason: when `set_app_handle` runs later, it picks
up the orphaned window and spawns the flush task with the remaining time. If the
deadline has already passed, the spawned task fires immediately. The `mark_flush_spawned`
helper plus the late-arrival path in `set_app_handle` race against each other safely —
the loser just bails.

## Gotchas

- The cached `ActiveSettings` snapshot is built lazily on the first `build_bundle` call,
  not at app startup. Settings that change after the first bundle won't appear in
  subsequent reports until restart. This matches the crash reporter's behavior — the
  whole point is to capture the state the user was in when the failure happened.
- `build_zip` uses a `BTreeMap` keyed by filename for deterministic ordering. The live
  `cmdr.log` sorts before rotated `cmdr.log.1`/`.2`/... siblings because `.` < any
  digit, so iterating ascending gives newest-first for the log files themselves.
- Per-entry mtimes are set explicitly (manifest = `now`, logs = source-file mtime).
  Without this, the `zip` crate's `SimpleFileOptions::default()` writes 1980-01-01 for
  every entry — extracted bundles look like ancient archives.
- Server-side ID generation may differ from the client's local ID. Always trust the
  `id` field in the upload response — that's the one the user reports back to us.
- `BundleScope::Window` line-trimming relies on the file chain's ISO-8601 timestamp
  format. Lines without a parseable leading timestamp pass through (the alternative —
  drop them — risks losing useful context). Pre-fix-3 logs that started with
  `HH:MM:SS.mmm` will NOT trim by line, only by file mtime. New logs (post fix #3)
  trim cleanly.
