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

- **Opt-in only** (`updates.crashReports` defaults to `false`). Consistent with the "no telemetry" stance.
- **No PII, ever.** Panic messages are sanitized to strip file paths before writing. No file names, usernames, device
  IDs, or license keys are included.
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

## What we never send

- File paths, volume names, environment variables, window titles
- License key, transaction ID, device ID
- Register dump, heap contents

## Files

| File               | Purpose                                                              |
| ------------------ | -------------------------------------------------------------------- |
| `mod.rs`           | Panic hook, signal handler registration, crash file read/write       |
| `symbolicate.rs`   | Next-launch symbolication of raw addresses from signal handler       |
| `tests.rs`         | Unit + integration tests for crash file I/O, sanitization, signals   |

### Commands and frontend (milestone 2)

| File                                               | Purpose                                                          |
| -------------------------------------------------- | ---------------------------------------------------------------- |
| `commands/crash_reporter.rs`                       | Tauri commands: check, dismiss, send crash reports               |
| `src/lib/tauri-commands/crash-reporter.ts`         | TypeScript wrappers for the three Tauri commands                 |
| `src/lib/crash-reporter/CrashReportDialog.svelte`  | Dialog: shows report, expandable details, send/dismiss buttons   |
| `src/lib/crash-reporter/CrashReportToastContent.svelte` | Toast content for auto-sent crash reports                   |

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
