# Error reporter

Builds a privacy-redacted zip of recent logs plus a manifest and ships it to `POST /error-report`. Flow A is the
"Send error report" dialog; Flow B is opt-in auto-send on user-visible errors.

## Module map

- `mod.rs`: public types, `upload`, the endpoint URLs, the `log_error!` macro.
- `bundle_builder.rs` / `bundle_capper.rs` / `tail_walker.rs`: the two build pipelines, the cap pass, the
  read-from-end log walker.
- `auto_dispatcher.rs`: Flow B. `auto_sent.rs`: what the last auto-send shipped, plus the amend call.
  `breadcrumbs.rs`: ring buffer of triage events.

## Must-knows

- **Use `log_error!` at every error-level site**, and downgrade a recoverable failure to `log::warn!`: the error level
  IS the auto-report threshold. `pnpm check log-error-macro` enforces the macro, not the judgment.
- **No email leaves the machine without an explicit per-report user action.** Senders take an `AttachedEmail`, and
  `AttachedEmail::from_flow_a_dialog` is its only constructor, so a background path can't mint one. Amending from the
  dialog carries one; Flow B's own send never does (`email_for_kind` strips it from `Auto`).
- **The amend key is a credential**: never log it, bundle it, or write it to disk. `AmendKey` has no `Display` and no
  `Serialize`, and its `Debug` prints a placeholder. Keep it that way.
- **Never widen what we send.** No license keys, device IDs, raw paths, volume names, SMB creds, settings beyond the
  resolved flags, or anything outside the log dir. `manifest.system` is the one PII-reviewed exception (sizes and
  coarse machine identity). Add nothing naming a drive, path, or person.
- **Auto notes get redacted; `User` notes ship verbatim**: an auto note is a raw error message nobody previewed.
- **One dialog session, one id.** The preview mints it, the send passes it back via `BundleRequest.id`. Skip it and
  the user holds an id no report was filed under.
- **Don't gate sending on `cfg!(debug_assertions)`.** Debug builds DO send (`buildMode` tags them `[DEV]`); only `CI`
  and the `playwright-e2e` feature short-circuit.
- **`diagId` is the `diag_` diagnostics id, NEVER the `anal_` analytics id**: that split keeps an attached email
  unjoinable from analytics.
- **The auto-dispatcher does NOT flush on shutdown, and that's load-bearing**: the panic courier opens a window for
  EVERY panic, so a flush would double-report every fatal one. No queue, no persistence.
- ❌ **Never move the crash-file stamp out of `flush`'s `Ok` arm**: earlier stamps a delivery that didn't happen.
  `crash_reporter/CLAUDE.md`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
