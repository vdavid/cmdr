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
- Sanitized panic message.
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

## What we never send

- File paths, volume names, environment variables, window titles.
- Hostname, or any per-volume *names* in the index-size breakdown (sizes only, unlabeled).
- License key, transaction id, device id.
- Register dump, heap contents.
