# Crash reporter

A crash that kills the app is offered on the next launch; a panic the app SURVIVES goes out in-session via the error
reporter's Flow B. Everything else: `error_reporter/`.

## Module map

- **`mod.rs`**: hook install + body, crash file I/O, crash-loop detection, `AppFate` resolution. **`contain.rs`**: the one exemption. **`panic_courier.rs`**: in-session delivery of a survived panic, off the hook's thread.
  **`survival.rs`**: the amendments only the live process can make. **`signal_handler.rs`**: the async-signal-safe SIGSEGV/SIGBUS/SIGABRT path and its raw file format.
  **`symbolicate.rs`**: next-launch symbolication of its addresses. Tests in `*_tests.rs` siblings.
- IPC: `commands/crash_reporter.rs`. Frontend: `src/lib/crash-reporter/`, driven from `(main)/+layout.svelte` after
  settings load.

Both paths write `crash-report.json` in the app data dir: the hook with full stdlib, the handler async-signal-safe (raw
addresses to a pre-opened fd).

## Must-knows (invariants and guardrails)

- **On by default.** `updates.crashReports` defaults to `true` (narrow, stack-shaped, sanitized). ❌ Never extend that
  default to `updates.errorReports` (an unbounded log bundle) or `updates.attachEmailToReports` (an identity).
  `$lib/crash-reporter/DETAILS.md` § The three report consents.
- **No PII, ever.** Panic messages go through `sanitize_panic_message` (shared
  [`crate::redact`](../redact/CLAUDE.md), then a 2,000-char cap) before writing; never route one to disk or the network
  on a path that skips it. The cap counts chars, not bytes: a byte-index cut would panic inside the hook. No paths,
  usernames, device ids, license keys, env vars, titles, or register/heap contents.
- **`system_snapshot` is attached in `process_pending_crash` at next launch, NEVER in the hook or signal handler**
  (compromised context). Always the stable form (`live: None`): live values would describe the fresh process.
  PII-free by construction; `../diagnostics_snapshot.rs`.
- **Attach the diagnostics id (`diag_`), NEVER the analytics id (`anal_`)**: the split (`analytics/CLAUDE.md` § "Two
  ids that never meet") keeps a voluntarily-attached email unjoinable to usage history.
- **`email` is a send-time field**, set only by the dialog's attach box (the crash hits disk before any email is
  known). NEVER read settings or the email in the crash-write path or the handler.
- **Dev mode: capture only, never send.**
- **Crash-loop guard.** A crash file under `CRASH_LOOP_THRESHOLD_SECS` (5 s) old sets `possible_crash_loop`, and the
  frontend asks instead of auto-sending.
- **Two amendments `survival.rs` makes to a pending report, both one-way.** `app_fate`: the hook writes `Unconfirmed`,
  survival upgrades it to `KeptRunning`, `process_pending_crash` resolves the rest to `Ended`. ❌ Never a `bool` — a
  `false` default would claim "the app quit" about every older crash file. `reported_in_session`: means DELIVERED, so
  ❌ stamp it only from `auto_dispatcher::flush`'s successful `upload`, and a stamped report is deleted rather than
  offered. DETAILS §§ App fate, Told once.
- ❌ **Nothing in the panic hook may be able to panic**, and `catch_unwind` there can't help (a panic inside a hook
  aborts before unwinding). Hence the courier thread; DETAILS § Two delivery paths.
- **`contain_panics(|| …)` is the ONE reporting exemption** (a foreign parser that panics on untrusted input): a
  caught panic is one `warn` line, no crash file, no courier. ❌ Wrap only the foreign calls. DETAILS § The one exemption.
- **The hook installs in `run()`, ahead of every fallible step**; `init` only hands it a path, so an unresolvable data
  dir costs the crash FILE, never the hook. **Keep-first**: the session's first panic writes the file.

## Gotchas

- **`unwrap()` on `io::Error` embeds the file path in the panic message**, so the sanitizer strips path-like patterns
  (`/Users/...`, `C:\...`, home prefixes).
- **ASLR-randomized addresses mean nothing without the `image_base` shipped with them.** Resolve it at `install()`
  into an atomic; **never** call dyld from the handler. DETAILS § Image base.

Delivery paths, the crash-file lifecycle, and the exact "what we send / never send" catalog: `DETAILS.md`.
