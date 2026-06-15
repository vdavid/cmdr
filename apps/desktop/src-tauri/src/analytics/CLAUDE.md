# Analytics (beta usage stats)

Anonymous beta usage analytics. A background loop posts `/heartbeat` (true daily-active signal + a PII-free config
snapshot) on launch and hourly. PostHog feature events ride the SAME consent gate and the SAME install id.

The two install ids live in the neutral [`crate::install_id`] module (not here), so the crash and error reporters can
depend on it without pulling in `analytics`.

## Files

- `mod.rs`: heartbeat loop, consent gate, payload struct, fire-and-forget send, and shared helpers (`suppressed`,
  `read_raw_settings`, `APP_HANDLE`, `analytics_consent_granted`) that `posthog` reuses.
- `posthog.rs`: the `capture(event, props)` path, pure `build_capture_body`, the debug-build `sanitize_props` net, the
  `option_env!` key mechanism.
- `config_shape.rs`: the pure config-shape builder and `CATEGORICAL_STRING_KEYS` allowlist (the ONE place the PII-free
  rule lives). Used as both the heartbeat `config` and the PostHog `$set`.

Wiring: `analytics::init(app.handle())` + `analytics::start()` from `lib.rs` setup (mirroring `space_poller`).
`install_id::init()` runs earlier (before the crash reporter) to snapshot the diag id for the panic hook.

## Must-knows

- **Two ids that never meet, by construction.** `anal_<uuid>` ([`install_id::analytics_id`]) is the heartbeat key and
  PostHog `distinct_id`, NEVER on a crash/error report. `diag_<uuid>` ([`install_id::diagnostics_id`]) is ONLY on
  crash/error reports, NEVER through analytics. Both random, both in one `install-ids.json`. Privacy invariant: a tester
  can attach their email to a report, so a shared id would make email → usage-history joinable on our servers. Don't
  merge, cross-attach, or send one through the other's pipeline.
- **Signal-safety: the crash signal handler must NOT call `diagnostics_id()`** (it allocates / locks; the handler is
  async-signal-safe). `install_id::init()` snapshots the diag id into a `OnceLock<String>` read via
  `diagnostics_id_snapshot()` on the panic-hook path; the signal path attaches the diag id at next-launch assembly (full
  stdlib), not in the handler.
- **Ids are Rust-owned, AppHandle-free files** in `install-ids.json` (not `settings.json`), resolving their data dir
  without an `AppHandle` (mirroring `settings/loader.rs`'s `early_load_*`). The frontend owns every `settings.json`
  write; minting an id there from Rust would race that ownership on first launch. No-arg accessors so the panic hook,
  next-launch assembly, and the loop can all call them.
- **Consent is tri-state, default-on, fully-silent opt-out.** Opt-out is `analytics.enabled` in `settings.json`; the
  frontend persists only non-default values, so an opted-in install has NO key. `analytics_consent_granted`: `None`
  (default) and `Some(true)` → granted, `Some(false)` → opted out. ONE gate for the heartbeat loop and `track_event`.
  Opt-out sends NOTHING (not even an "I opted out" bit), so estimate the opt-out rate from the update-check denominator.
- **PII-free by allowlist, NEVER by redaction** (`config_shape.rs`). Include every key whose JSON value is a bool or
  number (auto-extends, PII-free by nature); plus the small `CATEGORICAL_STRING_KEYS` allowlist (categorical enums like
  theme, sort mode, AI provider); exclude every other string and all objects/arrays; add `fdaGranted` (runtime state)
  explicitly. To ship a new categorical string setting, add its id to `CATEGORICAL_STRING_KEYS`; NEVER loosen the
  bool/number rule to "include all strings." The `excludes_pii_shaped_strings` test is the invariant. Hard nevers across
  the pipeline: file names, contents, paths, search queries, AI prompts, keystrokes, screenshots.
- **Dev/CI suppression.** `suppressed()` is true in debug builds and when `CI` is set, so dev/CI never pollute
  production analytics. `CMDR_ANALYTICS_FORCE=1` overrides (beats even in debug/CI), so an integration test can drive the
  loop against a localhost Worker.
- **One backend path, one consent gate.** Backend events call `posthog::capture` directly; frontend events call the
  `track_event` IPC (`commands/analytics.rs`), a thin pass-through to `capture`. The IPC takes `props_json: String`
  (frontend `JSON.stringify`s) because the prop set is open and `serde_json::Value` can't cross specta. No capability
  entry needed.
- **Every PostHog prop value MUST be categorical / count / bool, never a path, name, query, prompt, or hostname.**
  Enforced by review. `posthog::sanitize_props` is a debug-build backstop: it only `warn!`s (never strips) on PII-shaped
  strings (`/`, `\`, `@`, `~/`), so production behavior is identical with it compiled out. Not a license to pass
  free-form strings.
- **Name events after the UI** (project rule): names use the feature's user-facing vocabulary (`pane_navigated`,
  `search_used`); props are categorical (`volume_kind`, `mode`). Events are an OPEN set: adding one is a one-liner.

Full details (heartbeat payload fields, PostHog `/capture/` body shape and key mechanism, the starter event set and
where each fires, how to add an event): [DETAILS.md](DETAILS.md).
