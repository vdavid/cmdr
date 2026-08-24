# Analytics (beta usage stats)

Anonymous beta usage analytics. A background loop posts `/heartbeat` (daily-active signal + a PII-free config snapshot)
hourly and on launch. PostHog feature events ride the SAME consent gate and the SAME install id. The two install ids
live in the neutral [`crate::install_id`] module, so the crash and error reporters use them without pulling in
`analytics`.

## Files

- `mod.rs`: heartbeat loop, consent gate, payload struct, fire-and-forget send, the shared `item_count_bucket`, and the
  helpers `posthog` reuses.
- `first_index.rs`: what a phased first index delivers, off the event stream.
- `posthog.rs`: the `capture` path, the debug-build PII net, the build-time key.
- `volume_sink.rs`: `PostHogVolumeAnalytics`, the storage backends' counter seam, feeding `capture`.
- `config_shape.rs`: the config-shape builder and `CATEGORICAL_STRING_KEYS` allowlist (the ONE place the PII-free rule
  lives), shared by the heartbeat `config` and the PostHog `$set`.

## Must-knows

- **Two ids that never meet, by construction.** `anal_<uuid>` ([`install_id::analytics_id`]) is the heartbeat key and
  PostHog `distinct_id`, NEVER on a crash/error report. `diag_<uuid>` ([`install_id::diagnostics_id`]) is ONLY on
  crash/error reports, NEVER through analytics. A tester can attach their email to a report, so a shared id would make
  email → usage-history joinable on our servers. Don't merge, cross-attach, or cross the pipelines.
- **Signal-safety: the crash signal handler must NOT call `diagnostics_id()`** (it allocates and locks; the handler is
  async-signal-safe). The panic-hook path reads the `install_id::init()` snapshot instead; the signal path attaches the
  diag id at next-launch assembly.
- **Ids are Rust-owned, AppHandle-free files** in `install-ids.json`, not `settings.json`: the frontend owns every
  `settings.json` write, and minting an id there from Rust would race that ownership on first launch. Accessors are
  no-arg, so the panic hook, next-launch assembly, and the loop can all call them.
- **Consent is tri-state, default-on, fully-silent opt-out.** Opt-out is `analytics.enabled` in `settings.json`; the
  frontend persists only non-default values, so an opted-in install has NO key. `analytics_consent_granted`: `None`
  (default) and `Some(true)` → granted, `Some(false)` → opted out.
  Opt-out sends NOTHING, not even an "I opted out" bit (so the opt-out rate comes from the update-check denominator).
- **PII-free by allowlist, NEVER by redaction** (`config_shape.rs`). Include every bool- or number-valued key
  (auto-extends, PII-free by nature) plus the small `CATEGORICAL_STRING_KEYS` allowlist (theme, sort mode, AI provider);
  exclude every other string, object, and array; add `fdaGranted` explicitly. A new categorical string setting joins
  `CATEGORICAL_STRING_KEYS`; NEVER loosen the bool/number rule to "include all strings."
  `excludes_pii_shaped_strings` is the invariant. Hard nevers pipeline-wide: file names, contents, paths, search
  queries, AI prompts, keystrokes, screenshots.
- **Only a real user's install may send.** `suppression_reason()` is the ONE gate for both pipelines: debug builds,
  plus any environment carrying `CI`, `CMDR_INSTANCE_ID`, `CMDR_DATA_DIR`, `CMDR_E2E_MODE`, or `CMDR_MOCK_FDA`
  (presence, not value). An isolated data dir mints a fresh `anal_` id, so a tooling instance that slips through
  registers as a brand-new user on every launch. ❌ Never shrink that list. `CMDR_ANALYTICS_FORCE=1` overrides
  everything, for the localhost-Worker integration test.
- **One backend path.** Backend events call `posthog::capture` directly; frontend events go through the `track_event`
  IPC (`commands/analytics.rs`), a thin pass-through. No capability entry needed.
- **Every PostHog prop value MUST be categorical, a count, or a bool, never a path, name, query, prompt, or hostname.**
  Enforced by review; `posthog::sanitize_props` only `warn!`s, and only in debug builds, so it's a smoke alarm rather
  than a filter.
- **Name events after the UI** (project rule): user-facing vocabulary (`pane_navigated`, `search_used`), categorical
  props (`volume_kind`, `mode`). The set is OPEN: adding one is a one-liner, and a count goes through
  `item_count_bucket`.

Full details (wiring, id storage, heartbeat payload, the `/capture/` body, the event set and where each fires, how to
add one, and the first-index events): `DETAILS.md`.
