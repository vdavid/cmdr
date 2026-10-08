# Analytics (beta usage stats)

Anonymous beta usage analytics. Feature events land in an on-disk spool; a loop posts `/heartbeat` to our Worker at
most every 3 h with a PII-free config snapshot, unreported uptime, and up to 500 spooled events, which the Worker
forwards to PostHog. The app calls no third-party host. Install ids live in [`crate::install_id`].

## Files

- `mod.rs`: the consent + suppression gate (`send_permission`), the spool handle, the shared `item_count_bucket`.
- `heartbeat.rs`: the throttled loop (`crate::send_schedule`), payload, uptime, and what a 2xx settles.
- `spool.rs`: the JSONL event spool (append, batch, acknowledge, cap).
- `first_index.rs`: what a phased first index delivers, off the event stream.
- `session.rs`: the session-length ladder; ❌ no `app_quit` (a crash can't report).
- `events.rs`: the `capture` path (validate, timestamp, spool) and the debug-build PII net.
- `volume_sink.rs`: `SpooledVolumeAnalytics`, the storage backends' counter seam, feeding `capture`.
- `config_shape.rs`: the config-shape builder and `CATEGORICAL_STRING_KEYS` allowlist (the ONE place the PII-free rule
  lives). The heartbeat ships it; the Worker reuses it as PostHog `$set`.

## Must-knows

- **Two ids that never meet, by construction.** `anal_<uuid>` ([`install_id::analytics_id`]) is the heartbeat key (and
  the Worker's PostHog `distinct_id`), NEVER on a crash/error report. `diag_<uuid>` ([`install_id::diagnostics_id`]) is ONLY on
  crash/error reports, NEVER through analytics. A tester can attach their email to a report, so a shared id would make
  email → usage-history joinable. Never cross them.
- **The crash signal handler must NOT call `diagnostics_id()`** (it allocates and locks). The panic hook reads the
  `install_id::init()` snapshot.
- **Ids live in Rust-owned `install-ids.json`**, not `settings.json`, whose every write the frontend owns.
- **Consent is default-on, fully-silent opt-out** (`analytics.enabled` in `settings.json`; an opted-in install has
  no key). Opt-out sends NOTHING, and the loop deletes the spool and unreported uptime. A managed `DisableUsageStats`
  is the same opt-out: consent and the config shape read `settings.json` through `managed_policy::overlay`, ❌ never
  raw.
- **PII-free by allowlist, NEVER by redaction** (`config_shape.rs`). Include every bool- or number-valued key plus the
  small `CATEGORICAL_STRING_KEYS` allowlist (theme, sort mode, AI provider); exclude every other string, object, and
  array; add `fdaGranted` and `managedByOrganization` explicitly. A new categorical string setting joins
  `CATEGORICAL_STRING_KEYS`; NEVER loosen the bool/number rule to "include all strings."
  `excludes_pii_shaped_strings` is the invariant.
- **Only a real user's install may send.** `suppression_reason()` is the ONE gate for both pipelines: debug builds, plus
  any environment carrying one of `crate::prod_instance::NON_PROD_ENV_VARS` (canonical there because the updater gates
  on it too; ❌ never shrink or restate it): an isolated data dir mints a fresh `anal_` id, a phantom new user.
  `CMDR_ANALYTICS_FORCE=1` overrides it for the localhost-Worker test.
- **One backend path.** Backend events call `events::capture` directly; frontend events go through the `track_event`
  IPC (`commands/analytics.rs`), a thin pass-through.
- **Only a 2xx removes anything**, and exactly what that beat sent, so what's recorded mid-beat survives it.
- **A spooled event carries its own props, id, and `appVersion`, nothing more.** The Worker adds identity (`source`,
  OS, arch) and config when it forwards.
- **Every PostHog prop value MUST be categorical, a count, or a bool, never a path, name, query, prompt, or hostname.**
  Enforced by review; `events::sanitize_props` only `warn!`s in debug builds: a smoke alarm, not a filter.
- **Name events after the UI**: user-facing vocabulary (`pane_navigated`, `search_used`), categorical props
  (`volume_kind`, `mode`). The set is OPEN; a count goes through `item_count_bucket`.

Wiring, id storage, the heartbeat's schedule and payload, the spool, the event set, and the first-index events:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
