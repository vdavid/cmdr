# Importance subsystem

Deterministic, cheap folder-importance scoring consumed by expensive features (the agent, media-ML enrichment, future
cleanup/prefetch). A pure read-consumer of `indexing/`, sibling to `search/`. Full design + milestones:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

**Scope now (M1+M2): the pure scorer, per-volume storage, the scheduler that fills it on scan completion, and the
navigation-visit signal.** The consumable read API (`weight_for`/`top_n`/`explain`/…) is M3 — don't wire consumers to
importance yet (M2 stores weights but exposes only test/scheduler reads).

## Module map

- `scorer/` — the PURE formula: `score(inputs, available, weights, now_secs)` + `explain(...)`. `types.rs`
  (`FolderSignals`, `SignalSet`, `PathClass`, `Explanation`), `weights.rs` (tunable `Weights`).
- `store/` — per-volume `importance.db`: `ImportanceStore` (open/recreate + reads), path-keyed `weights` + `visits` +
  `meta`. `writer.rs` — `ImportanceWriter`, ONE writer thread per DB.
- `scheduler/` — bus + startup-sweep-driven full-volume recompute, coalesced per `volume_id`. `signals.rs` (assemble
  `FolderSignals` from the index), `classify.rs` (shared categorical classifiers), `last_used.rs` (macOS Spotlight
  sampling), `commands.rs` (`record_visit`).
- `fixtures.rs` (`cfg(test)`) — `SyntheticHome` over `InMemoryVolume` + per-folder signal derivation.

## Must-knows

- **The scorer is PURE: no `rusqlite`, no `Volume`, no filesystem, no clock.** "Now" is passed in as a `u64` — the whole
  reason the formula is unit-testable without a running app. `score` delegates to `explain`, so there's ONE formula.
- **`FolderSignals` is the persisted raw signal vector; its serde shape is load-bearing.** Adding / renaming / reordering
  a field changes what the store reads back. Keep it `serde` + `specta::Type`, camelCase, `folder_signals_serde_roundtrips`
  green.
- **Denylist and hidden/system are FLOOR overrides, not additive terms.** They cap the score at `0.0` regardless of the
  weighted sum, OUTSIDE the `SignalContribution` sum. The additive `Visibility` term is the separate soft signal.
- **Missing optional signals REDISTRIBUTE, never fabricate.** An unavailable signal's weight scales onto the available
  ones (SMB has no Spotlight ⇒ `last_used` spreads); availability (`SignalSet`) is distinct from a `None` value. Don't
  fill a missing signal with a default.
- **Classification is typed, never a string/substring branch** (`no-string-matching`): `PathClass`, `SignalSet`,
  `SignalKind` are enums; the denylist is set-membership on the folded name (reuses `search::SYSTEM_DIR_EXCLUDES`).
- **The default `Weights` are an UNVALIDATED starting point** (tuned in M3). Don't treat the numbers as correct or
  hardcode them elsewhere — pass a `Weights` through.
- **The `explain` breakdown must sum to the score when unfloored** (each `contribution == weight * raw`). Pinned by tests
  + a proptest; don't break the invariant when adding a signal.

### M2 storage + scheduler (depth in [DETAILS.md](DETAILS.md))

- **`store/` carries the index's disposable-cache discipline verbatim**: `platform_case` on EVERY connection (reused from
  `indexing::store`), delete-and-recreate on `SCHEMA_VERSION` mismatch, ONE `ImportanceWriter` thread per DB, path-keyed
  rows.
- **Staleness predicate: `row.as_of_generation < store.recompute_generation()`.** A pass writes all rows AND bumps the
  generation in ONE transaction — never split them, or a reader sees a bumped generation with un-written rows.
- **Categorical signals live in `classify.rs`, shared by `signals` (prod) AND `fixtures` (tests)** — don't re-derive
  denylist / path-class / project-marker in either; they MUST agree, and shared code guarantees it.
- **Drive recompute off the bus's `ScanCompleted` + the startup sweep (`ready_volume_ids`), NEVER off phase events**
  (network volumes never emit `Aggregating`/`Reconciling`). Coalesce per `volume_id`: a running pass sets a re-run flag,
  never a second pass.
- **`record_visit` is fire-and-forget + failure-silent** (returns `Ok(())` on a write error; never blocks navigation).
  `kMDItemLastUsedDate` sampling: dedicated OS thread + autoreleasepool, capped — never rayon, never inline on IPC.

## Adding a signal

Add the field to `FolderSignals` (+ `neutral()`), a `SignalKind` variant (+ `ALL`), a `Weights` coefficient (+
`additive_weight`), and a `raw_signal_value` arm; if optional, add a `SignalSet` flag + `signal_available` arm. Cover its
contribution direction with a test and keep the explain-sums invariant. Signal catalog + rationale: [DETAILS.md](DETAILS.md).
