# Analytics (beta usage stats)

Anonymous beta usage analytics. A background loop posts `/heartbeat` (daily-active signal + a PII-free config snapshot)
on launch and hourly. PostHog feature events ride the SAME consent gate and the SAME install id. The two install ids
live in the neutral [`crate::install_id`] module (not here), so the crash and error reporters depend on them without
pulling in `analytics`.

## Files

- `mod.rs`: heartbeat loop, consent gate, payload struct, fire-and-forget send, and the helpers `posthog` reuses.
- `posthog.rs`: the `capture` path, the debug-build PII net, the build-time key.
- `config_shape.rs`: the config-shape builder and `CATEGORICAL_STRING_KEYS` allowlist (the ONE place the PII-free rule
  lives), shared by the heartbeat `config` and the PostHog `$set`.

## Must-knows

- **Two ids that never meet, by construction.** `anal_<uuid>` ([`install_id::analytics_id`]) is the heartbeat key and
  PostHog `distinct_id`, NEVER on a crash/error report. `diag_<uuid>` ([`install_id::diagnostics_id`]) is ONLY on
  crash/error reports, NEVER through analytics. A tester can attach their email to a report, so a shared id would make
  email → usage-history joinable on our servers. Don't merge, cross-attach, or cross the pipelines.
- **Signal-safety: the crash signal handler must NOT call `diagnostics_id()`** (it allocates and locks; the handler is
  async-signal-safe). The panic-hook path reads the `install_id::init()` snapshot instead; the signal path attaches the
  diag id at next-launch assembly. See `DETAILS.md`.
- **Ids are Rust-owned, AppHandle-free files** in `install-ids.json` (not `settings.json`). The frontend owns every
  `settings.json` write; minting an id there from Rust would race that ownership on first launch. Accessors are no-arg
  so the panic hook, next-launch assembly, and the loop can all call them.
- **Consent is tri-state, default-on, fully-silent opt-out.** Opt-out is `analytics.enabled` in `settings.json`; the
  frontend persists only non-default values, so an opted-in install has NO key. `analytics_consent_granted`: `None`
  (default) and `Some(true)` → granted, `Some(false)` → opted out. ONE gate for the heartbeat loop and `track_event`.
  Opt-out sends NOTHING, not even an "I opted out" bit (so the opt-out rate comes from the update-check denominator).
- **PII-free by allowlist, NEVER by redaction** (`config_shape.rs`). Include every bool- or number-valued key
  (auto-extends, PII-free by nature); plus the small `CATEGORICAL_STRING_KEYS` allowlist (categorical enums like theme,
  sort mode, AI provider); exclude every other string and all objects and arrays; add `fdaGranted` explicitly. To ship
  a new categorical string setting, add its id to `CATEGORICAL_STRING_KEYS`; NEVER loosen the bool/number rule to
  "include all strings." `excludes_pii_shaped_strings` is the invariant. Hard nevers pipeline-wide: file names,
  contents, paths, search queries, AI prompts, keystrokes, screenshots.
- **Dev/CI suppression.** `suppressed()` is true in debug builds and when `CI` is set, so dev/CI never pollute
  production analytics. `CMDR_ANALYTICS_FORCE=1` overrides (even in debug/CI), letting an integration test drive the
  loop against a localhost Worker.
- **One backend path, one consent gate.** Backend events call `posthog::capture` directly; frontend events call the
  `track_event` IPC (`commands/analytics.rs`), a thin pass-through to `capture`. No capability entry needed.
- **Every PostHog prop value MUST be categorical, a count, or a bool, never a path, name, query, prompt, or hostname.**
  Enforced by review. `posthog::sanitize_props` is a debug-build backstop: it only `warn!`s (never strips) on PII-shaped
  strings (`/`, `\`, `@`, `~/`), so production is identical with it compiled out. Not a license to pass free-form
  strings.
- **Name events after the UI** (project rule): user-facing vocabulary (`pane_navigated`, `search_used`), categorical
  props (`volume_kind`, `mode`). Events are an OPEN set: adding one is a one-liner.

Full details (wiring, id storage, heartbeat payload, PostHog `/capture/` body and key mechanism, the starter event set
and where each fires, how to add an event): `DETAILS.md`.
