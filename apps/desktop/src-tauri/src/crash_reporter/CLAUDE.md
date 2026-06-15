# Crash reporter

Lightweight, privacy-respecting crash reporting. Captures crash data locally, then offers to send it on next launch.
For unexpected aborts only; everything else goes through the error reporter (`error_reporter/`).

## Module map

- **`mod.rs`**: panic hook + signal handler registration, crash file read/write, crash-loop detection.
- **`symbolicate.rs`**: next-launch symbolication of raw addresses from the signal handler.
- **`tests.rs`**: crash file I/O, sanitization, signals.
- IPC in `commands/crash_reporter.rs` (`check_pending_crash_report`, dismiss, send). Frontend in `src/lib/crash-reporter/`
  (`CrashReportDialog.svelte`, `CrashReportToastContent.svelte`); `(main)/+layout.svelte` calls
  `checkPendingCrashReport` after settings load.

Two capture paths write to `crash-report.json` in the app data dir (alongside `settings.json`, via
`resolved_app_data_dir()`): the **panic hook** (full stdlib, captures `Backtrace`, sanitized message, thread + app
metadata) and the **signal handler** for SIGSEGV/SIGBUS/SIGABRT (async-signal-safe, writes raw instruction-pointer
bytes to a pre-opened fd; symbolicated on next launch).

## Must-knows (invariants and guardrails)

- **Opt-in only.** `updates.crashReports` defaults to `false` because crash reports carry a debug backtrace. This stays
  a separate consent gate from the anonymous beta analytics (heartbeat + PostHog), which default on.
- **No PII, ever.** Panic messages are sanitized via the shared [`crate::redact`](../redact/CLAUDE.md) module before
  writing (`sanitize_panic_message` in `mod.rs` is a thin wrapper around `redact::redact_panic_message`). Don't add file
  paths, usernames, device ids, license keys, env vars, window titles, or register/heap contents to the payload.
- **Attach the diagnostics id (`diag_`), NEVER the analytics id (`anal_`).** The two-id split (see `analytics/CLAUDE.md`
  § "Two ids that never meet") keeps a voluntarily-attached email unjoinable to the analytics stream; if `anal_` ever
  rode a report, an attached email could be joined to usage history. The `diag_id` is attached at report-assembly time,
  never in the signal handler: the panic path reads the cheap `OnceLock` snapshot
  (`install_id::diagnostics_id_snapshot()`); the signal path is async-signal-safe (no alloc, no locks), so it attaches
  the id at next-launch assembly in `process_pending_crash` via `install_id::diagnostics_id()`.
- **`email` is a send-time field, populated only by the dialog when the user ticks the attach box.** The crash is
  written to disk before any email is known, and the signal context has no settings access. NEVER read settings or the
  email in the crash-write path or the signal handler.
- **Dev mode: capture only, never send.** Crash files are written (useful for testing) but the send path is skipped to
  avoid polluting production data.
- **Crash-loop guard.** If the crash file is less than `CRASH_LOOP_THRESHOLD_SECS` (5 s) before the current launch,
  `is_crash_loop` sets `possible_crash_loop` and the frontend shows the dialog instead of auto-sending.

## Gotchas

- **`unwrap()` on `io::Error` embeds the file path in the panic message**, so the sanitizer must strip path-like
  patterns (`/Users/...`, `C:\...`, home prefixes) before writing.
- **The signal handler's pre-opened fd goes stale if the data dir is deleted while running.** Acceptable: it requires
  deliberate user action and loses at most one report.
- **Signal-handler raw addresses are absolute virtual addresses randomized by ASLR per launch**, and only useful for
  symbolication if the app version hasn't changed between crash and relaunch. When versions differ, raw addresses are
  still sent (formatted as hex) for grouping. True symbolication would need the binary image base in the crash file,
  which isn't stored yet.
- **Signal-safety of `backtrace()`**: `execinfo.h`'s `backtrace()` is async-signal-safe on macOS; on Linux glibc's is
  safe in practice but not POSIX-guaranteed.

The full crash-file lifecycle and the exact "what we send / never send" payload catalog are in [DETAILS.md](DETAILS.md).
