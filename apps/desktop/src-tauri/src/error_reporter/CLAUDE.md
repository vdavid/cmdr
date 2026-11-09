# Error reporter

Builds a redacted zip of recent logs plus a manifest for `POST /error-report`. Flow A: the
"Send error report" dialog. Flow B: opt-in auto-send on user-visible errors.

## Module map

- `mod.rs`: public types, `upload`, endpoint URLs, `log_error!`.
- `bundle_builder.rs` / `bundle_capper.rs` / `tail_walker.rs`: ZIP pipelines, cap pass, and read-from-end walker.
- `preview_file.rs`: the bundle as one text file for the File viewer.
- `auto_dispatcher.rs`: Flow B. `auto_sent.rs`: what the last auto-send shipped, plus the amend call.
  `breadcrumbs.rs`: ring buffer of triage events. `state_history/`: the typed state ring and its report transform.

## Must-knows

- **Use `log_error!` at every error-level site**, and downgrade a recoverable failure to `log::warn!`: the error level
  IS the auto-report threshold. `pnpm check log-error-macro` enforces the macro, not the judgment.
- **No email leaves the machine without an explicit per-report user action.** Senders take an `AttachedEmail`, and
  `AttachedEmail::from_flow_a_dialog` is its only constructor, so a background path can't mint one. Amending from the
  dialog carries one; Flow B's own send never does (`email_for_kind` strips it from `Auto`).
- **The amend key is a credential**: never log it, bundle it, or write it to disk. `AmendKey` has no `Display` and no
  `Serialize`, and its `Debug` prints a placeholder. Keep it that way.
- **Only our own crates' DEBUG/TRACE ships; INFO+ from anyone, redacted.** The file chain tags our lines at write time
  (`../logging/provenance.rs`), and both pipelines filter on that tag, continuations following their header. ❌ Never
  decide provenance from the target column: our short targets can equal a third-party crate's. DETAILS § "Only our own
  debug output ships".
- **Never widen what we send.** No license keys, device IDs, raw paths, volume names, SMB creds, settings off
  `triage_settings.rs`'s list, or anything outside the log dir. `manifest.system` is the one PII-reviewed exception (sizes and
  coarse machine identity). Add nothing naming a drive, path, or person.
- **Diagnostic state stays typed and process-local** (at most eight captures). Raw identities never enter logs or
  disk; bundle assembly transforms them with that report's context.
- **Breadcrumbs accept only `BreadcrumbEvent`.** Add a reviewed enum variant and typed fields for new diagnostic facts;
  never restore free-form messages, maps, or JSON. Unknown IPC fields fail closed, and command events retain only ids
  from the frontend's authoritative `COMMAND_IDS` tuple.
- **`User` notes ship verbatim; `Auto` notes go through the report's redaction.** The auto note quotes the first message
  as a `detail=` field with its log markers kept (`LoggedText`), so the report pass hashes, redacts, and caps it.
- **One dialog session, one id.** The preview mints it, the send passes it back via `BundleRequest.id`. Skip it and
  the user holds an id no report was filed under. The same ID also rebuilds the same report-scoped redaction context.
- **Don't gate sending on `cfg!(debug_assertions)`.** Debug builds DO send (`buildMode` tags them `[DEV]`); only `CI`
  and the `playwright-e2e` feature short-circuit.
- **`diagId` is the `diag_` diagnostics id, NEVER the `anal_` analytics id**: that split keeps an attached email
  unjoinable from analytics.
- **A managed `DisableCrashAndErrorReports` wins over a stored `updates.errorReports: true`.** Every send rides
  `server_request::send`; the send commands and Flow B's `take_window_to_send` also ask `check_policy` first, so no
  bundle gets built for nothing. `save_error_report_to_disk` (every build) stays allowed.
- **The auto-dispatcher does NOT flush on shutdown, and that's load-bearing**: the panic courier opens a window for
  EVERY panic, so a flush would double-report every fatal one.
- ❌ **Never move the crash-file stamp out of `flush`'s `Ok` arm**: earlier stamps a delivery that didn't happen.
  `crash_reporter/CLAUDE.md`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
