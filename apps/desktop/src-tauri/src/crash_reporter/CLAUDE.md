# Crash reporter

A crash that kills the app is offered on the next launch; a panic the app SURVIVES goes out in-session via the error
reporter's Flow B. Everything else: `error_reporter/`.

## Module map

- **`mod.rs`** (hook install, crash file I/O, the `CrashReport` shape), **`next_launch.rs`** (reading the previous
  session's evidence and assembling a report: fate, snapshot, crash-loop, the macOS extract),
  **`contain.rs`** (the one exemption),
  **`panic_courier.rs`** (in-session delivery of a survived panic), **`survival.rs`** (amendments only a live process
  can make), **`signal_handler.rs`** (the async-signal-safe path + raw file format), **`symbolicate.rs`** (its
  addresses), **`os_crash_report.rs`** (macOS's own exception line and stack). Tests in `*_tests.rs` siblings.
- IPC: `commands/crash_reporter.rs`. Frontend: `src/lib/crash-reporter/`, from `(main)/+layout.svelte` after settings
  load.

Both paths write `crash-report.json` in the app data dir: the hook with full stdlib, the handler async-signal-safe.

## Must-knows (invariants and guardrails)

- **On by default.** `updates.crashReports` is `true` (narrow, stack-shaped, sanitized). ❌ Never extend that default
  to `updates.errorReports` (an unbounded log bundle) or `updates.attachEmailToReports` (an identity).
  `$lib/crash-reporter/DETAILS.md` § The three report consents.
- **No PII, ever.** Panic messages go through `sanitize_panic_message` ([`crate::redact`](../redact/CLAUDE.md), then
  a 2,000-CHAR cap; a byte-index cut would panic inside the hook). ❌ Never route one to disk or the network on a path
  that skips it. No paths, usernames, device ids, license keys, env vars, titles, or register/heap contents.
- **`system_snapshot` and the macOS crash-report extract attach in `process_pending_crash` at next launch, NEVER in
  the hook or signal handler** (compromised context). The snapshot is always stable-form (`live: None`), since live
  values would describe the fresh process. `../diagnostics_snapshot.rs`.
- **Attach the diagnostics id (`diag_`), NEVER the analytics id (`anal_`)**: that split (`analytics/CLAUDE.md` § "Two
  ids that never meet") keeps a voluntarily-attached email unjoinable to usage history.
- **`email` is a send-time field**, set only by the dialog's attach box. ❌ Never read settings or the email in the
  crash-write path or the handler.
- **Dev mode: capture only, never send.** **Crash-loop guard**: a crash file under 5 s old sets `possible_crash_loop`,
  and the frontend asks instead of auto-sending.
- **Two one-way amendments `survival.rs` makes**, both DETAILS §§ App fate, Told once. ❌ `app_fate` is never a
  `bool`: a `false` default would claim "the app quit" about every older file. ❌ `reported_in_session` means
  DELIVERED, so stamp it only from `auto_dispatcher::flush`'s successful `upload`.
- ❌ **Nothing in the panic hook may be able to panic**, and `catch_unwind` can't help (a panic inside a hook aborts
  before unwinding). Hence the courier thread; DETAILS § Two delivery paths.
- **`contain_panics(|| …)` is the ONE reporting exemption** (a foreign parser that panics on untrusted input): one
  `warn` line, no crash file, no courier. ❌ Wrap only the foreign calls. DETAILS § The one exemption.
- **The hook installs in `run()` ahead of every fallible step**, so an unresolvable data dir costs the crash FILE, never
  the hook. **Keep-first**: the session's first panic writes the file.

## Gotchas

- **`unwrap()` on `io::Error` embeds the file path in the panic message**, so the sanitizer strips path-like patterns.
- **ASLR-randomized addresses mean nothing without the `image_base` shipped with them.** Resolve it at `install()` into
  an atomic; ❌ never call dyld from the handler. DETAILS § Image base.
- **What we lift from macOS's own crash report is an ALLOWLIST** (exception + the faulting thread's frames), so a
  future macOS field can't ship itself. ❌ Never widen it to `crashReporterKey`, `bootSessionUUID`, or the
  `parentProc` / `coalitionName` family: they name the machine and other apps, and the redactor passes them straight
  through. DETAILS § macOS crash reports.

Delivery paths, the crash-file lifecycle, the macOS-report allowlist, and the "what we send / never send" catalog:
`DETAILS.md`.
