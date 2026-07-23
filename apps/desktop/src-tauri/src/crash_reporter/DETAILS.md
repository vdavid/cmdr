# Crash reporter: details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the crash-file
lifecycle and the exact payload catalog.

## Crash file lifecycle

1. App crashes; the handler writes `crash-report.json`.
2. Next launch: `check_pending_crash_report` finds the file and parses it defensively (discards if corrupt).
3. If `updates.crashReports` is `true` and it's not a crash loop: auto-send and show a toast.
4. Otherwise: show a dialog letting the user inspect and choose to send or dismiss. Radical transparency: the dialog
   shows the exact JSON payload before sending.
5. The file is deleted after send or dismiss.

## What we send

- Full symbolicated backtrace (function names + offsets, not file paths).
- Exception type + signal, faulting address.
- App version, macOS version, CPU architecture.
- App uptime, thread count.
- Sanitized panic message (`panicMessage`): redacted through `crate::redact`, then capped at
  `PANIC_MESSAGE_MAX_CHARS` (2,000) with a `… (truncated)` marker. `None` for signal crashes, which carry no payload.
  The cap exists because the ingestion endpoint rejects a report body over 64 KB, so an uncapped `assert_eq!` dump of a
  big struct would cost the whole report instead of its own tail. The api server caps again on its side.
- Active feature flags (booleans/enums only: `indexing.enabled`, `ai.provider`, `developer.mcpEnabled`,
  `developer.verboseLogging`).
- `buildMode` (`"release"` or `"debug"`, from `cfg!(debug_assertions)`): lets the api server distinguish dev-run crashes
  from production ones in the email summary.
- `shortId` (`CRASH-XXXXX`): generated at crash-file-write time via `crate::short_id::generate("CRASH")` (shared
  alphabet with error reports). Shown to the user in the next-launch dialog so they can reference the report.
- `diagId` (`diag_<uuid>`): the diagnostics id from `crate::install_id`, so sequential reports from one install group
  together. See the `CLAUDE.md` invariant on why this is never the `anal_` analytics id and is attached at assembly
  time, not in the signal handler.
- `email` (optional): a beta tester's contact email, populated only by the dialog at send time when the user ticks the
  attach-email box. The dialog threads it into `send_crash_report(report)`.
- `systemSnapshot` (optional): the stable machine snapshot from [`crate::diagnostics_snapshot`] — Mac model, CPU counts,
  OS build, total RAM, the data-dir volume's free/total bytes, and drive-index sizes (total plus an unlabeled
  per-database list). Attached at next-launch assembly in `process_pending_crash`, never in the panic hook or signal
  handler; `live` is always `None` for crashes (see the `CLAUDE.md` invariant). PII-free: no hostname, paths, or volume
  names.
- `imageBase` (optional): the main executable's load address at crash time, as `"0x…"`. See § Image base.

## Image base

Signal-crash `backtraceFrames` are absolute virtual addresses, and ASLR re-slides the binary on every launch, so on
their own they can't be compared between two launches, let alone two users. `imageBase` is the missing half:
`frame - imageBase` is a stable per-build offset, which makes identical crash sites group across installs and lets
`atos -o <binary> -l <imageBase> <frame…>` resolve them wherever that build's symbols are available.

- **Resolved at `install()` time**, via `_dyld_get_image_header(0)` (index 0 is the main executable), and stored in an
  `AtomicU64`. The handler only does an atomic load, which is async-signal-safe; the dyld call is not, so it must never
  move into the handler.
- **The signal path reports the base recorded in the raw crash file, never the current process's.** The report is
  assembled after relaunch, and that process has a different slide, so using it would make every offset wrong. The panic
  path is the opposite case: it runs in the crashing process, so it reads the live value.
- **Raw crash file format is v2**: header (magic, version, signal, frame count) then the 8-byte base, then frames, then
  the padded app version. A v1 file fails the version check and is discarded, which is fine (raw files never outlive the
  next launch).
- **PII-free by construction**: a randomized virtual address, no user data. Deliberately only the numeric base and
  **never a loaded-image path list** — macOS's own `.ips` includes those, and they embed `/Users/<name>`.
- `None` on non-macOS Unix (no `_dyld_*`), and for reports written before the field existed.
- Symbolication still needs the matching build's symbols. The release pipeline doesn't archive dSYMs today, so in
  practice this buys cross-install **grouping** now, and full symbolication once dSYMs are retained per release.

## What we never send

- File paths, volume names, environment variables, window titles.
- Hostname, or any per-volume *names* in the index-size breakdown (sizes only, unlabeled).
- License key, transaction id, device id.
- Register dump, heap contents.
