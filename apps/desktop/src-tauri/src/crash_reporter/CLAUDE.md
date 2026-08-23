# Crash reporter

Lightweight, privacy-respecting crash reporting. A crash that kills the app is offered on the next launch; a panic the
app SURVIVES also goes out in the same session, via the error reporter's Flow B. Everything else: `error_reporter/`.

## Module map

- **`mod.rs`**: hook install + body, crash file read/write, crash-loop detection. **`panic_courier.rs`**: in-session
  delivery of a survived panic, off the hook's thread. **`signal_handler.rs`**: the async-signal-safe SIGSEGV/SIGBUS/
  SIGABRT path and its raw file format. **`symbolicate.rs`**: next-launch symbolication of its addresses.
- **`tests.rs`** / **`panic_courier_tests.rs`**: crash file I/O, sanitization, signals; courier panic-safety.
- IPC in `commands/crash_reporter.rs` (`check_pending_crash_report`, dismiss, send). Frontend in `src/lib/crash-reporter/`
  (`CrashReportDialog.svelte`, `CrashReportToastContent.svelte`); `(main)/+layout.svelte` calls
  `checkPendingCrashReport` after settings load.

Two capture paths write `crash-report.json` in the app data dir (`resolved_app_data_dir()`): the **panic hook** (full
stdlib: `Backtrace`, sanitized message, thread + app metadata) and the **signal handler** for SIGSEGV/SIGBUS/SIGABRT
(async-signal-safe: raw addresses to a pre-opened fd, symbolicated on next launch).

## Must-knows (invariants and guardrails)

- **Opt-in only.** `updates.crashReports` defaults to `false`, since a crash report carries a debug backtrace. It's a
  separate consent gate from the anonymous beta analytics (heartbeat + PostHog), which default on.
- **No PII, ever.** Panic messages go through `sanitize_panic_message` (shared
  [`crate::redact`](../redact/CLAUDE.md), then a 2,000-char cap) before writing; never route one to disk or the network
  on a path that skips it. The cap counts chars, not bytes: a byte-index cut would panic inside the panic hook and lose
  the report. Don't add file paths, usernames, device ids, license keys, env vars, window titles, or register/heap
  contents to the payload.
- **`system_snapshot` is attached in `process_pending_crash` at next launch, NEVER in the hook or signal handler**
  (compromised context: no sysctl/sysinfo/shell-outs). Always the stable form (`live: None`), since live values would
  describe the fresh process. PII-free by construction; `../diagnostics_snapshot.rs`.
- **Attach the diagnostics id (`diag_`), NEVER the analytics id (`anal_`)**: the two-id split (`analytics/CLAUDE.md`
  § "Two ids that never meet") keeps a voluntarily-attached email unjoinable to usage history. At assembly time only,
  never in the signal handler: the panic path reads `install_id::diagnostics_id_snapshot()`, the signal path gets it in
  `process_pending_crash`.
- **`email` is a send-time field**, set only by the dialog when the user ticks the attach box (the crash hits disk
  before any email is known). NEVER read settings or the email in the crash-write path or the signal handler.
- **Dev mode: capture only, never send.** Files are written (handy for testing); the send path is skipped so dev runs
  don't pollute production data.
- **Crash-loop guard.** A crash file less than `CRASH_LOOP_THRESHOLD_SECS` (5 s) old sets `possible_crash_loop`, and
  the frontend shows the dialog instead of auto-sending.
- ❌ **Nothing in the panic hook may be able to panic**, and `catch_unwind` there can't help (a panic inside a hook
  aborts before unwinding). Hence the courier thread; DETAILS § Two delivery paths.
- **The hook installs in `run()`, ahead of every fallible step**; `init` only hands it a path, so an unresolvable data
  dir costs the crash FILE, never the hook. **Keep-first**: the session's first panic writes the file, later ones don't
  clobber it and carry no short id in-session.

## Gotchas

- **`unwrap()` on `io::Error` embeds the file path in the panic message**, so the sanitizer strips path-like patterns
  (`/Users/...`, `C:\...`, home prefixes) before writing.
- **ASLR-randomized addresses mean nothing without the `image_base` shipped with them.** Resolve it at `install()`
  into an atomic; **never** call dyld from the handler. The signal path reports the base from the RAW FILE, never the
  relaunched process's. DETAILS § Image base.

Delivery paths, the crash-file lifecycle, and the exact "what we send / never send" catalog: `DETAILS.md`.
