# Crash reporter

Lightweight, privacy-respecting crash reporting. Captures crash data locally, then offers to send it on next launch.

## Architecture

Two capture paths handle different crash types:

- **Panic hook** (`mod.rs`): Catches Rust `panic!`/`unwrap()`/`expect()` failures. Runs in normal Rust code, so it has
  full stdlib access. Captures `std::backtrace::Backtrace`, sanitized panic message, thread info, and app metadata.
  Writes a JSON crash file to the app data dir.
- **Signal handler** (`mod.rs`): Catches SIGSEGV, SIGBUS, SIGABRT (like stack overflows). Runs in an async-signal-safe
  context, so it can only capture raw instruction pointer addresses and write them as raw bytes to a pre-opened fd.
  Symbolication happens on next launch.

Both paths write to `crash-report.json` in the app data dir (same dir as `settings.json`, resolved by
`resolved_app_data_dir()`).

## Crash file lifecycle

1. App crashes, handler writes `crash-report.json`
2. Next launch: `check_pending_crash_report` finds the file, parses it (defensively, discards if corrupt)
3. If `updates.crashReports` is `true`: auto-send and show a toast
4. Otherwise: show a dialog letting the user inspect and choose to send or dismiss
5. File is deleted after send or dismiss

## Key design decisions

- **Opt-in only** (`updates.crashReports` defaults to `false`). Crash reports carry a debug backtrace, so they stay
  opt-in even though the anonymous beta usage analytics (heartbeat + PostHog events) default on. The two are deliberately
  separate consent gates.
- **No PII, ever.** Panic messages are sanitized via the shared
  [`crate::redact`](../redact/CLAUDE.md) module to strip file paths, hostnames, IPs, emails,
  and URL userinfo before writing. The `sanitize_panic_message` function in `mod.rs` is a
  thin wrapper around `redact::redact_panic_message`. No file names, usernames, device IDs,
  or license keys are included.
- **Dev mode: capture only, never send.** Crash files are written (useful for testing), but the send path is skipped to
  avoid polluting production data.
- **Crash loop protection**: If the crash file's timestamp is less than five seconds before the current launch, skip
  auto-send and show the dialog instead.
- **Radical transparency**: The dialog shows the exact JSON payload before sending.

## What we send

- Full symbolicated backtrace (function names + offsets, not file paths)
- Exception type + signal, faulting address
- App version, macOS version, CPU architecture
- App uptime, thread count
- Sanitized panic message
- Active feature flags (booleans/enums only: `indexing.enabled`, `ai.provider`, `developer.mcpEnabled`,
  `developer.verboseLogging`)
- `buildMode` (`"release"` or `"debug"`, from `cfg!(debug_assertions)`): lets the api server distinguish dev-run
  crashes from production ones in the email summary
- `shortId` (`CRASH-XXXXX`): generated at crash-file-write time via [`crate::short_id::generate("CRASH")`]
  (shared with error reports). Shown to the user in the next-launch dialog so they can reference the report.
- `diagId` (`diag_<uuid>`): the diagnostics id from [`crate::install_id`], so sequential reports from one install group
  together. **Attached at report-assembly time, NEVER in the signal handler.** The panic-hook path reads the cheap
  `OnceLock` snapshot (`install_id::diagnostics_id_snapshot()`, resolved at `install_id::init()`); the signal path is
  async-signal-safe (no alloc, no locks) so it attaches the id at **next-launch assembly** in `process_pending_crash`
  (full stdlib, via `install_id::diagnostics_id()`). **NEVER the `anal_` analytics id**: the two-id split (see
  `analytics/CLAUDE.md` § "Two ids that never meet") keeps a voluntarily-attached email unjoinable to the analytics
  stream. If the analytics id ever rode a report, an attached email could be joined to the install's usage history,
  which is exactly the linkage we promise not to have.
- `email` (optional): a beta tester's contact email, populated **only by the dialog at send time** when the user ticks
  the attach-email box. It's a **send-time** field, not a crash-time one: the crash is written to disk before any email
  is known, and the signal context has no settings access anyway. NEVER read settings or the email in the crash build
  path or the signal handler. The dialog threads it into `send_crash_report(report)`.

## What we never send

- File paths, volume names, environment variables, window titles
- License key, transaction ID, device ID
- Register dump, heap contents

## Files

- **`mod.rs`**: Panic hook, signal handler registration, crash file read/write
- **`symbolicate.rs`**: Next-launch symbolication of raw addresses from signal handler
- **`tests.rs`**: Unit + integration tests for crash file I/O, sanitization, signals

### Commands and frontend (milestone 2)

- **`commands/crash_reporter.rs`**: Tauri commands: check, dismiss, send crash reports
- **`src/lib/tauri-commands/crash-reporter.ts`**: TypeScript wrappers for the three Tauri commands
- **`src/lib/crash-reporter/CrashReportDialog.svelte`**: Dialog: shows report, expandable details, send/dismiss buttons
- **`src/lib/crash-reporter/CrashReportToastContent.svelte`**: Toast content for auto-sent crash reports

The startup flow in `(main)/+layout.svelte` calls `checkPendingCrashReport` after settings are loaded. If
`updates.crashReports` is true and it's not a crash loop, it auto-sends and shows a toast. Otherwise, it shows the
dialog.

## Gotchas

- `unwrap()` on `io::Error` embeds the file path in the panic message. The sanitizer must strip path-like patterns
  (`/Users/...`, `C:\...`, home dir prefixes) before writing.
- The signal handler's pre-opened fd becomes stale if the app data dir is deleted while the app runs. This is acceptable
  since it requires deliberate user action and only loses one crash report.
- Raw addresses from the signal handler are only useful for symbolication if the app version hasn't changed between crash
  and relaunch. If versions differ, send raw addresses only (still useful for grouping).
- Signal handler raw addresses are absolute virtual addresses, randomized by ASLR on each launch. True symbolication
  would require storing the binary's image base address in the crash file, which isn't done yet. For now, raw addresses
  are formatted as hex and are useful for grouping identical crash sites across reports.
- The signal handler uses `execinfo.h`'s `backtrace()` which is async-signal-safe on macOS. On Linux, glibc's
  implementation is also safe in practice but not guaranteed by POSIX.

Full details: [DETAILS.md](DETAILS.md).
