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

## Must-knows

- **Use `log_error!` at all error-level sites in the desktop crate**, not raw `log::error!`. If a failure is
  recoverable, expected, or not user-impacting, downgrade to `log::warn!`. The error-level threshold IS the auto-report
  threshold; `pnpm check log-error-macro` fails on any new raw `log::error!` outside the macro definition.
- **Flow B never attaches an email (load-bearing privacy rule).** `BundleManifest.email` may be set only on Flow A
  (`BundleKind::User`, the dialog with the attach-email checkbox). Flow B (`BundleKind::Auto`) always ships
  `email: None`. Enforced structurally: `build_bundle` routes the email through
  `bundle_builder::email_for_kind(kind, email)`, which returns `None` for `Auto` regardless of what's passed. Don't
  wire an email into the auto path expecting it to ship.
- **Only `BundleKind::Auto` notes get redacted; `User` notes ship verbatim.** Auto notes are built from a raw error
  message (often containing paths like `current_exe()`) the user never previews, so the same `redact_line_salted` pass
  that scrubs log lines also scrubs the auto note. Every log line is redacted via `crate::redact::redact_line` before it
  hits the zip.
- **Never widen what we send.** No license keys, transaction/device IDs, raw paths, volume names, SMB creds, settings
  beyond the resolved feature flags, or anything outside the log dir (no app data files, no `settings.json`).
- **Don't gate `upload()` on `cfg!(debug_assertions)`.** Debug builds DO upload (that's "Send error report" working in
  dev); the manifest's `buildMode: "debug"` makes the api server prefix the Discord title with `[DEV]`. `upload`
  short-circuits only on `CI` env and the `playwright-e2e` feature (compile-time, so E2E reports can't flood the live
  channel).
- **The server uses the client-supplied `id` verbatim.** Don't regenerate server-side: the trailing UUID in the R2 key
  already guarantees uniqueness, and a mismatched id in preview vs. toast confuses users.
- **`diagId` is the `diag_` diagnostics id, NEVER the `anal_` analytics id** (see `analytics/CLAUDE.md` § "Two ids that
  never meet"): the split keeps a voluntarily-attached email unjoinable to the analytics stream.
- **The auto-dispatcher does NOT flush on shutdown, by design.** A crash inside the 60s debounce window drops the
  pending flush; panics are covered by `crash_reporter` instead, and soft errors restart a window on the next launch.
  Don't add a queue or on-disk persistence: the manual flow is the safety net.
- **Don't introduce a max-line-length assumption in the tail walker** (`CHUNK_SIZE` 64 KB): backtrace symbol metadata
  produces ~10 KB lines with no upper bound; a long line spans multiple chunks and accumulates in `pending`.
- **The compressed-size counter is a lower bound** (the deflater buffers ~64 KB unflushed). Budget conservatively; don't
  read the buffer's `len()` via `ZipWriter::get_mut()` (unsafe, desyncs seek state).

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
