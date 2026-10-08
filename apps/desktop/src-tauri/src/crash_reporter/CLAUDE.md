# Crash reporter

A crash that kills the app is offered on the next launch; a panic the app SURVIVES goes out in-session via the error
reporter's Flow B. Everything else: `error_reporter/`.

## Module map

- **`mod.rs`**: report type, privacy transform, hook, and file I/O. **`next_launch.rs`**: previous-session assembly.
  **`pending_delivery.rs`**: authoritative pending-file send and dismissal. **`contain.rs`**: parser exemption.
  **`panic_courier.rs`** / **`survival.rs`**: survived-panic delivery and amendments. **`signal_handler.rs`** /
  **`symbolicate.rs`** / **`os_crash_report.rs`**: native crash evidence. Tests are siblings.
- IPC: `commands/crash_reporter.rs`. Frontend: `src/lib/crash-reporter/`.

Both paths write `crash-report.json` in the app data dir: the hook with full stdlib, the handler async-signal-safe.

## Must-knows (invariants and guardrails)

- **On by default.** `updates.crashReports` is `true`. ❌ Never extend that default
  to `updates.errorReports` (an unbounded log bundle) or `updates.attachEmailToReports` (an identity).
  `$lib/crash-reporter/DETAILS.md` § The three report consents.
- **The pending file is authoritative at send.** Send takes only the preview's `short_id` plus optional email, reloads
  the file, checks the id, and reapplies `prepare_for_delivery` (typed-field validation, report-scoped redaction and
  caps on every string, panic message and thread name included).
- **`system_snapshot` and the macOS extract attach in `process_pending_crash`, NEVER in the compromised hook or
  handler.** The snapshot is stable-only (`live: None`), since live values describe the new process.
- **Attach the diagnostics id (`diag_`), NEVER the analytics id (`anal_`)** (`analytics/CLAUDE.md` § "Two ids that
  never meet").
- **`email` is a typed send-time exception.** Delivery preparation clears embedded email; only
  `AttachedEmail::from_flow_a_dialog` can add it back after an explicit dialog action. ❌ Never read settings or the
  email in capture or automatic-send paths.
- **The report id binds consent; a claim scopes deletion.** Send renames the file to `crash-report.sending.<id>.json`
  before uploading and deletes only that. ❌ Never `rename` a claim back: `release_claim` hard-links it, so a newer
  crash in the slot survives. DETAILS § One delivery transform.
- ❌ **The hook and handler never read the managed policy** (locks, XPC); only delivery does, and a managed
  `DisableCrashAndErrorReports` discards the pending file unoffered.
- **Dev mode: capture only, never send.** **Crash-loop guard**: a crash file under 5 s old sets `possible_crash_loop`;
  the frontend asks.
- **`survival.rs` makes two one-way amendments.** ❌ `app_fate` is not a `bool`: `false` would misclassify old files.
  ❌ `reported_in_session` means DELIVERED, so stamp it only after `auto_dispatcher::flush` uploads.
- ❌ **Nothing in the panic hook may be able to panic**, and `catch_unwind` can't help (a panic inside a hook aborts
  before unwinding). Hence the courier thread; DETAILS § Two delivery paths.
- **`contain_panics(|| …)` is the ONE reporting exemption** (a foreign parser that panics on untrusted input): one
  `warn` line, no crash file, no courier. ❌ Wrap only the foreign calls. DETAILS § The one exemption.
- **The hook installs in `run()` ahead of every fallible step.** **Keep-first**: the session's first panic writes the
  file.

## Gotchas

- **ASLR-randomized addresses mean nothing without the `image_base` shipped with them.** Resolve it at `install()` into
  an atomic; ❌ never call dyld from the handler. DETAILS § Image base.
- **What we lift from macOS's own crash report is an ALLOWLIST** (exception + the faulting thread's frames), so a
  future macOS field can't ship itself. ❌ Never widen it to `crashReporterKey`, `bootSessionUUID`, or the
  `parentProc` / `coalitionName` family: they name the machine and other apps. DETAILS § macOS crash reports.

Delivery paths, the crash-file lifecycle, the managed-policy step, the macOS-report allowlist, and the "what we send /
never send" catalog: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
