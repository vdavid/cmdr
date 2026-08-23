# Error reporter

Builds a privacy-redacted zip bundle of recent log files plus a JSON manifest, then (in prod) ships it to
`POST /error-report` on the api server. Two entry flows: Flow A (user-initiated "Send error report" dialog) and Flow B
(opt-in auto-send on user-visible errors).

## Module map

- `mod.rs`: public surface (`BundleKind`, `BundleScope`, `BundleManifest`, `ResolvedSettings`, `generate_short_id`,
  `upload`, the `log_error!` macro).
- `bundle_builder.rs` / `bundle_capper.rs` / `tail_walker.rs`: the two build pipelines, the cap pass, and the
  read-from-end log walker.
- `auto_dispatcher.rs`: Flow B (debounced auto-send). `breadcrumbs.rs`: bounded ring buffer of triage events.
  `report_error()` is `log_error!`'s body as a function, for the one caller holding someone else's backtrace (the crash
  reporter's panic courier). Everything else uses the MACRO.

## Must-knows

- **Use `log_error!` at every error-level site in the desktop crate.** Downgrade a recoverable, expected, or
  non-user-impacting failure to `log::warn!`: the error-level threshold IS the auto-report threshold.
  `pnpm check log-error-macro` fails on a raw `log::error!` outside the macro definition.
- **Flow B never attaches an email (load-bearing privacy rule).** `BundleManifest.email` is settable only on Flow A
  (`BundleKind::User`, the dialog with the attach-email checkbox); Flow B (`BundleKind::Auto`) always ships
  `email: None`. Enforced structurally by `bundle_builder::email_for_kind(kind, email)`, which returns `None` for
  `Auto` whatever it's handed. Don't wire an email into the auto path expecting it to ship.
- **Only `BundleKind::Auto` notes get redacted; `User` notes ship verbatim.** An auto note comes from a raw error
  message the user never previews (often with paths from `current_exe()`), so `redact_line_salted` scrubs it like a log
  line. Every log line goes through `crate::redact::redact_line` before it hits the zip.
- **Never widen what we send.** No license keys, transaction/device IDs, raw paths, volume names, SMB creds, settings
  beyond the resolved feature flags, or anything outside the log dir. `manifest.system`
  ([`crate::diagnostics_snapshot`]) is the one PII-reviewed exception: coarse machine identity, aggregate
  RAM/disk/index *sizes*, and an UNLABELED per-volume size list. Add nothing that names a drive, path, or person.
- **Don't gate `upload()` on `cfg!(debug_assertions)`.** Debug builds DO upload (that's "Send error report" in dev);
  `buildMode: "debug"` makes the api server prefix the Discord title with `[DEV]`. `upload` short-circuits only on the
  `CI` env and the `playwright-e2e` feature, so E2E reports can't flood the live channel.
- **The server uses the client-supplied `id` verbatim.** The trailing UUID in the R2 key already guarantees
  uniqueness, and a mismatched id in preview vs. toast confuses users.
- **`diagId` is the `diag_` diagnostics id, NEVER the `anal_` analytics id** (see `analytics/CLAUDE.md` § "Two ids that
  never meet"): the split keeps a voluntarily-attached email unjoinable to the analytics stream.
- **The auto-dispatcher does NOT flush on shutdown, by design.** A crash inside the 60s debounce window drops the
  pending flush; panics are covered by `crash_reporter` instead, and soft errors restart a window on the next launch.
  Don't add a queue or on-disk persistence: the manual flow is the safety net.
- **That non-flush is load-bearing.** The panic courier opens a window for EVERY panic; the window dying with the
  process is what keeps FATAL panics off this path (the crash file reports those). A shutdown flush would double-report
  every one. DETAILS § Panics as a Flow B source.
- **No max-line-length assumption in the tail walker** (`CHUNK_SIZE` 64 KB): backtrace symbol metadata makes ~10 KB
  lines with no upper bound, and a long one spans chunks and accumulates in `pending`.
- **The compressed-size counter is a lower bound** (the deflater buffers ~64 KB unflushed). Budget conservatively; don't
  read the buffer's `len()` via `ZipWriter::get_mut()` (unsafe, desyncs seek state).

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
